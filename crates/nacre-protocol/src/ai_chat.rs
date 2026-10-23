//! Daemon-hosted AI chat. `ai.chat` holds one bounded snapshot per hosted
//! channel; the methods below drive it. Provider transcripts stay in the
//! providers' own stores, so only the newest messages are published here.
use crate::{ErrorCode, Failure, Method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const TOPIC: &str = "ai.chat";
pub const EVENT_SEND_FAILED: &str = "send_failed";
pub const MAX_CHANNELS: usize = 4;
pub const MAX_MESSAGES: usize = 64;
/// Limits are measured on the serialized JSON so a whole topic stays far below
/// the 1 MiB frame limit even when every channel is full.
pub const MAX_TEXT_JSON: usize = 48 * 1024;
pub const MAX_MESSAGES_JSON: usize = 128 * 1024;
pub const MAX_QUESTION_JSON: usize = 32 * 1024;
pub const MAX_SEND_CHARS: usize = 64_000;
pub const MAX_ATTACHMENTS: usize = 8;
pub const MAX_SAVED: usize = 30;
const MAX_ATTACHMENT_JSON: usize = 4096;
const MAX_ANSWERS_JSON: usize = 16 * 1024;

fn invalid(message: &str) -> Failure {
    Failure::new(ErrorCode::Validation, message, false)
}
/// `""` is the default (edge) channel; other channels are lowercase ids.
pub fn valid_channel(channel: &str) -> bool {
    channel.len() <= 48
        && channel
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn is_false(value: &bool) -> bool {
    !*value
}
fn json_len(value: &impl Serialize) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |v| v.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Codex,
    Claude,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// The fields of the N0023 socket `state` event, without its `type`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiState {
    pub default_provider: Provider,
    pub in_workspace: bool,
    pub provider: Provider,
    #[schemars(length(max = 128))]
    pub account: String,
    #[schemars(length(max = 1024))]
    pub title: String,
    #[schemars(length(max = 128))]
    pub thread_id: String,
    #[schemars(length(max = 128))]
    pub model: String,
    #[schemars(length(max = 8))]
    pub features: Vec<String>,
    #[schemars(length(max = 128))]
    pub composer_key: String,
    pub busy: bool,
    #[schemars(length(max = 512))]
    pub status: String,
    #[schemars(length(max = 1024))]
    pub error: String,
    /// Approval or question card exactly as the sidebar contract defines it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "object_schema")]
    pub question: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiMessage {
    #[schemars(length(max = 256))]
    pub id: String,
    pub role: Role,
    pub text: String,
    /// The text was cut at the published bound; the provider keeps it whole.
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiChannel {
    pub state: AiState,
    /// Oldest first; keys are ten-digit creation ordinals.
    #[schemars(schema_with = "messages_schema")]
    pub messages: BTreeMap<String, AiMessage>,
    /// Older messages exist beyond the published window.
    #[serde(default, skip_serializing_if = "is_false")]
    pub trimmed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiChat {
    pub available: bool,
    #[schemars(schema_with = "channels_schema")]
    pub channels: BTreeMap<String, AiChannel>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiChannelRef {
    #[schemars(length(max = 48), regex(pattern = r"^[a-z0-9-]*$"))]
    pub channel: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiSend {
    #[schemars(length(max = 48), regex(pattern = r"^[a-z0-9-]*$"))]
    pub channel: String,
    #[schemars(length(min = 1, max = 64000))]
    pub text: String,
    #[serde(default)]
    #[schemars(schema_with = "attachments_schema")]
    pub attachments: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiApprove {
    #[schemars(length(max = 48), regex(pattern = r"^[a-z0-9-]*$"))]
    pub channel: String,
    #[schemars(length(min = 1, max = 256))]
    pub id: String,
    #[schemars(schema_with = "object_schema")]
    pub answers: Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiResume {
    #[schemars(length(max = 48), regex(pattern = r"^[a-z0-9-]*$"))]
    pub channel: String,
    #[schemars(length(min = 1, max = 128))]
    pub key: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiProvider {
    #[schemars(length(max = 48), regex(pattern = r"^[a-z0-9-]*$"))]
    pub channel: String,
    pub provider: Provider,
}
/// One saved chat; `key` is accepted by `ai.chat.resume`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiSaved {
    #[schemars(length(min = 1, max = 128))]
    pub key: String,
    #[schemars(length(max = 256))]
    pub id: String,
    #[schemars(length(max = 100))]
    pub title: String,
    pub agent: Provider,
    #[schemars(length(max = 128))]
    pub account: String,
    #[schemars(length(max = 32))]
    pub state: String,
}
/// Result of `ai.chat.saved`: newest first, at most [`MAX_SAVED`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiSavedList {
    #[schemars(length(max = 30))]
    pub chats: Vec<AiSaved>,
}
impl AiSavedList {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = |c: &AiSaved| {
            (1..=128).contains(&c.key.len())
                && c.id.len() <= 256
                && c.title.chars().count() <= 100
                && c.account.len() <= 128
                && c.state.len() <= 32
        };
        (self.chats.len() <= MAX_SAVED && self.chats.iter().all(ok))
            .then_some(())
            .ok_or_else(|| invalid("Invalid saved chat list"))
    }
}

/// Event published when the worker refused a submission, so the composer can
/// restore the unsent text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiSendFailed {
    #[schemars(length(max = 48), regex(pattern = r"^[a-z0-9-]*$"))]
    pub channel: String,
    #[schemars(length(max = 64000))]
    pub text: String,
    #[schemars(schema_with = "attachments_schema")]
    pub attachments: Vec<Value>,
}

fn object_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"object"})
}
fn attachments_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"array","maxItems":MAX_ATTACHMENTS,"items":{"type":"object"}})
}
fn messages_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let value = generator.subschema_for::<AiMessage>();
    schemars::json_schema!({"type":"object","maxProperties":MAX_MESSAGES,"propertyNames":{"pattern":"^[0-9]{10}$"},"additionalProperties":value})
}
fn channels_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let value = generator.subschema_for::<AiChannel>();
    schemars::json_schema!({"type":"object","maxProperties":MAX_CHANNELS,"propertyNames":{"maxLength":48,"pattern":"^[a-z0-9-]*$"},"additionalProperties":value})
}

fn attachments_valid(attachments: &[Value]) -> bool {
    attachments.len() <= MAX_ATTACHMENTS
        && attachments
            .iter()
            .all(|a| a.is_object() && json_len(a) <= MAX_ATTACHMENT_JSON)
}
impl AiChannelRef {
    pub fn validate(&self) -> Result<(), Failure> {
        valid_channel(&self.channel)
            .then_some(())
            .ok_or_else(|| invalid("Invalid assistant channel"))
    }
}
impl AiSend {
    pub fn validate(&self) -> Result<(), Failure> {
        let chars = self.text.trim().chars().count();
        if !valid_channel(&self.channel)
            || chars == 0
            || self.text.chars().count() > MAX_SEND_CHARS
            || !attachments_valid(&self.attachments)
        {
            return Err(invalid("Invalid chat message"));
        }
        Ok(())
    }
}
impl AiApprove {
    pub fn validate(&self) -> Result<(), Failure> {
        if !valid_channel(&self.channel)
            || self.id.is_empty()
            || self.id.len() > 256
            || !self.answers.is_object()
            || json_len(&self.answers) > MAX_ANSWERS_JSON
        {
            return Err(invalid("Invalid chat answer"));
        }
        Ok(())
    }
}
impl AiResume {
    pub fn validate(&self) -> Result<(), Failure> {
        if !valid_channel(&self.channel) || self.key.is_empty() || self.key.len() > 128 {
            return Err(invalid("Invalid saved chat"));
        }
        Ok(())
    }
}
impl AiProvider {
    pub fn validate(&self) -> Result<(), Failure> {
        valid_channel(&self.channel)
            .then_some(())
            .ok_or_else(|| invalid("Invalid assistant channel"))
    }
}
impl AiSendFailed {
    pub fn validate(&self) -> Result<(), Failure> {
        if !valid_channel(&self.channel)
            || self.text.chars().count() > MAX_SEND_CHARS
            || !attachments_valid(&self.attachments)
        {
            return Err(invalid("Invalid send-failed event"));
        }
        Ok(())
    }
}
impl AiState {
    fn valid(&self) -> bool {
        self.account.len() <= 128
            && self.title.len() <= 1024
            && self.thread_id.len() <= 128
            && self.model.len() <= 128
            && self.features.len() <= 8
            && self.features.iter().all(|f| f.len() <= 32)
            && self.composer_key.len() <= 128
            && self.status.len() <= 512
            && self.error.len() <= 1024
            && self
                .question
                .as_ref()
                .is_none_or(|q| q.is_object() && json_len(q) <= MAX_QUESTION_JSON)
    }
}
impl AiChannel {
    pub fn validate(&self) -> Result<(), Failure> {
        let mut total = 0;
        let ordered = self
            .messages
            .keys()
            .all(|k| k.len() == 10 && k.bytes().all(|b| b.is_ascii_digit()));
        for message in self.messages.values() {
            let text = json_len(&message.text);
            if message.id.len() > 256 || text > MAX_TEXT_JSON {
                return Err(invalid("Invalid chat message"));
            }
            total += text + message.id.len();
        }
        if !self.state.valid()
            || !ordered
            || self.messages.len() > MAX_MESSAGES
            || total > MAX_MESSAGES_JSON
        {
            return Err(invalid("Invalid chat channel"));
        }
        Ok(())
    }
}
impl AiChat {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.channels.len() > MAX_CHANNELS || self.channels.keys().any(|c| !valid_channel(c)) {
            return Err(invalid("Invalid chat topic"));
        }
        self.channels.values().try_for_each(AiChannel::validate)
    }
}

/// Method registry; the daemon advertises it while the chat service runs.
pub fn methods() -> BTreeMap<String, Method> {
    use crate::Capability;
    let schemas = schemas();
    let schema = |name: &str| schemas[name].clone();
    [
        ("ai.chat.open", "ai-chat-channel", "ai-chat-ack", 10_000),
        ("ai.chat.send", "ai-chat-send", "ai-chat-ack", 3_000),
        ("ai.chat.cancel", "ai-chat-channel", "ai-chat-ack", 3_000),
        ("ai.chat.approve", "ai-chat-approve", "ai-chat-ack", 3_000),
        ("ai.chat.new", "ai-chat-channel", "ai-chat-ack", 3_000),
        ("ai.chat.resume", "ai-chat-resume", "ai-chat-ack", 3_000),
        ("ai.chat.provider", "ai-chat-provider", "ai-chat-ack", 3_000),
        ("ai.chat.workspace", "ai-chat-channel", "ai-chat-ack", 3_000),
        // Reads every project's provider history through the existing list helpers.
        ("ai.chat.saved", "ai-chat-channel", "ai-chat-saved", 45_000),
    ]
    .into_iter()
    .map(|(name, params, result, timeout)| {
        (
            name.to_owned(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout,
                params: schema(params),
                result: schema(result),
            },
        )
    })
    .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiAck {
    pub channel: String,
}

fn schemas() -> BTreeMap<String, Value> {
    fn one<T: JsonSchema>() -> Value {
        serde_json::to_value(schemars::schema_for!(T)).expect("ai chat schema")
    }
    BTreeMap::from([
        ("ai-chat".to_owned(), one::<AiChat>()),
        ("ai-chat-channel".to_owned(), one::<AiChannelRef>()),
        ("ai-chat-send".to_owned(), one::<AiSend>()),
        ("ai-chat-approve".to_owned(), one::<AiApprove>()),
        ("ai-chat-resume".to_owned(), one::<AiResume>()),
        ("ai-chat-provider".to_owned(), one::<AiProvider>()),
        ("ai-chat-send-failed".to_owned(), one::<AiSendFailed>()),
        ("ai-chat-ack".to_owned(), one::<AiAck>()),
        ("ai-chat-saved".to_owned(), one::<AiSavedList>()),
    ])
}

/// Generated schemas, Luau types and the golden transcript.
pub(crate) fn exports() -> BTreeMap<String, String> {
    use crate::artifacts::luau;
    let schemas = schemas();
    let mut out = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, type_name) in [
        ("ai-chat", "AiChat"),
        ("ai-chat-channel", "AiChannelRef"),
        ("ai-chat-send", "AiSend"),
        ("ai-chat-approve", "AiApprove"),
        ("ai-chat-resume", "AiResume"),
        ("ai-chat-provider", "AiProvider"),
        ("ai-chat-send-failed", "AiSendFailed"),
        ("ai-chat-ack", "AiAck"),
        ("ai-chat-saved", "AiSavedList"),
    ] {
        types.push_str(&format!(
            "export type {type_name} = {}\n",
            luau(&schemas[name])
        ));
    }
    let definitions: BTreeMap<&String, &Value> = schemas
        .values()
        .filter_map(|schema| schema["$defs"].as_object())
        .flatten()
        .collect();
    for (name, schema) in definitions {
        types.push_str(&format!("export type {name} = {}\n", luau(schema)));
    }
    for (name, schema) in &schemas {
        out.insert(
            format!("protocol/schema/{name}.json"),
            serde_json::to_string_pretty(schema).expect("ai chat schema") + "\n",
        );
    }
    types.push_str("return {}\n");
    out.insert("protocol/luau/ai-chat.d.luau".into(), types);
    out.insert(
        "protocol/fixtures/ai-chat.jsonl".into(),
        transcript()
            .iter()
            .map(|f| serde_json::to_string(f).expect("ai chat fixture") + "\n")
            .collect(),
    );
    out
}

fn sample_state(busy: bool, status: &str) -> Value {
    json!({"defaultProvider":"codex","inWorkspace":false,"provider":"codex","account":"Default","title":"New chat","threadId":"","model":"","features":["attachments"],"composerKey":"fixture-key","busy":busy,"status":status,"error":""})
}
/// Streaming turn, approval, cancellation and a refused submission.
pub fn transcript() -> Vec<Value> {
    let question = json!({"id":"approval-1","kind":"approval","questions":[{"id":"approval","question":"Run fixture command?","options":[{"label":"Allow once"},{"label":"Decline"}]}]});
    let mut asking = sample_state(true, "Working");
    asking["question"] = question;
    vec![
        json!({"t":"snap","topic":TOPIC,"seq":0,"data":{"available":true,"channels":{}}}),
        json!({"t":"call","id":1,"m":"ai.chat.open","p":{"channel":"p1"}}),
        json!({"t":"patch","topic":TOPIC,"base":0,"seq":1,"data":{"channels":{"p1":{"state":sample_state(false,""),"messages":{}}}}}),
        json!({"t":"ret","id":1,"ok":{"channel":"p1"}}),
        json!({"t":"call","id":2,"m":"ai.chat.send","p":{"channel":"p1","text":"hello"}}),
        json!({"t":"patch","topic":TOPIC,"base":1,"seq":2,"data":{"channels":{"p1":{"state":sample_state(true,"Connecting"),"messages":{"0000000001":{"id":"user-1","role":"user","text":"hello"}}}}}}),
        json!({"t":"ret","id":2,"ok":{"channel":"p1"}}),
        json!({"t":"patch","topic":TOPIC,"base":2,"seq":3,"data":{"channels":{"p1":{"messages":{"0000000002":{"id":"assistant-1","role":"assistant","text":"stream"}}}}}}),
        json!({"t":"patch","topic":TOPIC,"base":3,"seq":4,"data":{"channels":{"p1":{"state":asking}}}}),
        json!({"t":"call","id":3,"m":"ai.chat.approve","p":{"channel":"p1","id":"approval-1","answers":{"approval":{"answers":["Allow once"]}}}}),
        json!({"t":"ret","id":3,"ok":{"channel":"p1"}}),
        json!({"t":"patch","topic":TOPIC,"base":4,"seq":5,"data":{"channels":{"p1":{"state":{"question":null},"messages":{"0000000002":{"text":"streamed answer"}}}}}}),
        json!({"t":"call","id":4,"m":"ai.chat.cancel","p":{"channel":"p1"}}),
        json!({"t":"ret","id":4,"ok":{"channel":"p1"}}),
        json!({"t":"patch","topic":TOPIC,"base":5,"seq":6,"data":{"channels":{"p1":{"state":{"busy":false,"status":""}}}}}),
        json!({"t":"call","id":5,"m":"ai.chat.saved","p":{"channel":"p1"}}),
        json!({"t":"ret","id":5,"ok":{"chats":[{"key":"0123456789abcdef01234567","id":"thread-1","title":"Fixture chat","agent":"codex","account":"Default","state":"saved"}]}}),
        json!({"t":"ev","topic":TOPIC,"name":EVENT_SEND_FAILED,"seq":1,"data":{"channel":"p1","text":"unsent","attachments":[]}}),
        json!({"t":"patch","topic":TOPIC,"base":6,"seq":7,"data":{"channels":{"p1":null}}}),
    ]
}
