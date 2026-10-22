//! Dashboard focus, clipboard, screenshots, AI usage and Brain recent contracts.
//! Paths and previews are local, bounded and private: never log or forward them.
use crate::core_services::{Entities, EntityId, Health, Refresh};
use crate::{ErrorCode, Failure, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const FOCUS_MAX_MINUTES: u32 = 180;
pub const PREVIEW_CHARS: usize = 200;
pub const MAX_CLIPBOARD_ENTRIES: usize = 50;
pub const MAX_SCREENSHOTS: usize = 6;
pub const MAX_BRAIN_NOTES: usize = 5;
pub const MAX_BRAIN_TASKS: usize = 6;
pub const MAX_USAGE_LINES: usize = 5;

fn invalid(message: &'static str) -> Failure {
    Failure::new(ErrorCode::Validation, message, false)
}
fn text_valid(value: &str, max_chars: usize) -> bool {
    value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}
fn path_valid(value: &str) -> bool {
    value.len() <= 4096
        && value.starts_with('/')
        && !value.contains(['\0', '\n'])
        && !value.split('/').any(|part| part == "..")
}
fn time_valid(value: u64) -> bool {
    value <= MAX_SAFE_INTEGER
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FocusStatus {
    Idle,
    Running,
    Paused,
    Done,
}

/// `running` carries `deadline_ms` (Unix milliseconds); consumers count down
/// locally, the daemon never publishes per-second ticks. `paused` carries the
/// frozen `remaining_s`. `idle` and `done` carry neither.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DashboardFocus {
    pub health: Health,
    pub status: FocusStatus,
    #[schemars(range(min = 0, max = 10800))]
    pub total_s: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub deadline_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 10800))]
    pub remaining_s: Option<u32>,
    /// The session turned Do Not Disturb on and restores it when it ends.
    pub owns_dnd: bool,
    pub previous_dnd: bool,
}
impl DashboardFocus {
    pub fn validate(&self) -> Result<(), Failure> {
        let live = matches!(self.status, FocusStatus::Running | FocusStatus::Paused);
        let shape = match self.status {
            FocusStatus::Running => {
                self.deadline_ms.is_some_and(|v| v >= 1 && time_valid(v))
                    && self.remaining_s.is_none()
            }
            FocusStatus::Paused => self.deadline_ms.is_none() && self.remaining_s.is_some(),
            FocusStatus::Idle | FocusStatus::Done => {
                self.deadline_ms.is_none() && self.remaining_s.is_none()
            }
        };
        if !shape
            || self.total_s > FOCUS_MAX_MINUTES * 60
            || (live && self.total_s == 0)
            || self.remaining_s.is_some_and(|v| v > FOCUS_MAX_MINUTES * 60)
            || (!live && self.owns_dnd)
        {
            return Err(invalid("Invalid focus state"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClipKind {
    Text,
    Image,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClipEntry {
    pub kind: ClipKind,
    /// The single-line preview the dashboard shows; at most 200 characters.
    #[schemars(length(max = 200))]
    pub preview: String,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub created_ms: u64,
}
/// Newest first. Entries are daemon-issued opaque IDs accepted by `clipboard.copy`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClipboardState {
    pub health: Health,
    pub entries: Entities<ClipEntry>,
}
impl ClipboardState {
    pub fn validate(&self) -> Result<(), Failure> {
        if !self.entries.valid()
            || self.entries.items.len() > MAX_CLIPBOARD_ENTRIES
            || self
                .entries
                .items
                .values()
                .any(|e| !text_valid(&e.preview, PREVIEW_CHARS) || !time_valid(e.created_ms))
        {
            return Err(invalid("Invalid clipboard state"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Screenshot {
    /// Absolute local file below the user's Pictures/Screenshots; open it with
    /// `screenshots.open` or read it with symlink-safe descriptor walks.
    #[schemars(length(min = 1, max = 4096))]
    pub path: String,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub created_ms: u64,
}
/// Newest first, at most six.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotsState {
    pub health: Health,
    pub items: Entities<Screenshot>,
}
impl ScreenshotsState {
    pub fn validate(&self) -> Result<(), Failure> {
        if !self.items.valid()
            || self.items.items.len() > MAX_SCREENSHOTS
            || self
                .items
                .items
                .values()
                .any(|s| !path_valid(&s.path) || !time_valid(s.created_ms))
        {
            return Err(invalid("Invalid screenshots state"));
        }
        Ok(())
    }
}

/// Provider-reported display lines after the header, as the dashboard shows them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiProvider {
    #[schemars(length(min = 1, max = 64))]
    pub name: String,
    #[schemars(length(max = 5), inner(length(max = 256)))]
    pub lines: Vec<String>,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub observed_ms: u64,
}
/// Empty and unavailable until the first `ai.usage.refresh`; Codex quota needs a
/// provider connection, so it is read only on request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AiUsageState {
    pub health: Health,
    pub providers: Entities<AiProvider>,
}
impl AiUsageState {
    pub fn validate(&self) -> Result<(), Failure> {
        if !self.providers.valid()
            || self.providers.items.len() > 4
            || self.providers.items.values().any(|p| {
                p.name.is_empty()
                    || !text_valid(&p.name, 64)
                    || p.lines.len() > MAX_USAGE_LINES
                    || p.lines.iter().any(|l| !text_valid(l, 256))
                    || !time_valid(p.observed_ms)
            })
        {
            return Err(invalid("Invalid AI usage state"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainNote {
    #[schemars(length(max = 100))]
    pub title: String,
    #[schemars(length(min = 1, max = 4096))]
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainTask {
    #[schemars(length(max = 160))]
    pub text: String,
    #[schemars(length(min = 1, max = 4096))]
    pub path: String,
}
/// Most recently changed notes (newest first) and open `- [ ]` tasks from them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrainRecentState {
    pub health: Health,
    pub notes: Entities<BrainNote>,
    pub tasks: Entities<BrainTask>,
}
impl BrainRecentState {
    pub fn validate(&self) -> Result<(), Failure> {
        if !self.notes.valid()
            || !self.tasks.valid()
            || self.notes.items.len() > MAX_BRAIN_NOTES
            || self.tasks.items.len() > MAX_BRAIN_TASKS
            || self
                .notes
                .items
                .values()
                .any(|n| !text_valid(&n.title, 100) || !path_valid(&n.path))
            || self
                .tasks
                .items
                .values()
                .any(|t| !text_valid(&t.text, 160) || !path_valid(&t.path))
        {
            return Err(invalid("Invalid Brain recent state"));
        }
        Ok(())
    }
}

/// Payload alternatives embedded in the existing snap message, not a new frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "topic", content = "data")]
pub enum DashboardExtrasSnapshot {
    #[serde(rename = "dashboard.focus")]
    Focus(DashboardFocus),
    #[serde(rename = "clipboard")]
    Clipboard(ClipboardState),
    #[serde(rename = "screenshots")]
    Screenshots(ScreenshotsState),
    #[serde(rename = "ai.usage")]
    AiUsage(AiUsageState),
    #[serde(rename = "brain.recent")]
    BrainRecent(BrainRecentState),
}
impl DashboardExtrasSnapshot {
    pub fn validate(&self) -> Result<(), Failure> {
        match self {
            Self::Focus(s) => s.validate(),
            Self::Clipboard(s) => s.validate(),
            Self::Screenshots(s) => s.validate(),
            Self::AiUsage(s) => s.validate(),
            Self::BrainRecent(s) => s.validate(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusStart {
    #[schemars(range(min = 1, max = 180))]
    pub minutes: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntityTarget {
    pub id: EntityId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "m", content = "p")]
pub enum DashboardExtrasCall {
    #[serde(rename = "focus.start")]
    FocusStart(FocusStart),
    #[serde(rename = "focus.pause")]
    FocusPause(Refresh),
    #[serde(rename = "focus.resume")]
    FocusResume(Refresh),
    #[serde(rename = "focus.stop")]
    FocusStop(Refresh),
    #[serde(rename = "clipboard.copy")]
    ClipboardCopy(EntityTarget),
    #[serde(rename = "clipboard.clear")]
    ClipboardClear(Refresh),
    #[serde(rename = "screenshots.open")]
    ScreenshotsOpen(EntityTarget),
    #[serde(rename = "ai.usage.refresh")]
    AiUsageRefresh(Refresh),
}
impl DashboardExtrasCall {
    pub fn validate(&self) -> Result<(), Failure> {
        let valid = match self {
            Self::FocusStart(p) => (1..=FOCUS_MAX_MINUTES).contains(&p.minutes),
            Self::ClipboardCopy(p) | Self::ScreenshotsOpen(p) => p.id.valid(),
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(Failure::new(
                ErrorCode::Validation,
                "Invalid dashboard request",
                false,
            ))
        }
    }
}

fn entities(value: Value) -> Value {
    let order = value
        .as_object()
        .expect("entities")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    json!({"items":value,"order":order})
}
fn lines(frames: &[Value]) -> String {
    frames
        .iter()
        .map(|f| {
            crate::framing::decode(&serde_json::to_vec(f).expect("golden")).expect("golden");
            serde_json::to_string(f).expect("golden") + "\n"
        })
        .collect()
}
fn typed(topic: &str, data: &Value) {
    serde_json::from_value::<DashboardExtrasSnapshot>(json!({"topic":topic,"data":data}))
        .expect("typed topic")
        .validate()
        .expect("valid topic");
}
fn call(id: u64, method: &str, params: Value) -> [Value; 2] {
    serde_json::from_value::<DashboardExtrasCall>(json!({"m":method,"p":params}))
        .expect("typed call")
        .validate()
        .expect("valid call");
    [
        json!({"t":"call","id":id,"m":method,"p":params}),
        json!({"t":"ret","id":id,"ok":{"changed":method!="screenshots.open" && method!="ai.usage.refresh"}}),
    ]
}

pub(crate) fn exports() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let schemas = [
        (
            "DashboardExtrasSnapshot",
            "dashboard-extras-state",
            schemars::schema_for!(DashboardExtrasSnapshot),
        ),
        (
            "DashboardExtrasCall",
            "dashboard-extras-call",
            schemars::schema_for!(DashboardExtrasCall),
        ),
        (
            "FocusStart",
            "dashboard-focus-start",
            schemars::schema_for!(FocusStart),
        ),
        (
            "EntityTarget",
            "dashboard-extras-target",
            schemars::schema_for!(EntityTarget),
        ),
    ];
    let mut luau =
        String::from("-- Generated by nacre-protocol; do not edit. MIT OR Apache-2.0.\n");
    for (name, file, schema) in schemas {
        let value = serde_json::to_value(schema).expect("dashboard schema");
        out.insert(
            format!("protocol/schema/{file}.json"),
            serde_json::to_string_pretty(&value).expect("dashboard schema") + "\n",
        );
        luau.push_str(&format!(
            "export type {name} = {}\n",
            crate::artifacts::luau(&value)
        ));
    }
    luau.push_str("return {}\n");
    out.insert("protocol/luau/dashboard-extras.d.luau".into(), luau);

    let health = json!({"available":true,"stale":false});
    let down = |message: &str| json!({"health":{"available":false,"stale":true,"error":{"code":"unavailable","message":message,"retry":true}}});
    let idle =
        json!({"health":health,"status":"idle","total_s":0,"owns_dnd":false,"previous_dnd":false});
    typed("dashboard.focus", &idle);
    let running = json!({"health":health,"status":"running","total_s":1500,"deadline_ms":1800001500000_u64,"owns_dnd":true,"previous_dnd":false});
    typed("dashboard.focus", &running);
    let mut frames = vec![json!({"t":"snap","topic":"dashboard.focus","seq":0,"data":idle})];
    for step in [
        call(1, "focus.start", json!({"minutes":25})).to_vec(),
        vec![
            json!({"t":"patch","topic":"dashboard.focus","base":0,"seq":1,"data":{"status":"running","total_s":1500,"deadline_ms":1800001500000_u64,"owns_dnd":true}}),
        ],
        vec![
            json!({"t":"call","id":6,"m":"focus.start","p":{"minutes":25}}),
            json!({"t":"ret","id":6,"err":{"code":"invalid_request","message":"A focus session is already active","retry":false}}),
        ],
        call(2, "focus.pause", json!({})).to_vec(),
        vec![
            json!({"t":"patch","topic":"dashboard.focus","base":1,"seq":2,"data":{"status":"paused","deadline_ms":null,"remaining_s":900}}),
        ],
        call(3, "focus.resume", json!({})).to_vec(),
        vec![
            json!({"t":"patch","topic":"dashboard.focus","base":2,"seq":3,"data":{"status":"running","deadline_ms":1800000900000_u64,"remaining_s":null}}),
        ],
        call(4, "focus.stop", json!({})).to_vec(),
        vec![
            json!({"t":"patch","topic":"dashboard.focus","base":3,"seq":4,"data":{"status":"idle","deadline_ms":null,"owns_dnd":false}}),
        ],
        vec![
            json!({"t":"call","id":5,"m":"focus.start","p":{"minutes":0}}),
            json!({"t":"ret","id":5,"err":{"code":"validation","message":"Invalid dashboard request","retry":false}}),
        ],
        vec![
            json!({"t":"patch","topic":"dashboard.focus","base":4,"seq":5,"data":{"status":"done"}}),
        ],
    ] {
        frames.extend(step);
    }
    out.insert(
        "protocol/fixtures/dashboard-focus.jsonl".into(),
        lines(&frames),
    );

    let clip = json!({"health":health,"entries":entities(json!({"c42":{"kind":"text","preview":"Public fixture text","created_ms":1800000000000_u64},"c41":{"kind":"image","preview":"[[ binary data 12 KiB png 100x100 ]]","created_ms":1799999990000_u64}}))});
    typed("clipboard", &clip);
    let mut frames = vec![json!({"t":"snap","topic":"clipboard","seq":0,"data":clip})];
    frames.extend(call(1, "clipboard.copy", json!({"id":"c41"})));
    frames.extend(call(2, "clipboard.clear", json!({})));
    frames.push(json!({"t":"patch","topic":"clipboard","base":0,"seq":1,"data":{"entries":{"items":{"c42":null,"c41":null},"order":[]}}}));
    frames.push(json!({"t":"call","id":3,"m":"clipboard.copy","p":{"id":"c41"}}));
    frames.push(json!({"t":"ret","id":3,"err":{"code":"stale_revision","message":"Clipboard entry no longer exists","retry":true}}));
    frames.push(json!({"t":"patch","topic":"clipboard","base":1,"seq":2,"data":down("Clipboard history is unavailable")}));
    out.insert(
        "protocol/fixtures/dashboard-clipboard.jsonl".into(),
        lines(&frames),
    );

    let shots = json!({"health":health,"items":entities(json!({"s0a1b2c3d4e5f6071":{"path":"/home/user/Pictures/Screenshots/screenshot_20270115_080000_000000.png","created_ms":1800000000000_u64}}))});
    typed("screenshots", &shots);
    let mut frames = vec![json!({"t":"snap","topic":"screenshots","seq":0,"data":shots})];
    frames.extend(call(
        1,
        "screenshots.open",
        json!({"id":"s0a1b2c3d4e5f6071"}),
    ));
    frames.push(json!({"t":"patch","topic":"screenshots","base":0,"seq":1,"data":{"items":{"items":{"s0a1b2c3d4e5f6071":null},"order":[]}}}));
    out.insert(
        "protocol/fixtures/dashboard-screenshots.jsonl".into(),
        lines(&frames),
    );

    let empty = json!({"health":{"available":false,"stale":false},"providers":entities(json!({}))});
    typed("ai.usage", &empty);
    let usage = json!({"health":health,"providers":entities(json!({"codex":{"name":"Codex","lines":["codex · 5h: 12% used · resets Mon 12:00"],"observed_ms":1800000000000_u64},"claude":{"name":"Claude Code","lines":["5 hour: 3% used · resets Mon 11:00"],"observed_ms":1800000000000_u64}}))});
    typed("ai.usage", &usage);
    let mut frames = vec![json!({"t":"snap","topic":"ai.usage","seq":0,"data":empty})];
    frames.extend(call(1, "ai.usage.refresh", json!({})));
    frames.push(json!({"t":"patch","topic":"ai.usage","base":0,"seq":1,"data":usage}));
    frames.push(json!({"t":"patch","topic":"ai.usage","base":1,"seq":2,"data":{"health":{"available":false,"stale":true,"error":{"code":"unavailable","message":"AI usage is unavailable","retry":true}}}}));
    out.insert(
        "protocol/fixtures/dashboard-ai-usage.jsonl".into(),
        lines(&frames),
    );

    let brain = json!({"health":health,"notes":entities(json!({"n0a1b2c3d4e5f6071":{"title":"Public fixture note","path":"/home/user/Documents/Nacre-Brain/projects/fixture.md"}})),"tasks":entities(json!({"t0a1b2c3d4e5f6071":{"text":"Public fixture task","path":"/home/user/Documents/Nacre-Brain/projects/fixture.md"}}))});
    typed("brain.recent", &brain);
    let frames = [
        json!({"t":"snap","topic":"brain.recent","seq":0,"data":brain}),
        json!({"t":"patch","topic":"brain.recent","base":0,"seq":1,"data":{"health":{"available":false,"stale":true,"error":{"code":"unavailable","message":"Brain vault is unavailable","retry":true}}}}),
    ];
    out.insert(
        "protocol/fixtures/dashboard-brain-recent.jsonl".into(),
        lines(&frames),
    );

    let store = |rev: u64, enabled: Value, order: Value| json!({"store":"dashboard-extras","rev":rev,"exists":true,"data":{"version":1,"enabled":enabled,"order":order},"dirty":rev>0});
    let frames = [
        json!({"t":"snap","topic":"settings.store.dashboard-extras","seq":0,"data":store(0,json!([]),json!(["focus","notifications"]))}),
        json!({"t":"call","id":1,"m":"settings.set","p":{"store":"dashboard-extras","key":["enabled"],"value":["focus"],"rev":0}}),
        json!({"t":"ret","id":1,"ok":store(1,json!(["focus"]),json!(["focus","notifications"]))}),
        json!({"t":"call","id":2,"m":"settings.set","p":{"store":"dashboard-extras","key":["order"],"value":["notifications","focus"],"rev":0}}),
        json!({"t":"ret","id":2,"err":{"code":"stale_revision","message":"Settings changed; refresh before writing","retry":true}}),
        json!({"t":"call","id":3,"m":"settings.set","p":{"store":"dashboard-extras","key":["enabled"],"value":["focus",{"x":1}],"rev":1}}),
        json!({"t":"ret","id":3,"err":{"code":"invalid_request","message":"Invalid settings value","retry":false}}),
    ];
    out.insert(
        "protocol/fixtures/settings-dashboard-extras.jsonl".into(),
        lines(&frames),
    );
    out
}
