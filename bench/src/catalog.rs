//! Every benchmark case, declared once.
//!
//! A case names an operation, the table it belongs in, and which
//! implementations can run it. Adding a case is an edit here and a line in
//! `run::plan`, which says which server answers it and which probe touches it.

/// One benchmark case.
#[derive(Debug)]
pub struct Case {
    /// The case name, as passed to `bench run --case`.
    pub name: &'static str,
    /// The name the report renders instead of `name`.
    ///
    /// `publish` and `publish_client` must stay distinct catalog names (case
    /// names are unique) while rendering identically, since the section they
    /// sit in already carries the distinction the name would otherwise repeat.
    pub display: &'static str,
    /// Which table the case appears in.
    pub group: &'static str,
    /// Which implementations can run it.
    pub implementations: &'static [&'static str],
}

/// Every case this harness knows how to run.
///
/// Both are one-way delivery, publisher-stamped to subscriber-received across
/// two processes. There is nothing else here on purpose: a case only earns a
/// place if more than one implementation can run it, or it says nothing about
/// how this project compares with the alternatives.
///
/// `tarwyn-busy` is this repo's server with `--busy-poll` covering the publish
/// interval: the same binary, measured with its readers spinning instead of
/// sleeping between messages, since that is the one setting that moves the
/// number by more than the noise.
pub const CASES: &[Case] = &[
    Case {
        name: "publish",
        display: "publish",
        group: "servers",
        implementations: &["tarwyn", "tarwyn-busy", "ntcore"],
    },
    Case {
        name: "publish_client",
        display: "publish",
        group: "clients",
        implementations: &["tarwyn", "tarwyn-busy", "ntcore"],
    },
];

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
