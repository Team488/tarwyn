//! Waking a reader just before its next message is due.
//!
//! Robot traffic is periodic. A reader that knows the period sleeps through
//! most of it, wakes shortly before the next message is due and spins the last
//! stretch, so the bytes are read by a running thread at a fraction of the
//! cost of spinning the whole period. An aperiodic stream blocks as before.

use std::io::{self, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// Consecutive arrivals in step before the next one is predicted.
const MIN_STREAK: u32 = 2;

/// A gap this long ends the streak; the stream has paused.
const MAX_GAP: Duration = Duration::from_millis(250);

/// The margin is clamped to this fraction of the period, so a dense stream
/// spins for at most half of each interval rather than all of it.
const MARGIN_FRACTION: u32 = 4;

/// The margin a reader uses unless told otherwise.
///
/// Wide enough to clear the timer's default slack and an idle core's exit on
/// the machines measured, narrow enough that a 50 Hz robot loop costs a few
/// percent of a core; see `bench/BENCHMARK.md`.
pub const DEFAULT_MARGIN: Duration = Duration::from_micros(200);

/// The last few arrivals on one socket, and what they say about the next.
///
/// `margin` is how early the reader wakes and how late it keeps spinning,
/// covering the timer's slack, the core's idle exit and the publisher's jitter.
#[derive(Debug)]
pub struct Predictor {
    last: Option<Instant>,
    interval: Option<Duration>,
    streak: u32,
    margin: Duration,
}

impl Predictor {
    /// A predictor that wakes `margin` before each due time and spins as long
    /// after it. A zero margin predicts nothing.
    pub fn new(margin: Duration) -> Self {
        Predictor {
            last: None,
            interval: None,
            streak: 0,
            margin,
        }
    }

    /// How early the reader wakes and how late it spins for the current
    /// period; zero when off.
    pub fn margin(&self) -> Duration {
        match self.interval {
            Some(interval) => self.margin.min(interval / MARGIN_FRACTION),
            None => self.margin,
        }
    }

    /// Notes that a message arrived at `now`.
    ///
    /// The interval is a running average of recent gaps. A gap far outside it,
    /// or longer than a quarter second, ends the streak: the stream paused or its
    /// rate changed, and the next due time is unknown until two arrivals agree.
    pub fn record(&mut self, now: Instant) {
        if let Some(last) = self.last {
            let gap = now.saturating_duration_since(last);
            let in_step = match self.interval {
                None => gap <= MAX_GAP,
                Some(interval) => gap <= MAX_GAP && gap <= interval * 4,
            };
            if in_step {
                self.interval = Some(match self.interval {
                    None => gap,
                    Some(interval) => interval / 4 * 3 + gap / 4,
                });
                self.streak += 1;
            } else {
                self.interval = None;
                self.streak = 0;
            }
        }
        self.last = Some(now);
    }

    /// When the next message is due, once enough arrivals agree on a period.
    pub fn next_due(&self) -> Option<Instant> {
        if self.margin.is_zero() || self.streak < MIN_STREAK {
            return None;
        }
        Some(self.last? + self.interval?)
    }
}

/// Reads from `socket`, timing the wait to the predicted arrival.
///
/// With a due time, sleeps until `margin` before it, then spins until `margin`
/// after it; a message that lands in that window is read by a running thread.
/// Without one, or once the window lapses, blocks like an ordinary read.
/// Every read that returns data is recorded on the predictor.
pub fn read_predicted(
    socket: &TcpStream,
    buf: &mut [u8],
    predictor: &mut Predictor,
) -> io::Result<usize> {
    if let Some(due) = predictor.next_due() {
        let margin = predictor.margin();
        let wake = due.checked_sub(margin).unwrap_or(due);
        if !wait_readable_until(socket, wake)? {
            let spin_end = due + margin;
            loop {
                match read_now(socket, buf) {
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Ok(n) => {
                        predictor.record(Instant::now());
                        return Ok(n);
                    }
                    Err(e) => return Err(e),
                }
                if Instant::now() >= spin_end {
                    break;
                }
                std::hint::spin_loop();
            }
        }
    }
    let n = (&*socket).read(buf)?;
    predictor.record(Instant::now());
    Ok(n)
}

/// Spins on `socket` until `deadline`, returning what a read yields or
/// `WouldBlock` if nothing arrived.
///
/// The fixed-window form of busy polling: a thread that is still spinning
/// when the bytes land pays neither the scheduler nor the core's idle exit.
pub fn read_spinning(socket: &TcpStream, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
    loop {
        match read_now(socket, buf) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            other => return other,
        }
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        std::hint::spin_loop();
    }
}

/// Reads what the socket already holds, without waiting for more.
///
/// A per-call flag rather than `set_nonblocking`, which is a property of the
/// open file description and would reach the writer's duplicate of the socket.
#[cfg(unix)]
pub fn read_now(socket: &TcpStream, buf: &mut [u8]) -> io::Result<usize> {
    use std::os::fd::AsRawFd;
    let received = unsafe {
        libc::recv(
            socket.as_raw_fd(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            libc::MSG_DONTWAIT,
        )
    };
    usize::try_from(received).map_err(|_| io::Error::last_os_error())
}

#[cfg(not(unix))]
pub fn read_now(_socket: &TcpStream, _buf: &mut [u8]) -> io::Result<usize> {
    Err(io::ErrorKind::WouldBlock.into())
}

/// Whether polling without changing the socket's flags is available here.
pub fn supported() -> bool {
    cfg!(unix)
}

/// Sleeps until `socket` is readable or `deadline` passes, saying which.
///
/// An interrupted wait resumes; a poll error is returned.
#[cfg(unix)]
fn wait_readable_until(socket: &TcpStream, deadline: Instant) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        let mut fds = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = poll_for(&mut fds, remaining);
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        return Ok(ready > 0);
    }
}

#[cfg(not(unix))]
fn wait_readable_until(_socket: &TcpStream, _deadline: Instant) -> io::Result<bool> {
    Ok(false)
}

/// `ppoll` where there is one, for a wait that is not rounded to milliseconds.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn poll_for(fds: &mut libc::pollfd, timeout: Duration) -> libc::c_int {
    let timeout = libc::timespec {
        tv_sec: timeout.as_secs() as libc::time_t,
        tv_nsec: libc::c_long::from(timeout.subsec_nanos()),
    };
    unsafe { libc::ppoll(fds, 1, &timeout, std::ptr::null()) }
}

/// `poll` rounds up to whole milliseconds, so the spin that follows covers
/// what the timer cannot.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn poll_for(fds: &mut libc::pollfd, timeout: Duration) -> libc::c_int {
    let millis = timeout.as_millis().min(libc::c_int::MAX as u128) as libc::c_int;
    unsafe { libc::poll(fds, 1, millis) }
}

#[cfg(test)]
mod tests {
    use super::{MIN_STREAK, Predictor};
    use std::time::{Duration, Instant};

    #[test]
    fn a_period_is_predicted_once_two_gaps_agree() {
        let mut p = Predictor::new(Duration::from_micros(200));
        let start = Instant::now();
        let step = Duration::from_millis(2);
        p.record(start);
        assert!(p.next_due().is_none(), "one arrival says nothing");
        p.record(start + step);
        assert!(
            p.next_due().is_none(),
            "one gap is not yet a period ({MIN_STREAK} needed)"
        );
        p.record(start + step * 2);
        assert_eq!(p.next_due(), Some(start + step * 3));
    }

    #[test]
    fn a_pause_ends_the_streak_and_a_rate_change_restarts_it() {
        let mut p = Predictor::new(Duration::from_micros(200));
        let start = Instant::now();
        let step = Duration::from_millis(2);
        for i in 0..3 {
            p.record(start + step * i);
        }
        assert!(p.next_due().is_some());
        p.record(start + step * 3 + Duration::from_secs(1));
        assert!(p.next_due().is_none(), "a long gap forgets the period");
        let resumed = start + step * 3 + Duration::from_secs(1);
        p.record(resumed + step);
        p.record(resumed + step * 2);
        assert_eq!(p.next_due(), Some(resumed + step * 3));
    }

    #[test]
    fn a_dense_stream_clamps_the_margin_to_a_fraction_of_its_period() {
        let mut p = Predictor::new(Duration::from_micros(200));
        let start = Instant::now();
        for i in 0..3 {
            p.record(start + Duration::from_micros(100 * i));
        }
        assert_eq!(p.margin(), Duration::from_micros(25));
    }

    #[test]
    fn a_zero_margin_never_predicts() {
        let mut p = Predictor::new(Duration::ZERO);
        let start = Instant::now();
        for i in 0..5 {
            p.record(start + Duration::from_millis(2 * i));
        }
        assert!(p.next_due().is_none());
    }
}
