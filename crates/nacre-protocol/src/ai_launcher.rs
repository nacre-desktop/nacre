//! Nacre AI launcher requests and bounded response shapes.
use crate::{Capability, Failure, Method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_CHATS: usize = 30;
const MAX_PROJECTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Open {
    #[schemars(length(min = 20, max = 20), regex(pattern = r"^chat[0-9a-f]{16}$"))]
    pub key: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct New {
    pub provider: crate::ai_chat::Provider,
    #[schemars(length(min = 1, max = 128))]
    pub project: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub action: LauncherAction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LauncherAction {
    Brain,
    Settings,
    Projects,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Ack {
    pub ok: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenAck {
    pub ok: bool,
    pub focused: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chat {
    #[schemars(length(min = 1, max = 64))]
    pub key: String,
    #[schemars(length(max = 256))]
    pub title: String,
    #[schemars(length(max = 128))]
    pub project: String,
    pub provider: crate::ai_chat::Provider,
    #[schemars(length(max = 32))]
    pub state: String,
    pub last_activity: f64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Project {
    #[schemars(length(min = 1, max = 128))]
    pub id: String,
    #[schemars(length(max = 128))]
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct List {
    #[schemars(length(max = 30))]
    pub chats: Vec<Chat>,
    #[schemars(length(max = 64))]
    pub projects: Vec<Project>,
    pub default_provider: crate::ai_chat::Provider,
    #[schemars(length(max = 2048))]
    pub warning: String,
}
impl Open {
    pub fn validate(&self) -> Result<(), Failure> {
        (self.key.len() == 20
            && self.key.starts_with("chat")
            && self.key[4..]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        .then_some(())
        .ok_or_else(|| {
            Failure::new(
                crate::ErrorCode::Validation,
                "Invalid launcher chat key",
                false,
            )
        })
    }
}
impl New {
    pub fn validate(&self) -> Result<(), Failure> {
        (!self.project.is_empty()
            && self.project.len() <= 128
            && !self.project.chars().any(char::is_control))
        .then_some(())
        .ok_or_else(|| {
            Failure::new(
                crate::ErrorCode::Validation,
                "Invalid launcher project",
                false,
            )
        })
    }
}
impl List {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.chats.len() > MAX_CHATS
            || self.projects.len() > MAX_PROJECTS
            || self.warning.chars().count() > 2048
            || self.chats.iter().any(|chat| {
                chat.key.is_empty()
                    || chat.key.chars().count() > 64
                    || chat.key.chars().any(char::is_control)
                    || chat.title.chars().count() > 256
                    || chat.title.chars().any(char::is_control)
                    || chat.project.is_empty()
                    || chat.project.chars().count() > 128
                    || chat.project.chars().any(char::is_control)
                    || chat.state.chars().count() > 32
                    || chat.state.chars().any(char::is_control)
                    || !chat.last_activity.is_finite()
                    || chat.last_activity < 0.0
            })
            || self.projects.iter().any(|project| {
                project.id.is_empty()
                    || project.id.chars().count() > 128
                    || project.id.chars().any(char::is_control)
                    || project.label.chars().count() > 128
                    || project.label.chars().any(char::is_control)
            })
        {
            return Err(Failure::new(
                crate::ErrorCode::Validation,
                "Invalid launcher list",
                false,
            ));
        }
        Ok(())
    }
}
pub fn methods() -> BTreeMap<String, Method> {
    let schema = schemas();
    let methods = [
        ("ai.launcher.list", "empty", "list", 45_000),
        ("ai.launcher.open", "open", "open-ack", 10_000),
        ("ai.launcher.new", "new", "ack", 10_000),
        ("ai.launcher.latest", "empty", "ack", 45_000),
        ("ai.launcher.action", "action", "ack", 10_000),
    ];
    methods
        .into_iter()
        .map(|(name, params, result, timeout)| {
            (
                name.to_owned(),
                Method {
                    v: 1,
                    class: Capability::Desktop,
                    timeout,
                    params: schema[params].clone(),
                    result: schema[result].clone(),
                },
            )
        })
        .collect()
}
fn schemas() -> BTreeMap<&'static str, Value> {
    fn one<T: JsonSchema>() -> Value {
        serde_json::to_value(schemars::schema_for!(T)).expect("launcher schema")
    }
    BTreeMap::from([
        ("empty", one::<Empty>()),
        ("open", one::<Open>()),
        ("new", one::<New>()),
        ("action", one::<Action>()),
        ("ack", one::<Ack>()),
        ("open-ack", one::<OpenAck>()),
        ("list", one::<List>()),
    ])
}
pub(crate) fn exports() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let schema = schemas();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, value) in &schema {
        let file = format!("ai-launcher-{name}");
        out.insert(
            format!("protocol/schema/{file}.json"),
            serde_json::to_string_pretty(value).expect("schema") + "\n",
        );
    }
    types.push_str("export type AiLauncherProvider = \"codex\" | \"claude\"\nexport type AiLauncherAction = \"brain\" | \"settings\" | \"projects\"\nexport type AiLauncherOpen = { key: string }\nexport type AiLauncherNew = { provider: AiLauncherProvider, project: string }\nexport type AiLauncherActionRequest = { action: AiLauncherAction }\nexport type AiLauncherAck = { ok: boolean }\nexport type AiLauncherOpenAck = { ok: boolean, focused: boolean }\nexport type AiLauncherChat = { key: string, title: string, project: string, provider: AiLauncherProvider, state: string, lastActivity: number }\nexport type AiLauncherProject = { id: string, label: string }\nexport type AiLauncherList = { chats: {AiLauncherChat}, projects: {AiLauncherProject}, defaultProvider: AiLauncherProvider, warning: string }\nreturn {}\n");
    out.insert("protocol/luau/ai-launcher.d.luau".into(), types);
    let transcript = [
        json!({"t":"call","id":1,"m":"ai.launcher.list","p":{}}),
        json!({"t":"ret","id":1,"ok":{"chats":[],"projects":[],"defaultProvider":"codex","warning":""}}),
        json!({"t":"call","id":2,"m":"ai.launcher.open","p":{"key":"chat0000000000000001"}}),
        json!({"t":"ret","id":2,"ok":{"ok":true,"focused":false}}),
    ];
    out.insert(
        "protocol/fixtures/ai-launcher.jsonl".into(),
        transcript
            .iter()
            .map(|v| serde_json::to_string(v).expect("fixture") + "\n")
            .collect(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_and_list_schema_are_bounded() {
        let methods = methods();
        for name in [
            "ai.launcher.list",
            "ai.launcher.open",
            "ai.launcher.new",
            "ai.launcher.latest",
            "ai.launcher.action",
        ] {
            assert!(methods.contains_key(name));
        }
        assert_eq!(methods["ai.launcher.list"].timeout, 45_000);
        assert_eq!(schemas()["list"]["properties"]["chats"]["maxItems"], 30);
    }

    #[test]
    fn list_rejects_unbounded_or_invalid_metadata() {
        let list = List {
            chats: vec![Chat {
                key: "key".into(),
                title: "Title".into(),
                project: "Project".into(),
                provider: crate::ai_chat::Provider::Codex,
                state: "idle".into(),
                last_activity: 1.0,
            }],
            projects: vec![Project {
                id: "demo".into(),
                label: "Demo".into(),
            }],
            default_provider: crate::ai_chat::Provider::Codex,
            warning: String::new(),
        };
        assert!(list.validate().is_ok());
        let mut invalid = list;
        invalid.chats[0].last_activity = f64::NAN;
        assert!(invalid.validate().is_err());
        assert!(Open {
            key: "chat0000000000000001".into()
        }
        .validate()
        .is_ok());
        assert!(Open {
            key: "not-a-chat-key".into()
        }
        .validate()
        .is_err());
    }
}
