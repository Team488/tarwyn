pub mod client;
pub mod nt4;
pub mod telemetry;
pub mod udp;

/// Run one delivery case for one implementation and role.
///
/// A delivery case needs a publisher process and a subscriber process,
/// started separately by the shell harness; `role` says which one this
/// invocation is.
///
/// # Errors
///
/// Returns [`std::io::ErrorKind::InvalidInput`] for a pairing no subject
/// implements, so a missing wiring fails rather than measuring nothing.
#[allow(clippy::too_many_arguments)]
pub fn run_delivery(
    case: &str,
    implementation: &str,
    role: &str,
    host: &str,
    payload: usize,
    rate: u64,
    count: u64,
    samples: u64,
) -> std::io::Result<()> {
    let label = format!("{case} {implementation}");
    match (case, implementation, role) {
        ("publish", "tarwyn-rust", "subscriber") => nt4::subscribe(host, payload, samples, &label),
        ("publish", "tarwyn-rust", "publisher") => nt4::publish(host, payload, rate, count),
        ("publish", "tarwyn-rust-client", "subscriber") => {
            nt4::subscribe(host, payload, samples, &label)
        }
        ("publish", "tarwyn-rust-client", "publisher") => {
            client::publish(host, payload, rate, count)
        }
        ("telemetry_publish", "tarwyn-rust", "subscriber") => {
            telemetry::subscribe(host, payload, samples, &label)
        }
        ("telemetry_publish", "tarwyn-rust", "publisher") => {
            telemetry::publish(host, payload, rate, count)
        }
        ("udp_floor", "reference", "subscriber") => {
            udp::subscribe(&udp_addr(host), payload, samples, &label)
        }
        ("udp_floor", "reference", "publisher") => {
            udp::publish(&udp_addr(host), payload, rate, count)
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("no subject runs {case} for {implementation} as {role}"),
        )),
    }
}

/// The UDP subject's socket address for a host argument.
///
/// The subject takes a socket address rather than a host; the default host
/// maps to its own default address, and any other host passes through
/// unchanged.
fn udp_addr(host: &str) -> String {
    if host == "127.0.0.1" {
        udp::DEFAULT_ADDR.to_string()
    } else {
        host.to_string()
    }
}
