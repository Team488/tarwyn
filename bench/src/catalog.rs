//! Every benchmark case, declared once.
//!
//! A case names an operation, the table it belongs in, how it is timed, and
//! which implementations can run it. `generate.sh` reads this through
//! `bench list-cases` rather than naming subjects itself, so adding a case is
//! an edit here and one file under `cases/`.

/// How a case is timed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Publisher-stamped to subscriber-received, across two processes.
    Delivery,
    /// The wall time of one blocking call, in the calling process.
    RoundTrip,
}

impl Mode {
    /// The word this mode is printed and parsed as.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Delivery => "delivery",
            Mode::RoundTrip => "round-trip",
        }
    }
}

/// One benchmark case.
#[derive(Debug)]
pub struct Case {
    /// The case name, as passed to `bench run --case`.
    pub name: &'static str,
    /// Which table the case appears in.
    pub group: &'static str,
    /// How the case is timed.
    pub mode: Mode,
    /// Which implementations can run it.
    pub implementations: &'static [&'static str],
}

/// Every case this harness knows how to run.
pub const CASES: &[Case] = &[
    Case {
        name: "publish",
        group: "delivery",
        mode: Mode::Delivery,
        implementations: &["tarwyn-rust", "tarwyn-rust-client", "ntcore", "tarwyn"],
    },
    Case {
        name: "telemetry_publish",
        group: "best-effort",
        mode: Mode::Delivery,
        implementations: &["tarwyn-rust"],
    },
    Case {
        name: "udp_floor",
        group: "best-effort",
        mode: Mode::Delivery,
        implementations: &["reference"],
    },
    Case {
        name: "get",
        group: "round-trip",
        mode: Mode::RoundTrip,
        implementations: &["tarwyn-rust-client"],
    },
    Case {
        name: "compare_and_set",
        group: "round-trip",
        mode: Mode::RoundTrip,
        implementations: &["tarwyn-rust-client"],
    },
    Case {
        name: "delete",
        group: "round-trip",
        mode: Mode::RoundTrip,
        implementations: &["tarwyn-rust-client"],
    },
    Case {
        name: "tables",
        group: "round-trip",
        mode: Mode::RoundTrip,
        implementations: &["tarwyn-rust-client"],
    },
    Case {
        name: "ping",
        group: "round-trip",
        mode: Mode::RoundTrip,
        implementations: &["tarwyn-rust-client"],
    },
];

/// The case with this name, if the catalog declares one.
#[allow(dead_code)]
pub fn find(name: &str) -> Option<&'static Case> {
    CASES.iter().find(|case| case.name == name)
}

#[cfg(test)]
mod tests {
    use super::{CASES, Mode, find};

    #[test]
    fn every_listed_case_can_be_found_by_name() {
        assert!(!CASES.is_empty(), "the catalog must declare cases");
        for case in CASES {
            let found = find(case.name).unwrap_or_else(|| panic!("{} is unreachable", case.name));
            assert_eq!(found.name, case.name);
        }
    }

    #[test]
    fn case_names_are_unique() {
        for (index, case) in CASES.iter().enumerate() {
            let duplicate = CASES[index + 1..]
                .iter()
                .any(|other| other.name == case.name);
            assert!(!duplicate, "{} is declared twice", case.name);
        }
    }

    #[test]
    fn ntcore_declares_no_round_trip_case() {
        for case in CASES {
            if matches!(case.mode, Mode::RoundTrip) {
                assert!(
                    !case.implementations.contains(&"ntcore"),
                    "{} claims ntcore, which has no request/reply plane",
                    case.name
                );
            }
        }
    }
}
