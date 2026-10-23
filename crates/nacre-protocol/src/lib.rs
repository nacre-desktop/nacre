//! Private, versioned desktop link. No UI toolkit or global runtime ownership.
#![deny(unsafe_code)]
pub mod ai_chat;
pub mod artifacts;
pub mod bar_services;
pub mod compositor;
mod core_artifacts;
pub mod core_services;
pub mod dashboard_extras;
pub mod framing;
pub mod helpers;
pub mod i18n;
pub mod menu;
pub mod network;
pub mod notifications;
pub mod osd;
pub mod patch;
pub mod pill_controls;
pub mod settings;
pub mod shell_actions;
pub mod wallpaper;
pub mod widths;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u16 = 1;
/// Default deadline for calls dispatched to the shell UI provider.
/// Covers short UI stalls while remaining below the ten-second transport limit.
pub const UI_CALL_TIMEOUT_MS: u32 = 3_000;
pub const MIN_VERSION: u16 = 1;
pub const MAX_FRAME: usize = 1_048_576;
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const MAX_CLIENTS: usize = 64;
pub const MAX_SUBSCRIPTIONS: usize = 128;
pub const MAX_CALLS: usize = 64;
pub const MAX_EVENTS: usize = 256;

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(untagged)]
pub enum Id {
    Integer(#[schemars(range(min = -9007199254740991_i64, max = 9007199254740991_i64))] i64),
    Text(#[schemars(length(min = 1, max = 128))] String),
}
impl Id {
    pub fn valid(&self) -> bool {
        match self {
            Self::Integer(value) => value.unsigned_abs() <= MAX_SAFE_INTEGER,
            Self::Text(value) => !value.is_empty() && value.len() <= 128,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedVersion,
    TooLarge,
    PermissionDenied,
    UnknownTopic,
    UnknownMethod,
    Unavailable,
    Busy,
    Timeout,
    StaleRevision,
    Validation,
    Io,
    Internal,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
    pub retry: bool,
    /// False only when the daemon proves no UI provider received the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatched: Option<bool>,
}
impl Failure {
    pub fn new(code: ErrorCode, message: impl Into<String>, retry: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retry,
            dispatched: None,
        }
    }
    pub fn dispatch_state(mut self, value: bool) -> Self {
        self.dispatched = Some(value);
        self
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Reply {
    Ok { ok: Value },
    Err { err: Failure },
}
impl<'de> Deserialize<'de> for Reply {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = BTreeMap::<String, Value>::deserialize(deserializer)?;
        match (fields.get("ok"), fields.get("err")) {
            (Some(ok), None) => Ok(Self::Ok { ok: ok.clone() }),
            (None, Some(err)) => Ok(Self::Err {
                err: serde_json::from_value(err.clone()).map_err(serde::de::Error::custom)?,
            }),
            _ => Err(serde::de::Error::custom(
                "Reply requires exactly one of ok and err",
            )),
        }
    }
}
impl JsonSchema for Reply {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Reply".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"oneOf":[
            {"type":"object","properties":{"ok":true},"required":["ok"],"not":{"required":["err"]}},
            {"type":"object","properties":{"err":generator.subschema_for::<Failure>()},"required":["err"],"not":{"required":["ok"]}}
        ]})
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Method {
    pub v: u16,
    pub class: Capability,
    #[schemars(range(min = 1, max = 120000))]
    pub timeout: u32,
    pub params: Value,
    pub result: Value,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Ui,
    Desktop,
    System,
    Privileged,
}
/// The always-present methods; service-specific registries extend this map.
pub fn core_methods() -> BTreeMap<String, Method> {
    [
        ("daemon.status", Capability::Desktop),
        ("daemon.ping", Capability::Desktop),
        ("ui.dispatch", Capability::Ui),
    ]
    .into_iter()
    .map(|(name, class)| {
        (
            name.into(),
            Method {
                v: 1,
                class,
                timeout: if class == Capability::Ui {
                    UI_CALL_TIMEOUT_MS
                } else {
                    1000
                },
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        )
    })
    .collect()
}

fn parameters() -> Value {
    serde_json::json!({})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_ui_method_advertises_three_second_deadline() {
        assert_eq!(core_methods()["ui.dispatch"].timeout, UI_CALL_TIMEOUT_MS);
        assert_eq!(UI_CALL_TIMEOUT_MS, 3_000);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Message {
    Hello {
        proto: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<u16>,
        client: String,
    },
    Welcome {
        proto: u16,
        min: u16,
        boot: String,
        topics: BTreeMap<String, u16>,
        methods: BTreeMap<String, Method>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frame_ms: Option<u32>,
    },
    Sub {
        topics: Vec<String>,
    },
    Unsub {
        topics: Vec<String>,
    },
    Resync {
        topics: Vec<String>,
    },
    Snap {
        topic: String,
        #[schemars(range(min = 0, max = 9007199254740991_u64))]
        seq: u64,
        #[schemars(with = "BTreeMap<String, Value>")]
        data: Value,
    },
    Patch {
        topic: String,
        #[schemars(range(min = 0, max = 9007199254740991_u64))]
        base: u64,
        #[schemars(range(min = 0, max = 9007199254740991_u64))]
        seq: u64,
        #[schemars(with = "BTreeMap<String, Value>")]
        data: Value,
    },
    Call {
        id: Id,
        m: String,
        #[serde(default = "parameters")]
        #[schemars(with = "BTreeMap<String, Value>")]
        p: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1, max = 120000))]
        timeout: Option<u32>,
    },
    Ret {
        id: Id,
        #[serde(flatten)]
        reply: Reply,
    },
    Ev {
        topic: String,
        name: String,
        #[schemars(range(min = 0, max = 9007199254740991_u64))]
        seq: u64,
        data: Value,
    },
    Lost {
        topic: String,
        #[schemars(range(min = 0, max = 9007199254740991_u64))]
        from: u64,
        #[schemars(range(min = 0, max = 9007199254740991_u64))]
        to: u64,
    },
    Ping {
        id: Id,
    },
    Pong {
        id: Id,
    },
    Register {
        provider: String,
        methods: BTreeMap<String, u16>,
    },
    Error {
        err: Failure,
    },
}
fn name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
impl Message {
    pub fn validate(&self) -> Result<(), Failure> {
        let valid = match self {
            Self::Hello { proto, min, client } => {
                *proto > 0
                    && min.unwrap_or(*proto) > 0
                    && min.unwrap_or(*proto) <= *proto
                    && name(client)
            }
            Self::Welcome {
                proto,
                min,
                boot,
                topics,
                methods,
                frame_ms,
            } => {
                *min > 0
                    && *min <= *proto
                    && name(boot)
                    && topics.len() <= MAX_SUBSCRIPTIONS
                    && topics.iter().all(|(n, v)| name(n) && *v > 0)
                    && methods.len() <= 512
                    && methods
                        .iter()
                        .all(|(n, m)| name(n) && m.v > 0 && (1..=120_000).contains(&m.timeout))
                    && frame_ms.is_none_or(|ms| (1..=1000).contains(&ms))
            }
            Self::Sub { topics } | Self::Unsub { topics } | Self::Resync { topics } => {
                topics.len() <= MAX_SUBSCRIPTIONS && topics.iter().all(|topic| name(topic))
            }
            Self::Snap { topic, seq, data } => {
                name(topic) && *seq <= MAX_SAFE_INTEGER && data.is_object()
            }
            Self::Patch {
                topic,
                base,
                seq,
                data,
            } => name(topic) && *base < *seq && *seq <= MAX_SAFE_INTEGER && data.is_object(),
            Self::Call { id, m, p, timeout } => {
                id.valid()
                    && name(m)
                    && p.is_object()
                    && timeout.is_none_or(|ms| (1..=120_000).contains(&ms))
            }
            Self::Ret { id, .. } | Self::Ping { id } | Self::Pong { id } => id.valid(),
            Self::Ev {
                topic,
                name: event,
                seq,
                ..
            } => name(topic) && name(event) && *seq <= MAX_SAFE_INTEGER,
            Self::Lost { topic, from, to } => name(topic) && from <= to && *to <= MAX_SAFE_INTEGER,
            Self::Register { provider, methods } => {
                provider == "ui"
                    && methods.len() <= 512
                    && methods
                        .iter()
                        .all(|(n, v)| name(n) && n.starts_with("ui.") && *v > 0)
            }
            Self::Error { .. } => true,
        };
        if valid {
            Ok(())
        } else {
            Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid message fields",
                false,
            ))
        }
    }
}

/// A view-independent state mirror; callers own reconnect scheduling and rendering.
#[derive(Default)]
pub struct Mirror {
    pub boot: Option<String>,
    pub topics: BTreeMap<String, (u64, Value)>,
    awaiting_snapshot: BTreeSet<String>,
}
#[derive(Debug, PartialEq)]
pub enum Update {
    Unchanged,
    Boot,
    Changed(String),
    Resync(String),
}
impl Mirror {
    pub fn receive(&mut self, message: &Message) -> Update {
        match message {
            Message::Welcome { boot, .. } if self.boot.as_ref() != Some(boot) => {
                self.boot = Some(boot.clone());
                // Keep last snapshots for display, but require fresh sequence ownership.
                self.awaiting_snapshot = self.topics.keys().cloned().collect();
                Update::Boot
            }
            Message::Snap { topic, seq, data } => {
                self.awaiting_snapshot.remove(topic);
                self.topics.insert(topic.clone(), (*seq, data.clone()));
                Update::Changed(topic.clone())
            }
            Message::Patch {
                topic,
                base,
                seq,
                data,
            } => {
                let Some((known, value)) = self.topics.get_mut(topic) else {
                    return Update::Resync(topic.clone());
                };
                if self.awaiting_snapshot.contains(topic) {
                    return Update::Resync(topic.clone());
                }
                if *seq <= *known {
                    return Update::Unchanged;
                }
                if *base != *known {
                    return Update::Resync(topic.clone());
                }
                patch::apply(value, data);
                *known = *seq;
                Update::Changed(topic.clone())
            }
            _ => Update::Unchanged,
        }
    }
}
