//! Every benchmark case, declared once. `run/plan.rs` says how each
//! one is launched.

/// One benchmark case.
#[derive(Debug)]
pub struct Case {
    /// The case name, as passed to `bench run --case`.
    pub name: &'static str,
    /// The name the report renders instead of `name`, so `publish` and
    /// `publish_client` can share one label in different sections.
    pub display: &'static str,
    /// Which table the case appears in.
    pub group: &'static str,
    /// Which implementations can run it.
    pub implementations: &'static [&'static str],
}

/// Every case this harness knows how to run: one-way delivery from a
/// publisher to one or more subscribers across processes.
pub const CASES: &[Case] = &[
    Case {
        name: "publish",
        display: "publish",
        group: "servers",
        implementations: &["tarwyn", "ntcore"],
    },
    Case {
        name: "publish_client",
        display: "publish",
        group: "clients",
        implementations: &["tarwyn", "ntcore"],
    },
    Case {
        name: "subscribe_client",
        display: "subscribe",
        group: "clients",
        implementations: &["tarwyn", "ntcore"],
    },
    Case {
        name: "fanout",
        display: "subscribe, 3 subscribers",
        group: "clients",
        implementations: &["tarwyn", "ntcore"],
    },
];

/// How many subscribers the `fanout` case attaches to one topic.
pub const FANOUT: usize = 3;

/// The case with this name, if the catalog declares one.
pub fn find(name: &str) -> Option<&'static Case> {
    CASES.iter().find(|case| case.name == name)
}

#[cfg(test)]
mod tests {
    use super::{CASES, find};

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
    fn every_case_is_contested() {
        for case in CASES {
            assert!(
                case.implementations.len() > 1,
                "{} is measured for one implementation, so it compares nothing",
                case.name
            );
        }
    }
}
