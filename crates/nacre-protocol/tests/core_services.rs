use nacre_protocol::{artifacts, core_services::*, framing, patch, Message};
use serde_json::{json, Value};

#[test]
fn validated_map_keys_keep_their_element_types_in_luau() {
    let exports = artifacts::generate();
    let types = &exports["protocol/luau/core-services.d.luau"];
    for entity in [
        "Device",
        "AccessPoint",
        "AudioNode",
        "Notification",
        "WeatherDay",
        "SystemProcess",
    ] {
        assert!(
            types.contains(&format!("[string]: {entity} }}")),
            "Missing map value type {entity}"
        );
    }
    for line in types
        .lines()
        .filter(|line| line.starts_with("export type Entities"))
    {
        assert!(!line.contains("any"), "Untyped entity map: {line}");
    }
}

#[test]
fn reserved_transcripts_replay_typed_calls_and_complete_snapshots() {
    let mut topics = 0;
    let mut calls = std::collections::BTreeSet::new();
    for (file, text) in artifacts::generate() {
        if !file.starts_with("protocol/fixtures/core-") {
            continue;
        }
        let errors = file.ends_with("core-errors.jsonl");
        let mut snapshot = Value::Null;
        for line in text.lines() {
            let raw: Value = serde_json::from_str(line).unwrap();
            let message = framing::decode(line.as_bytes()).unwrap();
            match message {
                Message::Snap { topic, data, .. } => {
                    snapshot = data;
                    let typed: CoreSnapshot =
                        serde_json::from_value(json!({"topic":topic,"data":snapshot})).unwrap();
                    assert!(typed.valid(), "{file}");
                    topics += 1;
                }
                Message::Patch { topic, data, .. } => {
                    patch::apply(&mut snapshot, &data);
                    let typed: CoreSnapshot =
                        serde_json::from_value(json!({"topic":topic,"data":snapshot})).unwrap();
                    assert!(typed.valid(), "{file}");
                    assert_eq!(snapshot["health"]["stale"], true);
                }
                Message::Call { m, .. } => {
                    let typed: CoreCall = serde_json::from_value(raw).unwrap();
                    assert_eq!(
                        typed.valid(),
                        !errors || m != "audio.set_volume" || line.contains("old-node")
                    );
                    calls.insert(m);
                }
                Message::Ret { .. } => {
                    if let Some(ok) = raw.get("ok") {
                        serde_json::from_value::<CoreResult>(ok.clone()).unwrap();
                    } else {
                        assert!(errors);
                    }
                }
                _ => panic!("Unexpected reserved transcript frame"),
            }
        }
    }
    assert_eq!(topics, 11);
    assert_eq!(calls.len(), 15);
    for method in calls {
        assert!(
            !nacre_protocol::core_methods().contains_key(&method),
            "Schemas must not advertise {method}"
        );
    }
}

#[test]
fn actions_reject_raw_paths_empty_ids_and_volume_escalation() {
    for id in [
        "",
        "../output",
        "/org/bluez/hci0",
        "$(touch fixture)",
        "x\nexec",
        &"x".repeat(129),
    ] {
        for (method, params) in [
            ("network.disconnect", json!({"device":id})),
            (
                "network.connect",
                json!({"device":"wifi-1","target":{"kind":"connection","id":id}}),
            ),
            ("bluetooth.pair", json!({"device":id})),
            ("bluetooth.set_power", json!({"adapter":id,"powered":true})),
            ("power.set_profile", json!({"profile":id})),
            ("audio.route", json!({"stream":"stream-1","target":id})),
        ] {
            let call: CoreCall = serde_json::from_value(json!({"m":method,"p":params})).unwrap();
            assert!(!call.valid());
        }
    }
    for volume in [-1.0, 1.01, 1.5, f64::INFINITY, f64::NAN] {
        assert!(!CoreCall::AudioVolume(AudioVolume {
            node: EntityId("output-1".into()),
            volume
        })
        .valid());
    }
    for volume in [0.0, 0.5, 1.0] {
        assert!(CoreCall::AudioVolume(AudioVolume {
            node: EntityId("output-1".into()),
            volume
        })
        .valid());
    }
    assert!(serde_json::from_value::<CoreCall>(json!({"m":"network.connect","p":{"device":"wifi-1","target":{"kind":"command","id":"fixture"}}})).is_err());
    let call: CoreCall = serde_json::from_value(json!({"m":"bluetooth.pair","p":{"device":"device-1","auto_accept":true},"mismatch":true,"t":"call","id":1})).unwrap();
    // Unknown optional fields never gain authority or become backend arguments.
    assert_eq!(
        serde_json::to_value(call).unwrap(),
        json!({"m":"bluetooth.pair","p":{"device":"device-1"}})
    );
}

#[test]
fn ordered_collections_and_unknown_optional_fields_keep_the_contract() {
    let original = json!({"topic":"audio","data":{"health":{"available":true,"stale":false},"nodes":{"items":{
        "output-1":{"name":"Speakers","kind":"output","volume":0.5,"muted":false,"ready":true}
    },"order":["output-1"]},"default_output":"output-1","future_optional":true}});
    let state: CoreSnapshot = serde_json::from_value(original.clone()).unwrap();
    assert!(state.valid());
    for (path, value) in [
        ("/data/nodes/order", json!(["output-1", "output-1"])),
        ("/data/nodes/order", json!(["missing"])),
        ("/data/default_output", json!("missing")),
        ("/data/nodes/items/output-1/volume", json!(-0.1)),
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(!serde_json::from_value::<CoreSnapshot>(invalid)
            .unwrap()
            .valid());
    }
    assert!(serde_json::from_value::<CoreSnapshot>(
        json!({"topic":"audio","data":{"health":{"available":false,"stale":true},"nodes":[]}})
    )
    .is_err());
    let mut amplified = original;
    amplified["data"]["nodes"]["items"]["output-1"]["volume"] = json!(1.5);
    assert!(serde_json::from_value::<CoreSnapshot>(amplified)
        .unwrap()
        .valid());
    assert!(timestamp_valid(nacre_protocol::MAX_SAFE_INTEGER));
    assert!(!timestamp_valid(nacre_protocol::MAX_SAFE_INTEGER + 1));
}

#[test]
fn tray_menu_cycles_depth_and_untrusted_image_urls_are_refused() {
    let entry = |children: Value| json!({"label":"Fixture","enabled":true,"visible":true,"separator":false,"children":children});
    let state = |items: Value| {
        let order = items
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        json!({"topic":"tray","data":{"health":{"available":true,"stale":false},"items":{"items":{"tray-1":{
            "title":"Fixture tray","visible":true,"item_is_menu":true,"attention":false,
            "menu":{"items":items,"order":order}
        }},"order":["tray-1"]}}})
    };
    assert!(!serde_json::from_value::<CoreSnapshot>(state(
        json!({"menu-1":entry(json!(["menu-1"]))})
    ))
    .unwrap()
    .valid());
    for length in [16, 17] {
        let items = (0..length)
            .map(|index| {
                (
                    format!("menu-{index}"),
                    entry(if index + 1 < length {
                        json!([format!("menu-{}", index + 1)])
                    } else {
                        json!([])
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        assert_eq!(
            serde_json::from_value::<CoreSnapshot>(state(Value::Object(items)))
                .unwrap()
                .valid(),
            length == 16
        );
    }
    let mut snapshot = state(json!({"menu-1":entry(json!([]))}));
    snapshot["data"]["items"]["items"]["tray-1"]["icon_path"] =
        json!("https://invalid.example/icon.png");
    assert!(!serde_json::from_value::<CoreSnapshot>(snapshot)
        .unwrap()
        .valid());
}
