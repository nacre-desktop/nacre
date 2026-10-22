use nacre_protocol::{
    artifacts, bar_services::*, framing, pill_controls::*, shell_actions::*, Message,
};
use serde_json::Value;

#[test]
fn bar_action_goldens_decode_into_the_published_request_types() {
    let mut calls = std::collections::BTreeSet::new();
    let files = artifacts::generate();
    for name in ["shell-actions", "bar-services", "pill-controls"] {
        let frames = &files[&format!("protocol/fixtures/{name}.jsonl")];
        for line in frames.lines() {
            if let Message::Call { m, p, .. } = framing::decode(line.as_bytes()).unwrap() {
                let valid = match m.as_str() {
                    "workspace.activate" => serde_json::from_value::<WorkspaceActivate>(p)
                        .unwrap()
                        .validate(),
                    "workspace.scroll" => serde_json::from_value::<WorkspaceScroll>(p)
                        .unwrap()
                        .validate(),
                    "window.focus" => serde_json::from_value::<WindowFocus>(p).unwrap().validate(),
                    "session.request" => serde_json::from_value::<SessionRequest>(p)
                        .unwrap()
                        .validate(),
                    "brightness.set" => serde_json::from_value::<BrightnessSet>(p)
                        .unwrap()
                        .validate(),
                    "brightness.adjust" => serde_json::from_value::<BrightnessAdjust>(p)
                        .unwrap()
                        .validate(),
                    "keyboard_light.adjust" => {
                        let _: KeyboardLightAdjust = serde_json::from_value(p).unwrap();
                        Ok(())
                    }
                    "tray.activate" | "tray.secondary_activate" | "tray.context_menu" => {
                        serde_json::from_value::<TrayItemAction>(p)
                            .unwrap()
                            .validate()
                    }
                    "tray.scroll" => serde_json::from_value::<TrayScroll>(p).unwrap().validate(),
                    "audio.adjust_volume" => serde_json::from_value::<VolumeAdjust>(p)
                        .unwrap()
                        .validate(),
                    "audio.toggle_mute" => {
                        let _: PillEmpty = serde_json::from_value(p).unwrap();
                        Ok(())
                    }
                    "bluetooth.set_enabled" => {
                        let _: BluetoothEnabled = serde_json::from_value(p).unwrap();
                        Ok(())
                    }
                    _ => panic!("Unexpected bar action {m}"),
                };
                assert!(valid.is_ok(), "Invalid golden request: {m}");
                calls.insert(m);
            }
        }
    }
    assert_eq!(
        calls.len(),
        14,
        "Every new method needs a typed golden call"
    );
}

#[test]
fn published_action_schemas_refuse_additional_fields() {
    let mut methods = nacre_protocol::shell_actions::methods();
    methods.extend(nacre_protocol::bar_services::methods());
    methods.extend(nacre_protocol::pill_controls::methods());
    assert_eq!(methods.len(), 14);
    for (name, method) in methods {
        assert_eq!(
            method.params["additionalProperties"],
            Value::Bool(false),
            "{name}"
        );
        assert_eq!(
            method.result["additionalProperties"],
            Value::Bool(false),
            "{name}"
        );
    }
}
