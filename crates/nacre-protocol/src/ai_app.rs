//! Nacre AI app (N0319): provider registry, projects, accounts and the chat
//! list over the daemon link, provider-neutral. Everything here is readable
//! by the shell, so no field carries a credential or anything derived from one;
//! account status comes from profile directories and CLI exit codes only.
use crate::{Capability, ErrorCode, Failure, Method};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const PROVIDERS_TOPIC: &str = "ai.providers";
pub const PROJECTS_TOPIC: &str = "ai.projects";
pub const ACCOUNTS_TOPIC: &str = "ai.accounts";
pub const MAX_PROJECTS: usize = 64;
pub const MAX_ACCOUNTS: usize = 64;
pub const MAX_CHATS: usize = 100;
pub const DEFAULT_CHATS: usize = 30;
pub const MAX_BRAIN_RESULTS: usize = 50;
pub const MAX_NOTE_CHARS: usize = 16_384;

fn invalid(message: &str) -> Failure {
    Failure::new(ErrorCode::Validation, message, false)
}
fn is_false(value: &bool) -> bool {
    !*value
}
fn plain(value: &str, max: usize) -> bool {
    value.chars().count() <= max && !value.chars().any(char::is_control)
}
/// Provider and account ids: `^[a-z0-9][a-z0-9_-]{0,47}$`.
pub fn valid_id(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphanumeric())
        && value.len() <= 48
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// A terminal coding agent (Codex, Claude Code).
    Cli,
    /// A model served on this machine; no adapter ships yet.
    Local,
}
/// One flag per app feature; the UI shows only what is true.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub chat: bool,
    pub resume: bool,
    pub accounts: bool,
    pub usage: bool,
    /// A model list is published. Neither CLI exposes one, so false today.
    pub models: bool,
    /// `ai.chats.start` can choose the thread id, so its reply carries `id`.
    pub caller_session_id: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProviderInfo {
    #[schemars(length(min = 1, max = 48), regex(pattern = r"^[a-z0-9][a-z0-9_-]*$"))]
    pub id: String,
    #[schemars(length(min = 1, max = 64))]
    pub label: String,
    pub kind: ProviderKind,
    pub capabilities: Capabilities,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Providers {
    #[schemars(length(max = 16))]
    pub providers: Vec<ProviderInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    #[schemars(length(min = 1, max = 128))]
    pub id: String,
    #[schemars(length(max = 128))]
    pub label: String,
    #[schemars(length(max = 4096))]
    pub path: String,
    /// SSH host of a remote project; absent when local.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 253))]
    pub host: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Projects {
    #[schemars(length(max = 64))]
    pub projects: Vec<Project>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// Not checked since the daemon started.
    Unknown,
    SignedIn,
    SignedOut,
    /// The provider CLI is not installed.
    Missing,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    #[schemars(length(min = 1, max = 48))]
    pub provider: String,
    /// `default` for the provider's own home, else the profile directory name.
    #[schemars(length(min = 1, max = 48), regex(pattern = r"^[a-z0-9][a-z0-9_-]*$"))]
    pub id: String,
    #[schemars(length(min = 1, max = 64))]
    pub label: String,
    /// Selected in the AI settings for this provider.
    pub active: bool,
    pub status: AccountStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub checked_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Accounts {
    #[schemars(length(max = 64))]
    pub accounts: Vec<Account>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Changed {
    pub changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatsPage {
    /// 0 (default) collects the history again; larger values page the last collection.
    #[serde(default)]
    #[schemars(range(min = 0, max = 100000))]
    pub offset: u32,
    #[serde(default = "default_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: u32,
}
fn default_limit() -> u32 {
    DEFAULT_CHATS as u32
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatState {
    Running,
    Saved,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    /// Stable record key; accepted by `ai.chats.resume`.
    #[schemars(length(min = 20, max = 20), regex(pattern = r"^chat[0-9a-f]{16}$"))]
    pub id: String,
    #[schemars(length(min = 1, max = 48))]
    pub provider: String,
    #[schemars(length(min = 1, max = 128))]
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 253))]
    pub host: Option<String>,
    #[schemars(length(max = 256))]
    pub title: String,
    pub state: ChatState,
    /// Provider timestamp, seconds since the epoch.
    pub updated_at: f64,
    #[schemars(length(max = 64))]
    pub account: String,
    /// Running terminal session name as the provider list reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 128))]
    pub session: Option<String>,
    /// False for remote records: the daemon never opens SSH from a stored record.
    #[serde(default, skip_serializing_if = "is_false")]
    pub resumable: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Chats {
    #[schemars(length(max = 100))]
    pub chats: Vec<Chat>,
    #[schemars(range(min = 0, max = 100000))]
    pub total: u32,
    #[schemars(length(max = 2048))]
    pub warning: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatStart {
    #[schemars(length(min = 1, max = 48), regex(pattern = r"^[a-z0-9][a-z0-9_-]*$"))]
    pub provider: String,
    #[schemars(length(min = 1, max = 128))]
    pub project: String,
    /// Profile id as listed in `ai.accounts`; the active profile when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 48), regex(pattern = r"^[a-z0-9][a-z0-9_-]*$"))]
    pub account: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Started {
    pub started: bool,
    /// The chat id `ai.chats.list` will report, only for providers with
    /// `caller_session_id`; absent otherwise (the provider creates the id later).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 20, max = 20), regex(pattern = r"^chat[0-9a-f]{16}$"))]
    pub id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatRef {
    #[schemars(length(min = 20, max = 20), regex(pattern = r"^chat[0-9a-f]{16}$"))]
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrainSearch {
    #[schemars(length(min = 1, max = 512))]
    pub query: String,
    #[serde(default = "default_results")]
    #[schemars(range(min = 1, max = 50))]
    pub limit: u32,
}
fn default_results() -> u32 {
    10
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrainHit {
    #[schemars(length(max = 256))]
    pub title: String,
    /// Vault-relative note path.
    #[schemars(length(min = 1, max = 4096))]
    pub path: String,
    #[schemars(range(min = 0, max = 100000000))]
    pub line: u64,
    #[schemars(length(max = 1024))]
    pub snippet: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BrainHits {
    #[schemars(length(max = 50))]
    pub results: Vec<BrainHit>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrainNote {
    /// First line is the title, the rest the body.
    #[schemars(length(min = 1, max = 16384))]
    pub text: String,
    /// Free text such as the chat id the note came from.
    #[serde(default)]
    #[schemars(length(max = 256))]
    pub source: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotePath {
    #[schemars(length(min = 1, max = 4096))]
    pub path: String,
}

impl Providers {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = |p: &ProviderInfo| valid_id(&p.id) && (1..=64).contains(&p.label.chars().count());
        (self.providers.len() <= 16 && self.providers.iter().all(ok))
            .then_some(())
            .ok_or_else(|| invalid("Invalid provider registry"))
    }
}
impl Projects {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = |p: &Project| {
            !p.id.is_empty()
                && plain(&p.id, 128)
                && plain(&p.label, 128)
                && plain(&p.path, 4096)
                && p.host.as_deref().is_none_or(|h| plain(h, 253))
        };
        (self.projects.len() <= MAX_PROJECTS && self.projects.iter().all(ok))
            .then_some(())
            .ok_or_else(|| invalid("Invalid project list"))
    }
}
impl Accounts {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = |a: &Account| {
            valid_id(&a.provider)
                && valid_id(&a.id)
                && (1..=64).contains(&a.label.chars().count())
                && a.checked_ms.is_none_or(|t| t <= crate::MAX_SAFE_INTEGER)
        };
        (self.accounts.len() <= MAX_ACCOUNTS && self.accounts.iter().all(ok))
            .then_some(())
            .ok_or_else(|| invalid("Invalid account list"))
    }
}
impl ChatsPage {
    pub fn validate(&self) -> Result<(), Failure> {
        ((1..=MAX_CHATS as u32).contains(&self.limit) && self.offset <= 100_000)
            .then_some(())
            .ok_or_else(|| invalid("Invalid chat page"))
    }
}
pub fn valid_chat_id(value: &str) -> bool {
    value.len() == 20
        && value.starts_with("chat")
        && value[4..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Chats {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = |c: &Chat| {
            valid_chat_id(&c.id)
                && valid_id(&c.provider)
                && !c.project.is_empty()
                && plain(&c.project, 128)
                && c.host.as_deref().is_none_or(|h| plain(h, 253))
                && plain(&c.title, 256)
                && c.updated_at.is_finite()
                && c.updated_at >= 0.0
                && plain(&c.account, 64)
                && c.session.as_deref().is_none_or(|s| plain(s, 128))
        };
        (self.chats.len() <= MAX_CHATS
            && self.total <= 100_000
            && plain(&self.warning, 2048)
            && self.chats.iter().all(ok))
        .then_some(())
        .ok_or_else(|| invalid("Invalid chat list"))
    }
}
impl ChatStart {
    pub fn validate(&self) -> Result<(), Failure> {
        (valid_id(&self.provider)
            && !self.project.is_empty()
            && plain(&self.project, 128)
            && self.account.as_deref().is_none_or(valid_id))
        .then_some(())
        .ok_or_else(|| invalid("Invalid chat start request"))
    }
}
impl ChatRef {
    pub fn validate(&self) -> Result<(), Failure> {
        valid_chat_id(&self.id)
            .then_some(())
            .ok_or_else(|| invalid("Invalid chat id"))
    }
}
impl BrainSearch {
    pub fn validate(&self) -> Result<(), Failure> {
        (!self.query.trim().is_empty()
            && plain(&self.query, 512)
            && (1..=MAX_BRAIN_RESULTS as u32).contains(&self.limit))
        .then_some(())
        .ok_or_else(|| invalid("Invalid Brain search"))
    }
}
impl BrainHits {
    pub fn validate(&self) -> Result<(), Failure> {
        let ok = |h: &BrainHit| {
            plain(&h.title, 256)
                && !h.path.is_empty()
                && h.path.len() <= 4096
                && h.line <= 100_000_000
                && h.snippet.chars().count() <= 1024
        };
        (self.results.len() <= MAX_BRAIN_RESULTS && self.results.iter().all(ok))
            .then_some(())
            .ok_or_else(|| invalid("Invalid Brain results"))
    }
}
impl BrainNote {
    pub fn validate(&self) -> Result<(), Failure> {
        (!self.text.trim().is_empty()
            && self.text.chars().count() <= MAX_NOTE_CHARS
            && plain(&self.source, 256))
        .then_some(())
        .ok_or_else(|| invalid("Invalid Brain note"))
    }
}

/// Topic schema names, as exported.
pub const TOPICS: [(&str, &str); 3] = [
    (PROVIDERS_TOPIC, "ai-app-providers"),
    (PROJECTS_TOPIC, "ai-app-projects"),
    (ACCOUNTS_TOPIC, "ai-app-accounts"),
];
/// Method, params schema, result schema, deadline in ms.
pub const METHODS: [(&str, &str, &str, u32); 7] = [
    (
        "ai.projects.refresh",
        "ai-app-empty",
        "ai-app-changed",
        5_000,
    ),
    (
        "ai.accounts.refresh",
        "ai-app-empty",
        "ai-app-changed",
        60_000,
    ),
    ("ai.chats.list", "ai-app-chats-page", "ai-app-chats", 60_000),
    (
        "ai.chats.start",
        "ai-app-chat-start",
        "ai-app-started",
        10_000,
    ),
    (
        "ai.chats.resume",
        "ai-app-chat-ref",
        "ai-launcher-open-ack",
        10_000,
    ),
    (
        "brain.search",
        "ai-app-brain-search",
        "ai-app-brain-hits",
        30_000,
    ),
    (
        "brain.note",
        "ai-app-brain-note",
        "ai-app-note-path",
        30_000,
    ),
];
pub fn handles(method: &str) -> bool {
    METHODS.iter().any(|m| m.0 == method)
}
pub fn methods() -> BTreeMap<String, Method> {
    let schemas = schemas();
    let launcher = crate::ai_launcher::methods();
    METHODS
        .into_iter()
        .map(|(name, params, result, timeout)| {
            let result = if result == "ai-launcher-open-ack" {
                launcher["ai.launcher.open"].result.clone()
            } else {
                schemas[result].clone()
            };
            (
                name.to_owned(),
                Method {
                    v: 1,
                    class: Capability::Desktop,
                    timeout,
                    params: schemas[params].clone(),
                    result,
                },
            )
        })
        .collect()
}

const TYPES: [(&str, &str); 14] = [
    ("ai-app-providers", "AiProviders"),
    ("ai-app-projects", "AiProjects"),
    ("ai-app-accounts", "AiAccounts"),
    ("ai-app-empty", "AiAppEmpty"),
    ("ai-app-changed", "AiAppChanged"),
    ("ai-app-chats-page", "AiChatsPage"),
    ("ai-app-chats", "AiChats"),
    ("ai-app-chat-start", "AiChatStart"),
    ("ai-app-started", "AiChatStarted"),
    ("ai-app-chat-ref", "AiChatRef"),
    ("ai-app-brain-search", "BrainSearch"),
    ("ai-app-brain-hits", "BrainHits"),
    ("ai-app-brain-note", "BrainNote"),
    ("ai-app-note-path", "BrainNotePath"),
];
pub fn schemas() -> BTreeMap<String, Value> {
    fn one<T: JsonSchema>() -> Value {
        serde_json::to_value(schemars::schema_for!(T)).expect("ai app schema")
    }
    [
        one::<Providers>(),
        one::<Projects>(),
        one::<Accounts>(),
        one::<Empty>(),
        one::<Changed>(),
        one::<ChatsPage>(),
        one::<Chats>(),
        one::<ChatStart>(),
        one::<Started>(),
        one::<ChatRef>(),
        one::<BrainSearch>(),
        one::<BrainHits>(),
        one::<BrainNote>(),
        one::<NotePath>(),
    ]
    .into_iter()
    .zip(TYPES)
    .map(|(schema, (name, _))| (name.to_owned(), schema))
    .collect()
}

/// Generated schemas, Luau types and the transcript.
pub(crate) fn exports() -> BTreeMap<String, String> {
    use crate::artifacts::luau;
    let schemas = schemas();
    let mut out = BTreeMap::new();
    let mut types =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, type_name) in TYPES {
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
    types.push_str("return {}\n");
    for (name, schema) in &schemas {
        out.insert(
            format!("protocol/schema/{name}.json"),
            serde_json::to_string_pretty(schema).expect("ai app schema") + "\n",
        );
    }
    out.insert("protocol/luau/ai-app.d.luau".into(), types);
    out.insert(
        "protocol/fixtures/ai-app.jsonl".into(),
        transcript()
            .iter()
            .map(|f| serde_json::to_string(f).expect("ai app fixture") + "\n")
            .collect(),
    );
    out
}

/// Registry at start, a project read, an account check, one chat page, a
/// start, a resume and the two Brain hooks.
pub fn transcript() -> Vec<Value> {
    let capabilities = |caller_session_id: bool| json!({"chat":true,"resume":true,"accounts":true,"usage":true,"models":false,"callerSessionId":caller_session_id});
    vec![
        json!({"t":"snap","topic":PROVIDERS_TOPIC,"seq":0,"data":{"providers":[
            {"id":"codex","label":"Codex","kind":"cli","capabilities":capabilities(false)},
            {"id":"claude","label":"Claude Code","kind":"cli","capabilities":capabilities(true)}]}}),
        json!({"t":"snap","topic":PROJECTS_TOPIC,"seq":0,"data":{"projects":[
            {"id":"general-chat","label":"General chat","path":"/home/user/.local/share/nacre/ai/chats"}]}}),
        json!({"t":"snap","topic":ACCOUNTS_TOPIC,"seq":0,"data":{"accounts":[
            {"provider":"codex","id":"default","label":"Default","active":true,"status":"unknown"},
            {"provider":"claude","id":"default","label":"Default","active":true,"status":"unknown"}]}}),
        json!({"t":"call","id":1,"m":"ai.projects.refresh","p":{}}),
        json!({"t":"patch","topic":PROJECTS_TOPIC,"base":0,"seq":1,"data":{"projects":[
            {"id":"general-chat","label":"General chat","path":"/home/user/.local/share/nacre/ai/chats"},
            {"id":"fixture","label":"Fixture","path":"/home/user/Projects/fixture"},
            {"id":"remote","label":"Remote","path":"/home/dev/remote","host":"dev-1"}]}}),
        json!({"t":"ret","id":1,"ok":{"changed":true}}),
        json!({"t":"call","id":2,"m":"ai.accounts.refresh","p":{}}),
        json!({"t":"patch","topic":ACCOUNTS_TOPIC,"base":0,"seq":1,"data":{"accounts":[
            {"provider":"codex","id":"default","label":"Default","active":true,"status":"signed_in","checkedMs":1800000000000_u64},
            {"provider":"claude","id":"default","label":"Default","active":true,"status":"missing","checkedMs":1800000000000_u64}]}}),
        json!({"t":"ret","id":2,"ok":{"changed":true}}),
        json!({"t":"call","id":3,"m":"ai.chats.list","p":{"limit":2}}),
        json!({"t":"ret","id":3,"ok":{"chats":[
            {"id":"chat0000000000000001","provider":"codex","project":"fixture","title":"Fixture chat","state":"running","updatedAt":1800000000.5,"account":"Default","session":"nacre-task-fixture","resumable":true},
            {"id":"chat0000000000000002","provider":"claude","project":"remote","host":"dev-1","title":"Remote chat","state":"saved","updatedAt":1799999000.0,"account":"Default"}],
            "total":3,"warning":""}}),
        json!({"t":"call","id":4,"m":"ai.chats.start","p":{"provider":"codex","project":"fixture"}}),
        json!({"t":"ret","id":4,"ok":{"started":true}}),
        json!({"t":"call","id":8,"m":"ai.chats.start","p":{"provider":"claude","project":"fixture","account":"default"}}),
        json!({"t":"ret","id":8,"ok":{"started":true,"id":"chat0000000000000003"}}),
        json!({"t":"call","id":5,"m":"ai.chats.resume","p":{"id":"chat0000000000000001"}}),
        json!({"t":"ret","id":5,"ok":{"ok":true,"focused":false}}),
        json!({"t":"call","id":6,"m":"brain.search","p":{"query":"fixture"}}),
        json!({"t":"ret","id":6,"ok":{"results":[{"title":"Fixture note","path":"projects/fixture.md","line":3,"snippet":"A fixture note"}]}}),
        json!({"t":"call","id":7,"m":"brain.note","p":{"text":"Fixture finding\nSaved from a chat","source":"chat0000000000000001"}}),
        json!({"t":"ret","id":7,"ok":{"path":"inbox/2026-11-13-fixture-finding.md"}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_defaults_and_bounds_hold() {
        let page: ChatsPage = serde_json::from_value(json!({})).unwrap();
        assert_eq!((page.offset, page.limit), (0, DEFAULT_CHATS as u32));
        assert!(ChatsPage {
            offset: 0,
            limit: 0
        }
        .validate()
        .is_err());
        assert!(serde_json::from_value::<ChatStart>(
            json!({"provider":"codex","project":"x","model":"gpt"})
        )
        .is_err());
        assert!(!valid_chat_id("chat000000000000000g"));
        assert!(valid_id("work-2") && !valid_id("-work") && !valid_id("a b"));
    }
}
