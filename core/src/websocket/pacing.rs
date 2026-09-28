//! Waking a reader just before its next message is due, for periodic
//! streams. An aperiodic stream blocks as usual.

use std::io::{self, Read};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// Consecutive arrivals in step before the next one is predicted.
const MIN_STREAK: u32 = 2;

/// A gap this long means the stream paused, so it ends the streak.
const MAX_GAP: Duration = Duration::from_millis(250);

/// The margin is clamped to this fraction of the period, so a dense stream
/// spins for at most half of each interval.
const MARGIN_FRACTION: u32 = 4;

/// The margin a reader uses unless told otherwise. It covers timer slack and
/// an idle core's wakeup for a few percent of a core (`bench/BENCHMARK.md`).
pub const DEFAULT_MARGIN: Duration = Duration::from_micros(200);

/// The recent arrivals on one socket, and when the next one is due.
#[derive(Debug)]
pub struct Predictor {
    last: Option<Instant>,
    interval: Option<Duration>,
    streak: u32,
    margin: Duration,
    tally: Tally,
}

/// How each read on a socket ended. A tail of `late` reads means the margin
/// is too narrow.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Reads with no prediction to time: the stream was aperiodic or new.
    pub blind: u64,
    /// The message landed inside the spin window.
    pub hit: u64,
    /// The message arrived before the reader woke, so the read blocked briefly.
    pub early: u64,
    /// The window lapsed with nothing, and the reader fell back to a blocking
    /// read.
    pub late: u64,
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
            tally: Tally::default(),
        }
    }

    /// How the reads so far ended.
    pub fn tally(&self) -> Tally {
        self.tally
    }

    /// How early the reader wakes and how late it spins for the current
    /// period, or zero when prediction is off.
    pub fn margin(&self) -> Duration {
        match self.interval {
            Some(interval) => self.margin.min(interval / MARGIN_FRACTION),
            None => self.margin,
        }
    }

    /// Notes that a message arrived at `now`. A gap far from the running
    /// interval, or over a quarter second, resets the streak.
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

/// Reads from `socket`: a `busy_poll` spin first, then the predicted wait.
/// Every read that returns data is recorded on the predictor.
pub fn read_paced(
    socket: &TcpStream,
    buf: &mut [u8],
    busy_poll: Duration,
    predictor: &mut Predictor,
) -> io::Result<usize> {
    if !busy_poll.is_zero() {
        match read_spinning(socket, buf, Instant::now() + busy_poll) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Ok(n) => {
                predictor.record(Instant::now());
                return Ok(n);
            }
            Err(e) => return Err(e),
        }
    }
    read_predicted(socket, buf, predictor)
}

/// Reads from `socket`, sleeping until `margin` before the predicted arrival
/// and spinning until `margin` after. Without a prediction, or past the
/// window, it blocks like an ordinary read.
pub fn read_predicted(
    socket: &TcpStream,
    buf: &mut [u8],
    predictor: &mut Predictor,
) -> io::Result<usize> {
    if let Some(due) = predictor.next_due() {
        let margin = predictor.margin();
        let wake = due.checked_sub(margin).unwrap_or(due);
        if wait_readable_until(socket, wake)? {
            predictor.tally.early += 1;
        } else {
            let spin_end = due + margin;
            loop {
                match read_now(socket, buf) {
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Ok(n) => {
                        predictor.tally.hit += 1;
                        predictor.record(Instant::now());
                        return Ok(n);
                    }
                    Err(e) => return Err(e),
                }
                if Instant::now() >= spin_end {
                    predictor.tally.late += 1;
                    break;
                }
                std::hint::spin_loop();
            }
        }
    } else {
        predictor.tally.blind += 1;
    }
    let n = (&*socket).read(buf)?;
    predictor.record(Instant::now());
    Ok(n)
}

/// Spins on `socket` until `deadline`, returning what a read yields or
/// `WouldBlock`.
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

/// Reads what the socket already holds, without waiting and without changing
/// the socket's flags.
#[cfg(unix)]
pub fn read_now(socket: &TcpStream, buf: &mut [u8]) -> io::Result<usize> {
    use std::os::fd::AsRawFd;
    // MSG_DONTWAIT per call, since `set_nonblocking` would also change the writer's copy.
    // SAFETY: `buf` is exclusively borrowed and the kernel writes at most `buf.len()` bytes.
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

/// Always `WouldBlock`, since off Unix a read cannot poll without changing
/// the socket's flags.
#[cfg(not(unix))]
pub fn read_now(_socket: &TcpStream, _buf: &mut [u8]) -> io::Result<usize> {
    Err(io::ErrorKind::WouldBlock.into())
}

/// Whether polling without changing the socket's flags is available here.
pub fn supported() -> bool {
    cfg!(unix)
}

/// Sleeps until `socket` is readable or `deadline` passes, and says which.
/// An interrupted wait resumes, and any other poll error is returned.
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

/// `ppoll` where there is one, for a wait not rounded to milliseconds.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn poll_for(fds: &mut libc::pollfd, timeout: Duration) -> libc::c_int {
    let timeout = libc::timespec {
        tv_sec: timeout.as_secs() as libc::time_t,
        tv_nsec: libc::c_long::from(timeout.subsec_nanos()),
    };
    // SAFETY: one borrowed `pollfd`, a timeout that outlives the call, and a null mask.
    unsafe { libc::ppoll(fds, 1, &timeout, std::ptr::null()) }
}

/// `poll`, rounded up to whole milliseconds. The spin after it covers the rest.
#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn poll_for(fds: &mut libc::pollfd, timeout: Duration) -> libc::c_int {
    let millis = timeout.as_millis().min(libc::c_int::MAX as u128) as libc::c_int;
    // SAFETY: `fds` is one borrowed `pollfd`, matching the count of 1.
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
