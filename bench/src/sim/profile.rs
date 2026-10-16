//! A workload profile: every client of a robot, what it publishes and what it
//! subscribes to.

use crate::sim::values;
use anyhow::{Context as _, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};
use std::collections::HashSet;
use std::path::Path;

/// Every client of one robot and the traffic it makes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    /// Where the profile came from, such as the capture it was built from.
    #[serde(default)]
    pub source: Json,
    pub clients: Vec<ClientSpec>,
}

/// One NT4 client: the robot, a coprocessor or a dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSpec {
    pub name: String,
    pub role: Role,
    /// Left out of the baseline, and copied by the clients axis.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub spare: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes: Vec<PublishSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscribes: Vec<SubscribeSpec>,
}

/// What a client stands in for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Robot,
    Coprocessor,
    Dashboard,
}

/// The group a topic is reported under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    /// A coprocessor's binary frames.
    Camera,
    /// Header-carrying topics at 10 Hz or more.
    Struct,
    /// Header-carrying topics under 10 Hz, including ones published once.
    Slow,
    /// Topics with no room for a header. They add load, and nothing measures them.
    Scalar,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Camera => "camera",
            Class::Struct => "struct",
            Class::Slow => "slow",
            Class::Scalar => "scalar",
        }
    }
}

impl std::str::FromStr for Class {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> anyhow::Result<Self> {
        serde_json::from_value(Json::String(text.to_string()))
            .map_err(|_| anyhow!("{text} is not a topic class"))
    }
}

/// One topic a client publishes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishSpec {
    pub topic: String,
    /// The NT4 type string, such as `struct:Pose2d`.
    #[serde(rename = "type")]
    pub type_str: String,
    /// Publishes per second. Zero publishes once.
    pub rate_hz: f64,
    /// The smallest and largest value seen, as the capture counted it.
    pub bytes: [usize; 2],
    pub class: Class,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub properties: Map<String, Json>,
}

/// One NT4 subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeSpec {
    pub topics: Vec<String>,
    #[serde(default)]
    pub prefix: bool,
    /// The NT4 options, sent as they are.
    #[serde(default)]
    pub options: Map<String, Json>,
}

impl SubscribeSpec {
    fn flag(&self, key: &str) -> bool {
        self.options
            .get(key)
            .and_then(Json::as_bool)
            .unwrap_or(false)
    }

    /// Whether the subscription asks for every value. Without it, a server may
    /// send only the latest one per period.
    pub fn all(&self) -> bool {
        self.flag("all")
    }

    /// Whether the subscription carries announcements only.
    pub fn topics_only(&self) -> bool {
        self.flag("topicsonly")
    }

    /// The batching period asked for, in seconds, or zero.
    pub fn periodic(&self) -> f64 {
        self.options
            .get("periodic")
            .and_then(Json::as_f64)
            .unwrap_or(0.0)
    }
}

/// Whether a value of `type_str` that is at least `min_bytes` can carry the
/// sequence and due-time header.
pub fn has_header(type_str: &str, min_bytes: usize) -> bool {
    values::header_bytes(values::body(type_str)).is_some_and(|need| min_bytes >= need)
}

/// The class a topic falls in from its role, type, rate and smallest size.
pub fn classify(role: Role, type_str: &str, rate_hz: f64, min_bytes: usize) -> Class {
    if !has_header(type_str, min_bytes) {
        return Class::Scalar;
    }
    match (role, values::body(type_str)) {
        (Role::Coprocessor, values::Body::Binary) => Class::Camera,
        _ if rate_hz >= 10.0 => Class::Struct,
        _ => Class::Slow,
    }
}

impl Profile {
    /// Read and validate a profile.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read or parsed, or fails
    /// [`Profile::validate`].
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
        let profile: Profile =
            serde_json::from_str(&text).with_context(|| path.display().to_string())?;
        profile.validate()?;
        Ok(profile)
    }

    /// Every topic and the client publishing it.
    pub fn publishes(&self) -> impl Iterator<Item = (&ClientSpec, &PublishSpec)> {
        self.clients
            .iter()
            .flat_map(|client| client.publishes.iter().map(move |topic| (client, topic)))
    }

    /// The client named `name`.
    #[cfg(test)]
    pub fn client(&self, name: &str) -> Option<&ClientSpec> {
        self.clients.iter().find(|client| client.name == name)
    }

    /// The topic named `topic`, wherever it is published.
    pub fn topic(&self, topic: &str) -> Option<&PublishSpec> {
        self.publishes()
            .map(|(_, spec)| spec)
            .find(|spec| spec.topic == topic)
    }

    /// Check the rules the roles depend on: a robot client, unique topics,
    /// finite rates, ordered sizes, room for a header where the class needs one,
    /// and prefixes on dashboards only.
    ///
    /// # Errors
    ///
    /// Returns an error naming the client or topic that breaks a rule.
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.clients.iter().any(|client| client.role == Role::Robot) {
            return Err(anyhow!("{}: no client has the robot role", self.name));
        }
        let mut seen = HashSet::new();
        for (client, spec) in self.publishes() {
            let at = format!("{}: {}", client.name, spec.topic);
            if !seen.insert(spec.topic.as_str()) {
                return Err(anyhow!("{at} is published by more than one client"));
            }
            if !spec.rate_hz.is_finite() || spec.rate_hz < 0.0 {
                return Err(anyhow!("{at} has rate {}", spec.rate_hz));
            }
            if spec.bytes[0] > spec.bytes[1] {
                return Err(anyhow!("{at} has a size range {:?}", spec.bytes));
            }
            if spec.class != Class::Scalar && !has_header(&spec.type_str, spec.bytes[0]) {
                return Err(anyhow!(
                    "{at} is class {} but a {} of {} B has no room for a header",
                    spec.class.as_str(),
                    spec.type_str,
                    spec.bytes[0]
                ));
            }
        }
        for client in &self.clients {
            if client.role != Role::Dashboard && client.subscribes.iter().any(|sub| sub.prefix) {
                return Err(anyhow!(
                    "{}: tarwyn_client subscribes to exact names only, so only a dashboard may use a prefix",
                    client.name
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(json: &str) -> Profile {
        serde_json::from_str(json).expect("the fixture parses")
    }

    const OK: &str = r#"{"name":"t","source":{},"clients":[
      {"name":"robot","role":"robot","publishes":[
        {"topic":"/a","type":"struct:Pose2d","rate_hz":25,"bytes":[27,27],"class":"struct"}],
       "subscribes":[{"topics":["/cam"],"options":{"all":true}}]},
      {"name":"cam","role":"coprocessor","publishes":[
        {"topic":"/cam","type":"raw","rate_hz":60,"bytes":[300,300],"class":"camera"}]},
      {"name":"viewer","role":"dashboard","subscribes":[{"topics":["/"],"prefix":true,"options":{"all":true}}]}]}"#;

    #[test]
    fn a_well_formed_profile_validates() {
        profile(OK).validate().unwrap();
    }

    #[test]
    fn a_topic_published_twice_is_refused() {
        let doubled = OK.replace(r#""topic":"/cam""#, r#""topic":"/a""#);
        let error = profile(&doubled).validate().unwrap_err();
        assert!(error.to_string().contains("/a"), "{error}");
    }

    #[test]
    fn a_prefix_subscription_off_a_dashboard_is_refused() {
        let prefixed = OK.replace(
            r#"{"topics":["/cam"],"options""#,
            r#"{"topics":["/cam"],"prefix":true,"options""#,
        );
        let error = profile(&prefixed).validate().unwrap_err();
        assert!(error.to_string().contains("exact names"), "{error}");
    }

    #[test]
    fn a_negative_rate_or_inverted_size_is_refused() {
        assert!(
            profile(&OK.replace("\"rate_hz\":25", "\"rate_hz\":-1"))
                .validate()
                .is_err()
        );
        assert!(
            profile(&OK.replace("[27,27]", "[30,27]"))
                .validate()
                .is_err()
        );
    }

    #[test]
    fn a_class_that_needs_a_header_needs_room_for_one() {
        let small = OK.replace("[27,27]", "[12,12]");
        assert!(profile(&small).validate().is_err());
    }

    #[test]
    fn a_profile_without_a_robot_is_refused() {
        let headless = OK.replace(r#""role":"robot""#, r#""role":"coprocessor""#);
        assert!(profile(&headless).validate().is_err());
    }

    #[test]
    fn classes_follow_role_type_and_rate() {
        assert_eq!(
            classify(Role::Coprocessor, "photonstruct:X", 60.0, 1358),
            Class::Camera
        );
        assert_eq!(
            classify(Role::Robot, "struct:Pose2d", 25.0, 27),
            Class::Struct
        );
        assert_eq!(classify(Role::Robot, "string", 2.0, 40), Class::Slow);
        assert_eq!(classify(Role::Robot, "double", 25.0, 9), Class::Scalar);
        assert_eq!(
            classify(Role::Robot, "structschema", 0.0, 15),
            Class::Scalar
        );
    }
}
