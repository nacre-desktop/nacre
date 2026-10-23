use nacre_protocol::{
    artifacts, framing,
    system_settings::{self, CapabilitiesState, TimeState},
    Message, Mirror, Update,
};

#[test]
fn transcript_replays_and_every_state_validates() {
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/system-settings.jsonl")
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
                let data = mirror.topics[topic].1.clone();
                match topic.as_str() {
                    system_settings::TIME_TOPIC => serde_json::from_value::<TimeState>(data)
                        .unwrap()
                        .validate()
                        .unwrap(),
                    system_settings::CAPABILITIES_TOPIC => {
                        serde_json::from_value::<CapabilitiesState>(data)
                            .unwrap()
                            .validate()
                            .unwrap()
                    }
                    other => panic!("unexpected topic {other}"),
                }
                states += 1;
            }
            Message::Call { m, p, .. } => {
                assert!(system_settings::methods().contains_key(m), "{m}");
                assert!(p.is_object());
                calls += 1;
            }
            _ => {}
        }
    }
    assert_eq!((states, calls), (3, 4));
    let time = &mirror.topics[system_settings::TIME_TOPIC].1;
    assert_eq!(time["timezone"], "UTC");
    assert_eq!(
        time["ntp"],
        serde_json::json!({"supported":true,"enabled":false})
    );
}

#[test]
fn schemas_and_types_are_published() {
    let generated = artifacts::generate();
    for path in [
        "protocol/schema/time-set-timezone.json",
        "protocol/schema/time-state.json",
        "protocol/schema/capabilities-state.json",
        "protocol/luau/system-settings.d.luau",
        "protocol/fixtures/system-settings.jsonl",
    ] {
        assert!(generated.contains_key(path), "missing {path}");
    }
    assert!(generated["protocol/luau/system-settings.d.luau"].contains("export type TimeState ="));
}
