pub mod client;
pub mod networktables;

use crate::harness::RowId;

/// Run one delivery case for one role, publisher or subscriber.
///
/// # Errors
///
/// Returns an error for a case or role that no probe implements.
pub fn run_delivery(
    id: &RowId,
    role: &str,
    host: &str,
    payload: usize,
    rate: u64,
    count: u64,
    samples: u64,
) -> anyhow::Result<()> {
    match (id.case.as_str(), role) {
        ("publish", "subscriber") => networktables::subscribe(host, payload, samples, id),
        ("publish", "publisher") => networktables::publish(host, payload, rate, count),
        ("publish_client", "subscriber") => networktables::subscribe(host, payload, samples, id),
        ("publish_client", "publisher") => client::publish(host, payload, rate, count),
        ("subscribe_client", "subscriber") => client::subscribe(host, payload, samples, id, 1),
        ("subscribe_client", "publisher") => networktables::publish(host, payload, rate, count),
        ("fanout", "subscriber") => {
            client::subscribe(host, payload, samples, id, crate::catalog::FANOUT)
        }
        ("fanout", "publisher") => networktables::publish(host, payload, rate, count),
        (case, role) => Err(anyhow::anyhow!("no probe runs {case} as {role}")),
    }
}
