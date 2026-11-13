use nacre_protocol::{
    artifacts, framing,
    session::{SessionState, STATE_TOPIC},
    Message, Mirror, Update,
};

#[test]
fn session_state_transcript_replays_with_rising_revisions() {
    let generated = artifacts::generate();
    assert!(generated.contains_key("protocol/schema/session-state.json"));
    assert!(generated["protocol/luau/session.d.luau"].contains("export type SessionState ="));
    let mut mirror = Mirror::default();
    let mut states = Vec::new();
    for line in generated["protocol/fixtures/session-state.jsonl"].lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        assert!(
            !matches!(mirror.receive(&message), Update::Resync(_)),
            "{line}"
        );
        if let Message::Snap { topic, .. } | Message::Patch { topic, .. } = &message {
            assert_eq!(topic, STATE_TOPIC);
            let state: SessionState =
                serde_json::from_value(mirror.topics[topic].1.clone()).unwrap();
            assert_eq!(state.sleeping, state.preparing_sleep);
            states.push(state);
        }
    }
    assert_eq!(states.first(), Some(&SessionState::default()));
    assert!(states
        .windows(2)
        .all(|w| w[1].revision == w[0].revision + 1));
    // Resume (preparing_sleep falls) arrives while still locked; unlock follows.
    assert!(states[3].preparing_sleep && !states[4].preparing_sleep && states[4].locked);
    assert!(!states[5].locked);
}
