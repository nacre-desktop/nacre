use nacre_protocol::{
    artifacts,
    core_services::{CoreCall, CoreSnapshot},
    framing, notifications, patch, Message,
};
use serde_json::{json, Value};

fn baseline_state() -> nacre_protocol::core_services::NotificationsState {
    let exports = artifacts::generate();
    let frame = exports["protocol/fixtures/notifications-lifecycle.jsonl"]
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|frame| frame["t"] == "snap")
        .unwrap();
    serde_json::from_value(frame["data"].clone()).unwrap()
}

#[test]
fn notification_calls_are_typed_but_conditionally_advertised() {
    let descriptors = notifications::method_descriptors();
    assert_eq!(descriptors.len(), 6);
    for (method, descriptor) in descriptors {
        assert_eq!(descriptor.v, 1);
        assert_eq!(descriptor.timeout, 3_000);
        assert_eq!(descriptor.class, nacre_protocol::Capability::Desktop);
        assert!(descriptor.params["properties"].is_object(), "{method}");
        assert!(!nacre_protocol::core_methods().contains_key(&method));
    }
    for (method, params) in [
        ("notifications.set_dnd", json!({"enabled":true})),
        (
            "notifications.dismiss",
            json!({"id":"n1","revision":1,"forget":true}),
        ),
        ("notifications.clear", json!({"scope":"history"})),
        (
            "notifications.invoke_action",
            json!({"id":"n1","revision":1,"action":"a1"}),
        ),
        (
            "notifications.set_hover",
            json!({"id":"n1","revision":1,"hovered":true}),
        ),
        ("notifications.set_inhibited", json!({"inhibited":true})),
    ] {
        let call: CoreCall = serde_json::from_value(json!({"m":method,"p":params})).unwrap();
        assert!(call.valid(), "{method}");
    }
    assert!(serde_json::from_value::<CoreCall>(
        json!({"m":"notifications.set_dnd","p":{"enabled":true,"path":"/tmp/x"}})
    )
    .is_err());
    assert!(serde_json::from_value::<notifications::NotificationEvent>(json!({"name":"closed","data":{"id":"n1","reason":"dismissed","state_seq":1,"summary":"secret"}})).is_err());
    let unsafe_revision = serde_json::from_value::<notifications::NotificationEvent>(
        json!({"name":"updated","data":{"id":"n1","revision":9007199254740992_u64,"state_seq":1}}),
    )
    .unwrap();
    assert!(!unsafe_revision.valid());
    assert!(serde_json::from_value::<CoreCall>(
        json!({"m":"notifications.dismiss","p":{"id":"n1","revision":1,"forget":null}})
    )
    .is_err());
}

#[test]
fn producer_goldens_are_generated_and_have_six_scenarios() {
    let exports = artifacts::generate();
    let goldens = exports
        .keys()
        .filter(|path| path.starts_with("protocol/fixtures/notifications-"))
        .count();
    assert_eq!(goldens, 6);
    assert!(exports.contains_key("protocol/schema/notification-event.json"));
    for (path, text) in exports
        .iter()
        .filter(|(path, _)| path.starts_with("protocol/fixtures/notifications-"))
    {
        let mut state = Value::Null;
        let mut state_seq = None;
        for line in text.lines() {
            let frame: Value = serde_json::from_str(line).unwrap();
            match framing::decode(line.as_bytes())
                .unwrap_or_else(|error| panic!("{path}: {line}: {error}"))
            {
                Message::Snap { topic, seq, data } if topic == "notifications" => {
                    let snapshot: CoreSnapshot =
                        serde_json::from_value(json!({"topic":topic,"data":data})).unwrap();
                    assert!(snapshot.valid(), "{path}: {line}");
                    if let CoreSnapshot::Notifications(value) = snapshot {
                        assert!(value.producer_valid(), "{path}: {line}");
                    }
                    state = frame["data"].clone();
                    state_seq = Some(seq);
                }
                Message::Patch {
                    topic,
                    base,
                    seq,
                    data,
                } if topic == "notifications" => {
                    if path.ends_with("notifications-loss-restart.jsonl") && state_seq != Some(base)
                    {
                        continue;
                    }
                    assert_eq!(state_seq, Some(base), "{path}: {line}");
                    let previous = state.clone();
                    patch::apply(&mut state, &data);
                    for (id, notice) in state["active"]["items"].as_object().unwrap() {
                        let old = &previous["active"]["items"][id];
                        if old.is_object() && old["revision"] != notice["revision"] {
                            for action in old["actions"]["items"].as_object().unwrap().keys() {
                                assert!(
                                    notice["actions"]["items"].get(action).is_none(),
                                    "{path}: replacement reused action {action}"
                                );
                            }
                        }
                    }
                    let snapshot: CoreSnapshot =
                        serde_json::from_value(json!({"topic":topic,"data":state})).unwrap();
                    assert!(snapshot.valid(), "{path}: {line}");
                    if let CoreSnapshot::Notifications(value) = snapshot {
                        assert!(value.producer_valid(), "{path}: {line}");
                    }
                    state_seq = Some(seq);
                }
                Message::Ev {
                    topic, name, data, ..
                } if topic == "notifications" => {
                    let event: notifications::NotificationEvent =
                        serde_json::from_value(json!({"name":name,"data":data}))
                            .unwrap_or_else(|error| panic!("{path}: {error}"));
                    assert!(event.valid(), "{path}: {line}");
                    let event_state_seq = frame["data"]["state_seq"].as_u64().unwrap();
                    assert!(
                        state_seq.is_some_and(|current| event_state_seq <= current),
                        "{path}: {line}"
                    );
                }
                Message::Call { .. } if path.ends_with("notifications-errors.jsonl") => {
                    if let Ok(call) = serde_json::from_value::<CoreCall>(frame.clone()) {
                        assert!(!call.valid() || frame["id"] != 7, "{path}: {line}");
                    }
                }
                Message::Call { .. } => {
                    let call: CoreCall = serde_json::from_value(frame.clone()).unwrap();
                    assert!(call.valid(), "{path}: {line}");
                }
                Message::Welcome { .. }
                | Message::Lost { .. }
                | Message::Resync { .. }
                | Message::Sub { .. }
                | Message::Register { .. }
                | Message::Ret { .. } => {}
                message => panic!("unexpected notifications frame in {path}: {message:?}"),
            }
        }
    }
}

#[test]
fn loss_and_restart_retain_last_good_then_restore_only_inert_history() {
    use nacre_protocol::{Mirror, Update};

    let exports = artifacts::generate();
    let trace = &exports["protocol/fixtures/notifications-loss-restart.jsonl"];
    let mut mirror = Mirror::default();
    let mut saw_coalesced = false;
    let mut saw_gap = false;
    for line in trace.lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        let update = mirror.receive(&message);
        match message {
            Message::Patch { seq: 3, .. } => {
                assert_eq!(update, Update::Changed("notifications".into()));
                assert_eq!(mirror.topics["notifications"].0, 3);
                assert_eq!(
                    mirror.topics["notifications"].1["active"]["items"]["notice-b1-g1-n1"]
                        ["summary"],
                    json!("Coalesced update")
                );
                saw_coalesced = true;
            }
            Message::Patch { seq: 4, .. } => {
                assert_eq!(update, Update::Resync("notifications".into()));
                assert_eq!(mirror.topics["notifications"].0, 3);
                assert!(mirror.topics["notifications"].1.get("obsolete").is_none());
                saw_gap = true;
            }
            Message::Welcome { boot, .. } if boot == "b2" => {
                assert_eq!(update, Update::Boot);
                assert_eq!(mirror.topics["notifications"].0, 3);
            }
            Message::Snap { seq: 0, data, .. }
                if data["history"]["items"]
                    .get("notice-b2-g2-restored1")
                    .is_some() =>
            {
                assert_eq!(update, Update::Changed("notifications".into()));
                assert_eq!(mirror.topics["notifications"].0, 0);
                assert!(data["active"]["items"].get("notice-b1-g1-n1").is_none());
                assert!(data["history"]["items"].get("notice-b1-g1-n1").is_none());
                let restored = &data["history"]["items"]["notice-b2-g2-restored1"];
                assert_eq!(restored["actions"], json!({"items":{},"order":[]}));
                assert_eq!(restored["paused"], false);
                assert!(restored.get("remaining_ms").is_none());
                assert!(restored.get("expires_ms").is_none());
            }
            _ => {}
        }
    }
    assert!(saw_coalesced && saw_gap);
}

#[test]
fn producer_bounds_are_strict_without_rejecting_legacy_state() {
    use nacre_protocol::core_services::{CoreSnapshot, Entities};
    use std::collections::BTreeMap;

    let snapshot_valid = |state: &nacre_protocol::core_services::NotificationsState| {
        CoreSnapshot::Notifications(state.clone()).valid()
    };
    let mut state = baseline_state();
    assert!(state.producer_valid());

    let mut pre_default_action_wire = serde_json::to_value(&state).unwrap();
    let notice = pre_default_action_wire["active"]["items"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    notice.as_object_mut().unwrap().remove("default_action");
    let decoded: nacre_protocol::core_services::NotificationsState =
        serde_json::from_value(pre_default_action_wire).unwrap();
    assert_eq!(
        decoded.active.items.values().next().unwrap().default_action,
        None
    );

    let mut legacy = state.clone();
    legacy.active.items.values_mut().next().unwrap().revision = None;
    legacy.active.items.values_mut().next().unwrap().retained = None;
    legacy.active.items.values_mut().next().unwrap().paused = None;
    assert!(snapshot_valid(&legacy));
    assert!(!legacy.producer_valid());

    let notice = state.active.items.values_mut().next().unwrap();
    notice.summary = "é".repeat(4097);
    assert!(snapshot_valid(&state));
    assert!(!state.producer_valid());

    let mut state = baseline_state();
    let notice = state.active.items.values_mut().next().unwrap();
    notice.urgency = "urgent".into();
    assert!(snapshot_valid(&state));
    assert!(!state.producer_valid());

    let mut state = baseline_state();
    let id = state.active.order[0].clone();
    let mut closed = state.active.items[&id].clone();
    closed.actions = Entities {
        items: BTreeMap::new(),
        order: Vec::new(),
    };
    state.history.items.insert(id.clone(), closed);
    state.history.order.push(id);
    state.history_count = Some(2);
    assert!(snapshot_valid(&state));
    assert!(!state.producer_valid());

    let mut state = baseline_state();
    let notice = state.active.items.values_mut().next().unwrap();
    notice.paused = Some(true);
    assert!(notice.expires_ms.is_some());
    assert!(!state.producer_valid());

    let mut state = baseline_state();
    state.history_storage.as_mut().unwrap().ready = false;
    state.history_storage.as_mut().unwrap().writable = false;
    state.history_count = None;
    state.history_more = None;
    assert!(state.producer_valid());
}

#[test]
fn producer_window_and_escaped_wire_budgets_are_enforced() {
    use nacre_protocol::core_services::{Entities, EntityId, NotificationAction};
    use std::collections::BTreeMap;

    let mut state = baseline_state();
    let original = state.active.items.values().next().unwrap().clone();
    state.active.items.clear();
    state.active.order.clear();
    state.popup_order = Some(Vec::new());
    for index in 0..129 {
        let id = EntityId(format!("n{index:03}"));
        state.active.items.insert(id.clone(), original.clone());
        state.active.order.push(id);
    }
    state.history_count = Some(129);
    assert!(nacre_protocol::core_services::CoreSnapshot::Notifications(state.clone()).valid());
    assert!(!state.producer_valid());

    let mut state = baseline_state();
    let mut closed = state.active.items.values().next().unwrap().clone();
    closed.actions = Entities {
        items: BTreeMap::new(),
        order: Vec::new(),
    };
    closed.remaining_ms = None;
    closed.expires_ms = None;
    for index in 0..65 {
        let id = EntityId(format!("h{index:03}"));
        state.history.items.insert(id.clone(), closed.clone());
        state.history.order.push(id);
    }
    state.history_count = Some(66);
    state.history_more = Some(false);
    assert!(nacre_protocol::core_services::CoreSnapshot::Notifications(state.clone()).valid());
    assert!(!state.producer_valid());

    let mut state = baseline_state();
    let original = state.active.items.values().next().unwrap().clone();
    state.active.items.clear();
    state.active.order.clear();
    state.popup_order = Some(Vec::new());
    let mut oversized_by_escaping = original;
    oversized_by_escaping.app_name = "\\".repeat(8192);
    oversized_by_escaping.summary = "\\".repeat(8192);
    oversized_by_escaping.body = "\\".repeat(8192);
    oversized_by_escaping.icon = Some("\\".repeat(8192));
    oversized_by_escaping.actions.items.clear();
    oversized_by_escaping.actions.order.clear();
    for index in 0..16 {
        let id = EntityId(format!("a{index:02}"));
        oversized_by_escaping.actions.items.insert(
            id.clone(),
            NotificationAction {
                label: "\\".repeat(1024),
            },
        );
        oversized_by_escaping.actions.order.push(id);
    }
    for index in 0..6 {
        let id = EntityId(format!("e{index:02}"));
        let mut item = oversized_by_escaping.clone();
        item.created_ms += index;
        state.active.items.insert(id.clone(), item);
        state.active.order.push(id);
    }
    state.history_count = Some(6);
    let encoded_active = serde_json::to_vec(&state.active).unwrap().len();
    assert!(encoded_active > 512 * 1024);
    assert!(nacre_protocol::core_services::CoreSnapshot::Notifications(state.clone()).valid());
    assert!(!state.producer_valid());

    let mut state = baseline_state();
    let mut closed = oversized_by_escaping;
    closed.remaining_ms = None;
    closed.expires_ms = None;
    state.history.items.clear();
    state.history.order.clear();
    for index in 0..6 {
        let id = EntityId(format!("h{index:02}"));
        let mut item = closed.clone();
        item.created_ms += index;
        state.history.items.insert(id.clone(), item);
        state.history.order.push(id);
    }
    state.history_count = Some(7);
    state.history_more = Some(false);
    let encoded_history = serde_json::to_vec(&state.history).unwrap().len();
    assert!(encoded_history > 256 * 1024);
    assert!(nacre_protocol::core_services::CoreSnapshot::Notifications(state.clone()).valid());
    assert!(!state.producer_valid());

    let mut actions = BTreeMap::new();
    for index in 0..17 {
        actions.insert(
            EntityId(format!("a{index:02}")),
            NotificationAction { label: "a".into() },
        );
    }
    let mut state = baseline_state();
    let notice = state.active.items.values_mut().next().unwrap();
    notice.actions = Entities {
        order: actions.keys().cloned().collect(),
        items: actions,
    };
    assert!(nacre_protocol::core_services::CoreSnapshot::Notifications(state.clone()).valid());
    assert!(!state.producer_valid());
}
