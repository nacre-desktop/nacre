use nacre_protocol::{
    weather_photo::{State, Status},
    Message,
};

#[test]
fn generated_photo_transcript_has_typed_states_and_additive_exports() {
    let files = nacre_protocol::artifacts::generate();
    assert!(files.contains_key("protocol/schema/weather-photo-state.json"));
    assert!(files.contains_key("protocol/luau/weather-photo.d.luau"));
    let mut snapshots = 0;
    for line in files["protocol/fixtures/weather-photo.jsonl"].lines() {
        let message: serde_json::Value = serde_json::from_str(line).unwrap();
        let _: Message = serde_json::from_value(message.clone()).unwrap();
        if message["t"] == "snap" {
            let state: State = serde_json::from_value(message["data"].clone()).unwrap();
            assert_eq!(state.ready, state.status == Status::Ready);
            assert_eq!(state.asset.is_some(), state.ready);
            if let Some(asset) = state.asset {
                assert!(asset.path.ends_with("/daemon-weather-photo/city.image"));
                assert!(!asset.author.is_empty());
                assert!(!asset.license.is_empty());
            }
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 2);
}
