use nacre_protocol::{
    artifacts,
    displays::{self, DisplaysState},
    framing, Message, Mirror, Update,
};

#[test]
fn transcript_replays_and_every_state_validates() {
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/displays.jsonl")
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
                assert_eq!(topic, displays::TOPIC);
                serde_json::from_value::<DisplaysState>(mirror.topics[topic].1.clone())
                    .unwrap()
                    .validate()
                    .unwrap();
                states += 1;
            }
            Message::Call { m, p, .. } => {
                assert!(displays::methods().contains_key(m), "{m}");
                assert!(p.is_object());
                calls += 1;
            }
            _ => {}
        }
    }
    assert_eq!((states, calls), (3, 4));
    let state = &mirror.topics[displays::TOPIC].1;
    assert!(state.get("pending").is_none(), "confirm ends the countdown");
    assert_eq!(state["outputs"][1]["x"], 1920);
}

#[test]
fn schemas_and_types_are_published() {
    let generated = artifacts::generate();
    for path in [
        "protocol/schema/displays-state.json",
        "protocol/schema/display-apply.json",
        "protocol/luau/displays.d.luau",
        "protocol/fixtures/displays.jsonl",
    ] {
        assert!(generated.contains_key(path), "missing {path}");
    }
    assert!(generated["protocol/luau/displays.d.luau"].contains("export type DisplaysState ="));
}
