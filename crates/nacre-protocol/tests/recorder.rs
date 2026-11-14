use nacre_protocol::{
    artifacts, control_center::Recorder, framing, recorder::TOPIC, Message, Mirror, Update,
};

#[test]
fn recorder_transcript_replays_a_recording_start_and_stop() {
    let generated = artifacts::generate();
    assert!(generated.contains_key("protocol/schema/recorder-state.json"));
    assert!(generated["protocol/luau/recorder.d.luau"].contains("export type RecorderState ="));
    let mut mirror = Mirror::default();
    let mut states = Vec::new();
    for line in generated["protocol/fixtures/recorder.jsonl"].lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        assert!(
            !matches!(mirror.receive(&message), Update::Resync(_)),
            "{line}"
        );
        if let Message::Snap { topic, .. } | Message::Patch { topic, .. } = &message {
            assert_eq!(topic, TOPIC);
            let state: Recorder = serde_json::from_value(mirror.topics[topic].1.clone()).unwrap();
            states.push(state.recording);
        }
    }
    assert_eq!(states, [false, true, false]);
}
