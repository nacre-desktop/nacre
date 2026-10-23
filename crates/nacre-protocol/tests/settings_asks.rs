use nacre_protocol::{
    artifacts, framing,
    settings_asks::{self, ShortcutsState},
    Message, Mirror, Update,
};

#[test]
fn transcript_replays_and_every_state_validates() {
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/settings-asks.jsonl")
        .unwrap();
    let mut mirror = Mirror::default();
    let (mut states, mut calls) = (0, 0);
    for line in fixture.lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        assert!(
            !matches!(mirror.receive(&message), Update::Resync(_)),
            "{line}"
        );
        match &message {
            Message::Snap { topic, .. } | Message::Patch { topic, .. } => {
                assert_eq!(topic, settings_asks::SHORTCUTS_TOPIC);
                serde_json::from_value::<ShortcutsState>(mirror.topics[topic].1.clone())
                    .unwrap()
                    .validate()
                    .unwrap();
                states += 1;
            }
            Message::Call { m, p, .. } => {
                assert!(settings_asks::methods().contains_key(m), "{m}");
                assert!(p.is_object());
                calls += 1;
            }
            _ => {}
        }
    }
    assert_eq!((states, calls), (2, 5));
    let shortcuts = &mirror.topics[settings_asks::SHORTCUTS_TOPIC].1;
    assert_eq!(shortcuts["items"]["order"], serde_json::json!(["k0"]));
}

#[test]
fn schemas_and_types_are_published() {
    let generated = artifacts::generate();
    for path in [
        "protocol/schema/shortcuts-state.json",
        "protocol/schema/system-status.json",
        "protocol/schema/tools-launch.json",
        "protocol/schema/audio-set-default.json",
        "protocol/luau/settings-asks.d.luau",
        "protocol/fixtures/settings-asks.jsonl",
    ] {
        assert!(generated.contains_key(path), "missing {path}");
    }
    assert!(generated["protocol/luau/settings-asks.d.luau"].contains("export type SystemStatus ="));
}
