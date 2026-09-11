pub mod client;
pub mod networktables;

use crate::harness::RowId;

/// Run one delivery case for one role.
///
/// A delivery case needs a publisher process and a subscriber process,
/// started separately by the shell harness; `role` says which one this
/// invocation is.
///
/// The implementation is not part of the dispatch. Which implementation a case
/// may be run for is the catalog's ruling, checked once before this is called;
/// which code runs is the case's, decided here. Naming both in one match let
/// the two disagree, and they did.
///
/// # Errors
///
/// Returns [`std::io::ErrorKind::InvalidInput`] for a case or role no probe
/// implements, so a missing wiring fails rather than measuring nothing.
pub fn run_delivery(
    id: &RowId,
    role: &str,
    host: &str,
    payload: usize,
    rate: u64,
    count: u64,
    samples: u64,
) -> std::io::Result<()> {
    match (id.case.as_str(), role) {
        ("publish", "subscriber") => networktables::subscribe(host, payload, samples, id),
        ("publish", "publisher") => networktables::publish(host, payload, rate, count),
        ("publish_client", "subscriber") => networktables::subscribe(host, payload, samples, id),
        ("publish_client", "publisher") => client::publish(host, payload, rate, count),
        (case, role) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("no probe runs {case} as {role}"),
        )),
    }
}
