//! `bench profile from-capture`: a capture of a running robot, and its
//! dashboard layouts, turned into a profile.

use crate::sim::profile::{ClientSpec, Profile, PublishSpec, Role, SubscribeSpec, classify};
use crate::sim::values::{Body, body};
use anyhow::anyhow;
use serde_json::{Map, Value as Json, json};
use std::collections::BTreeMap;

/// Where coprocessor topics live: `/photonvision/<camera>/...`.
const CAMERA_PREFIX: &str = "/photonvision/";
/// AdvantageScope's batching period for live values, in seconds.
const ADVANTAGESCOPE_PERIODIC: f64 = 0.02;
/// The prefixes AdvantageScope subscribes under in AdvantageKit mode.
const ADVANTAGEKIT_PREFIXES: [&str; 2] = ["/AdvantageKit/", "/Tuning/"];
/// The fields AdvantageScope always subscribes to in AdvantageKit mode, on top
/// of what its layout shows.
const ADVANTAGESCOPE_ALWAYS: [&str; 3] = [
    "/AdvantageKit/.schema",
    "/AdvantageKit/Timestamp",
    "/AdvantageKit/DriverStation/",
];

/// The dashboard layouts the dashboards' subscriptions come from.
#[derive(Debug, Default)]
pub struct Layouts {
    /// An Elastic layout, such as `robot-configuration.json`.
    pub elastic: Option<Json>,
    /// An AdvantageScope layout, such as `LoggedLayout.json`.
    pub advantagescope: Option<Json>,
}

/// What a capture alone cannot say.
#[derive(Debug)]
pub struct CaptureOptions {
    pub name: String,
    /// The rate every camera publishes at, replacing what the capture saw.
    pub camera_fps: f64,
    /// Recorded as the profile's source.
    pub source: Json,
}

impl CaptureOptions {
    /// Options for a profile called `name`, with cameras at 60 fps.
    pub fn named(name: &str) -> Self {
        CaptureOptions {
            name: name.to_string(),
            camera_fps: 60.0,
            source: json!({}),
        }
    }
}

/// The smallest value of a type, as the capture would count it, for a topic
/// that never changed while it was watched.
fn smallest(type_str: &str) -> usize {
    match body(type_str) {
        Body::Bool => 1,
        Body::Double | Body::Int | Body::Float => 9,
        Body::Binary => crate::sim::values::header_bytes(Body::Binary).unwrap_or(19),
        _ => 3,
    }
}

/// The camera a topic belongs to, if it is under [`CAMERA_PREFIX`].
fn camera_of(topic: &str) -> Option<&str> {
    let rest = topic.strip_prefix(CAMERA_PREFIX)?;
    rest.split_once('/').map(|(camera, _)| camera)
}

/// Every `(topic, period)` an Elastic layout's widgets show, at any depth.
fn elastic_topics(layout: &Json, out: &mut Vec<(String, f64)>) {
    match layout {
        Json::Object(object) => {
            if let Some(properties) = object.get("properties").and_then(Json::as_object)
                && let Some(topic) = properties.get("topic").and_then(Json::as_str)
            {
                let period = properties
                    .get("period")
                    .and_then(Json::as_f64)
                    .unwrap_or(0.1);
                out.push((topic.to_string(), period));
            }
            object.values().for_each(|value| elastic_topics(value, out));
        }
        Json::Array(items) => items.iter().for_each(|item| elastic_topics(item, out)),
        _ => {}
    }
}

/// Every `logKey` an AdvantageScope layout shows, at any depth.
fn log_keys(layout: &Json, out: &mut Vec<String>) {
    match layout {
        Json::Object(object) => {
            if let Some(key) = object.get("logKey").and_then(Json::as_str) {
                out.push(key.to_string());
            }
            object.values().for_each(|value| log_keys(value, out));
        }
        Json::Array(items) => items.iter().for_each(|item| log_keys(item, out)),
        _ => {}
    }
}

fn subscription(topics: Vec<String>, prefix: bool, options: Json) -> SubscribeSpec {
    SubscribeSpec {
        topics,
        prefix,
        options: options.as_object().cloned().unwrap_or_default(),
    }
}

fn dashboard(name: &str, spare: bool, subscribes: Vec<SubscribeSpec>) -> ClientSpec {
    ClientSpec {
        name: name.to_string(),
        role: Role::Dashboard,
        spare,
        publishes: Vec::new(),
        subscribes,
    }
}

/// Build a profile from a `capture.py` capture and the dashboards' layouts.
///
/// Meta topics are left out. Each camera under `/photonvision/` becomes its
/// own coprocessor at `camera_fps`, and the robot subscribes to its camera
/// frames. Everything else is the robot's, at the captured rate.
///
/// # Errors
///
/// Returns an error when the capture has no topic list, or the result fails
/// [`Profile::validate`].
pub fn from_capture(
    capture: &Json,
    layouts: &Layouts,
    options: &CaptureOptions,
) -> anyhow::Result<Profile> {
    let topics = capture
        .get("topics")
        .and_then(Json::as_array)
        .ok_or_else(|| anyhow!("the capture has no topics list"))?;
    let mut robot = Vec::new();
    let mut cameras: BTreeMap<String, Vec<PublishSpec>> = BTreeMap::new();
    for row in topics {
        let field = |key: &str| row.get(key).and_then(Json::as_str).unwrap_or_default();
        let topic = field("topic");
        if topic.is_empty() || topic.starts_with('$') {
            continue;
        }
        let type_str = field("type");
        let hz = row.get("hz").and_then(Json::as_f64).unwrap_or(0.0);
        let captured: Vec<usize> = row
            .get("bytes")
            .and_then(Json::as_array)
            .map(|range| {
                range
                    .iter()
                    .filter_map(Json::as_u64)
                    .map(|b| b as usize)
                    .collect()
            })
            .unwrap_or_default();
        let bytes = match captured.as_slice() {
            [low, high] if *high > 0 => [*low, *high],
            _ => [smallest(type_str); 2],
        };
        let camera = camera_of(topic);
        let (role, rate_hz) = match camera {
            Some(_) if hz > 0.0 => (Role::Coprocessor, options.camera_fps),
            Some(_) => (Role::Coprocessor, 0.0),
            None => (Role::Robot, hz),
        };
        let spec = PublishSpec {
            topic: topic.to_string(),
            type_str: type_str.to_string(),
            rate_hz,
            bytes,
            class: classify(role, type_str, rate_hz, bytes[0]),
            properties: row
                .get("properties")
                .and_then(Json::as_object)
                .cloned()
                .unwrap_or_default(),
        };
        match camera {
            Some(camera) => cameras.entry(camera.to_lowercase()).or_default().push(spec),
            None => robot.push(spec),
        }
    }

    let frames: Vec<String> = cameras
        .values()
        .flatten()
        .filter(|spec| spec.class == crate::sim::profile::Class::Camera)
        .map(|spec| spec.topic.clone())
        .collect();
    let mut clients = vec![ClientSpec {
        name: "robot".to_string(),
        role: Role::Robot,
        spare: false,
        publishes: robot,
        subscribes: frames
            .into_iter()
            .map(|topic| subscription(vec![topic], false, json!({"all": true})))
            .collect(),
    }];
    clients.extend(cameras.into_iter().map(|(camera, publishes)| ClientSpec {
        name: format!("camera-{camera}"),
        role: Role::Coprocessor,
        spare: false,
        publishes,
        subscribes: Vec::new(),
    }));

    if let Some(layout) = &layouts.elastic {
        let mut widgets = Vec::new();
        elastic_topics(layout, &mut widgets);
        let mut by_period: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        for (topic, period) in widgets {
            let topics = by_period.entry(period.to_bits()).or_default();
            if !topics.contains(&topic) {
                topics.push(topic);
            }
        }
        let subscribes = by_period
            .into_iter()
            .map(|(period, topics)| {
                subscription(topics, true, json!({"periodic": f64::from_bits(period)}))
            })
            .collect();
        clients.push(dashboard("elastic", false, subscribes));
    }
    let live = json!({"all": true, "periodic": ADVANTAGESCOPE_PERIODIC});
    let prefixes: Vec<String> = ADVANTAGEKIT_PREFIXES
        .iter()
        .map(|p| p.to_string())
        .collect();
    if let Some(layout) = &layouts.advantagescope {
        let mut keys = Vec::new();
        log_keys(layout, &mut keys);
        let mut fields: Vec<String> = ADVANTAGESCOPE_ALWAYS
            .iter()
            .map(|f| f.to_string())
            .collect();
        for key in keys {
            let field = format!("/AdvantageKit{key}");
            if !fields.contains(&field) {
                fields.push(field);
            }
        }
        let mut subscribes = vec![subscription(
            prefixes.clone(),
            true,
            json!({"topicsonly": true}),
        )];
        subscribes.extend(
            fields
                .into_iter()
                .map(|field| subscription(vec![field], true, live.clone())),
        );
        clients.push(dashboard("advantagescope", false, subscribes));
    }
    clients.push(dashboard(
        "advantagescope-logging",
        true,
        vec![subscription(prefixes, true, live)],
    ));

    let profile = Profile {
        name: options.name.clone(),
        source: options.source.clone(),
        clients,
    };
    profile.validate()?;
    Ok(profile)
}

/// The profile as JSON with one topic or subscription per line, so a diff of
/// a regenerated profile stays readable.
pub fn to_lines(profile: &Profile) -> anyhow::Result<String> {
    let mut out = String::from("{\n");
    out.push_str(&format!(
        " \"name\": {},\n",
        serde_json::to_string(&profile.name)?
    ));
    out.push_str(&format!(
        " \"source\": {},\n",
        serde_json::to_string(&profile.source)?
    ));
    out.push_str(" \"clients\": [\n");
    for (index, client) in profile.clients.iter().enumerate() {
        let mut head: Map<String, Json> = Map::new();
        head.insert("name".into(), json!(client.name));
        head.insert("role".into(), serde_json::to_value(client.role)?);
        if client.spare {
            head.insert("spare".into(), json!(true));
        }
        let head = serde_json::to_string(&head)?;
        out.push_str(&format!("  {},\n", &head[..head.len() - 1]));
        let list = |items: Vec<String>| items.join(",\n    ");
        let publishes = client
            .publishes
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?;
        let subscribes = client
            .subscribes
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?;
        out.push_str(&format!(
            "   \"publishes\": [\n    {}\n   ],\n",
            list(publishes)
        ));
        out.push_str(&format!(
            "   \"subscribes\": [\n    {}\n   ]}}",
            list(subscribes)
        ));
        out.push_str(if index + 1 < profile.clients.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    out.push_str(" ]\n}\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::profile::Class;
    use serde_json::json;

    fn capture() -> serde_json::Value {
        serde_json::from_str(include_str!("../../tests/fixtures/capture.json")).unwrap()
    }

    #[test]
    fn a_capture_becomes_a_profile_with_cameras_split_out() {
        let profile =
            from_capture(&capture(), &Layouts::default(), &CaptureOptions::named("t")).unwrap();
        assert!(profile.publishes().all(|(_, p)| !p.topic.starts_with('$')));
        let camera = profile
            .client("camera-apriltag_front_camera")
            .expect("one client per camera");
        let raw = camera
            .publishes
            .iter()
            .find(|p| p.topic.ends_with("rawBytes"))
            .unwrap();
        assert_eq!((raw.rate_hz, raw.class), (60.0, Class::Camera));
        let robot = profile.client("robot").unwrap();
        assert_eq!(
            robot.subscribes[0].topics,
            ["/photonvision/Apriltag_Front_Camera/rawBytes"]
        );
        assert!(robot.subscribes[0].all());
        let schema = robot
            .publishes
            .iter()
            .find(|p| p.topic.starts_with("/.schema"))
            .unwrap();
        assert_eq!(
            (schema.rate_hz, schema.class, schema.bytes),
            (0.0, Class::Slow, [19, 19])
        );
        let pose = robot
            .publishes
            .iter()
            .find(|p| p.topic.ends_with("RobotPose"))
            .unwrap();
        assert_eq!((pose.rate_hz, pose.class), (24.96, Class::Struct));
    }

    #[test]
    fn elastic_widgets_become_one_subscription_per_period() {
        let layouts = Layouts {
            elastic: Some(json!({"tabs": [{"grid_layout": {"containers": [
                {"properties": {"topic": "/SmartDashboard/Auto", "period": 0.06}},
                {"children": [{"properties": {"topic": "/Preferences/kP", "period": 0.06}}]},
                {"properties": {"topic": "/SmartDashboard/Field", "period": 0.1}}
            ]}}]})),
            advantagescope: None,
        };
        let profile = from_capture(&capture(), &layouts, &CaptureOptions::named("t")).unwrap();
        let elastic = profile.client("elastic").expect("an elastic dashboard");
        let mut periods: Vec<(f64, usize)> = elastic
            .subscribes
            .iter()
            .map(|sub| (sub.periodic(), sub.topics.len()))
            .collect();
        periods.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(periods, [(0.06, 2), (0.1, 1)]);
        assert!(
            elastic
                .subscribes
                .iter()
                .all(|sub| sub.prefix && !sub.all())
        );
    }

    #[test]
    fn advantagescope_fields_get_the_akit_prefix() {
        let layouts = Layouts {
            elastic: None,
            advantagescope: Some(json!({"hubs": [{"state": {"tabs": {"tabs": [
                {"controller": {"sources": [{"logKey": "/RealOutputs/PoseSubsystem/RobotPose"}]}}
            ]}}}]})),
        };
        let profile = from_capture(&capture(), &layouts, &CaptureOptions::named("t")).unwrap();
        let scope = profile.client("advantagescope").unwrap();
        assert!(scope.subscribes[0].topics_only());
        let fields: Vec<&str> = scope.subscribes[1..]
            .iter()
            .map(|sub| sub.topics[0].as_str())
            .collect();
        assert!(fields.contains(&"/AdvantageKit/RealOutputs/PoseSubsystem/RobotPose"));
        assert!(fields.contains(&"/AdvantageKit/.schema"));
        assert!(
            scope.subscribes[1..]
                .iter()
                .all(|sub| sub.all() && sub.periodic() == 0.02)
        );
        let logging = profile.client("advantagescope-logging").unwrap();
        assert!(logging.spare);
        assert_eq!(logging.subscribes[0].topics, ["/AdvantageKit/", "/Tuning/"]);
    }

    #[test]
    fn the_line_format_reads_back_as_the_same_profile() {
        let profile =
            from_capture(&capture(), &Layouts::default(), &CaptureOptions::named("t")).unwrap();
        let back: Profile = serde_json::from_str(&to_lines(&profile).unwrap()).unwrap();
        back.validate().unwrap();
        assert_eq!(back.publishes().count(), profile.publishes().count());
        assert_eq!(back.clients.len(), profile.clients.len());
    }
}
