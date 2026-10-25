//! N0046: control-centre action contracts and the `controls` state topic.
//!
//! Every method is additive and v1. A method is advertised in the welcome registry only while its
//! backend is present; the state topic omits a member whose backend is absent.
use crate::{
    core_services::Health, pill_controls::PillEmpty, Capability, ErrorCode, Failure, Method,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const TOPIC: &str = "controls";
pub const MAX_OUTPUTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AudioTarget {
    /// The default output at the time of the call.
    Sink,
    /// The default input at the time of the call.
    Source,
    /// The PipeWire node named by `node`.
    Node,
    /// N0061: one application stream, `node` being its `audio` topic id (`sink-input:<n>` for
    /// playback, `source-output:<n>` for capture).
    Stream,
}
/// Absolute volume. `value` is the **linear** level in [0,1], the scale the QML slider wrote
/// (`audio.volume`) and the one `AudioNode.volume` / `readback.linear_volume` report. A slider
/// that is perceptual (cube root) converts with `value = slider^3` before calling.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioSetVolume {
    pub target: AudioTarget,
    /// PipeWire `node.name` for `node`, a stream id for `stream`; forbidden otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub node: Option<String>,
    #[schemars(range(min = 0.0, max = 1.0))]
    pub value: f64,
}
/// N0051: make one device the default output or input. `node` is the PipeWire `node.name` of
/// an `Audio/Sink` or `Audio/Source` device (the item id Bead lists); anything else is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioSetDefault {
    #[schemars(length(min = 1, max = 128))]
    pub node: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioSetMute {
    pub target: AudioTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub node: Option<String>,
    pub muted: bool,
}
/// Absolute keyboard backlight in raw steps; the maximum is `controls.keyboard.max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyboardLightSet {
    #[schemars(range(min = 0, max = 65535))]
    pub level: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PowerProfileName {
    PowerSaver,
    Balanced,
    Performance,
}
impl PowerProfileName {
    pub fn name(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PowerSetProfile {
    pub profile: PowerProfileName,
}
/// Shared by `airplane.set`, `night_light.set` and `stay_awake.set`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnabledSet {
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RecorderMode {
    /// Interactive area selection (slurp), as the QML "record" action.
    Region,
    /// The focused output.
    Screen,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecorderStart {
    pub mode: RecorderMode,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ScreenshotMode {
    /// Area selection copied to the clipboard (the control-centre tile).
    Region,
    /// The whole screen, saved and copied.
    Screen,
    /// The focused window, saved and copied.
    Window,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotCapture {
    pub mode: ScreenshotMode,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AppearanceMode {
    Light,
    Dark,
    /// Follow the wallpaper-derived automatic mode.
    Auto,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppearanceSet {
    pub mode: AppearanceMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct KeyboardLight {
    #[schemars(range(min = 0, max = 65535))]
    pub level: u32,
    #[schemars(range(min = 1, max = 65535))]
    pub max: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Toggle {
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NightLight {
    pub enabled: bool,
    /// Another controller owns night light; `night_light.set` is refused.
    pub external: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Recorder {
    pub recording: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Appearance {
    pub mode: AppearanceMode,
}
/// The `controls` topic. A member is present only while its backend is. `brightness` maps an
/// output name to a percent for the outputs whose level is known (internal panel; DDC outputs are
/// read when a client subscribes and after `brightness.set`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ControlsState {
    pub health: Health,
    pub brightness: BTreeMap<String, u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<KeyboardLight>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airplane: Option<Toggle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub night_light: Option<NightLight>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stay_awake: Option<Toggle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorder: Option<Recorder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
}

fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid control action", false).dispatch_state(false)
}
fn node_name(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-' | b'@' | b'+')
        })
}
/// `sink-input:<n>` or `source-output:<n>`: the only stream ids the daemon publishes.
pub fn stream_id(value: &str) -> Option<(&'static str, u32)> {
    let (kind, number) = value.split_once(':')?;
    let kind = ["sink-input", "source-output"]
        .into_iter()
        .find(|known| *known == kind)?;
    if !(1..=10).contains(&number.len()) || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok().map(|n| (kind, n))
}
fn target_valid(target: AudioTarget, node: &Option<String>) -> bool {
    match (target, node) {
        (AudioTarget::Node, Some(name)) => node_name(name),
        (AudioTarget::Stream, Some(id)) => stream_id(id).is_some(),
        (AudioTarget::Node | AudioTarget::Stream, None) | (_, Some(_)) => false,
        (_, None) => true,
    }
}
impl AudioSetVolume {
    pub fn validate(&self) -> Result<(), Failure> {
        if target_valid(self.target, &self.node)
            && self.value.is_finite()
            && (0.0..=1.0).contains(&self.value)
        {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl AudioSetDefault {
    pub fn validate(&self) -> Result<(), Failure> {
        node_name(&self.node).then_some(()).ok_or_else(invalid)
    }
}
impl AudioSetMute {
    pub fn validate(&self) -> Result<(), Failure> {
        if target_valid(self.target, &self.node) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl KeyboardLightSet {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.level <= 65535 {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl ControlsState {
    pub fn validate(&self) -> Result<(), Failure> {
        let outputs = self.brightness.len() <= MAX_OUTPUTS
            && self.brightness.iter().all(|(name, percent)| {
                (1..=128).contains(&name.len())
                    && !name.chars().any(char::is_control)
                    && *percent <= 100
            });
        let keyboard = self
            .keyboard
            .as_ref()
            .is_none_or(|k| k.max >= 1 && k.level <= k.max);
        if outputs && keyboard {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}

/// Name, params schema, capability class and default deadline of each method.
fn table() -> Vec<(&'static str, serde_json::Value, Capability, u32)> {
    let schema = |value: schemars::Schema| serde_json::to_value(value).expect("control schema");
    vec![
        (
            "audio.set_volume",
            schema(schemars::schema_for!(AudioSetVolume)),
            Capability::System,
            10_000,
        ),
        (
            "audio.set_mute",
            schema(schemars::schema_for!(AudioSetMute)),
            Capability::System,
            10_000,
        ),
        (
            "audio.set_default",
            schema(schemars::schema_for!(AudioSetDefault)),
            Capability::System,
            10_000,
        ),
        (
            "keyboard_light.set",
            schema(schemars::schema_for!(KeyboardLightSet)),
            Capability::System,
            30_000,
        ),
        (
            "power.set_profile",
            schema(schemars::schema_for!(PowerSetProfile)),
            Capability::System,
            10_000,
        ),
        (
            "airplane.set",
            schema(schemars::schema_for!(EnabledSet)),
            Capability::System,
            15_000,
        ),
        (
            "night_light.set",
            schema(schemars::schema_for!(EnabledSet)),
            Capability::System,
            30_000,
        ),
        (
            "stay_awake.set",
            schema(schemars::schema_for!(EnabledSet)),
            Capability::Desktop,
            15_000,
        ),
        (
            "recorder.start",
            schema(schemars::schema_for!(RecorderStart)),
            Capability::Desktop,
            10_000,
        ),
        (
            "recorder.stop",
            schema(schemars::schema_for!(PillEmpty)),
            Capability::Desktop,
            30_000,
        ),
        (
            "screenshot.capture",
            schema(schemars::schema_for!(ScreenshotCapture)),
            Capability::Desktop,
            10_000,
        ),
        (
            "color_picker.pick",
            schema(schemars::schema_for!(PillEmpty)),
            Capability::Desktop,
            10_000,
        ),
        (
            "wallpaper.next",
            schema(schemars::schema_for!(PillEmpty)),
            Capability::Desktop,
            120_000,
        ),
        (
            "wallpaper.previous",
            schema(schemars::schema_for!(PillEmpty)),
            Capability::Desktop,
            120_000,
        ),
        (
            "appearance.set_mode",
            schema(schemars::schema_for!(AppearanceSet)),
            Capability::Desktop,
            120_000,
        ),
    ]
}

fn result_schema(method: &str) -> serde_json::Value {
    let (properties, required) = if method == "keyboard_light.set" {
        (
            json!({"changed":{"type":"boolean"},"level":{"type":"integer","minimum":0},"max":{"type":"integer","minimum":1}}),
            json!(["changed", "level", "max"]),
        )
    } else {
        (json!({"changed":{"type":"boolean"}}), json!(["changed"]))
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

/// Every method of this unit; the daemon advertises the subset whose backend is present.
pub fn methods() -> BTreeMap<String, Method> {
    table()
        .into_iter()
        .map(|(name, params, class, timeout)| {
            (
                name.into(),
                Method {
                    v: 1,
                    class,
                    timeout,
                    params,
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
            "AudioTarget",
            "audio-target",
            schemars::schema_for!(AudioTarget),
        ),
        (
            "AudioSetVolume",
            "audio-set-volume",
            schemars::schema_for!(AudioSetVolume),
        ),
        (
            "AudioSetMute",
            "audio-set-mute",
            schemars::schema_for!(AudioSetMute),
        ),
        (
            "AudioSetDefault",
            "audio-set-default",
            schemars::schema_for!(AudioSetDefault),
        ),
        (
            "KeyboardLightSet",
            "keyboard-light-set",
            schemars::schema_for!(KeyboardLightSet),
        ),
        (
            "PowerSetProfile",
            "power-set-profile",
            schemars::schema_for!(PowerSetProfile),
        ),
        (
            "EnabledSet",
            "enabled-set",
            schemars::schema_for!(EnabledSet),
        ),
        (
            "RecorderStart",
            "recorder-start",
            schemars::schema_for!(RecorderStart),
        ),
        (
            "ScreenshotCapture",
            "screenshot-capture",
            schemars::schema_for!(ScreenshotCapture),
        ),
        (
            "AppearanceSet",
            "appearance-set",
            schemars::schema_for!(AppearanceSet),
        ),
        (
            "ControlsState",
            "controls-state",
            schemars::schema_for!(ControlsState),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("control schema");
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
    files.insert("protocol/luau/control-center.d.luau".into(), types);
    let calls = [
        (
            "audio.set_volume",
            json!({"target":"sink","value":0.25}),
            json!({"changed":true}),
        ),
        (
            "audio.set_volume",
            json!({"target":"source","value":0.5}),
            json!({"changed":true}),
        ),
        (
            "audio.set_mute",
            json!({"target":"node","node":"alsa_output.pci-0000_00_1f.3.analog-stereo","muted":true}),
            json!({"changed":true}),
        ),
        (
            "audio.set_default",
            json!({"node":"alsa_output.pci-0000_00_1f.3.analog-stereo"}),
            json!({"changed":true}),
        ),
        (
            "keyboard_light.set",
            json!({"level":2}),
            json!({"changed":true,"level":2,"max":3}),
        ),
        (
            "power.set_profile",
            json!({"profile":"power-saver"}),
            json!({"changed":true}),
        ),
        (
            "airplane.set",
            json!({"enabled":true}),
            json!({"changed":true}),
        ),
        (
            "night_light.set",
            json!({"enabled":true}),
            json!({"changed":true}),
        ),
        (
            "stay_awake.set",
            json!({"enabled":false}),
            json!({"changed":true}),
        ),
        (
            "recorder.start",
            json!({"mode":"region"}),
            json!({"changed":true}),
        ),
        ("recorder.stop", json!({}), json!({"changed":true})),
        (
            "screenshot.capture",
            json!({"mode":"window"}),
            json!({"changed":true}),
        ),
        ("color_picker.pick", json!({}), json!({"changed":true})),
        ("wallpaper.next", json!({}), json!({"changed":true})),
        ("wallpaper.previous", json!({}), json!({"changed":true})),
        (
            "appearance.set_mode",
            json!({"mode":"auto"}),
            json!({"changed":true}),
        ),
    ];
    let mut frames = vec![
        json!({"t":"snap","topic":TOPIC,"seq":0,"data":{"health":{"available":true,"stale":false},"brightness":{"eDP-1":60},"keyboard":{"level":1,"max":3},"airplane":{"enabled":false},"night_light":{"enabled":false,"external":false},"stay_awake":{"enabled":false},"recorder":{"recording":false},"appearance":{"mode":"dark"}}}),
        json!({"t":"patch","topic":TOPIC,"base":0,"seq":1,"data":{"brightness":{"eDP-1":65},"recorder":{"recording":true},"night_light":{"enabled":true}}}),
    ];
    for (index, (method, params, ok)) in calls.into_iter().enumerate() {
        let id = 500 + index as u64;
        frames.push(json!({"t":"call","id":id,"m":method,"p":params}));
        frames.push(json!({"t":"ret","id":id,"ok":ok}));
    }
    files.insert(
        "protocol/fixtures/control-center.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("control transcript") + "\n")
            .collect(),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_targets_require_a_node_name_only_for_node() {
        let volume = |target, node: Option<&str>, value| AudioSetVolume {
            target,
            node: node.map(Into::into),
            value,
        };
        assert!(volume(AudioTarget::Sink, None, 0.5).validate().is_ok());
        assert!(volume(AudioTarget::Source, None, 1.0).validate().is_ok());
        assert!(
            volume(AudioTarget::Node, Some("alsa_output.pci-0000:00:1f.3"), 0.0)
                .validate()
                .is_ok()
        );
        for bad in [
            volume(AudioTarget::Node, None, 0.5),
            volume(AudioTarget::Sink, Some("x"), 0.5),
            volume(AudioTarget::Node, Some("bad name"), 0.5),
            volume(AudioTarget::Node, Some("-x; rm"), 0.5),
            volume(AudioTarget::Node, Some(&"n".repeat(129)), 0.5),
            volume(AudioTarget::Sink, None, 1.01),
            volume(AudioTarget::Sink, None, -0.01),
            volume(AudioTarget::Sink, None, f64::NAN),
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
        assert!(AudioSetMute {
            target: AudioTarget::Node,
            node: None,
            muted: true
        }
        .validate()
        .is_err());
    }
    #[test]
    fn stream_targets_take_only_daemon_stream_ids() {
        let volume = |node: Option<&str>| AudioSetVolume {
            target: AudioTarget::Stream,
            node: node.map(Into::into),
            value: 0.5,
        };
        for good in ["sink-input:7", "source-output:4294967295", "sink-input:0"] {
            assert!(volume(Some(good)).validate().is_ok(), "{good}");
        }
        for bad in [
            None,
            Some("sink-input:"),
            Some("sink-input:-1"),
            Some("sink-input:1 2"),
            Some("sink-input:12345678901"),
            Some("sink-input:99999999999"),
            Some("sink:3"),
            Some("alsa_output.x"),
            Some("sink-input:7;rm"),
        ] {
            assert!(volume(bad).validate().is_err(), "{bad:?}");
        }
        assert_eq!(stream_id("source-output:12"), Some(("source-output", 12)));
    }
    #[test]
    fn requests_reject_unknown_fields_and_names_are_stable() {
        assert!(serde_json::from_value::<EnabledSet>(json!({"enabled":true,"unit":"x"})).is_err());
        assert!(serde_json::from_value::<PowerSetProfile>(json!({"profile":"turbo"})).is_err());
        assert_eq!(
            serde_json::to_value(PowerSetProfile {
                profile: PowerProfileName::PowerSaver
            })
            .unwrap(),
            json!({"profile":"power-saver"})
        );
        assert!(serde_json::from_value::<ScreenshotCapture>(json!({"mode":"area"})).is_err());
        assert!(serde_json::from_value::<AppearanceSet>(json!({"mode":"auto"})).is_ok());
        let methods = methods();
        assert_eq!(methods.len(), 15);
        assert_eq!(
            methods.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "airplane.set",
                "appearance.set_mode",
                "audio.set_default",
                "audio.set_mute",
                "audio.set_volume",
                "color_picker.pick",
                "keyboard_light.set",
                "night_light.set",
                "power.set_profile",
                "recorder.start",
                "recorder.stop",
                "screenshot.capture",
                "stay_awake.set",
                "wallpaper.next",
                "wallpaper.previous",
            ]
        );
        assert!(AudioSetDefault {
            node: "alsa_output.pci-0000_00_1f.3.analog-stereo".into()
        }
        .validate()
        .is_ok());
        assert!(AudioSetDefault {
            node: "bad name".into()
        }
        .validate()
        .is_err());
        assert!(
            serde_json::from_value::<AudioSetDefault>(json!({"node":"x","target":"sink"})).is_err()
        );
        assert!(methods.values().all(|m| m.v == 1 && m.timeout <= 120_000));
    }
    #[test]
    fn state_bounds() {
        let mut state = ControlsState {
            health: Health {
                available: true,
                stale: false,
                error: None,
            },
            brightness: BTreeMap::from([("eDP-1".into(), 100)]),
            keyboard: Some(KeyboardLight { level: 3, max: 3 }),
            airplane: None,
            night_light: None,
            stay_awake: None,
            recorder: None,
            appearance: None,
        };
        assert!(state.validate().is_ok());
        state.keyboard = Some(KeyboardLight { level: 4, max: 3 });
        assert!(state.validate().is_err());
        state.keyboard = None;
        state.brightness.insert("DP-1".into(), 101);
        assert!(state.validate().is_err());
    }
    #[test]
    fn exported_transcript_state_is_valid() {
        let files = exports();
        let first = files["protocol/fixtures/control-center.jsonl"]
            .lines()
            .next()
            .unwrap();
        let frame: serde_json::Value = serde_json::from_str(first).unwrap();
        let state: ControlsState = serde_json::from_value(frame["data"].clone()).unwrap();
        state.validate().unwrap();
    }
}
