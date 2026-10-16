//! Which profile topics a client's subscriptions reach, and how.

pub use crate::sim::measure::Mode;
use crate::sim::profile::{Class, Profile, SubscribeSpec};

/// How a client's subscriptions reach one topic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matched {
    pub mode: Mode,
    /// The longest batching period among the subscriptions that carry its
    /// values, in seconds.
    pub periodic: f64,
}

/// Whether `sub` names `topic`, as a prefix or exactly.
fn names(sub: &SubscribeSpec, topic: &str) -> bool {
    sub.topics.iter().any(|pattern| {
        if sub.prefix {
            topic.starts_with(pattern.as_str())
        } else {
            topic == pattern
        }
    })
}

/// How `subs` deliver `topic`'s values: every value when any matching
/// subscription asks for all of them, otherwise the latest per period.
/// `None` when no subscription carries its values.
pub fn matching(subs: &[SubscribeSpec], topic: &str) -> Option<Matched> {
    let carrying: Vec<&SubscribeSpec> = subs
        .iter()
        .filter(|sub| !sub.topics_only() && names(sub, topic))
        .collect();
    if carrying.is_empty() {
        return None;
    }
    let mode = if carrying.iter().any(|sub| sub.all()) {
        Mode::All
    } else {
        Mode::Coalesce
    };
    let periodic = carrying
        .iter()
        .map(|sub| sub.periodic())
        .fold(0.0, f64::max);
    Some(Matched { mode, periodic })
}

/// How many values of `class` a receiver with `subs` should see in `seconds`:
/// the header topics it asks for in full, at their rates times `multiplier`.
pub fn expected(
    profile: &Profile,
    subs: &[SubscribeSpec],
    class: Class,
    multiplier: u32,
    seconds: f64,
) -> f64 {
    if class == Class::Scalar {
        return 0.0;
    }
    profile
        .publishes()
        .map(|(_, spec)| spec)
        .filter(|spec| spec.class == class && spec.rate_hz > 0.0)
        .filter(|spec| matching(subs, &spec.topic).is_some_and(|m| m.mode == Mode::All))
        .map(|spec| spec.rate_hz * f64::from(multiplier) * seconds)
        .sum()
}

/// The p99 latency a receiver with `subs` may see for `class` before the class
/// counts as broken: one period of the class's fastest topic it receives, plus
/// the batching period it asked the server for.
pub fn allowance_us(
    profile: &Profile,
    subs: &[SubscribeSpec],
    class: Class,
    multiplier: u32,
) -> f64 {
    let mut fastest = 0.0_f64;
    let mut periodic = 0.0_f64;
    for (_, spec) in profile.publishes() {
        if spec.class != class {
            continue;
        }
        if let Some(matched) = matching(subs, &spec.topic) {
            fastest = fastest.max(spec.rate_hz);
            periodic = periodic.max(matched.periodic);
        }
    }
    let period_us = if fastest > 0.0 {
        1e6 / (fastest * f64::from(multiplier))
    } else {
        1e6
    };
    period_us + periodic * 1e6
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tiny() -> Profile {
        serde_json::from_str(include_str!("../../tests/fixtures/tiny-profile.json")).unwrap()
    }

    fn sub(topics: &[&str], prefix: bool, options: serde_json::Value) -> SubscribeSpec {
        SubscribeSpec {
            topics: topics.iter().map(|t| t.to_string()).collect(),
            prefix,
            options: options.as_object().cloned().unwrap(),
        }
    }

    #[test]
    fn every_value_wins_over_the_latest_only() {
        let subs = [
            sub(&["/robot/"], true, json!({"periodic": 0.1})),
            sub(
                &["/robot/pose"],
                false,
                json!({"all": true, "periodic": 0.02}),
            ),
        ];
        let matched = matching(&subs, "/robot/pose").unwrap();
        assert_eq!(matched.mode, Mode::All);
        assert_eq!(matched.periodic, 0.1);
        assert_eq!(matching(&subs, "/robot/mode").unwrap().mode, Mode::Coalesce);
    }

    #[test]
    fn announcements_only_and_strangers_carry_no_values() {
        let subs = [
            sub(&["/robot/"], true, json!({"topicsonly": true})),
            sub(&["/robot/pose"], false, json!({"all": true})),
        ];
        assert!(matching(&subs, "/robot/mode").is_none());
        assert!(
            matching(&subs, "/robot/pose/x").is_none(),
            "an exact name is not a prefix"
        );
        assert!(matching(&subs, "/elsewhere").is_none());
    }

    #[test]
    fn expected_counts_the_header_topics_asked_for_in_full() {
        let p = tiny();
        let subs = &p.client("viewer").unwrap().subscribes;
        assert_eq!(
            expected(&p, subs, Class::Struct, 2, 10.0),
            25.0 * 2.0 * 10.0
        );
        assert_eq!(expected(&p, subs, Class::Slow, 1, 10.0), 2.0 * 10.0);
        assert_eq!(expected(&p, subs, Class::Scalar, 1, 10.0), 0.0);
        let glance = &p.client("glance").unwrap().subscribes;
        assert_eq!(
            expected(&p, glance, Class::Slow, 1, 10.0),
            0.0,
            "coalescing counts nothing"
        );
    }

    #[test]
    fn the_receivers_periodic_extends_the_allowance() {
        let p = tiny();
        let subs = [sub(&["/"], true, json!({"all": true, "periodic": 0.02}))];
        assert_eq!(
            allowance_us(&p, &subs, Class::Struct, 2),
            1e6 / 50.0 + 20_000.0
        );
        assert_eq!(
            allowance_us(&p, &subs, Class::Camera, 1),
            1e6 / 60.0 + 20_000.0
        );
    }
}
