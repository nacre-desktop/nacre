//! Semantic shell actions shared by Nacre and other desktop shells.
use crate::{Capability, ErrorCode, Failure, Method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceActivate {
    #[schemars(length(min = 1, max = 128))]
    pub output: String,
    pub workspace: WorkspaceTarget,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum WorkspaceTarget {
    Opaque(#[schemars(length(min = 1, max = 128))] String),
    Index(#[schemars(range(min = 1, max = 255))] u16),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceDirection {
    Next,
    Prev,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceScroll {
    #[schemars(length(min = 1, max = 128))]
    pub output: String,
    pub direction: WorkspaceDirection,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum CollectionOperation {
    Activate {
        #[schemars(length(min = 17, max = 40))]
        collection: String,
    },
    Create {
        #[serde(default)]
        #[schemars(length(max = 64))]
        name: Option<String>,
    },
    Rename {
        #[schemars(length(min = 17, max = 40))]
        collection: String,
        #[serde(default)]
        #[schemars(length(max = 64))]
        name: Option<String>,
    },
    SetWallpaper {
        #[schemars(length(min = 17, max = 40))]
        collection: String,
    },
    SetAccent {
        #[schemars(length(min = 17, max = 40))]
        collection: String,
        enabled: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CollectionAction {
    #[schemars(length(min = 1, max = 128))]
    pub output: String,
    pub action: CollectionOperation,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowFocus {
    /// Optional guard: when present the window must be on this output. Absent
    /// means output-agnostic (the window's own output is used).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub output: Option<String>,
    #[schemars(length(min = 1, max = 128))]
    pub window: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionAction {
    Lock,
    Logout,
    Suspend,
    Reboot,
    Shutdown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    pub action: SessionAction,
    #[serde(default)]
    pub confirmed: bool,
}

impl WorkspaceActivate {
    pub fn validate(&self) -> Result<(), Failure> {
        if !valid_id(&self.output)
            || match &self.workspace {
                WorkspaceTarget::Opaque(value) => !valid_id(value),
                WorkspaceTarget::Index(index) => !(1..=255).contains(index),
            }
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl WorkspaceScroll {
    pub fn validate(&self) -> Result<(), Failure> {
        if !valid_id(&self.output) {
            return Err(invalid());
        }
        Ok(())
    }
}
impl CollectionAction {
    pub fn validate(&self) -> Result<(), Failure> {
        let valid_collection = |value: &str| {
            value.strip_prefix("niri-collection:").is_some_and(|id| {
                !id.is_empty()
                    && id.bytes().all(|byte| byte.is_ascii_digit())
                    && id.parse::<u64>().is_ok()
            })
        };
        let valid_name = |name: &Option<String>| {
            name.as_ref().is_none_or(|name| {
                let trimmed = name.trim();
                !trimmed.is_empty() && trimmed.len() <= 64 && !trimmed.chars().any(char::is_control)
            })
        };
        if !valid_id(&self.output)
            || match &self.action {
                CollectionOperation::Activate { collection } => !valid_collection(collection),
                CollectionOperation::Create { name } => !valid_name(name),
                CollectionOperation::Rename { collection, name } => {
                    !valid_collection(collection) || !valid_name(name)
                }
                CollectionOperation::SetWallpaper { collection } => !valid_collection(collection),
                CollectionOperation::SetAccent { collection, .. } => !valid_collection(collection),
            }
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl WindowFocus {
    pub fn validate(&self) -> Result<(), Failure> {
        if self
            .output
            .as_deref()
            .is_some_and(|output| !valid_id(output))
            || !valid_id(&self.window)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl SessionRequest {
    pub fn validate(&self) -> Result<(), Failure> {
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid shell action", false).dispatch_state(false)
}
pub fn methods() -> BTreeMap<String, Method> {
    [
        ("workspace.activate", schemars::schema_for!(WorkspaceActivate)),
        ("workspace.scroll", schemars::schema_for!(WorkspaceScroll)),
        ("collection.action", schemars::schema_for!(CollectionAction)),
        ("window.focus", schemars::schema_for!(WindowFocus)),
        ("session.request", schemars::schema_for!(SessionRequest)),
    ].into_iter().map(|(name, schema)| {
        (name.into(), Method {
            v: 1,
            class: if name == "session.request" { Capability::System } else { Capability::Desktop },
            timeout: 10_000,
            params: serde_json::to_value(schema).expect("shell action schema"),
            result: json!({"type":"object","properties":{"changed":{"type":"boolean"}},"required":["changed"],"additionalProperties":false}),
        })
    }).collect()
}
pub fn success(changed: bool) -> Value {
    json!({"changed":changed})
}

pub(crate) fn exports() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    let mut definitions = BTreeMap::new();
    for (name, file, schema) in [
        (
            "WorkspaceActivate",
            "workspace-activate",
            schemars::schema_for!(WorkspaceActivate),
        ),
        (
            "WorkspaceScroll",
            "workspace-scroll",
            schemars::schema_for!(WorkspaceScroll),
        ),
        (
            "CollectionAction",
            "collection-action",
            schemars::schema_for!(CollectionAction),
        ),
        (
            "WindowFocus",
            "window-focus",
            schemars::schema_for!(WindowFocus),
        ),
        (
            "SessionRequest",
            "session-request",
            schemars::schema_for!(SessionRequest),
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("shell action schema");
        if let Some(defs) = schema.get("$defs").and_then(Value::as_object) {
            for (key, definition) in defs {
                if let Some(previous) = definitions.insert(key.clone(), definition.clone()) {
                    assert_eq!(previous, *definition, "Inconsistent shared type {key}");
                }
            }
        }
        types.push_str(&format!(
            "export type {name} = {}\n",
            crate::artifacts::luau(&schema)
        ));
        out.insert(
            format!("protocol/schema/{file}.json"),
            serde_json::to_string_pretty(&schema).expect("schema JSON") + "\n",
        );
    }
    for (name, definition) in definitions {
        types.push_str(&format!(
            "export type {name} = {}\n",
            crate::artifacts::luau(&definition)
        ));
    }
    types.push_str("return {}\n");
    out.insert("protocol/luau/shell-actions.d.luau".into(), types);
    let frames = [
        json!({"t":"call","id":301,"m":"workspace.activate","p":{"output":"DP-1","workspace":"niri-workspace:42"}}),
        json!({"t":"ret","id":301,"ok":{"changed":true}}),
        json!({"t":"call","id":307,"m":"collection.action","p":{"output":"DP-1","action":{"operation":"create","name":"work"}}}),
        json!({"t":"ret","id":307,"ok":{"changed":true}}),
        json!({"t":"call","id":305,"m":"workspace.activate","p":{"output":"DP-2","workspace":6}}),
        json!({"t":"ret","id":305,"ok":{"changed":true}}),
        json!({"t":"call","id":302,"m":"workspace.scroll","p":{"output":"DP-2","direction":"prev"}}),
        json!({"t":"ret","id":302,"ok":{"changed":true}}),
        json!({"t":"call","id":303,"m":"window.focus","p":{"output":"DP-1","window":"address:0x1234"}}),
        json!({"t":"ret","id":303,"ok":{"changed":true}}),
        json!({"t":"call","id":306,"m":"window.focus","p":{"window":"address:0x1234"}}),
        json!({"t":"ret","id":306,"ok":{"changed":true}}),
        json!({"t":"call","id":304,"m":"session.request","p":{"action":"shutdown","confirmed":true}}),
        json!({"t":"ret","id":304,"ok":{"changed":true}}),
    ];
    out.insert(
        "protocol/fixtures/shell-actions.jsonl".into(),
        frames
            .iter()
            .map(|frame| serde_json::to_string(frame).expect("shell action transcript") + "\n")
            .collect(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_action_inputs_are_bounded() {
        assert!(WorkspaceActivate {
            output: "DP-1".into(),
            workspace: WorkspaceTarget::Opaque("niri-workspace:1".into())
        }
        .validate()
        .is_ok());
        assert!(WorkspaceActivate {
            output: "x\n".into(),
            workspace: WorkspaceTarget::Opaque("1".into())
        }
        .validate()
        .is_err());
        for index in [0, 256] {
            let request = serde_json::from_value::<WorkspaceActivate>(
                json!({"output":"DP-1","workspace":index}),
            )
            .unwrap();
            assert!(request.validate().is_err());
        }
        assert!(WindowFocus {
            output: Some("DP-1".into()),
            window: "x".repeat(129)
        }
        .validate()
        .is_err());
        let agnostic =
            serde_json::from_value::<WindowFocus>(json!({"window":"niri-window:7"})).unwrap();
        assert!(agnostic.output.is_none() && agnostic.validate().is_ok());
        assert!(CollectionAction {
            output: "DP-1".into(),
            action: CollectionOperation::Activate {
                collection: "niri-collection:42".into(),
            },
        }
        .validate()
        .is_ok());
        assert!(CollectionAction {
            output: "DP-1".into(),
            action: CollectionOperation::Rename {
                collection: "niri-collection:42".into(),
                name: Some("  ".into()),
            },
        }
        .validate()
        .is_err());
        assert_eq!(methods().len(), 5);
        assert!(serde_json::from_value::<WorkspaceActivate>(
            json!({"output":"DP-1","workspace":"1","command":"exec"})
        )
        .is_err());
        assert!(serde_json::from_value::<SessionRequest>(
            json!({"action":"logout","confirmed":true,"command":"exec"})
        )
        .is_err());
        let exports = exports();
        let types = &exports["protocol/luau/shell-actions.d.luau"];
        assert!(types.contains("export type WorkspaceDirection ="));
        assert!(types.contains("export type SessionAction ="));
        assert!(types.contains("export type WorkspaceTarget ="));
        assert!(types.contains("export type CollectionOperation ="));
    }
}
