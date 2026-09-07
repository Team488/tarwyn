//! Round-trip cases: operations that block the caller until the server answers.
//!
//! The number is the call's own wall time in the calling process, which is what
//! the caller experiences. Calls are paced like every other case, because calls
//! issued back to back run warm and read far faster than a paced one.

use crate::harness::Pacer;
use std::time::Instant;

/// Pace `count` calls and return the nanoseconds each successful one took.
///
/// A call returning `false` failed; failures are left out of the latencies
/// rather than recorded as fast, since a call that did not happen is not a
/// measurement of one that did.
#[allow(dead_code)]
pub fn measure_calls<F>(rate_hz: u64, count: u64, mut call: F) -> Vec<u64>
where
    F: FnMut() -> bool,
{
    let mut pacer = Pacer::new(rate_hz);
    let mut latencies = Vec::with_capacity(count as usize);
    for _ in 0..count {
        pacer.wait();
        let started = Instant::now();
        let ok = call();
        let elapsed = started.elapsed();
        if ok {
            latencies.push(elapsed.as_nanos() as u64);
        }
    }
    latencies
}

#[cfg(test)]
mod tests {
    use super::measure_calls;
    use std::time::Duration;

    #[test]
    fn the_reported_median_tracks_the_call_it_timed() {
        let mut calls = 0;
        let latencies = measure_calls(2000, 40, || {
            calls += 1;
            std::thread::sleep(Duration::from_micros(300));
            true
        });

        assert_eq!(calls, 40, "every scheduled call must be made");
        assert_eq!(latencies.len(), 40);
        let mut sorted = latencies.clone();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        assert!(
            (250_000..800_000).contains(&median),
            "a 300 us call should read near 300 us, got {} ns",
            median
        );
    }

    #[test]
    fn a_failed_call_is_counted_but_not_timed() {
        let latencies = measure_calls(2000, 10, || false);
        assert!(
            latencies.is_empty(),
            "a failed call has no latency to report"
        );
    }
}
