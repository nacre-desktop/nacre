//! Registered settings stores. Paths and privileged/provider writes are not RPC inputs.
use crate::{ErrorCode, Failure, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SettingsGet {
    pub store: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SettingsSet {
    pub store: String,
    #[schemars(length(min = 1, max = 16))]
    pub key: Vec<String>,
    pub value: Value,
    #[schemars(range(max = 9007199254740991_u64))]
    pub rev: u64,
}

/// Existing QML publishers only; unknown fields already in a store are retained.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SettingsPublish {
    #[schemars(regex(pattern = "^(control-center|dashboard-extras|battery-alerts)$"))]
    pub store: String,
    pub value: Value,
    #[schemars(range(max = 9007199254740991_u64))]
    pub rev: u64,
}

/// Existing helper-owned fields in six Settings stores, never arbitrary paths.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SettingsProviderWrite {
    #[schemars(regex(
        pattern = "^(colors|wallpaper-picker|dashboard|cli|apps|launcher-workflows)$"
    ))]
    pub store: String,
    pub value: Value,
    #[schemars(range(max = 9007199254740991_u64))]
    pub rev: u64,
}

impl SettingsProviderWrite {
    pub fn validate(&self) -> Result<(), Failure> {
        if ![
            "colors",
            "wallpaper-picker",
            "dashboard",
            "cli",
            "apps",
            "launcher-workflows",
        ]
        .contains(&self.store.as_str())
            || !self.value.is_object()
            || self.rev > MAX_SAFE_INTEGER
        {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid provider request",
                false,
            ));
        }
        Ok(())
    }
}

impl SettingsPublish {
    pub fn validate(&self) -> Result<(), Failure> {
        if !["control-center", "dashboard-extras", "battery-alerts"].contains(&self.store.as_str())
            || !self.value.is_object()
            || self.rev > MAX_SAFE_INTEGER
        {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid publisher request",
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DesktopSettingsCall {
    #[schemars(length(min = 1, max = 16))]
    pub arguments: Vec<String>,
    /// Required for mutations; state reads may omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SettingsReadError {
    /// Registry-relative filename, never an arbitrary path supplied by a caller.
    pub file: String,
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SettingsStore {
    pub store: String,
    pub rev: u64,
    pub exists: bool,
    pub data: Value,
    pub dirty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SettingsReadError>,
}

pub fn store_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

pub fn key_path(value: &[String]) -> bool {
    !value.is_empty()
        && value.len() <= 16
        && value.iter().all(|key| {
            !key.is_empty()
                && key.len() <= 128
                && !["__proto__", "constructor", "prototype"].contains(&key.as_str())
                && key
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
        })
}

impl SettingsSet {
    pub fn validate(&self) -> Result<(), Failure> {
        if !store_id(&self.store) || !key_path(&self.key) || self.rev > MAX_SAFE_INTEGER {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid settings request",
                false,
            ));
        }
        Ok(())
    }
}

impl DesktopSettingsCall {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.arguments.is_empty()
            || self.arguments.len() > 16
            || self
                .arguments
                .iter()
                .any(|arg| arg.len() > 65536 || arg.contains('\0'))
            || self.rev.is_some_and(|rev| rev > MAX_SAFE_INTEGER)
            || ![
                "state",
                "init",
                "set",
                "display",
                "display-edit",
                "confirm",
                "revert",
                "preset",
                "save-workflow",
            ]
            .contains(&self.arguments[0].as_str())
            || self.arguments[0] != "state" && self.rev.is_none()
        {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid desktop settings request",
                false,
            ));
        }
        Ok(())
    }
}

pub(crate) fn exports() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, file, schema) in [
        (
            "SettingsGet",
            "settings-get",
            schemars::schema_for!(SettingsGet),
        ),
        (
            "SettingsSet",
            "settings-set",
            schemars::schema_for!(SettingsSet),
        ),
        (
            "SettingsPublish",
            "settings-publish",
            schemars::schema_for!(SettingsPublish),
        ),
        (
            "SettingsProviderWrite",
            "settings-provider-write",
            schemars::schema_for!(SettingsProviderWrite),
        ),
        (
            "DesktopSettingsCall",
            "desktop-settings-call",
            schemars::schema_for!(DesktopSettingsCall),
        ),
        (
            "SettingsReadError",
            "settings-read-error",
            schemars::schema_for!(SettingsReadError),
        ),
        (
            "SettingsStore",
            "settings-store",
            schemars::schema_for!(SettingsStore),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("finite settings schema");
        out.insert(
            format!("protocol/schema/{file}.json"),
            serde_json::to_string_pretty(&schema).expect("settings schema") + "\n",
        );
        types.push_str(&format!(
            "export type {name} = {}\n",
            crate::artifacts::luau(&schema)
        ));
    }
    types.push_str("return {}\n");
    out.insert("protocol/luau/settings.d.luau".into(), types);
    let frames = [
        json!({"t":"call","id":180,"m":"settings.get","p":{"store":"desktop"}}),
        json!({"t":"ret","id":180,"ok":{"store":"desktop","rev":0,"exists":false,"data":{"dnd":false},"dirty":false}}),
        json!({"t":"snap","topic":"settings.store.desktop","seq":0,"data":{"store":"desktop","rev":0,"exists":false,"data":{"dnd":false},"dirty":false}}),
        json!({"t":"call","id":181,"m":"settings.set","p":{"store":"desktop","key":["dnd"],"value":true,"rev":0}}),
        json!({"t":"ret","id":181,"ok":{"store":"desktop","rev":1,"exists":false,"data":{"dnd":true},"dirty":true}}),
        json!({"t":"patch","topic":"settings.store.desktop","base":0,"seq":1,"data":{"rev":1,"data":{"dnd":true},"dirty":true}}),
        json!({"t":"patch","topic":"settings.store.desktop","base":1,"seq":2,"data":{"exists":true,"dirty":false}}),
        json!({"t":"call","id":182,"m":"settings.set","p":{"store":"desktop","key":["dnd"],"value":false,"rev":0}}),
        json!({"t":"ret","id":182,"err":{"code":"stale_revision","message":"Settings changed; refresh before writing","retry":true}}),
        json!({"t":"patch","topic":"settings.store.desktop","base":2,"seq":3,"data":{"error":{"file":"nacre/desktop.json","line":2,"message":"Invalid settings document"}}}),
    ];
    out.insert(
        "protocol/fixtures/settings.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("settings transcript") + "\n")
            .collect(),
    );
    out.insert("protocol/fixtures/settings-widgets.jsonl".into(), [
        json!({"t":"call","id":200,"m":"settings.get","p":{"store":"widgets"}}),
        json!({"t":"ret","id":200,"ok":{"store":"widgets","rev":0,"exists":false,"data":{"layout":null},"dirty":false}}),
        json!({"t":"snap","topic":"settings.store.widgets","seq":0,"data":{"store":"widgets","rev":0,"exists":false,"data":{"layout":null},"dirty":false}}),
        json!({"t":"call","id":201,"m":"settings.set","p":{"store":"widgets","key":["layout"],"value":{"version":1,"hosts":{}},"rev":0}}),
        json!({"t":"ret","id":201,"ok":{"store":"widgets","rev":1,"exists":false,"data":{"layout":{"version":1,"hosts":{}}},"dirty":true}}),
        json!({"t":"patch","topic":"settings.store.widgets","base":0,"seq":1,"data":{"rev":1,"data":{"layout":{"version":1,"hosts":{}}},"dirty":true}}),
        json!({"t":"call","id":202,"m":"settings.set","p":{"store":"widgets","key":["layout"],"value":null,"rev":1}}),
        json!({"t":"ret","id":202,"ok":{"store":"widgets","rev":2,"exists":false,"data":{"layout":null},"dirty":true}}),
        json!({"t":"patch","topic":"settings.store.widgets","base":1,"seq":2,"data":{"rev":2,"data":{"layout":null},"dirty":true}}),
        json!({"t":"call","id":203,"m":"settings.set","p":{"store":"widgets","key":["layout"],"value":{"version":1,"hosts":{}},"rev":0}}),
        json!({"t":"ret","id":203,"err":{"code":"stale_revision","message":"Settings changed; refresh before writing","retry":true}}),
    ].iter().map(|frame| serde_json::to_string(frame).expect("widgets transcript")+"\n").collect());
    out.insert("protocol/fixtures/settings-publish.jsonl".into(), [
        json!({"t":"call","id":190,"m":"settings.publish","p":{"store":"battery-alerts","value":{"version":1,"severity":2},"rev":0}}),
        json!({"t":"ret","id":190,"ok":{"store":"battery-alerts","rev":1,"exists":false,"data":{"version":1,"severity":2},"dirty":true}}),
        json!({"t":"snap","topic":"settings.store.battery-alerts","seq":0,"data":{"store":"battery-alerts","rev":1,"exists":false,"data":{"version":1,"severity":2},"dirty":true}}),
        json!({"t":"patch","topic":"settings.store.battery-alerts","base":0,"seq":1,"data":{"exists":true,"dirty":false}}),
    ].iter().map(|frame| serde_json::to_string(frame).expect("publisher transcript")+"\n").collect());
    out.insert("protocol/fixtures/settings-provider.jsonl".into(), [
        json!({"t":"call","id":191,"m":"settings.provider","p":{"store":"colors","value":{"version":1,"favorites":[]},"rev":0}}),
        json!({"t":"ret","id":191,"ok":{"store":"colors","rev":1,"exists":false,"data":{"version":1,"favorites":[]},"dirty":true}}),
        json!({"t":"snap","topic":"settings.store.colors","seq":0,"data":{"store":"colors","rev":1,"exists":false,"data":{"version":1,"favorites":[]},"dirty":true}}),
        json!({"t":"patch","topic":"settings.store.colors","base":0,"seq":1,"data":{"exists":true,"dirty":false}}),
    ].iter().map(|frame| serde_json::to_string(frame).expect("provider transcript")+"\n").collect());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_revisions_are_bounded_before_store_lookup() {
        let mut request = SettingsSet {
            store: "desktop".into(),
            key: vec!["dnd".into()],
            value: json!(true),
            rev: 0,
        };
        assert!(request.validate().is_ok());
        for key in [
            "__proto__",
            "constructor",
            "prototype",
            "../desktop",
            "a/b",
            "",
        ] {
            request.key = vec![key.into()];
            assert!(request.validate().is_err(), "{key}");
        }
        request.key = vec!["dnd".into()];
        request.store = "../desktop".into();
        assert!(request.validate().is_err());
        request.store = "desktop".into();
        request.rev = MAX_SAFE_INTEGER + 1;
        assert!(request.validate().is_err());
    }

    #[test]
    fn controller_mutations_require_a_revision_and_an_owned_action() {
        let mut request = DesktopSettingsCall {
            arguments: vec!["state".into()],
            rev: None,
        };
        assert!(request.validate().is_ok());
        request.arguments = vec!["set".into(), "dnd".into(), "true".into()];
        assert!(request.validate().is_err());
        request.rev = Some(0);
        assert!(request.validate().is_ok());
        request.arguments[0] = "exec".into();
        assert!(request.validate().is_err());
    }
}
