//! N0063: the `displays` topic and `display.apply|confirm|revert`, the contract behind the Bead
//! Settings Displays page. It carries what the QML `NacreDisplaysPage` showed (per output: size,
//! refresh, scale, available modes) plus the optional facts the compositors report (make, model,
//! position, transform, variable refresh).
//!
//! Every change is a transaction: `display.apply` applies it and the daemon reverts it by itself
//! after `CONFIRM_SECONDS` unless `display.confirm` arrives (`display.revert` reverts at once).
//! Everything is additive and v1.
use crate::{
    core_services::Health, pill_controls::PillEmpty, Capability, ErrorCode, Failure, Method,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const TOPIC: &str = "displays";
/// QML: "Display changes revert after 20 seconds unless you keep them."
pub const CONFIRM_SECONDS: u32 = 20;
/// The scales the Settings page offers (QML `NacreDisplaysPage`); anything else is refused.
pub const SCALES: [f64; 7] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
pub const MAX_OUTPUTS: usize = 16;
pub const MAX_MODES: usize = 256;

/// How the connected displays sit relative to the primary one (QML "Extend right/left, Mirror").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayArrangement {
    ExtendRight,
    ExtendLeft,
    Mirror,
}
/// One connected output. Sizes are physical pixels of the current mode, positions logical.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DisplayOutput {
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 64))]
    pub make: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 64))]
    pub model: Option<String>,
    pub enabled: bool,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
    /// The entry of `modes` that is the current mode, when the compositor lists it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 40))]
    pub mode: Option<String>,
    /// `WIDTHxHEIGHT@RATEHz`, the strings `display.apply` accepts.
    #[schemars(length(max = 256))]
    pub modes: Vec<String>,
    pub scale: f64,
    pub x: i32,
    pub y: i32,
    /// 0 normal, 1 to 3 rotated 90/180/270 degrees, 4 to 7 the flipped variants.
    #[schemars(range(max = 7))]
    pub transform: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vrr: Option<bool>,
}
/// A change awaiting `display.confirm`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DisplayPending {
    pub seconds: u32,
    /// Unix milliseconds at which the daemon's armed rollback restores the previous layout.
    pub deadline_ms: u64,
}
/// The `displays` topic. `outputs` lists the enabled outputs; while `health.available` is false
/// the last list is kept and `health.stale` says so.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DisplaysState {
    pub health: Health,
    /// False when the compositor cannot configure outputs (QML: `monitorConfiguration`).
    pub can_configure: bool,
    /// False when it cannot mirror them (QML: `monitorMirroring`; niri).
    pub can_mirror: bool,
    /// The scales `display.apply` accepts.
    #[schemars(length(max = 16))]
    pub scales: Vec<f64>,
    #[schemars(length(max = 16))]
    pub outputs: Vec<DisplayOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<DisplayPending>,
}
/// Either an arrangement (`arrangement`, with `output` the primary display) or an edit of one
/// output (`scale`, and optionally `mode`, with `output` the target). Exactly one of the two.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisplayApply {
    #[schemars(length(min = 1, max = 64))]
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrangement: Option<DisplayArrangement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 40))]
    pub mode: Option<String>,
}

fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid display request", false).dispatch_state(false)
}
/// `WIDTHxHEIGHT@RATEHz` with plain decimal numbers, as the compositors list modes.
pub fn mode_valid(mode: &str) -> bool {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    let Some((size, rate)) = mode.split_once('@') else {
        return false;
    };
    let Some((width, height)) = size.split_once('x') else {
        return false;
    };
    let Some(rate) = rate.strip_suffix("Hz") else {
        return false;
    };
    let rate_ok = rate.split_once('.').map_or_else(
        || digits(rate),
        |(whole, part)| digits(whole) && digits(part),
    );
    mode.len() <= 40 && digits(width) && digits(height) && rate_ok
}
pub fn output_name_valid(name: &str) -> bool {
    (1..=64).contains(&name.len()) && !name.chars().any(char::is_control)
}
impl DisplayApply {
    pub fn validate(&self) -> Result<(), Failure> {
        let shape = match (self.arrangement, self.scale) {
            (Some(_), None) => self.mode.is_none(),
            (None, Some(scale)) => SCALES.contains(&scale),
            _ => false,
        };
        (output_name_valid(&self.output) && shape && self.mode.as_deref().is_none_or(mode_valid))
            .then_some(())
            .ok_or_else(invalid)
    }
}
impl DisplaysState {
    pub fn validate(&self) -> Result<(), Failure> {
        let output = |o: &DisplayOutput| {
            output_name_valid(&o.name)
                && o.transform <= 7
                && o.scale.is_finite()
                && o.refresh_hz.is_finite()
                && o.modes.len() <= MAX_MODES
                && o.modes.iter().all(|m| m.len() <= 40)
        };
        (self.outputs.len() <= MAX_OUTPUTS && self.outputs.iter().all(output))
            .then_some(())
            .ok_or_else(invalid)
    }
}

fn table() -> Vec<(
    &'static str,
    serde_json::Value,
    serde_json::Value,
    Capability,
    u32,
)> {
    let schema = |value: schemars::Schema| serde_json::to_value(value).expect("display schema");
    let pending = schema(schemars::schema_for!(DisplayPending));
    let changed = json!({"type":"object","properties":{"changed":{"type":"boolean"}},"required":["changed"],"additionalProperties":false});
    vec![
        (
            "display.apply",
            schema(schemars::schema_for!(DisplayApply)),
            json!({"type":"object","properties":{"pending":pending},"required":["pending"],"additionalProperties":false}),
            Capability::Desktop,
            30_000,
        ),
        (
            "display.confirm",
            schema(schemars::schema_for!(PillEmpty)),
            changed.clone(),
            Capability::Desktop,
            30_000,
        ),
        (
            "display.revert",
            schema(schemars::schema_for!(PillEmpty)),
            changed,
            Capability::Desktop,
            30_000,
        ),
    ]
}
/// Every method of this unit; the daemon advertises them while the compositor can configure
/// outputs.
pub fn methods() -> BTreeMap<String, Method> {
    table()
        .into_iter()
        .map(|(name, params, result, class, timeout)| {
            (
                name.into(),
                Method {
                    v: 1,
                    class,
                    timeout,
                    params,
                    result,
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
            "DisplaysState",
            "displays-state",
            schemars::schema_for!(DisplaysState),
        ),
        (
            "DisplayApply",
            "display-apply",
            schemars::schema_for!(DisplayApply),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("display schema");
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
    files.insert("protocol/luau/displays.d.luau".into(), types);
    let output = |name: &str, make: &str, model: &str, x: i64, mode: &str| json!({"name":name,"make":make,"model":model,"enabled":true,"width":1920,"height":1080,"refresh_hz":60.0,"mode":mode,"modes":["1920x1080@60.000Hz","1920x1080@144.000Hz","1280x720@60.000Hz"],"scale":1.0,"x":x,"y":0,"transform":0,"vrr":false});
    let calls = [
        (
            "display.apply",
            json!({"output":"DP-1","arrangement":"extend-right"}),
            json!({"pending":{"seconds":20,"deadline_ms":1_792_000_020_000_u64}}),
        ),
        (
            "display.apply",
            json!({"output":"DP-2","scale":1.5,"mode":"1920x1080@144.000Hz"}),
            json!({"pending":{"seconds":20,"deadline_ms":1_792_000_050_000_u64}}),
        ),
        ("display.confirm", json!({}), json!({"changed":true})),
        ("display.revert", json!({}), json!({"changed":false})),
    ];
    let mut frames = vec![
        json!({"t":"snap","topic":TOPIC,"seq":0,"data":{"health":{"available":true,"stale":false},"can_configure":true,"can_mirror":false,"scales":SCALES,"outputs":[output("DP-1","Dell","U2415",0,"1920x1080@60.000Hz"),output("DP-2","LG","27GL850",1920,"1920x1080@60.000Hz")]}}),
        json!({"t":"patch","topic":TOPIC,"base":0,"seq":1,"data":{"pending":{"seconds":20,"deadline_ms":1_792_000_020_000_u64}}}),
        json!({"t":"patch","topic":TOPIC,"base":1,"seq":2,"data":{"pending":null}}),
    ];
    for (index, (method, params, ok)) in calls.into_iter().enumerate() {
        let id = 700 + index as u64;
        frames.push(json!({"t":"call","id":id,"m":method,"p":params}));
        frames.push(json!({"t":"ret","id":id,"ok":ok}));
    }
    files.insert(
        "protocol/fixtures/displays.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("display transcript") + "\n")
            .collect(),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    fn apply(value: serde_json::Value) -> Result<(), Failure> {
        serde_json::from_value::<DisplayApply>(value)
            .map_err(|_| invalid())?
            .validate()
    }
    #[test]
    fn an_apply_is_exactly_an_arrangement_or_an_edit() {
        assert!(apply(json!({"output":"DP-1","arrangement":"mirror"})).is_ok());
        assert!(apply(json!({"output":"DP-1","scale":1.25})).is_ok());
        assert!(apply(json!({"output":"DP-1","scale":2,"mode":"3840x2160@59.940Hz"})).is_ok());
        for bad in [
            json!({"output":"DP-1"}),
            json!({"output":"DP-1","arrangement":"mirror","scale":1}),
            json!({"output":"DP-1","arrangement":"mirror","mode":"1x1@1Hz"}),
            json!({"output":"DP-1","arrangement":"stack"}),
            json!({"output":"DP-1","scale":1.1}),
            json!({"output":"","scale":1}),
            json!({"output":"DP\n1","scale":1}),
            json!({"output":"DP-1","scale":1,"mode":"1920x1080"}),
            json!({"output":"DP-1","scale":1,"mode":"1920x1080@60"}),
            json!({"output":"DP-1","scale":1,"mode":"1920x1080@60.Hz"}),
            json!({"output":"DP-1","scale":1,"extra":true}),
        ] {
            assert!(apply(bad.clone()).is_err(), "{bad}");
        }
    }
    #[test]
    fn methods_are_stable() {
        let methods = methods();
        assert_eq!(methods.len(), 3);
        assert!(methods
            .values()
            .all(|m| m.v == 1 && m.timeout == 30_000 && m.class == Capability::Desktop));
    }
}
