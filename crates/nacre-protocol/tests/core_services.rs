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
    let notifications = types
        .lines()
        .find(|line| line.starts_with("export type NotificationsState ="))
        .expect("notification type");
    assert!(
        notifications.contains("[\"popup_order\"]: ({EntityId} | nil)?"),
        "Nullable popup IDs lost their element type"
    );
}

#[test]
fn reserved_transcripts_replay_typed_calls_and_complete_snapshots() {
    let mut topics = 0;
    let mut calls = std::collections::BTreeSet::new();
    for (file, text) in artifacts::generate() {
        if !file.starts_with("protocol/fixtures/core-")
            || file.starts_with("protocol/fixtures/core-compat-")
        {
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
    assert_eq!(topics, 12);
    assert_eq!(calls.len(), 19);
    for method in calls {
        assert!(
            !nacre_protocol::core_methods().contains_key(&method),
            "Schemas must not advertise {method}"
        );
    }
}

fn compatibility_states(name: &str) -> Vec<Value> {
    let exports = artifacts::generate();
    let text = &exports[&format!("protocol/fixtures/core-compat-{name}.jsonl")];
    let mut state = Value::Null;
    let mut states = Vec::new();
    for (index, line) in text.lines().enumerate() {
        match framing::decode(line.as_bytes()).unwrap() {
            Message::Snap { topic, seq, data } => {
                assert_eq!(index, 0);
                assert_eq!(seq, 0);
                state = data;
                let typed: CoreSnapshot =
                    serde_json::from_value(json!({"topic":topic,"data":state})).unwrap();
                assert!(typed.valid(), "Invalid {name} snapshot");
            }
            Message::Patch {
                topic,
                seq,
                base,
                data,
            } => {
                assert_eq!(base, index as u64 - 1);
                assert_eq!(seq, index as u64);
                patch::apply(&mut state, &data);
                let typed: CoreSnapshot =
                    serde_json::from_value(json!({"topic":topic,"data":state})).unwrap();
                assert!(typed.valid(), "Invalid {name} patch {seq}");
            }
            _ => panic!("Unexpected compatibility frame"),
        }
        states.push(state.clone());
    }
    states
}

#[test]
fn notification_popup_eligibility_is_service_owned_and_history_is_independent() {
    let states = compatibility_states("notifications-popup-policy");
    assert_eq!(states.len(), 7);
    assert_eq!(states[0]["popup_order"], json!(["notice-1"]));
    assert_eq!(states[1]["inhibited"], true);
    assert_eq!(states[1]["popup_order"], json!([]));
    assert_eq!(states[2]["inhibited"], false);
    assert_eq!(states[2]["popup_order"], json!([]));
    assert_eq!(states[3]["popup_order"], json!(["notice-2"]));
    assert_eq!(
        states[4]["active"]["items"]["notice-2"]["remaining_ms"],
        3000
    );
    assert_eq!(states[5]["active"]["items"]["notice-3"]["timeout_ms"], 0);
    assert!(states[5]["active"]["items"]["notice-3"]
        .get("remaining_ms")
        .is_none());
    assert!(states[6]["active"]["items"].get("notice-2").is_none());
    assert_eq!(states[6]["history"]["items"]["notice-2"]["remaining_ms"], 0);
    let history = compatibility_states("notifications-history-storage");
    assert_eq!(history.len(), 3);
    assert_eq!(history[0]["history_count"], 2);
    assert_eq!(history[1]["history_count"], 5);
    assert_eq!(history[1]["history"]["order"].as_array().unwrap().len(), 1);
    assert!(history[1]["history_storage"].get("error").is_none());
    assert_eq!(history[2]["history_storage"]["writable"], false);
    for state in history {
        assert_eq!(state["health"], json!({"available":true,"stale":false}));
    }
}

#[test]
fn explicit_audio_readback_never_substitutes_zero_for_unknown() {
    let states = compatibility_states("audio-readback");
    assert_eq!(states.len(), 6);
    let node = |index: usize| &states[index]["nodes"]["items"]["output-1"];
    assert_eq!(node(0)["ready"], false);
    assert_eq!(node(0)["readback"], json!({}));
    assert_eq!(node(1)["readback"]["linear_volume"], 0.125);
    assert_eq!(node(1)["readback"]["perceptual_volume"], 0.5);
    assert_eq!(node(2)["ready"], false);
    assert_eq!(node(2)["volume"], 0.125); // Retained legacy value is not a reading.
    assert_eq!(
        node(2)["readback"],
        json!({"linear_volume":0.125,"muted":false})
    );
    assert_eq!(node(3)["readback"], json!({"muted":false}));
    assert_eq!(node(4)["readback"], json!({}));
    assert_eq!(node(5)["readback"], json!({"muted":true}));
}

#[test]
fn primary_connection_and_custom_icons_have_explicit_identity() {
    let network = compatibility_states("network-primary");
    assert_eq!(network.len(), 3);
    assert_eq!(
        network[0]["connections"]["order"].as_array().unwrap().len(),
        2
    );
    assert_eq!(network[0]["primary_connection"], "connection-wired");
    assert_eq!(network[1]["primary_connection"], "connection-wireless");
    assert!(network[2].get("primary_connection").is_none());
    let tray = compatibility_states("tray-custom-icon");
    assert_eq!(tray.len(), 2);
    assert_eq!(
        tray[0]["items"]["items"]["tray-1"]["icon_path"],
        "/tmp/nacre-fixtures/icons/tray.png"
    );
    assert_eq!(
        tray[1]["items"]["items"]["tray-1"]["icon_path"],
        "/tmp/nacre-fixtures/icons/tray-attention.png"
    );
}

#[test]
fn battery_is_independent_and_osd_repeats_without_replaying_expired_events() {
    let battery = compatibility_states("battery");
    assert_eq!(battery.len(), 4);
    assert!(battery[0].get("percentage").is_none());
    assert_eq!(battery[1]["percentage"], 62.5);
    assert_eq!(battery[2]["charging"], true);
    assert!(battery[3].get("percentage").is_none());
    assert!(battery[3].get("charging").is_none());
    let osd = compatibility_states("osd");
    assert_eq!(osd.len(), 11);
    assert!(osd[0].get("current").is_none());
    assert_eq!(osd[1]["current"]["serial"], 1);
    assert_eq!(osd[2]["current"]["serial"], 2);
    assert_eq!(osd[1]["current"]["value"], osd[2]["current"]["value"]);
    assert_eq!(osd[3]["current"]["channel"], "brightness");
    assert!(osd[3]["current"].get("muted").is_none());
    assert_eq!(osd[4]["current"]["channel"], "microphone");
    assert_eq!(osd[4]["current"]["muted"], true);
    assert!(osd[4]["current"].get("value").is_none());
    for index in [5, 7, 10] {
        assert!(osd[index].get("current").is_none());
    }
    assert_eq!(osd[8]["current"], osd[9]["current"]);
    let exports = artifacts::generate();
    assert_eq!(
        exports
            .keys()
            .filter(|file| file.starts_with("protocol/fixtures/core-compat-"))
            .count(),
        10
    );
}

#[test]
fn dashboard_metrics_player_and_frequency_extensions_keep_unknown_values_omitted() {
    let frequency = compatibility_states("network-frequency");
    let point = |index: usize| frequency[index]["access_points"]["items"]["ap-1"].clone();
    assert_eq!(point(0)["frequency_mhz"], 5180);
    assert_eq!(point(1)["frequency_mhz"], 2437);
    assert!(point(2).get("frequency_mhz").is_none());
    let system = compatibility_states("system-metrics");
    assert_eq!(system.len(), 5);
    assert!(system[0].get("cpu_percent").is_none() && system[0].get("memory").is_none());
    assert_eq!(system[1]["network"].as_array().unwrap().len(), 1);
    assert_eq!(system[2]["cpu_percent"], 12.5);
    assert_eq!(system[2]["gpu"]["memory_total"], 8589934592_u64);
    assert!(system[3].get("cpu_percent").is_none());
    assert_eq!(system[3]["gpu"], json!({}));
    assert_eq!(system[4]["health"]["stale"], true);
    let player = compatibility_states("player-lifecycle");
    assert_eq!(player.len(), 7);
    let first = |index: usize| player[index]["players"]["items"]["player-1"].clone();
    assert_eq!(first(1)["position_ms"], 120000);
    assert_eq!(first(2)["status"], "paused");
    assert!(first(3).get("art_path").is_none() && first(3).get("length_ms").is_none());
    assert_eq!(player[4]["active"], "player-2");
    assert_eq!(player[5]["players"]["order"], json!(["player-2"]));
    assert!(player[6].get("active").is_none());
}

#[test]
fn dashboard_metrics_and_player_reject_impossible_values() {
    let valid = |topic: &str, state: Value| {
        serde_json::from_value::<CoreSnapshot>(json!({"topic":topic,"data":state}))
            .is_ok_and(|state| state.valid())
    };
    let system = compatibility_states("system-metrics")[2].clone();
    assert!(valid("dashboard.system", system.clone()));
    for (key, value) in [
        ("cpu_percent", json!(100.5)),
        ("cpu_percent", json!(-1.0)),
        ("memory", json!({"used":2,"total":1})),
        ("memory", json!({"used":0,"total":0})),
        ("gpu", json!({"percent":101.0})),
        ("gpu", json!({"memory_used":9,"memory_total":8})),
        ("network", json!(vec![json!({"down_bps":1,"up_bps":1}); 33])),
        ("network", json!([{"down_bps":-1,"up_bps":1}])),
    ] {
        let mut state = system.clone();
        state[key] = value;
        assert!(!valid("dashboard.system", state), "{key}");
    }
    let mut old = system;
    for key in ["cpu_percent", "memory", "gpu", "network"] {
        old.as_object_mut().unwrap().remove(key);
    }
    assert!(valid("dashboard.system", old), "Older producers stay valid");
    let player = compatibility_states("player-lifecycle")[0].clone();
    assert!(valid("dashboard.player", player.clone()));
    for (key, value) in [
        ("art_path", json!("https://example.invalid/art.png")),
        ("art_path", json!("relative.png")),
        ("art_path", json!("/tmp/a\nb")),
        ("position_ms", json!(200001)),
        ("position_updated_ms", Value::Null),
        ("length_ms", json!(0)),
        ("identity", json!("x".repeat(257))),
        ("title", json!("x".repeat(513))),
    ] {
        let mut state = player.clone();
        state["players"]["items"]["player-1"][key] = value;
        assert!(!valid("dashboard.player", state), "{key}");
    }
    let mut uncontrollable = player.clone();
    uncontrollable["players"]["items"]["player-1"]["can_control"] = json!(false);
    assert!(!valid("dashboard.player", uncontrollable));
    let mut foreign = player;
    foreign["active"] = json!("missing");
    assert!(!valid("dashboard.player", foreign));
}

#[test]
fn compatibility_rejects_foreign_ids_unit_confusion_and_invalid_readings() {
    let valid = |topic: &str, state: Value| {
        serde_json::from_value::<CoreSnapshot>(json!({"topic":topic,"data":state}))
            .is_ok_and(|state| state.valid())
    };
    for primary in ["missing", "../connection"] {
        let mut state = compatibility_states("network-primary")[0].clone();
        state["primary_connection"] = json!(primary);
        assert!(!valid("network", state));
    }
    let mut inactive = compatibility_states("network-primary")[1].clone();
    inactive["primary_connection"] = json!("connection-wired");
    assert!(!valid("network", inactive));
    for popups in [json!(["missing"]), json!(["notice-1", "notice-1"])] {
        let mut state = compatibility_states("notifications-popup-policy")[0].clone();
        state["popup_order"] = popups;
        assert!(!valid("notifications", state));
    }
    let mut count = compatibility_states("notifications-history-storage")[0].clone();
    count["history_count"] = json!(1); // One stored plus one nontransient active.
    assert!(!valid("notifications", count));
    for (linear, perceptual) in [(0.125, 0.125), (-0.1, 0.5), (0.125, -0.5)] {
        let mut state = compatibility_states("audio-readback")[1].clone();
        state["nodes"]["items"]["output-1"]["readback"] = json!({
            "linear_volume":linear,"perceptual_volume":perceptual});
        assert!(!valid("audio", state));
    }
    for percentage in [-0.1, 100.1] {
        let mut state = compatibility_states("battery")[1].clone();
        state["percentage"] = json!(percentage);
        assert!(!valid("battery", state));
    }
    for (channel, unit, value) in [
        ("volume", "fraction", 0.5),
        ("brightness", "linear", 0.5),
        ("brightness", "fraction", 1.1),
        ("capslock", "boolean", 0.5),
        ("microphone", "linear", -0.1),
    ] {
        assert!(!valid(
            "osd",
            json!({"health":{"available":true,"stale":false},
            "current":{"serial":1,"channel":channel,"unit":unit,"value":value,
                "expires_ms":1800000002000_u64}})
        ));
    }
    for serial in [0, nacre_protocol::MAX_SAFE_INTEGER + 1] {
        let mut state = compatibility_states("osd")[1].clone();
        state["current"]["serial"] = json!(serial);
        assert!(!valid("osd", state));
    }
    let mut stale_mute = compatibility_states("osd")[1].clone();
    patch::apply(
        &mut stale_mute,
        &json!({"current":{"serial":3,
        "channel":"brightness","unit":"fraction","value":0.75}}),
    );
    assert!(!valid("osd", stale_mute));
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut audio: CoreSnapshot = serde_json::from_value(json!({"topic":"audio",
            "data":compatibility_states("audio-readback")[1]}))
        .unwrap();
        let CoreSnapshot::Audio(ref mut state) = audio else {
            unreachable!()
        };
        state
            .nodes
            .items
            .get_mut(&EntityId("output-1".into()))
            .unwrap()
            .readback
            .as_mut()
            .unwrap()
            .linear_volume = Some(invalid);
        assert!(!audio.valid());
        let mut battery: CoreSnapshot = serde_json::from_value(json!({"topic":"battery",
            "data":compatibility_states("battery")[1]}))
        .unwrap();
        let CoreSnapshot::Battery(ref mut state) = battery else {
            unreachable!()
        };
        state.percentage = Some(invalid);
        assert!(!battery.valid());
        let mut osd: CoreSnapshot = serde_json::from_value(json!({"topic":"osd",
            "data":compatibility_states("osd")[1]}))
        .unwrap();
        let CoreSnapshot::Osd(ref mut state) = osd else {
            unreachable!()
        };
        state.current.as_mut().unwrap().value = Some(invalid);
        assert!(!osd.valid());
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
