use nacre_protocol::{
    artifacts,
    control_center::{self, ControlsState},
    framing, Message, Mirror, Update,
};

#[test]
fn transcript_replays_and_every_state_validates() {
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/control-center.jsonl")
        .unwrap();
    let mut mirror = Mirror::default();
    let (mut states, mut calls) = (0, 0);
    for line in fixture.lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        let update = mirror.receive(&message);
        assert!(!matches!(update, Update::Resync(_)), "{line}");
        match &message {
            Message::Snap { topic, .. } | Message::Patch { topic, .. } => {
                let state: ControlsState =
                    serde_json::from_value(mirror.topics[topic].1.clone()).unwrap();
                state.validate().unwrap();
                states += 1;
            }
            Message::Call { m, p, .. } => {
                // Every golden call is a method of this unit and fits its published schema shape.
                assert!(control_center::methods().contains_key(m), "{m}");
                assert!(p.is_object());
                calls += 1;
            }
            _ => {}
        }
    }
    assert_eq!((states, calls), (2, 14));
    let state = &mirror.topics[control_center::TOPIC].1;
    assert_eq!(state["brightness"]["eDP-1"], 65);
    assert_eq!(state["recorder"]["recording"], true);
    assert_eq!(state["keyboard"]["max"], 3);
}

#[test]
fn schemas_and_types_are_published() {
    let generated = artifacts::generate();
    for path in [
        "protocol/schema/audio-set-volume.json",
        "protocol/schema/controls-state.json",
        "protocol/luau/control-center.d.luau",
        "protocol/fixtures/control-center.jsonl",
    ] {
        assert!(generated.contains_key(path), "missing {path}");
    }
    assert!(
        generated["protocol/luau/control-center.d.luau"].contains("export type ControlsState =")
    );
}
