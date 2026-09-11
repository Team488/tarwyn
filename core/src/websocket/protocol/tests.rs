use super::{NtRegistry, Outbound, data_type_from_string, encode_once, type_string};
use crate::value::Value;
use crate::websocket::message::RTT_TOPIC_ID;
use serde_json::{Map, Value as Json, json};

fn texts(routes: &[(u64, Outbound)]) -> Vec<(u64, Json)> {
    routes
        .iter()
        .filter_map(|(c, o)| match o {
            Outbound::Text(s) => {
                let frame: Json = serde_json::from_str(s).expect("valid control json");
                let mut msgs = match frame {
                    Json::Array(items) => items,
                    other => vec![other],
                };
                assert_eq!(msgs.len(), 1, "one control message per route");
                Some((*c, msgs.remove(0)))
            }
            Outbound::Value(_) => None,
        })
        .collect()
}

fn values(routes: &[(u64, Outbound)]) -> Vec<(u64, &[u8])> {
    routes
        .iter()
        .filter_map(|(c, o)| match o {
            Outbound::Value(b) => Some((*c, b.as_ref())),
            Outbound::Text(_) => None,
        })
        .collect()
}

#[test]
fn a_reconnecting_client_never_takes_a_live_clients_name() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "a");
    reg.on_connect(2, "robot", "b");
    reg.on_connect(3, "robot", "c");
    assert_eq!(reg.client_name(3), Some("robot@2"));

    reg.on_disconnect(2);
    reg.on_connect(4, "robot", "d");
    assert_eq!(
        reg.client_name(4),
        Some("robot@1"),
        "the freed name is the one to reuse, not the one still in use"
    );
    assert_eq!(reg.client_name(3), Some("robot@2"));
}

#[test]
fn releasing_a_base_name_leaves_the_suffixed_ones_taken() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "a");
    reg.on_connect(2, "robot", "b");
    assert_eq!(reg.client_name(2), Some("robot@1"));

    reg.on_disconnect(1);
    reg.on_connect(3, "robot", "c");
    reg.on_connect(4, "robot", "d");
    assert_eq!(reg.client_name(3), Some("robot"));
    assert_eq!(
        reg.client_name(4),
        Some("robot@2"),
        "robot@1 is still answering, so the next connection has to skip it"
    );
}

#[test]
fn a_client_cannot_hold_more_publishers_than_the_cap() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "one", "a");
    for pubuid in 0..super::MAX_PER_CLIENT as u32 {
        reg.handle_publish(1, &format!("t{pubuid}"), pubuid, "double", Map::new());
    }
    assert!(reg.topic_id("t0").is_some());

    let over = super::MAX_PER_CLIENT as u32;
    reg.handle_publish(1, "over", over, "double", Map::new());

    assert!(
        reg.topic_id("over").is_none(),
        "the cap has to turn a new publisher away, or one client can grow the \
             registry without limit"
    );
    assert!(
        !reg.handle_publish(1, "t0", 0, "double", Map::new())
            .is_empty(),
        "a publisher UID already held has to keep working at the cap"
    );
}

#[test]
fn a_client_cannot_hold_more_subscriptions_than_the_cap() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "one", "a");
    for subuid in 0..super::MAX_PER_CLIENT as u32 {
        reg.handle_subscribe(1, &[format!("t{subuid}")], subuid, false, false, Map::new());
    }
    reg.handle_publish(2, "capped", 1, "double", Map::new());

    let over = super::MAX_PER_CLIENT as u32;
    let routes = reg.handle_subscribe(1, &["capped".into()], over, false, false, Map::new());

    assert!(
        routes.is_empty(),
        "the cap has to turn a new subscription away"
    );
}

#[test]
fn republishing_a_publisher_uid_is_a_reannounce_not_a_second_publisher() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "one", "a");
    reg.handle_publish(1, "alpha", 7, "double", Map::new());
    reg.handle_publish(1, "alpha", 7, "double", Map::new());

    reg.handle_unpublish(1, 7);

    assert!(
        reg.topic_id("alpha").is_none(),
        "the repeat counted as a second publisher, so one unpublish never \
             reaches zero and the topic is pinned"
    );
}

#[test]
fn a_disconnect_releases_the_topics_that_client_published() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "publisher", "a");
    reg.handle_publish(1, "alpha", 7, "double", Map::new());
    assert!(reg.topic_id("alpha").is_some());

    reg.on_disconnect(1);

    assert!(
        reg.topic_id("alpha").is_none(),
        "the only publisher is gone, so nothing keeps the topic alive"
    );
}

#[test]
fn a_disconnect_leaves_a_surviving_publisher_able_to_release_the_topic() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "one", "a");
    reg.on_connect(2, "two", "b");
    reg.handle_publish(1, "alpha", 7, "double", Map::new());
    reg.handle_publish(2, "alpha", 9, "double", Map::new());

    reg.on_disconnect(1);
    assert!(
        reg.topic_id("alpha").is_some(),
        "a live publisher still holds the topic"
    );

    reg.handle_unpublish(2, 9);
    assert!(
        reg.topic_id("alpha").is_none(),
        "the disconnect has to give its count back, or the last unpublish \
             never reaches zero"
    );
}

#[test]
fn a_disconnect_keeps_a_persistent_topic() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "publisher", "a");
    let mut properties = Map::new();
    properties.insert("persistent".to_string(), Json::Bool(true));
    reg.handle_publish(1, "alpha", 7, "double", properties);

    reg.on_disconnect(1);

    assert!(
        reg.topic_id("alpha").is_some(),
        "a persistent topic outlives its publishers by definition"
    );
}

#[test]
fn type_strings_match_numeric_table() {
    for (data_type, name) in [
        (0, "boolean"),
        (1, "double"),
        (2, "int"),
        (3, "float"),
        (4, "string"),
        (5, "raw"),
        (16, "boolean[]"),
        (17, "double[]"),
        (18, "int[]"),
        (19, "float[]"),
        (20, "string[]"),
    ] {
        assert_eq!(type_string(data_type), Some(name));
        assert_eq!(data_type_from_string(name), data_type);
    }
    assert_eq!(type_string(99), None);
}

#[test]
fn unknown_type_strings_are_carried_as_binary() {
    for name in [
        "json",
        "msgpack",
        "protobuf",
        "rpc",
        "struct:Pose2d",
        "structschema",
    ] {
        let expected = if name == "json" { 4 } else { 5 };
        assert_eq!(
            data_type_from_string(name),
            expected,
            "NT4 carries any type string outside the table as binary"
        );
    }
}

#[test]
fn announce_echoes_the_publishers_own_type_string() {
    let mut reg = NtRegistry::new();
    let routes = reg.handle_publish(1, "pose", 1, "struct:Pose2d", serde_json::Map::new());
    let t = texts(&routes);
    assert_eq!(
        t[0].1["params"]["type"], "struct:Pose2d",
        "a struct topic must announce its own type string, not \"raw\""
    );
}

#[test]
fn encode_once_is_wire_exact_4tuple() {
    // Mirror of message.rs's NT4 golden: id=50, ts=0x07270E00, type=1,
    // double 5.545.
    let bytes = encode_once(&Value::Double(5.545), 0x0727_0E00, 50);
    assert_eq!(
        bytes.as_ref(),
        [
            0x94, 0x32, 0xd2, 0x07, 0x27, 0x0e, 0x00, 0x01, 0xcb, 0x40, 0x16, 0x2e, 0x14, 0x7a,
            0xe1, 0x47, 0xae,
        ]
    );
}

#[test]
fn publish_announces_with_pubuid_and_creates_topic() {
    let mut reg = NtRegistry::new();
    let routes = reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    assert_eq!(
        texts(&routes),
        vec![(
            1,
            json!({"method":"announce","params":{
                "id":0,"name":"gyro","properties":{},"type":"double","pubuid":7
            }})
        )]
    );
}

#[test]
fn duplicate_publish_reuses_same_id_and_reamounces() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    let routes = reg.handle_publish(1, "gyro", 8, "double", serde_json::Map::new());
    let t = texts(&routes);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].0, 1);
    assert_eq!(
        t[0].1["params"]["id"], 0,
        "duplicate publish must reuse id 0"
    );
    assert_eq!(t[0].1["params"]["pubuid"], 8);
}

#[test]
fn unpublish_deletes_when_last_publisher_and_unannounces() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    let routes = reg.handle_unpublish(1, 7);
    assert_eq!(
        texts(&routes),
        vec![(
            1,
            json!({"method":"unannounce","params":{"id":0,"name":"gyro"}})
        )]
    );
}

#[test]
fn a_persistent_topic_round_trips_through_a_snapshot() {
    let mut reg = NtRegistry::new();
    let mut props = serde_json::Map::new();
    props.insert("persistent".into(), json!(true));
    reg.handle_publish(1, "gyro", 7, "double", props);
    reg.handle_value(1, 7, Value::Double(4.25), 100);

    let saved = reg.persistent_snapshot();
    assert_eq!(saved.len(), 1, "a persistent topic with a value is saved");
    assert_eq!(saved[0].0, "gyro");
    assert_eq!(saved[0].1, "double");

    let mut fresh = NtRegistry::new();
    fresh.restore_persistent(saved, 200);
    let routes = fresh.handle_subscribe(
        2,
        &["gyro".to_string()],
        1,
        false,
        false,
        serde_json::Map::new(),
    );
    assert_eq!(
        values(&routes).len(),
        1,
        "a restored topic serves its value to a new subscriber"
    );
}

#[test]
fn a_topic_without_the_persistent_property_is_not_saved() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    reg.handle_value(1, 7, Value::Double(4.25), 100);
    assert!(reg.persistent_snapshot().is_empty());
}

#[test]
fn persistent_property_survives_last_publisher() {
    for key in ["persistent", "retained"] {
        let mut reg = NtRegistry::new();
        reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
        let mut update = serde_json::Map::new();
        update.insert(key.into(), json!(true));
        reg.handle_setproperties(1, "gyro", update);
        let routes = reg.handle_unpublish(1, 7);
        assert!(
            texts(&routes).is_empty(),
            "a topic marked {key} must not unannounce on last publisher"
        );
    }
}

#[test]
fn publish_time_persistent_property_survives_last_publisher() {
    let mut reg = NtRegistry::new();
    let mut props = serde_json::Map::new();
    props.insert("persistent".into(), json!(true));
    reg.handle_publish(1, "gyro", 7, "double", props);
    let routes = reg.handle_unpublish(1, 7);
    assert!(
        texts(&routes).is_empty(),
        "persistent set at publish time must also keep the topic"
    );
}

#[test]
fn retained_topic_survives_last_publisher() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    reg.set_retained("gyro", true);
    let routes = reg.handle_unpublish(1, 7);
    assert!(
        texts(&routes).is_empty(),
        "retained topic must not unannounce on last publisher"
    );
}

#[test]
fn topic_id_reused_after_delete() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "a", 1, "double", serde_json::Map::new());
    reg.handle_unpublish(1, 1);
    let routes = reg.handle_publish(1, "b", 2, "double", serde_json::Map::new());
    let t = texts(&routes);
    assert_eq!(t[0].1["params"]["id"], 0, "freed id 0 must be reused");
    assert_eq!(t[0].1["params"]["name"], "b");
}

#[test]
fn multiple_subscribers_receive_value() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
    reg.handle_subscribe(
        2,
        &["child".to_string()],
        10,
        false,
        false,
        serde_json::Map::new(),
    );
    reg.handle_subscribe(
        3,
        &["child".to_string()],
        11,
        false,
        false,
        serde_json::Map::new(),
    );
    let routes = reg.handle_value(1, 1, Value::Double(1.5), 100);
    let v = values(&routes);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].0, 2);
    assert_eq!(v[1].0, 3);
    assert_eq!(v[0].1, encode_once(&Value::Double(1.5), 100, 0).as_ref());
}

#[test]
fn data_type_mismatch_ignores_value() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
    let routes = reg.handle_value(1, 1, Value::Int32(7), 100);
    assert!(
        routes.is_empty(),
        "mismatched data_type value must be ignored"
    );
}

#[test]
fn properties_update_ack_only_to_same_client() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    reg.handle_subscribe(
        2,
        &["gyro".to_string()],
        10,
        false,
        false,
        serde_json::Map::new(),
    );
    let mut update = serde_json::Map::new();
    update.insert("unit".into(), json!("deg"));
    let routes = reg.handle_setproperties(1, "gyro", update);
    let t = texts(&routes);
    assert_eq!(t.len(), 2);
    let with_ack = t.iter().find(|(c, _)| *c == 1).expect("publisher ack");
    assert_eq!(
        with_ack.1,
        json!({"method":"properties","params":{
            "name":"gyro","update":{"unit":"deg"},"ack":true
        }})
    );
    let no_ack = t.iter().find(|(c, _)| *c == 2).expect("subscriber");
    assert_eq!(
        no_ack.1,
        json!({"method":"properties","params":{
            "name":"gyro","update":{"unit":"deg"}
        }})
    );
}

#[test]
fn subscribe_sends_retained_value() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
    reg.handle_value(1, 1, Value::Double(1.5), 100);
    let routes = reg.handle_subscribe(
        2,
        &["child".to_string()],
        10,
        false,
        false,
        serde_json::Map::new(),
    );
    let v = values(&routes);
    assert_eq!(
        v,
        vec![(2, encode_once(&Value::Double(1.5), 100, 0).as_ref())]
    );
}

#[test]
fn prefix_subscribe_receives_announce_for_new_topic_without_pubuid() {
    let mut reg = NtRegistry::new();
    reg.handle_subscribe(
        2,
        &["gyro".to_string()],
        10,
        true,
        false,
        serde_json::Map::new(),
    );
    let routes = reg.handle_publish(1, "gyro/yaw", 7, "double", serde_json::Map::new());
    let t = texts(&routes);
    assert_eq!(t.len(), 2);
    let sub = t
        .iter()
        .find(|(c, _)| *c == 2)
        .expect("prefix subscriber must be announced");
    assert!(
        sub.1["params"].get("pubuid").is_none(),
        "no pubuid on shared announce"
    );
    assert_eq!(sub.1["params"]["name"], "gyro/yaw");
    assert_eq!(sub.1["params"]["type"], "double");
}

#[test]
fn timestamp_echoes_id_minus_one_with_server_time() {
    let mut reg = NtRegistry::new();
    let routes = reg.handle_timestamp(1, Value::Double(1.5), 1234);
    let v = values(&routes);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].0, 1);
    assert_eq!(
        v[0].1,
        encode_once(&Value::Double(1.5), 1234, RTT_TOPIC_ID).as_ref()
    );
}

#[test]
fn a_topicsonly_subscriber_is_announced_but_sent_no_values() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    reg.handle_value(1, 7, Value::Double(1.5), 100);
    let routes = reg.handle_subscribe(
        2,
        &["gyro".to_string()],
        10,
        false,
        true,
        serde_json::Map::new(),
    );
    assert_eq!(
        texts(&routes).len(),
        1,
        "a topicsonly subscriber is still announced"
    );
    assert!(
        values(&routes).is_empty(),
        "a topicsonly subscriber must not receive the retained value"
    );
    let routes = reg.handle_value(1, 7, Value::Double(2.5), 200);
    assert!(
        values(&routes).is_empty(),
        "a topicsonly subscriber must not receive later values either"
    );
}

#[test]
fn an_uncached_topic_replays_nothing_to_a_late_subscriber() {
    let mut reg = NtRegistry::new();
    let mut props = serde_json::Map::new();
    props.insert("cached".into(), json!(false));
    reg.handle_publish(1, "gyro", 7, "double", props);
    reg.handle_value(1, 7, Value::Double(1.5), 100);
    let routes = reg.handle_subscribe(
        2,
        &["gyro".to_string()],
        10,
        false,
        false,
        serde_json::Map::new(),
    );
    assert!(
        values(&routes).is_empty(),
        "an uncached topic must not retain a value for late subscribers"
    );
}

#[test]
fn a_null_property_update_deletes_the_property() {
    let mut reg = NtRegistry::new();
    let mut props = serde_json::Map::new();
    props.insert("unit".into(), json!("deg"));
    reg.handle_publish(1, "gyro", 7, "double", props);
    let mut update = serde_json::Map::new();
    update.insert("unit".into(), Json::Null);
    reg.handle_setproperties(1, "gyro", update);
    let routes = reg.handle_subscribe(
        2,
        &["gyro".to_string()],
        10,
        false,
        false,
        serde_json::Map::new(),
    );
    let announce = &texts(&routes)[0].1;
    assert!(
        announce["params"]["properties"].get("unit").is_none(),
        "a null update must delete the property, not store a json null"
    );
}

#[test]
fn unsubscribe_removes_fan_out() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "child", 1, "double", serde_json::Map::new());
    reg.handle_subscribe(
        2,
        &["child".to_string()],
        10,
        false,
        false,
        serde_json::Map::new(),
    );
    reg.handle_unsubscribe(2, 10);
    let routes = reg.handle_value(1, 7, Value::Double(1.5), 100);
    assert!(
        routes.is_empty(),
        "no subscribers must receive the value after unsubscribe"
    );
}

#[test]
fn stale_pubuid_unpublish_after_topic_deleted_is_ignored() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    reg.handle_publish(2, "gyro", 9, "double", serde_json::Map::new());
    // Client 1 unannounces, deleting the topic while client 2's pubuid
    // still points at it.
    reg.handle_unannounce(1, "gyro");
    let routes = reg.handle_unpublish(2, 9);
    assert!(
        routes.is_empty(),
        "stale pubuid after topic deletion must be ignored, not panic"
    );
}

#[test]
fn prefix_subscriber_receives_values_on_new_topic_without_resubscribe() {
    let mut reg = NtRegistry::new();
    reg.handle_subscribe(
        1,
        &["/robot".to_string()],
        1,
        true,
        false,
        serde_json::Map::new(),
    );
    let routes = reg.handle_publish(2, "/robot/arm", 9, "double", serde_json::Map::new());
    let t = texts(&routes);
    assert!(
        t.iter().any(|(c, _)| *c == 1),
        "prefix subscriber must be announced for the new topic"
    );
    let routes = reg.handle_value(2, 9, Value::Double(1.5), 100);
    let v = values(&routes);
    assert!(
        v.iter().any(|(c, _)| *c == 1),
        "prefix subscriber must receive values without re-subscribing"
    );
}

#[test]
fn publisher_receives_own_value_when_subscribed() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "x", 7, "double", serde_json::Map::new());
    reg.handle_subscribe(
        1,
        &["x".to_string()],
        1,
        false,
        false,
        serde_json::Map::new(),
    );
    let routes = reg.handle_value(1, 7, Value::Double(1.5), 100);
    let v = values(&routes);
    assert!(
        v.iter().any(|(c, _)| *c == 1),
        "a subscribed publisher must receive its own value"
    );
}

#[test]
fn explicit_unannounce_removes_topic() {
    let mut reg = NtRegistry::new();
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    let t = texts(&reg.handle_unannounce(1, "gyro"));
    assert_eq!(
        t,
        vec![(
            1,
            json!({"method":"unannounce","params":{"id":0,"name":"gyro"}})
        )]
    );
}

#[test]
fn on_connect_creates_meta_topics() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "127.0.0.1:10001");
    for name in [
        "$clients",
        "$clientpub$robot",
        "$clientsub$robot",
        "$serversub",
        "$serverpub",
    ] {
        assert!(
            reg.topic_id(name).is_some(),
            "meta topic {name} must exist after connect"
        );
    }
    assert_eq!(reg.client_name(1), Some("robot"));
}

#[test]
fn dedup_client_name_appends_at_n() {
    let mut reg = NtRegistry::new();
    assert_eq!(reg.dedup_client_name("robot"), "robot");
    assert_eq!(reg.dedup_client_name("robot"), "robot@1");
    assert_eq!(reg.dedup_client_name("robot"), "robot@2");
}

#[test]
fn meta_topics_hidden_from_empty_prefix_subscribers() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "127.0.0.1:10001");
    let routes = reg.handle_subscribe(
        2,
        &["".to_string()],
        10,
        true,
        false,
        serde_json::Map::new(),
    );
    let t = texts(&routes);
    assert!(
        t.iter()
            .all(|(_, m)| !m["params"]["name"].as_str().unwrap().starts_with('$')),
        "empty-prefix subscriber must not be announced meta topics"
    );
}

#[test]
fn meta_topics_created_later_reach_an_existing_dollar_subscriber() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "dashboard", "127.0.0.1:10001");
    reg.handle_subscribe(
        1,
        &["$".to_string()],
        1,
        true,
        false,
        serde_json::Map::new(),
    );

    let routes = reg.on_connect(2, "robot", "127.0.0.1:10002");
    let announced: Vec<String> = texts(&routes)
        .into_iter()
        .filter(|(c, _)| *c == 1)
        .filter_map(|(_, m)| {
            (m["method"] == "announce").then(|| m["params"]["name"].as_str().unwrap().to_owned())
        })
        .collect();
    assert!(
        announced.contains(&"$clientpub$robot".to_string()),
        "a $ subscriber must be announced meta topics created after it subscribed, got {announced:?}"
    );
}

#[test]
fn meta_clients_reports_the_peer_address() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "dashboard", "10.4.88.2:51820");
    reg.handle_subscribe(
        1,
        &["$".to_string()],
        1,
        true,
        false,
        serde_json::Map::new(),
    );
    let routes = reg.on_connect(2, "robot", "10.4.88.7:44100");
    let payload = values(&routes)
        .into_iter()
        .find_map(|(c, bytes)| (c == 1).then(|| bytes.to_vec()))
        .expect("a $clients value must reach the subscriber");
    let text = String::from_utf8_lossy(&payload).to_string();
    assert!(
        text.contains("10.4.88.7:44100"),
        "$clients conn must carry host:port, not the client name"
    );
}

#[test]
fn meta_topics_visible_to_dollar_subscribers() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "127.0.0.1:10001");
    let routes = reg.handle_subscribe(
        2,
        &["$".to_string()],
        10,
        true,
        false,
        serde_json::Map::new(),
    );
    let t = texts(&routes);
    assert!(
        t.iter()
            .any(|(_, m)| m["params"]["name"].as_str().unwrap().starts_with('$')),
        "a $ subscriber must be announced meta topics"
    );
}

#[test]
fn publish_updates_clientpub_and_pub_meta() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "127.0.0.1:10001");
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    assert!(
        reg.topic_id("$clientpub$robot").is_some(),
        "$clientpub$robot must exist"
    );
    assert!(reg.topic_id("$pub$gyro").is_some(), "$pub$gyro must exist");
}

#[test]
fn subscribe_updates_clientsub_and_sub_meta() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "127.0.0.1:10001");
    reg.handle_publish(1, "gyro", 7, "double", serde_json::Map::new());
    reg.handle_subscribe(
        1,
        &["gyro".to_string()],
        10,
        false,
        false,
        serde_json::Map::new(),
    );
    assert!(
        reg.topic_id("$clientsub$robot").is_some(),
        "$clientsub$robot must exist"
    );
    assert!(reg.topic_id("$sub$gyro").is_some(), "$sub$gyro must exist");
}

#[test]
fn on_disconnect_removes_per_client_meta_topics() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "127.0.0.1:10001");
    reg.on_disconnect(1);
    assert!(
        reg.topic_id("$clientpub$robot").is_none(),
        "$clientpub$robot must be removed on disconnect"
    );
    assert!(
        reg.topic_id("$clientsub$robot").is_none(),
        "$clientsub$robot must be removed on disconnect"
    );
    assert!(
        reg.topic_id("$clients").is_some(),
        "$clients must survive a single disconnect"
    );
}

#[test]
fn meta_payload_is_msgpack_array_of_maps() {
    let mut reg = NtRegistry::new();
    reg.on_connect(1, "robot", "127.0.0.1:10001");
    let id = reg.topic_id("$clients").unwrap();
    let topic = reg.topics.get(&id).unwrap();
    assert_eq!(topic.type_str, "msgpack");
    assert!(topic.retained, "meta topics must be retained");
    assert!(topic.current.is_some(), "meta topics must cache a value");
    let bytes = match &topic.current.as_ref().unwrap().value {
        Value::Bytes(b) => b.clone(),
        other => panic!("meta value must be Bytes, got {other:?}"),
    };
    assert_eq!(bytes[0] & 0xf0, 0x90, "payload must be an array");
    assert_eq!(bytes[1] & 0xf0, 0x80, "element must be a map");
}
