//! Semantic controls used by independent desktop bars.
use crate::{Capability, ErrorCode, Failure, Method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrayItemAction {
    #[schemars(length(min = 1, max = 128))]
    pub item: String,
    #[serde(default)]
    #[schemars(range(min = -32768, max = 32767))]
    pub x: i16,
    #[serde(default)]
    #[schemars(range(min = -32768, max = 32767))]
    pub y: i16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TrayScrollAxis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrayScroll {
    #[schemars(length(min = 1, max = 128))]
    pub item: String,
    pub axis: TrayScrollAxis,
    /// Raw SNI scroll delta; a conventional wheel notch is 120 units.
    #[schemars(range(min = -1200, max = 1200))]
    pub delta: i16,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrightnessSet {
    #[schemars(length(min = 1, max = 128))]
    pub output: String,
    #[schemars(range(min = 0, max = 100))]
    pub percent: u8,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrightnessAdjust {
    #[serde(default)]
    #[schemars(length(min = 1, max = 128))]
    pub output: Option<String>,
    #[schemars(range(min = -100, max = 100))]
    pub delta: i8,
    #[serde(default)]
    pub key_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardLightStep {
    Up,
    Down,
}
/// One keyboard-backlight step; `up` wraps from the maximum back to off, like the QML shell.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyboardLightAdjust {
    pub step: KeyboardLightStep,
    #[serde(default)]
    pub key_ms: Option<u64>,
}

impl TrayItemAction {
    pub fn validate(&self) -> Result<(), Failure> {
        if valid_id(&self.item) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl TrayScroll {
    pub fn validate(&self) -> Result<(), Failure> {
        if valid_id(&self.item) && (-1200..=1200).contains(&self.delta) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl BrightnessSet {
    pub fn validate(&self) -> Result<(), Failure> {
        if valid_id(&self.output) && self.percent <= 100 {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl BrightnessAdjust {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.output.as_deref().is_none_or(valid_id) && (-100..=100).contains(&self.delta) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid bar service action", false).dispatch_state(false)
}

/// Replies carry the resulting level so a shell can render OSD feedback without a state readback.
fn result_schema(method: &str) -> serde_json::Value {
    let level = json!({"type":"integer","minimum":0});
    let (properties, required) = match method {
        "brightness.set" | "brightness.adjust" => (
            json!({"changed":{"type":"boolean"},"output":{"type":"string"},"level_percent":{"type":"integer","minimum":0,"maximum":100}}),
            json!(["changed", "output", "level_percent"]),
        ),
        "keyboard_light.adjust" => (
            json!({"changed":{"type":"boolean"},"level":level,"max":{"type":"integer","minimum":1}}),
            json!(["changed", "level", "max"]),
        ),
        _ => (json!({"changed":{"type":"boolean"}}), json!(["changed"])),
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

pub fn methods() -> BTreeMap<String, Method> {
    [
        ("tray.activate", schemars::schema_for!(TrayItemAction)),
        (
            "tray.secondary_activate",
            schemars::schema_for!(TrayItemAction),
        ),
        ("tray.context_menu", schemars::schema_for!(TrayItemAction)),
        ("tray.scroll", schemars::schema_for!(TrayScroll)),
        ("brightness.set", schemars::schema_for!(BrightnessSet)),
        ("brightness.adjust", schemars::schema_for!(BrightnessAdjust)),
        (
            "keyboard_light.adjust",
            schemars::schema_for!(KeyboardLightAdjust),
        ),
    ]
    .into_iter()
    .map(|(name, schema)| {
        (
            name.into(),
            Method {
                v: 1,
                class: if name.starts_with("brightness.") || name.starts_with("keyboard_light.") {
                    Capability::System
                } else {
                    Capability::Desktop
                },
                timeout: if name.starts_with("brightness.") {
                    30_000
                } else {
                    10_000
                },
                params: serde_json::to_value(schema).expect("bar service schema"),
                result: result_schema(name),
            },
        )
    })
    .collect()
}

pub(crate) fn exports() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, file, schema) in [
        (
            "TrayItemAction",
            "tray-item-action",
            schemars::schema_for!(TrayItemAction),
        ),
        (
            "TrayScrollAxis",
            "tray-scroll-axis",
            schemars::schema_for!(TrayScrollAxis),
        ),
        (
            "TrayScroll",
            "tray-scroll",
            schemars::schema_for!(TrayScroll),
        ),
        (
            "BrightnessSet",
            "brightness-set",
            schemars::schema_for!(BrightnessSet),
        ),
        (
            "BrightnessAdjust",
            "brightness-adjust",
            schemars::schema_for!(BrightnessAdjust),
        ),
        (
            "KeyboardLightStep",
            "keyboard-light-step",
            schemars::schema_for!(KeyboardLightStep),
        ),
        (
            "KeyboardLightAdjust",
            "keyboard-light-adjust",
            schemars::schema_for!(KeyboardLightAdjust),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("bar service schema");
        types.push_str(&format!(
            "export type {name} = {}\n",
            crate::artifacts::luau(&schema)
        ));
        files.insert(
            format!("protocol/schema/{file}.json"),
            serde_json::to_string_pretty(&schema).expect("schema JSON") + "\n",
        );
    }
    types.push_str("return {}\n");
    files.insert("protocol/luau/bar-services.d.luau".into(), types);
    let frames = [
        json!({"t":"call","id":401,"m":"tray.activate","p":{"item":"sni:abc","x":15,"y":9}}),
        json!({"t":"ret","id":401,"ok":{"changed":true}}),
        json!({"t":"call","id":404,"m":"tray.secondary_activate","p":{"item":"sni:abc","x":15,"y":9}}),
        json!({"t":"ret","id":404,"ok":{"changed":true}}),
        json!({"t":"call","id":405,"m":"tray.context_menu","p":{"item":"sni:abc","x":15,"y":9}}),
        json!({"t":"ret","id":405,"ok":{"changed":true}}),
        json!({"t":"call","id":402,"m":"tray.scroll","p":{"item":"sni:abc","axis":"vertical","delta":-2}}),
        json!({"t":"ret","id":402,"ok":{"changed":true}}),
        json!({"t":"call","id":403,"m":"brightness.set","p":{"output":"output-1","percent":65}}),
        json!({"t":"ret","id":403,"ok":{"changed":true,"output":"output-1","level_percent":65}}),
        json!({"t":"call","id":406,"m":"brightness.adjust","p":{"output":"output-1","delta":-5}}),
        json!({"t":"ret","id":406,"ok":{"changed":true,"output":"output-1","level_percent":60}}),
        json!({"t":"call","id":408,"m":"brightness.adjust","p":{"delta":5,"key_ms":1782450000000_u64}}),
        json!({"t":"ret","id":408,"ok":{"changed":true,"output":"output-1","level_percent":65}}),
        json!({"t":"call","id":407,"m":"keyboard_light.adjust","p":{"step":"up","key_ms":1782450000000_u64}}),
        json!({"t":"ret","id":407,"ok":{"changed":true,"level":2,"max":3}}),
    ];
    files.insert(
        "protocol/fixtures/bar-services.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("bar service transcript") + "\n")
            .collect(),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bar_inputs_are_bounded_and_methods_are_semantic() {
        assert!(TrayItemAction {
            item: "sni:abc".into(),
            x: 15,
            y: 9
        }
        .validate()
        .is_ok());
        assert!(TrayItemAction {
            item: "x\n".into(),
            x: 0,
            y: 0
        }
        .validate()
        .is_err());
        assert!(TrayScroll {
            item: "sni:abc".into(),
            axis: TrayScrollAxis::Vertical,
            delta: -120
        }
        .validate()
        .is_ok());
        assert!(TrayScroll {
            item: "sni:abc".into(),
            axis: TrayScrollAxis::Vertical,
            delta: 1201
        }
        .validate()
        .is_err());
        assert!(serde_json::from_value::<TrayItemAction>(
            json!({"item":"sni:a","x":0,"y":0,"extra":true})
        )
        .is_err());
        assert!(BrightnessSet {
            output: "DP-1".into(),
            percent: 100
        }
        .validate()
        .is_ok());
        assert!(BrightnessAdjust {
            output: Some("DP-1".into()),
            delta: 101,
            key_ms: None,
        }
        .validate()
        .is_err());
        assert!(BrightnessAdjust {
            output: None,
            delta: 5,
            key_ms: Some(1782450000000),
        }
        .validate()
        .is_ok());
        assert!(KeyboardLightAdjust::deserialize(json!({"step":"sideways"})).is_err());
        assert!(KeyboardLightAdjust::deserialize(json!({"step":"up","x":1})).is_err());
        assert_eq!(methods().len(), 7);
        let reply = &methods()["keyboard_light.adjust"].result;
        assert_eq!(reply["required"], json!(["changed", "level", "max"]));
    }
}
