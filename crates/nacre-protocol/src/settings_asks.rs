//! N0051: the rest of what the Bead Settings pages asked for (B0063): the `shortcuts` topic,
//! `system.status`, `tools.launch`, `capabilities.refresh` and `launcher.clear_history`.
//!
//! Everything is additive and v1. The other asks live beside their siblings: `bluetooth.set_trusted`
//! (`core_services`), `audio.set_default` (`control_center`), `clipboard.clear_history`
//! (`dashboard_extras`), the `software-input` store (`settings.set`) and the `bluetooth` and
//! `battery` capability flags (`capabilities`).
use crate::{
    core_services::{Entities, Health},
    pill_controls::PillEmpty,
    Capability, ErrorCode, Failure, Method,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

pub const SHORTCUTS_TOPIC: &str = "shortcuts";
pub const MAX_SHORTCUTS: usize = 512;

/// One key binding as the compositor reports it, with the Nacre menu route it opens, if any.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Shortcut {
    /// `SUPER+SHIFT+S`
    #[schemars(length(min = 1, max = 64))]
    pub key: String,
    #[schemars(length(max = 256))]
    pub description: String,
    /// The key table the binding lives in; absent for the main one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 64))]
    pub submap: Option<String>,
    /// A Nacre menu route, such as `actions.emoji`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub menu: Option<String>,
}
/// The `shortcuts` topic: the bindings of the running compositor, `SUPER` ones first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShortcutsState {
    pub health: Health,
    pub items: Entities<Shortcut>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AurProvenance {
    /// `required` (every recipe is reviewed) or `native`.
    #[schemars(length(max = 32))]
    pub mode: String,
    /// Where the policy value comes from, e.g. `registered-profile-setting`.
    #[schemars(length(max = 64))]
    pub source: String,
    #[schemars(length(max = 64))]
    pub profile: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceSample {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 64))]
    pub label: Option<String>,
    #[schemars(range(min = 0.0))]
    pub cpu_core_percent: f64,
    #[schemars(range(min = 0.0))]
    pub memory_mib: f64,
}
/// The System page's read-only status: what the QML Maintenance page showed (`maintenance state`)
/// plus the update-cache age and the AUR review policy provenance. Strings are display text from
/// local state, bounded and without control characters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemStatus {
    #[schemars(length(max = 64))]
    pub checked: String,
    /// Installed release revision, if one is recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 64))]
    pub release: Option<String>,
    /// Unit name to its state (`active`, `inactive`, `failed`, `ready on demand`, `unavailable`).
    pub services: BTreeMap<String, String>,
    pub failed_services: Vec<String>,
    /// The compositor's configuration errors; empty when valid.
    #[schemars(length(max = 2048))]
    pub config_errors: String,
    /// Managed files whose content differs from what the installer wrote.
    pub drift: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 256))]
    pub brain_sync: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance: Option<ResourceSample>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub session_event: Option<String>,
    /// Age of the package update cache; absent when it was never written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates_age_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aur_review: Option<AurProvenance>,
}

/// A fixed program the Settings pages start. Nothing else about the command line is caller input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    /// The audio routing manager (`pavucontrol`).
    AudioManager,
    /// The network connection editor (`nm-connection-editor`).
    ConnectionEditor,
    /// `nacre setup` in a terminal: installs the optional AI and Brain add-ons.
    Setup,
    /// The AI workspace (accounts, usage and projects) in a terminal.
    AiWorkspace,
    /// The AUR review policy and where it comes from, in a terminal.
    AurReviewPolicy,
    /// Start `power-profiles-daemon` (polkit asks the session's agent).
    PowerService,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolsLaunch {
    pub tool: Tool,
}

pub const STATUS_ITEMS: usize = 256;
fn text_ok(value: &str, max: usize) -> bool {
    value.chars().count() <= max && !value.chars().any(char::is_control)
}
fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid settings request", false).dispatch_state(false)
}
impl Shortcut {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = (1..=64).contains(&self.key.chars().count())
            && text_ok(&self.key, 64)
            && text_ok(&self.description, 256)
            && self.submap.as_deref().is_none_or(|s| text_ok(s, 64))
            && self.menu.as_deref().is_none_or(|s| text_ok(s, 128));
        ok.then_some(()).ok_or_else(invalid)
    }
}
impl ShortcutsState {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = self.items.valid()
            && self.items.items.len() <= MAX_SHORTCUTS
            && self.items.items.values().all(|s| s.validate().is_ok());
        ok.then_some(()).ok_or_else(invalid)
    }
}
impl SystemStatus {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = text_ok(&self.checked, 64)
            && self.services.len() <= 32
            && self
                .services
                .iter()
                .all(|(name, state)| text_ok(name, 96) && text_ok(state, 48))
            && self.failed_services.len() <= 64
            && self.failed_services.iter().all(|s| text_ok(s, 256))
            && self.config_errors.chars().count() <= 2048
            && self.drift.len() <= STATUS_ITEMS
            && self.drift.iter().all(|s| text_ok(s, 512));
        ok.then_some(()).ok_or_else(invalid)
    }
}

/// Name, params schema, result schema, capability class and deadline.
fn table() -> Vec<(
    &'static str,
    serde_json::Value,
    serde_json::Value,
    Capability,
    u32,
)> {
    let schema = |value: schemars::Schema| serde_json::to_value(value).expect("settings schema");
    let flag = |name: &str| json!({"type":"object","properties":{name:{"type":"boolean"}},"required":[name],"additionalProperties":false});
    vec![
        (
            "shortcuts.refresh",
            schema(schemars::schema_for!(PillEmpty)),
            flag("requested"),
            Capability::Desktop,
            10_000,
        ),
        (
            "capabilities.refresh",
            schema(schemars::schema_for!(PillEmpty)),
            flag("requested"),
            Capability::Desktop,
            10_000,
        ),
        (
            "launcher.clear_history",
            schema(schemars::schema_for!(PillEmpty)),
            flag("changed"),
            Capability::Desktop,
            30_000,
        ),
        (
            "system.status",
            schema(schemars::schema_for!(PillEmpty)),
            schema(schemars::schema_for!(SystemStatus)),
            Capability::Desktop,
            30_000,
        ),
        (
            "tools.launch",
            schema(schemars::schema_for!(ToolsLaunch)),
            flag("launched"),
            Capability::Desktop,
            10_000,
        ),
    ]
}
/// Every method of this unit; the daemon advertises the subset whose backend is present.
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
            "ShortcutsState",
            "shortcuts-state",
            schemars::schema_for!(ShortcutsState),
        ),
        (
            "SystemStatus",
            "system-status",
            schemars::schema_for!(SystemStatus),
        ),
        (
            "ToolsLaunch",
            "tools-launch",
            schemars::schema_for!(ToolsLaunch),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("settings schema");
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
    files.insert("protocol/luau/settings-asks.d.luau".into(), types);
    let calls = [
        ("shortcuts.refresh", json!({}), json!({"requested":true})),
        ("capabilities.refresh", json!({}), json!({"requested":true})),
        ("launcher.clear_history", json!({}), json!({"changed":true})),
        (
            "tools.launch",
            json!({"tool":"audio-manager"}),
            json!({"launched":true}),
        ),
        (
            "system.status",
            json!({}),
            json!({"checked":"2026-10-23T18:00:00.000000+00:00","release":"0123456789ab","services":{"nacre-shell.service":"active","xdg-desktop-portal.service":"ready on demand"},"failed_services":[],"config_errors":"","drift":[],"brain_sync":"No synchronization attempt recorded","updates_age_seconds":3600,"aur_review":{"mode":"required","source":"general-profile-default","profile":"default"}}),
        ),
    ];
    let mut frames = vec![
        json!({"t":"snap","topic":SHORTCUTS_TOPIC,"seq":0,"data":{"health":{"available":true,"stale":false},"items":{"items":{"k0":{"key":"SUPER+SPACE","description":"Launcher"},"k1":{"key":"SUPER+PERIOD","description":"Emoji","menu":"actions.emoji"}},"order":["k0","k1"]}}}),
        json!({"t":"patch","topic":SHORTCUTS_TOPIC,"base":0,"seq":1,"data":{"items":{"items":{"k1":null},"order":["k0"]}}}),
    ];
    for (index, (method, params, ok)) in calls.into_iter().enumerate() {
        let id = 700 + index as u64;
        frames.push(json!({"t":"call","id":id,"m":method,"p":params}));
        frames.push(json!({"t":"ret","id":id,"ok":ok}));
    }
    files.insert(
        "protocol/fixtures/settings-asks.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("settings transcript") + "\n")
            .collect(),
    );
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn methods_are_stable_and_bounded() {
        let methods = methods();
        assert_eq!(methods.len(), 5);
        assert!(methods.values().all(|m| m.v == 1 && m.timeout <= 120_000));
    }
    #[test]
    fn tools_are_a_closed_set() {
        for name in [
            "audio-manager",
            "connection-editor",
            "setup",
            "ai-workspace",
            "aur-review-policy",
            "power-service",
        ] {
            assert!(serde_json::from_value::<ToolsLaunch>(json!({"tool":name})).is_ok());
        }
        for bad in [
            json!({"tool":"bash"}),
            json!({"tool":"setup","args":["-c"]}),
            json!({}),
        ] {
            assert!(serde_json::from_value::<ToolsLaunch>(bad).is_err());
        }
    }
    #[test]
    fn shortcuts_are_bounded_display_text() {
        let good = Shortcut {
            key: "SUPER+A".into(),
            description: "Open".into(),
            submap: None,
            menu: Some("actions.emoji".into()),
        };
        assert!(good.validate().is_ok());
        for bad in [
            Shortcut {
                key: String::new(),
                ..good.clone()
            },
            Shortcut {
                description: "a\nb".into(),
                ..good.clone()
            },
            Shortcut {
                key: "k".repeat(65),
                ..good.clone()
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }
}
