//! Round-trip cases: operations that block the caller until the server answers.
//!
//! The number is the call's own wall time in the calling process, which is what
//! the caller experiences. Calls are paced like every other case, because calls
//! issued back to back run warm and read far faster than a paced one.

use crate::harness::Pacer;
use std::time::Instant;
use tarwyn_client::TarwynClient;
use tarwyn_protobuf::protobuf::supported_values::Kind;

/// The channel every round-trip case reads and writes.
const CHANNEL: &str = "bench_rt";

/// Run one round-trip case, returning the nanoseconds each call took.
///
/// `warmup` calls are made and discarded first, so a connection still being
/// established is not measured. For `compare_and_set` the seed write leaves
/// the channel holding `1.5`, so the first call's `expected` mismatches and
/// is rejected; `warmup` is raised to at least 1 for that case so the
/// rejection lands in the discarded warmup rather than the measured run, and
/// every measured call after it compares `2.5` against the `2.5` the prior
/// call wrote, taking the server's accept path rather than its reject path.
///
/// # Errors
///
/// Returns [`std::io::ErrorKind::InvalidInput`] if the case is not one this
/// module implements.
#[allow(dead_code)]
pub fn run(
    case: &str,
    host: &str,
    rate_hz: u64,
    count: u64,
    warmup: u64,
) -> std::io::Result<Vec<u64>> {
    let client = TarwynClient::connect(host);
    client.send_double(CHANNEL, 1.5);
    std::thread::sleep(std::time::Duration::from_millis(500));

    let warmup = if case == "compare_and_set" {
        warmup.max(1)
    } else {
        warmup
    };

    let mut call: Box<dyn FnMut() -> bool> = match case {
        "get" => Box::new(|| client.get(CHANNEL).is_some()),
        "compare_and_set" => Box::new(|| {
            let _ = client.compare_and_set(CHANNEL, Some(Kind::Double(2.5)), Kind::Double(2.5));
            true
        }),
        "delete" => Box::new(|| {
            client.send_double(CHANNEL, 1.5);
            client.delete(CHANNEL) > 0
        }),
        "tables" => Box::new(|| !client.tables("").is_empty()),
        "ping" => Box::new(|| client.ping().is_some()),
        other => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{other} is not a round-trip case"),
            ));
        }
    };

    let _ = measure_calls(rate_hz, warmup, &mut call);
    Ok(measure_calls(rate_hz, count, call))
}

/// Pace `count` calls and return the nanoseconds each successful one took.
///
/// A call returning `false` failed; failures are left out of the latencies
/// rather than recorded as fast, since a call that did not happen is not a
/// measurement of one that did.
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

    #[test]
    fn an_unknown_case_is_refused_rather_than_silently_skipped() {
        let error = super::run("no_such_case", "127.0.0.1", 500, 1, 0).unwrap_err();
        assert!(
            error.to_string().contains("no_such_case"),
            "the error must name the case, got {error}"
        );
    }
}
