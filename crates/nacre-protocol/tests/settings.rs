use nacre_protocol::{artifacts, framing, settings::*, Message, Mirror, Update};
use serde_json::Value;

#[test]
fn provider_contract_replays_and_refuses_other_registered_stores() {
    let generated = artifacts::generate();
    let mut mirror = Mirror::default();
    for line in generated["protocol/fixtures/settings-provider.jsonl"].lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        if let Message::Call { p, .. } = &message {
            serde_json::from_value::<SettingsProviderWrite>(p.clone())
                .unwrap()
                .validate()
                .unwrap();
        }
        assert!(!matches!(mirror.receive(&message), Update::Resync(_)));
    }
    assert_eq!(mirror.topics["settings.store.colors"].1["dirty"], false);
    for store in [
        "profile",
        "desktop",
        "brain-auth",
        "software-input",
        "../colors",
        "battery-alerts",
    ] {
        assert!(SettingsProviderWrite {
            store: store.into(),
            value: serde_json::json!({}),
            rev: 0
        }
        .validate()
        .is_err());
    }
}

#[test]
fn settings_transcript_replays_revision_flush_and_last_good_error() {
    let generated = artifacts::generate();
    let trace = &generated["protocol/fixtures/settings.jsonl"];
    let mut mirror = Mirror::default();
    for line in trace.lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        match &message {
            Message::Call { m, p, .. } if m == "settings.get" => {
                let request: SettingsGet = serde_json::from_value(p.clone()).unwrap();
                assert!(store_id(&request.store));
            }
            Message::Call { m, p, .. } if m == "settings.set" => {
                serde_json::from_value::<SettingsSet>(p.clone())
                    .unwrap()
                    .validate()
                    .unwrap();
            }
            _ => {}
        }
        assert!(!matches!(mirror.receive(&message), Update::Resync(_)));
        if let Some((_, value)) = mirror.topics.get("settings.store.desktop") {
            serde_json::from_value::<SettingsStore>(value.clone()).unwrap();
        }
    }
    let state = &mirror.topics["settings.store.desktop"].1;
    assert_eq!(state["rev"], 1);
    assert_eq!(state["dirty"], false);
    assert_eq!(state["data"]["dnd"], true);
    assert_eq!(state["error"]["line"], 2);
}

#[test]
fn settings_luau_defines_nested_errors_and_rejects_reserved_path_components() {
    let generated = artifacts::generate();
    let types = &generated["protocol/luau/settings.d.luau"];
    assert!(types.contains("export type SettingsReadError ="));
    assert!(types.contains("export type SettingsStore ="));
    for key in ["__proto__", "prototype", "constructor", "../desktop", "a/b"] {
        let request: SettingsSet = serde_json::from_value(
            serde_json::json!({"store":"desktop","key":[key],"value":true,"rev":0}),
        )
        .unwrap();
        assert!(request.validate().is_err());
    }
    let schema: Value =
        serde_json::from_str(&generated["protocol/schema/settings-set.json"]).unwrap();
    assert_eq!(schema["properties"]["key"]["maxItems"], 16);
}

#[test]
fn existing_publisher_transcript_replays_and_has_no_arbitrary_store_writer() {
    let generated = artifacts::generate();
    let mut mirror = Mirror::default();
    for line in generated["protocol/fixtures/settings-publish.jsonl"].lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        if let Message::Call { p, .. } = &message {
            serde_json::from_value::<SettingsPublish>(p.clone())
                .unwrap()
                .validate()
                .unwrap();
        }
        assert!(!matches!(mirror.receive(&message), Update::Resync(_)));
    }
    assert_eq!(
        mirror.topics["settings.store.battery-alerts"].1["dirty"],
        false
    );
    for store in [
        "desktop",
        "brain-auth",
        "software-input",
        "../battery-alerts",
    ] {
        assert!(SettingsPublish {
            store: store.into(),
            value: serde_json::json!({}),
            rev: 0
        }
        .validate()
        .is_err());
    }
}
