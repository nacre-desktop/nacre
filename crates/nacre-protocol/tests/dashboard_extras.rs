use nacre_protocol::{
    artifacts,
    dashboard_extras::{
        BrainRecentState, ClipboardState, DashboardExtrasCall, DashboardExtrasSnapshot,
        DashboardFocus, ScreenshotsState,
    },
    framing, Message, Mirror, Update,
};
use serde_json::json;

const TOPICS: [(&str, &str); 5] = [
    ("dashboard-focus", "dashboard.focus"),
    ("dashboard-clipboard", "clipboard"),
    ("dashboard-screenshots", "screenshots"),
    ("dashboard-ai-usage", "ai.usage"),
    ("dashboard-brain-recent", "brain.recent"),
];

#[test]
fn transcripts_replay_and_every_published_state_validates() {
    let generated = artifacts::generate();
    for (name, topic) in TOPICS {
        let mut mirror = Mirror::default();
        for line in generated[&format!("protocol/fixtures/{name}.jsonl")].lines() {
            let message = framing::decode(line.as_bytes()).unwrap();
            assert!(
                !matches!(mirror.receive(&message), Update::Resync(_)),
                "{name}: {line}"
            );
            match &message {
                Message::Snap { topic: t, .. } | Message::Patch { topic: t, .. } if t == topic => {
                    let state = json!({"topic":topic,"data":mirror.topics[topic].1});
                    let typed: DashboardExtrasSnapshot = serde_json::from_value(state).unwrap();
                    typed
                        .validate()
                        .unwrap_or_else(|_| panic!("{name}: {line}"));
                }
                _ => {}
            }
        }
    }
    let settings = &generated["protocol/fixtures/settings-dashboard-extras.jsonl"];
    assert_eq!(settings.lines().count(), 7);
}

#[test]
fn focus_state_shapes_are_exclusive() {
    let base = |v: serde_json::Value| -> DashboardFocus {
        let mut state = json!({"health":{"available":true,"stale":false},"owns_dnd":false,"previous_dnd":false});
        state
            .as_object_mut()
            .unwrap()
            .extend(v.as_object().unwrap().clone());
        serde_json::from_value(state).unwrap()
    };
    assert!(base(json!({"status":"idle","total_s":0}))
        .validate()
        .is_ok());
    assert!(
        base(json!({"status":"running","total_s":60,"deadline_ms":1}))
            .validate()
            .is_ok()
    );
    assert!(
        base(json!({"status":"paused","total_s":60,"remaining_s":5}))
            .validate()
            .is_ok()
    );
    assert!(base(json!({"status":"done","total_s":60}))
        .validate()
        .is_ok());
    for invalid in [
        json!({"status":"running","total_s":60}),
        json!({"status":"running","total_s":60,"deadline_ms":1,"remaining_s":5}),
        json!({"status":"paused","total_s":60,"deadline_ms":1}),
        json!({"status":"idle","total_s":0,"deadline_ms":1}),
        json!({"status":"running","total_s":0,"deadline_ms":1}),
        json!({"status":"running","total_s":10801,"deadline_ms":1}),
        json!({"status":"idle","total_s":0,"owns_dnd":true}),
    ] {
        assert!(base(invalid.clone()).validate().is_err(), "{invalid}");
    }
}

#[test]
fn private_rows_are_bounded() {
    let health = json!({"available":true,"stale":false});
    let entries = |preview: String| -> ClipboardState {
        serde_json::from_value(json!({"health":health,"entries":{"items":{"c1":{"kind":"text","preview":preview,"created_ms":1}},"order":["c1"]}})).unwrap()
    };
    assert!(entries("x".repeat(200)).validate().is_ok());
    assert!(entries("x".repeat(201)).validate().is_err());
    assert!(entries("line\nbreak".into()).validate().is_err());
    let shots = |path: &str| -> ScreenshotsState {
        serde_json::from_value(json!({"health":health,"items":{"items":{"s1":{"path":path,"created_ms":1}},"order":["s1"]}})).unwrap()
    };
    assert!(shots("/home/u/Pictures/Screenshots/a.png")
        .validate()
        .is_ok());
    for path in ["relative.png", "/home/u/../etc/passwd", "/a\0b"] {
        assert!(shots(path).validate().is_err(), "{path}");
    }
    let brain = |count: usize| -> BrainRecentState {
        let items: serde_json::Map<_, _> = (0..count)
            .map(|i| (format!("n{i}"), json!({"title":"t","path":"/v/a.md"})))
            .collect();
        let order: Vec<_> = (0..count).map(|i| format!("n{i}")).collect();
        serde_json::from_value(json!({"health":health,"notes":{"items":items,"order":order},"tasks":{"items":{},"order":[]}})).unwrap()
    };
    assert!(brain(5).validate().is_ok());
    assert!(brain(6).validate().is_err());
}

#[test]
fn calls_reject_bad_parameters_and_unknown_fields() {
    let call = |m: &str, p: serde_json::Value| {
        serde_json::from_value::<DashboardExtrasCall>(json!({"m":m,"p":p}))
    };
    for (m, p) in [
        ("focus.start", json!({"minutes":1})),
        ("focus.start", json!({"minutes":180})),
        ("focus.pause", json!({})),
        ("focus.resume", json!({})),
        ("focus.stop", json!({})),
        ("clipboard.copy", json!({"id":"c1"})),
        ("clipboard.clear", json!({})),
        ("clipboard.clear_history", json!({})),
        ("screenshots.open", json!({"id":"s1"})),
        ("ai.usage.refresh", json!({})),
    ] {
        call(m, p).unwrap().validate().unwrap();
    }
    for (m, p) in [
        ("focus.start", json!({"minutes":0})),
        ("focus.start", json!({"minutes":181})),
        ("focus.start", json!({"minutes":-1})),
        ("focus.start", json!({"minutes":5,"extra":1})),
        ("clipboard.copy", json!({"id":"../x"})),
        ("clipboard.copy", json!({})),
        ("screenshots.open", json!({"id":""})),
        ("clipboard.copy", json!({"id":"c1","path":"/etc/passwd"})),
    ] {
        assert!(
            call(m, p.clone())
                .ok()
                .is_none_or(|c| c.validate().is_err()),
            "{m} {p}"
        );
    }
}
