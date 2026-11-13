// SPDX-License-Identifier: GPL-3.0-only
//! B0043: the AI sidebar as a Bead panel over the published `ai.chat` topic and its
//! methods (N0032). The daemon owns the provider workers; this module only reads
//! the bounded snapshot of the edge channel (`""`), turns it into windowed cells
//! for the Luau view, and sends the published calls. Closed, it holds nothing:
//! the topic lease, the layout caches and the heartbeat belong to the open panel.
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{
    Stores, Value,
    source::{Source, Subscription},
};
use bead::ui::{
    markdown,
    transcript::{Entry, Meter, Scale, Style, Transcript},
};
use nacre_protocol::ai_chat::{AiChannel, Provider, Role};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Default panel width; callers may pass another to `Chat::new`.
pub const WIDTH: f64 = 470.;
const SANS: &str = "IBM Plex Sans";
const MONO: &str = "JetBrainsMono Nerd Font";
const HEARTBEAT: Duration = Duration::from_secs(10);
const PULSE_TICK: Duration = Duration::from_millis(50);
const LAUNCHER_REFRESH: Duration = Duration::from_secs(10);
const LAUNCHER_CACHE_LIMIT: usize = 64 * 1024;
const LAUNCHER_CACHE_ROWS: usize = 30;
const OPEN: &str = "ai.chat.open";
const LAUNCHER_LIST: &str = "ai.launcher.list";
const MAX_QUESTIONS: usize = 4;

type Notify = Arc<dyn Fn(bead::wayland::UiEvent) + Send + Sync>;

// Keep the framework's Markdown and window caches, but reserve no message-action
// strip: Copy lives in the label band. Code-copy buttons retain their own strip.
struct CompactTranscript {
    inner: Transcript,
    measure: Transcript,
    heights: BTreeMap<String, (u64, f32, f32)>,
    rows: BTreeMap<String, (f32, f32, f32)>,
}
impl CompactTranscript {
    fn new(style: Style, meter: fn() -> Meter) -> Self {
        Self {
            inner: Transcript::new(style.clone(), meter()),
            measure: Transcript::new(style, meter()),
            heights: BTreeMap::new(),
            rows: BTreeMap::new(),
        }
    }
    fn meter(&self) -> &Meter {
        self.inner.meter()
    }
    fn style(&self) -> &Style {
        self.inner.style()
    }
    fn release(&mut self) {
        self.inner.release();
        self.measure.release();
        self.heights.clear();
        self.rows.clear();
    }
    fn layout(&mut self, entries: &[Entry]) -> f32 {
        self.inner.layout(entries);
        let scale = &self.inner.style().scale;
        let (gap, strip) = (scale.md, scale.xxl + 3.5 * scale.xs);
        // D7: a message without a label has no label band either (kit ask 1).
        let band = scale.md + scale.xs + 1.;
        let (mut original, mut compact) = (0., 0.);
        self.rows.clear();
        for entry in entries {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                &entry.text,
                &entry.label,
                &entry.fill,
                entry.markdown,
                &entry.code_action,
            )
                .hash(&mut hash);
            let hash = hash.finish();
            let cached = self
                .heights
                .entry(entry.key.clone())
                .or_insert((!hash, 0., 0.));
            if cached.0 != hash {
                let height = self.measure.layout(std::slice::from_ref(entry));
                let code = entry.markdown && !markdown::code_blocks(&entry.text).is_empty();
                let unlabeled = if entry.label.is_empty() { band } else { 0. };
                *cached = (
                    hash,
                    height,
                    height - if code { 0. } else { strip } - unlabeled,
                );
            }
            self.rows
                .insert(entry.key.clone(), (original, compact, cached.2));
            original += cached.1 + gap;
            compact += cached.2 + gap;
        }
        self.heights.retain(|key, _| self.rows.contains_key(key));
        (compact - gap).max(0.)
    }
    fn window(&mut self, scroll: f32, height: f32, entries: &[Entry]) -> Vec<Json> {
        let overscan = self.inner.style().overscan;
        let (low, high) = (scroll - overscan, scroll + height + overscan);
        let first = self
            .rows
            .values()
            .filter(|(_, top, h)| top + h >= low && *top <= high)
            .map(|(original, _, _)| *original)
            .reduce(f32::min)
            .unwrap_or(0.);
        let last = self
            .rows
            .iter()
            .filter(|(_, (_, top, h))| top + h >= low && *top <= high)
            .map(|(key, (original, _, _))| original + self.heights[key].1)
            .reduce(f32::max)
            .unwrap_or(first);
        // D7 stand-ins until the kit transcript aligns and labels per entry (kit ask 1):
        // an unlabeled message (the user's own) drops its label band, and a one-line one
        // hugs the right edge; code blocks sit in a `surface` block; captions are variant.
        let (width, pad, xs, band) = {
            let s = &self.inner.style().scale;
            (self.inner.style().width, s.md, s.xs, s.md + s.xs + 1.)
        };
        // Only the window's own user messages are measured, not the whole history.
        let narrow: BTreeMap<&str, Option<f32>> = entries
            .iter()
            .filter(|entry| {
                entry.label.is_empty()
                    && self
                        .rows
                        .get(&entry.key)
                        .is_some_and(|(_, top, h)| top + h >= low && *top <= high)
            })
            .map(|entry| {
                let single = !entry.text.contains('\n');
                let style = self.inner.style();
                let text = self
                    .inner
                    .meter()
                    .width(&entry.text, &style.sans, style.scale.body);
                (
                    entry.key.as_str(),
                    (single && text + 2. * pad < width).then_some(text + 2. * pad),
                )
            })
            .collect();
        let mut cells = self.inner.window(first, last - first, entries);
        cells.retain_mut(|cell| {
            let id = cell["id"].as_str().unwrap_or_default();
            let Some((key, index)) = id.rsplit_once(':') else {
                return false;
            };
            let Some(&(original, top, h)) = self.rows.get(key) else {
                return false;
            };
            if top + h < low || top > high {
                return false;
            }
            let unlabeled = narrow.get(key).copied();
            let (bubble, label) = (index == "0", index == "1");
            if unlabeled.is_some() && label {
                return false;
            }
            let key = key.to_owned();
            let lift = if unlabeled.is_some() && !bubble {
                band
            } else {
                0.
            };
            cell["y"] =
                json!(cell["y"].as_f64().unwrap_or_default() + f64::from(top - original - lift));
            cell["message"] = json!(key);
            if bubble {
                cell["h"] = json!(h);
            }
            if let Some(Some(w)) = unlabeled {
                cell["w"] = json!(if bubble { w } else { w - 2. * pad });
                cell["x"] = json!(width - w + if bubble { 0. } else { pad });
            }
            if label {
                cell["c"] = json!("onSurfaceVariant");
            } else if !bubble && cell["b"] == "surfaceContainerHigh" {
                cell["b"] = json!("surface");
            }
            true
        });
        let scale = &self.inner.style().scale;
        let button = scale.xxl;
        let mut ordered = Vec::with_capacity(cells.len() * 2);
        for cell in cells {
            // Keep each hover action adjacent to its message in keyboard order.
            let copy = cell["id"].as_str().unwrap().ends_with(":0").then(|| {
                let key = cell["message"].as_str().unwrap();
                let x = match narrow.get(key) {
                    Some(Some(w)) => width - w - button - xs,
                    _ => width - button - xs,
                };
                json!({
                    "id": format!("{key}:copy"), "message": key, "k": "button",
                    "a": format!("copy:{key}"), "t": tr("bead.panel.chat-copy", &[]),
                    "x": x,
                    "y": cell["y"].as_f64().unwrap() + f64::from(scale.xs),
                    "w": button, "h": button, "s": scale.caption,
                })
            });
            ordered.push(cell);
            ordered.extend(copy);
        }
        ordered
    }
}

// ---- geometry ----------------------------------------------------------------

/// Panel geometry as multiples of the shared size scale; nothing here names a pixel.
struct Metrics {
    edge: f64,
    gap: f64,
    button: f64,
    header: f64,
    inner: f32,
    body: f32,
    info: f32,
    composer_min: f64,
    composer_max: f64,
    question_min: f64,
    question_max: f64,
}
impl Metrics {
    fn new(width: f64, scale: &Scale) -> Self {
        let (xs, sm, md) = (
            f64::from(scale.xs),
            f64::from(scale.sm),
            f64::from(scale.md),
        );
        let edge = md + xs;
        let button = 2. * md + 2.5 * xs;
        let header = f64::from((scale.body * 1.4).ceil() + (scale.caption * 1.4).ceil()) + xs / 4.;
        Self {
            edge,
            gap: sm,
            button,
            header,
            inner: (width - 2. * edge) as f32,
            body: scale.body,
            info: scale.caption,
            composer_min: button + sm,
            composer_max: 40. * xs,
            question_min: 20. * xs,
            question_max: 60. * xs,
        }
    }
}

// ---- state -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Open,
    Send,
    Plain,
    LauncherList,
    LauncherAction,
}
struct Pending {
    call: Call,
    ticket: CallTicket,
}
struct Parsed {
    key: (String, u64),
    available: bool,
    channel: Option<Arc<AiChannel>>,
}
#[derive(Default)]
struct St {
    launcher_handoff: bool,
    pending: Vec<Pending>,
    opened_at: Option<Instant>,
    message: String,
    follow: bool,
    written: f64,
    viewport: (f32, f32),
    parsed: Option<Parsed>,
    thread: String,
    question: String,
    launcher: Option<Json>,
    launcher_asked: Option<Instant>,
}
struct Ticker {
    _stop: std::sync::mpsc::Sender<()>,
    period: Duration,
}
pub struct Chat {
    source: Arc<dyn Source>,
    client: Option<Client>,
    transcript: RefCell<CompactTranscript>,
    metrics: Metrics,
    st: RefCell<St>,
    notify: RefCell<Option<Notify>>,
    ticker: RefCell<Option<Ticker>>,
    heartbeat: Duration,
    /// Message text by ordinal for the copy actions of the projected transcript.
    texts: RefCell<BTreeMap<String, String>>,
    memory: Option<crate::popouts::ChatMemory>,
    launcher_cache_path: Option<std::path::PathBuf>,
    /// The daemon's `capabilities` flags; `ai` absent or false: the optional package is missing.
    capabilities: Option<Arc<dyn Source>>,
}

/// What a view action asks the host to do besides updating stores.
#[derive(Default)]
pub struct Outcome {
    pub stores: BTreeMap<String, Value>,
    pub copy: Option<String>,
}

// The daemon publishes `send_failed` as an event, not a patch. The shell's one
// daemon link hands every event to `route_event`; the open panel drains the queue.
static FAILED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static WAKER: Mutex<Option<Notify>> = Mutex::new(None);
pub fn route_event(message: &nacre_protocol::Message) {
    use nacre_protocol::{Message, ai_chat};
    let Message::Ev {
        topic, name, data, ..
    } = message
    else {
        return;
    };
    if topic != ai_chat::TOPIC || name != ai_chat::EVENT_SEND_FAILED {
        return;
    }
    let Ok(failed) = serde_json::from_value::<ai_chat::AiSendFailed>(data.clone()) else {
        return;
    };
    if failed.validate().is_err() || !failed.channel.is_empty() {
        return;
    }
    {
        let mut queue = FAILED.lock().unwrap_or_else(|p| p.into_inner());
        if queue.len() < 8 {
            queue.push(failed.text);
        }
    }
    let waker = WAKER.lock().unwrap_or_else(|p| p.into_inner()).clone();
    if let Some(waker) = waker {
        waker(bead::wayland::UiEvent::Model(BTreeMap::new()));
    }
}

/// The shell's font policy for measuring: Fontconfig, as for painting.
pub fn system_meter() -> Meter {
    Meter::new(Arc::new(|scene| {
        crate::font_sources::select(scene).unwrap_or_default()
    }))
}
fn number(value: f64) -> Value {
    Value::Number(value)
}
fn text(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}
/// The launcher's rows for a search: running chats first (the D7 "Running" group), then the
/// daemon's order (newest first).
fn launcher_rows(data: &Json, query: &str) -> Vec<Json> {
    let mut rows: Vec<Json> = data["chats"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|row| {
            query.is_empty()
                || ["title", "provider"]
                    .into_iter()
                    .filter_map(|k| row[k].as_str())
                    .any(|value| value.to_lowercase().contains(query))
        })
        .collect();
    rows.sort_by_key(|row| row["state"] != "running");
    rows
}
fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Codex => "Codex",
        Provider::Claude => "Claude",
    }
}

impl Chat {
    fn default_launcher_cache_path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
            })?;
        Some(base.join("nacre/ai-panel/chats.json"))
    }
    fn load_launcher_cache(path: &std::path::Path) -> Option<Json> {
        use std::io::Read;
        let file = std::fs::File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() || metadata.len() > LAUNCHER_CACHE_LIMIT as u64 {
            return None;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take((LAUNCHER_CACHE_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > LAUNCHER_CACHE_LIMIT {
            return None;
        }
        let mut data: Json = serde_json::from_slice(&bytes).ok()?;
        if let Some(object) = data.as_object_mut() {
            object.retain(|key, _| matches!(key.as_str(), "defaultProvider" | "chats"));
            if object.get("defaultProvider").and_then(Json::as_str) != Some("claude") {
                object.insert("defaultProvider".into(), json!("codex"));
            }
        }
        let chats = data["chats"].as_array_mut()?;
        chats.truncate(LAUNCHER_CACHE_ROWS);
        chats.retain(|row| {
            row["key"]
                .as_str()
                .is_some_and(|v| !v.is_empty() && v.len() <= 256)
                && row["title"]
                    .as_str()
                    .is_some_and(|v| !v.is_empty() && v.chars().count() <= 256)
        });
        for row in chats {
            if let Some(object) = row.as_object_mut() {
                object.remove("project");
                object.retain(|key, _| {
                    matches!(
                        key.as_str(),
                        "key" | "title" | "provider" | "state" | "lastActivity"
                    )
                });
                // Cached activity is not current liveness evidence.
                object.insert("state".into(), json!("saved"));
            }
        }
        Some(data)
    }
    fn save_launcher_cache(path: &std::path::Path, data: &Json) {
        let Some(chats) = data["chats"].as_array() else {
            return;
        };
        let cached = json!({
            "defaultProvider": if data["defaultProvider"].as_str() == Some("claude") { "claude" } else { "codex" },
            "chats": chats.iter().filter(|row| row["key"].as_str().is_some_and(|v| !v.is_empty() && v.len() <= 256)
                && row["title"].as_str().is_some_and(|v| !v.is_empty() && v.chars().count() <= 256))
                .take(LAUNCHER_CACHE_ROWS).map(|row| json!({
                    "key": row["key"], "title": row["title"], "provider": row["provider"],
                    "state": row["state"], "lastActivity": row["lastActivity"]
                })).collect::<Vec<_>>()
        });
        let Ok(bytes) = serde_json::to_vec(&cached) else {
            return;
        };
        if bytes.len() > LAUNCHER_CACHE_LIMIT {
            return;
        }
        let Some(parent) = path.parent() else {
            return;
        };
        use std::os::unix::fs::DirBuilderExt;
        if std::fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(parent)
            .is_err()
            && !parent.is_dir()
        {
            return;
        }
        static NEXT_CACHE_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let suffix = NEXT_CACHE_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp = path.with_extension(format!("{}.{}.tmp", std::process::id(), suffix));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let Ok(mut file) = options.open(&temp) else {
            return;
        };
        use std::io::Write;
        if file.write_all(&bytes).is_ok() {
            drop(file);
            if std::fs::rename(&temp, path).is_err() {
                let _ = std::fs::remove_file(temp);
            }
        } else {
            let _ = std::fs::remove_file(temp);
        }
    }
    pub(crate) fn launcher_active(&self, stores: &Stores) -> bool {
        stores.get("panel.open") != Some(&Value::Bool(false))
            && stores.get("chat.section") != Some(&Value::Text("chat".into()))
    }

    pub(crate) fn launcher_search_focused(&self, stores: &Stores) -> bool {
        self.launcher_active(stores)
            && stores.get("bead.focused") == Some(&Value::Text("chat.search".into()))
    }

    pub fn new(client: Option<Client>, source: Arc<dyn Source>, meter: fn() -> Meter) -> Self {
        let launcher_cache_path = if cfg!(test) {
            None
        } else {
            Self::default_launcher_cache_path()
        };
        let cached_launcher = launcher_cache_path
            .as_deref()
            .and_then(Self::load_launcher_cache);
        let mut style = Style::new(WIDTH as f32, SANS, MONO);
        let metrics = Metrics::new(WIDTH, &style.scale);
        style.width = metrics.inner;
        Self {
            source,
            client,
            transcript: RefCell::new(CompactTranscript::new(style, meter)),
            metrics,
            st: RefCell::new(St {
                launcher: cached_launcher,
                follow: true,
                viewport: (1200., 800.),
                ..Default::default()
            }),
            notify: Default::default(),
            ticker: Default::default(),
            heartbeat: HEARTBEAT,
            texts: Default::default(),
            memory: None,
            launcher_cache_path,
            capabilities: None,
        }
    }
    /// Feed an unbounded synthetic channel into projection tests without weakening the
    /// published `ai.chat` validator used by production sources.
    #[cfg(test)]
    pub(crate) fn set_test_channel(&self, channel: AiChannel) {
        let snapshot = self.source.snapshot();
        self.st.borrow_mut().parsed = Some(Parsed {
            key: (snapshot.epoch.clone(), snapshot.revision),
            available: true,
            channel: Some(Arc::new(channel)),
        });
    }
    pub fn with_capabilities(mut self, capabilities: Option<Arc<dyn Source>>) -> Self {
        self.capabilities = capabilities;
        self
    }
    /// N0234: published flags without `ai` mean Nacre AI is not installed (unknown: assume it is).
    fn missing(&self) -> bool {
        self.capabilities.as_ref().is_some_and(|source| {
            let snapshot = source.snapshot();
            snapshot.connected
                && snapshot.data["flags"].is_object()
                && snapshot.data["flags"]["ai"] != true
        })
    }
    /// Process-local keep of the draft and section while the panel is closed.
    pub fn with_memory(mut self, memory: crate::popouts::ChatMemory) -> Self {
        self.memory = Some(memory);
        self
    }
    #[cfg(test)]
    pub(crate) fn with_launcher_cache_path(mut self, path: std::path::PathBuf) -> Self {
        self.st.borrow_mut().launcher = Self::load_launcher_cache(&path);
        self.launcher_cache_path = Some(path);
        self
    }
    pub fn remembered(&self) -> BTreeMap<String, Value> {
        self.memory
            .as_ref()
            .and_then(|memory| memory.lock().ok().map(|saved| saved.clone()))
            .unwrap_or_default()
    }
    fn remember(&self, key: &str, value: &str) {
        if let Some(memory) = &self.memory
            && let Ok(mut saved) = memory.lock()
            && value.len() <= 65536
            && saved.get(key) != Some(&Value::Text(value.into()))
        {
            saved.insert(key.into(), Value::Text(value.into()));
        }
    }
    /// Heartbeat period (tests shorten it).
    #[cfg(test)]
    pub fn with_heartbeat(mut self, heartbeat: Duration) -> Self {
        self.heartbeat = heartbeat;
        self
    }
    pub fn set_viewport(&self, _width: u32, height: u32) {
        self.st.borrow_mut().viewport.1 = height as f32;
    }
    pub fn subscribe(&self, notify: Notify) -> bead::core::source::Result<Vec<Subscription>> {
        *self.notify.borrow_mut() = Some(notify.clone());
        *WAKER.lock().unwrap_or_else(|p| p.into_inner()) = Some(notify.clone());
        let callback: bead::core::source::Callback = {
            let notify = notify.clone();
            Arc::new(move |_| notify(bead::wayland::UiEvent::Model(BTreeMap::new())))
        };
        let mut subscriptions = vec![self.source.subscribe(callback.clone())?];
        if let Some(capabilities) = &self.capabilities {
            subscriptions.push(capabilities.subscribe(callback.clone())?);
        }
        if let Some(client) = &self.client {
            subscriptions.push(client.subscribe(callback)?);
        }
        Ok(subscriptions)
    }

    fn ready(&self, method: &str) -> Option<(String, Duration)> {
        if self.missing() {
            return None;
        }
        let client = self.client.as_ref()?;
        let status = client.status();
        let advertised = status.methods.get(method).filter(|m| m.v == 1)?;
        if !client.snapshot().connected {
            return None;
        }
        Some((
            status.boot.clone()?,
            Duration::from_millis(u64::from(advertised.timeout).clamp(1, 45_000)),
        ))
    }
    fn begin(&self, call: Call, method: &str, params: Json, wake: CallWake) -> bool {
        let mut st = self.st.borrow_mut();
        let Some(client) = &self.client else {
            st.message = "bead.panel.chat-unavailable".into();
            return false;
        };
        let Some((boot, timeout)) = self.ready(method) else {
            st.message = "bead.panel.chat-unavailable".into();
            return false;
        };
        if st.pending.len() >= 8 {
            return false;
        }
        if call == Call::LauncherAction
            && st
                .pending
                .iter()
                .any(|pending| pending.call == Call::LauncherAction)
        {
            return false;
        }
        match client.begin_call(
            method.into(),
            params,
            timeout,
            CallGuard { boot, version: 1 },
            Some(wake),
        ) {
            Ok(ticket) => {
                st.pending.push(Pending { call, ticket });
                // The heartbeat must not wipe an error the reader has not seen yet.
                if call != Call::Open {
                    st.message.clear();
                }
                true
            }
            Err(_) => {
                st.message = "bead.common.request-failed".into();
                false
            }
        }
    }
    fn waker(&self) -> CallWake {
        let notify = self.notify.borrow().clone();
        Arc::new(move || {
            if let Some(notify) = &notify {
                notify(bead::wayland::UiEvent::Model(BTreeMap::new()));
            }
        })
    }
    /// The open heartbeat: one wake per interval while the panel is shown.
    fn tick(&self, live: bool, working: bool) {
        let mut ticker = self.ticker.borrow_mut();
        if !live {
            *ticker = None;
            return;
        }
        let period = if working { PULSE_TICK } else { self.heartbeat };
        if ticker
            .as_ref()
            .is_some_and(|current| current.period == period)
        {
            return;
        }
        *ticker = None;
        let Some(notify) = self.notify.borrow().clone() else {
            return;
        };
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new()
            .name("chat-heartbeat".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    wait.recv_timeout(period)
                {
                    notify(bead::wayland::UiEvent::Model(BTreeMap::new()));
                }
            });
        if spawned.is_ok() {
            *ticker = Some(Ticker {
                _stop: stop,
                period,
            });
        }
    }

    fn poll(&self) {
        let mut st = self.st.borrow_mut();
        let mut index = 0;
        while index < st.pending.len() {
            let Some(reply) = st.pending[index].ticket.try_take() else {
                index += 1;
                continue;
            };
            let call = st.pending.remove(index).call;
            match (call, reply) {
                (Call::LauncherList, Ok(value)) => {
                    let warning = value["warning"].as_str().unwrap_or("");
                    let mut clean = value.clone();
                    if let Some(object) = clean.as_object_mut() {
                        object.remove("projects");
                        object.remove("warning");
                    }
                    if let Some(chats) = clean["chats"].as_array_mut() {
                        chats.truncate(LAUNCHER_CACHE_ROWS);
                        chats.retain(|row| {
                            row["key"]
                                .as_str()
                                .is_some_and(|v| !v.is_empty() && v.len() <= 256)
                                && row["title"]
                                    .as_str()
                                    .is_some_and(|v| !v.is_empty() && v.chars().count() <= 256)
                        });
                        for row in chats {
                            if let Some(object) = row.as_object_mut() {
                                object.remove("project");
                            }
                        }
                    }
                    let has_rows = clean["chats"]
                        .as_array()
                        .is_some_and(|rows| !rows.is_empty());
                    if warning.is_empty() || has_rows {
                        if let Some(path) = &self.launcher_cache_path {
                            Self::save_launcher_cache(path, &clean);
                        }
                        st.launcher = Some(clean);
                    }
                    st.launcher_asked = Some(Instant::now());
                    st.message = if warning.is_empty() {
                        String::new()
                    } else {
                        "bead.common.request-failed".into()
                    };
                }
                (Call::LauncherList, Err(_)) => {
                    st.launcher_asked = Some(Instant::now());
                    st.message = "bead.common.request-failed".into();
                }
                (Call::LauncherAction, Ok(_)) => {
                    st.launcher_asked = None;
                    st.launcher_handoff = true;
                }
                (Call::LauncherAction, Err(_)) => {
                    st.launcher_asked = None;
                    st.message = "bead.common.request-failed".into();
                }
                (Call::Open, Ok(_)) => st.opened_at = Some(Instant::now()),
                (_, Ok(_)) => {}
                // Retry at the next heartbeat, not in a loop.
                (Call::Open, Err(_)) => {
                    st.opened_at = Some(Instant::now());
                    st.message = "bead.common.request-failed".into();
                }
                (_, Err(e)) if e.code == "stale_revision" => {
                    st.message = "bead.common.changed-elsewhere".into();
                }
                (_, Err(_)) => st.message = "bead.common.request-failed".into(),
            }
        }
    }

    fn parsed(&self, st: &mut St) {
        let snapshot = self.source.snapshot();
        let key = (snapshot.epoch.clone(), snapshot.revision);
        if st.parsed.as_ref().is_some_and(|p| p.key == key) {
            return;
        }
        let available = snapshot.connected && snapshot.data["available"].as_bool() == Some(true);
        let channel = snapshot
            .data
            .get("channels")
            .and_then(|channels| channels.get(""))
            .and_then(|value| serde_json::from_value::<AiChannel>(value.clone()).ok())
            .filter(|channel| channel.validate().is_ok());
        let channel = channel.map(Arc::new);
        // An invalid or missing update keeps the last good channel while disconnected.
        let channel = channel.or_else(|| st.parsed.take().and_then(|p| p.channel));
        st.parsed = Some(Parsed {
            key,
            available,
            channel,
        });
    }

    /// Project the open panel. `stores` supplies the composer text, the scroll offset
    /// and the answer fields; a hidden panel drops its caches and projects nothing.
    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        let open = stores.get("panel.open") != Some(&Value::Bool(false));
        if !open {
            self.tick(false, false);
            let mut st = self.st.borrow_mut();
            st.parsed = None;
            self.transcript.borrow_mut().release();
            return BTreeMap::new();
        }
        self.poll();
        let launcher_visible =
            !matches!(stores.get("chat.section"), Some(Value::Text(section)) if section == "chat");
        let working = self
            .st
            .borrow()
            .launcher
            .as_ref()
            .and_then(|value| value["chats"].as_array())
            .is_some_and(|rows| {
                launcher_visible && rows.iter().any(|row| row["state"] == "running")
            });
        self.tick(true, working);
        let handoff = std::mem::take(&mut self.st.borrow_mut().launcher_handoff);
        let section = match stores.get("chat.section") {
            Some(Value::Text(s)) => s.clone(),
            _ => "launcher".into(),
        };
        let draft = match stores.get("chat.draft") {
            Some(Value::Text(s)) => s.clone(),
            _ => String::new(),
        };
        let wake = self.waker();
        let mut out = BTreeMap::new();
        out.insert("chat.missing".into(), Value::Bool(self.missing()));
        if handoff {
            out.insert("bead.focused".into(), text(""));
            if stores.get("view.detached") != Some(&Value::Bool(true))
                && stores.get("chat.pinned") != Some(&Value::Bool(true))
            {
                out.insert("panel.open".into(), Value::Bool(false));
            }
        }
        out.insert("chat.section".into(), text(section.clone()));
        self.remember("chat.section", &section);
        let labels = ["bead.panel.chat-new", "bead.panel.chat-workspace", "bead.panel.launcher-new-in-project", "bead.panel.chat-codex", "bead.panel.chat-claude"]
            .into_iter().map(|id| {
                let label = tr(id, &[]);
                let transcript = self.transcript.borrow();
                let meter = transcript.meter();
                json!({"text": label,"body": meter.width(&label,SANS,self.metrics.body),"caption": meter.width(&label,SANS,self.metrics.info)})
            }).collect();
        out.insert("chat.labels".into(), Value::list(labels));
        // Keep the worker alive and recover after a reconnect (QML: open on start, every 10 s).
        let stale = section == "chat" && {
            let st = self.st.borrow();
            st.opened_at.is_none_or(|at| at.elapsed() >= self.heartbeat)
                && !st.pending.iter().any(|p| p.call == Call::Open)
        };
        if stale && self.ready(OPEN).is_some() {
            self.begin(Call::Open, OPEN, json!({"channel": ""}), wake.clone());
        }
        if self.ready(OPEN).is_none() {
            self.st.borrow_mut().opened_at = None;
        }
        if section != "chat" {
            let should_list = {
                let st = self.st.borrow();
                !st.pending.iter().any(|p| p.call == Call::LauncherList)
                    && st
                        .launcher_asked
                        .is_none_or(|at| at.elapsed() >= LAUNCHER_REFRESH)
            };
            if should_list
                && self.ready(LAUNCHER_LIST).is_some()
                && self.begin(Call::LauncherList, LAUNCHER_LIST, json!({}), wake.clone())
            {
                self.st.borrow_mut().launcher_asked = Some(Instant::now());
            }
            let st = self.st.borrow();
            let data = st.launcher.as_ref().cloned().unwrap_or_else(|| json!({}));
            let query = match stores.get("launcher.search") {
                Some(Value::Text(s)) => s.to_lowercase(),
                _ => String::new(),
            };
            let filtered = launcher_rows(&data, &query);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let mut group = "";
            let rows: Vec<Json> = filtered
                .iter()
                .map(|row| {
                    let provider = match row["provider"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .as_str()
                    {
                        "claude" => "Claude",
                        "codex" => "Codex",
                        _ => "",
                    };
                    let state = match row["state"].as_str().unwrap_or("").to_lowercase().as_str() {
                        "running" => "running",
                        _ => "idle",
                    };
                    let timestamp = row["lastActivity"].as_f64().unwrap_or(0.);
                    let age = (now - timestamp).max(0.);
                    let count = ((age / 60.).floor() as u64).to_string();
                    let last = if timestamp <= 0. {
                        tr("bead.panel.launcher-activity-unknown", &[])
                    } else if age < 60. {
                        tr("bead.panel.launcher-just-now", &[])
                    } else if age < 3600. {
                        tr("bead.panel.launcher-minutes-ago", &[("count", &count)])
                    } else if age < 86400. {
                        tr(
                            "bead.panel.launcher-hours-ago",
                            &[("count", &((age / 3600.).floor() as u64).to_string())],
                        )
                    } else {
                        tr(
                            "bead.panel.launcher-days-ago",
                            &[("count", &((age / 86400.).floor() as u64).to_string())],
                        )
                    };
                    let next = if state == "running" {
                        "bead.panel.launcher-running"
                    } else if age < 86400. {
                        "bead.panel.launcher-today"
                    } else {
                        "bead.panel.launcher-earlier"
                    };
                    let heading = (next != group).then(|| tr(next, &[]));
                    group = next;
                    let mut item = row.clone();
                    if let Some(obj) = item.as_object_mut() {
                        obj.remove("project");
                        obj.insert("provider".into(), json!(provider));
                        obj.insert("state".into(), json!(state));
                        obj.insert("lastActivityLabel".into(), json!(last));
                        obj.insert("group".into(), json!(heading.unwrap_or_default()));
                    }
                    item
                })
                .collect();
            out.insert("launcher.chats".into(), Value::list(rows));
            let pulse = if working {
                let phase = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();
                (0.5 + 0.5 * (phase * std::f64::consts::TAU / 1.6).sin()).clamp(0., 1.)
            } else {
                0.
            };
            out.insert("launcher.pulse".into(), number(pulse));
            let selected_provider = match stores.get("launcher.provider") {
                Some(Value::Text(v)) if matches!(v.as_str(), "claude" | "codex") => v.clone(),
                _ => data["defaultProvider"]
                    .as_str()
                    .filter(|v| matches!(*v, "codex" | "claude"))
                    .unwrap_or("codex")
                    .to_owned(),
            };
            out.insert("launcher.provider".into(), text(selected_provider.clone()));
            out.insert(
                "launcher.search".into(),
                text(match stores.get("launcher.search") {
                    Some(Value::Text(v)) => v.clone(),
                    _ => String::new(),
                }),
            );
            let selected = match stores.get("launcher.selected") {
                Some(Value::Text(v))
                    if filtered.iter().any(|row| row["key"].as_str() == Some(v)) =>
                {
                    v.clone()
                }
                _ => filtered
                    .first()
                    .and_then(|v| v["key"].as_str())
                    .unwrap_or("")
                    .to_owned(),
            };
            out.insert("launcher.selected".into(), text(selected));
            let warning = data["warning"].as_str().unwrap_or("");
            out.insert(
                "launcher.warning".into(),
                text(if !warning.is_empty() {
                    tr("bead.common.request-failed", &[])
                } else if !st.message.is_empty() {
                    tr(&st.message, &[])
                } else if self.ready(LAUNCHER_LIST).is_none() {
                    tr("bead.panel.chat-unavailable", &[])
                } else {
                    String::new()
                }),
            );
            out.insert(
                "launcher.quick_chat".into(),
                Value::Bool(stores.get("launcher.quick_chat") == Some(&Value::Bool(true))),
            );
            out.insert(
                "launcher.empty_state".into(),
                text(
                    if !filtered.is_empty()
                        || !st.message.is_empty()
                        || !warning.is_empty()
                        || self.ready(LAUNCHER_LIST).is_none()
                    {
                        String::new()
                    } else if st.launcher.is_none() && self.ready(LAUNCHER_LIST).is_some() {
                        tr("bead.panel.launcher-loading", &[])
                    } else if !query.is_empty() {
                        tr("bead.panel.launcher-empty-search", &[])
                    } else {
                        tr("bead.panel.launcher-empty-history", &[])
                    },
                ),
            );
            let action_pending = st.pending.iter().any(|p| p.call == Call::LauncherAction);
            out.insert(
                "launcher.can_new".into(),
                Value::Bool(
                    !action_pending
                        && !selected_provider.is_empty()
                        && self.ready("ai.launcher.new").is_some(),
                ),
            );
            out.insert(
                "launcher.can_open".into(),
                Value::Bool(
                    !action_pending
                        && self.ready("ai.launcher.open").is_some()
                        && !filtered.is_empty(),
                ),
            );
            let scale = self.transcript.borrow().style().scale.clone();
            let preferred = (f64::from(st.viewport.1) - 150.)
                .clamp(105. * f64::from(scale.xs), 202.5 * f64::from(scale.xs));
            let height = preferred.min((f64::from(st.viewport.1) - 2. * self.metrics.edge).max(1.));
            out.insert("chat.height".into(), number(height));
            out.insert("chat.edge".into(), number(self.metrics.edge));
            out.insert("chat.button".into(), number(self.metrics.button));
            out.insert("chat.inner".into(), number(f64::from(self.metrics.inner)));
            if !handoff && stores.get("bead.focused").is_none() {
                out.insert("bead.focused".into(), text("chat.search"));
            }
            return out;
        }
        out.insert("launcher.quick_chat".into(), Value::Bool(true));
        let mut st = self.st.borrow_mut();
        self.parsed(&mut st);
        let connected = st.parsed.as_ref().is_some_and(|p| p.available);
        let channel = st.parsed.as_ref().and_then(|p| p.channel.clone());
        let state = channel.as_ref().map(|c| c.state.clone());
        let in_workspace = state.as_ref().is_some_and(|s| s.in_workspace);
        let busy = state.as_ref().is_some_and(|s| s.busy);
        // A refused submission puts its text back in front of what was typed since.
        let failed: Vec<String> =
            std::mem::take(&mut *FAILED.lock().unwrap_or_else(|p| p.into_inner()));
        let restored = failed.into_iter().fold(draft.clone(), |current, lost| {
            if current.is_empty() {
                lost
            } else {
                format!("{lost}\n\n{current}")
            }
        });
        if restored != draft {
            out.insert("chat.draft".into(), text(restored.clone()));
        }
        let draft = restored;
        self.remember("chat.draft", &draft);

        // Geometry (QML LeftDrawer: 470 wide, at most 810 high and 150 short of the screen).
        let m = &self.metrics;
        let transcript = self.transcript.borrow();
        let meter = transcript.meter();
        let scale = transcript.style().scale.clone();
        let unit = f64::from(scale.xs);
        let height = (f64::from(st.viewport.1) - 150.).clamp(105. * unit, 202.5 * unit);
        let bottom = height - m.edge;
        let composer_h = meter.height(&draft, SANS, m.body, m.inner - 2. * scale.md) as f64
            + 2. * f64::from(scale.md);
        let composer = composer_h.clamp(m.composer_min, m.composer_max);
        // D7: one card holds the field (top) and its action row (bottom), `gap` inside each edge.
        let composer_top = bottom - m.gap - m.button - f64::from(scale.xs) - composer - m.gap;
        let mut floor = composer_top - m.gap;

        // Approval / question card.
        let question = state.as_ref().and_then(|s| s.question.clone());
        let qid = question
            .as_ref()
            .and_then(|q| q["id"].as_str())
            .unwrap_or("")
            .to_owned();
        if qid != st.question {
            st.question = qid.clone();
            for i in 0..MAX_QUESTIONS {
                out.insert(format!("chat.answer.{i}"), text(""));
            }
        }
        let answer = |i: usize| -> String {
            match out
                .get(&format!("chat.answer.{i}"))
                .or_else(|| stores.get(&format!("chat.answer.{i}")))
            {
                Some(Value::Text(s)) => s.clone(),
                _ => String::new(),
            }
        };
        let mut rows = Vec::new();
        let mut qextent = 0f32;
        let mut all_answered = question.is_some();
        if let Some(question) = &question {
            let items = question["questions"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for (i, item) in items.iter().take(MAX_QUESTIONS).enumerate() {
                let prompt = item["question"].as_str().unwrap_or("");
                let h = meter
                    .height(prompt, SANS, m.body, m.inner - 2. * scale.md)
                    .max(m.body * 1.4);
                rows.push(json!({"id": format!("q{i}"), "k": "text", "i": i, "t": prompt, "h": h, "sel": false, "a": ""}));
                qextent += h + 6.;
                let current = answer(i);
                for (j, option) in item["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(8)
                    .enumerate()
                {
                    let label = option["label"].as_str().unwrap_or("").to_owned();
                    rows.push(json!({"id": format!("q{i}o{j}"), "k": "option", "i": i, "t": label, "h": 32, "sel": current.trim() == label && !label.is_empty(), "a": format!("option:{i}:{j}")}));
                    qextent += 37.;
                }
                rows.push(json!({"id": format!("q{i}i"), "k": "input", "i": i, "t": "", "h": 35, "sel": false, "a": ""}));
                qextent += 45.;
                all_answered &= !current.trim().is_empty();
            }
            if items.is_empty() || items.len() > MAX_QUESTIONS {
                all_answered = false;
            }
            rows.push(json!({"id": "reply", "k": "reply", "i": 0, "t": "", "h": 36, "sel": all_answered && !in_workspace, "a": "reply"}));
            qextent += 36.;
        }
        let question_h = if question.is_some() {
            f64::from(qextent)
                .clamp(m.question_min, m.question_max)
                .min(height * 0.35)
                .max(m.question_min)
        } else {
            0.
        };
        let question_top = floor - question_h;
        if question.is_some() {
            floor = question_top - 8.;
        }

        // Status or error line.
        let status_text = match &state {
            Some(s) if !s.error.is_empty() => s.error.clone(),
            Some(s) if !s.status.is_empty() => s.status.clone(),
            _ if !st.message.is_empty() => tr(&st.message, &[]),
            None if !connected => tr("bead.panel.chat-connecting", &[]),
            _ => String::new(),
        };
        let is_error =
            state.as_ref().is_some_and(|s| !s.error.is_empty()) || !st.message.is_empty();
        let status_h = if status_text.is_empty() {
            0.
        } else {
            f64::from(meter.height(&status_text, SANS, m.info, m.inner))
                .max(f64::from(m.info) * 1.4)
        };
        let status_top = floor - status_h;
        if status_h > 0. {
            floor = status_top - 8.;
        }
        let transcript_top = m.edge + m.header + f64::from(scale.xs);
        let transcript_h = (floor + m.gap - 10. - transcript_top).max(60.);
        drop(transcript);

        // Messages: the reusable transcript lays them out and windows them.
        let mut texts: BTreeMap<String, String> = BTreeMap::new();
        let mut entries = Vec::new();
        if let Some(channel) = &channel {
            for (ordinal, message) in &channel.messages {
                let user = message.role == Role::User;
                texts.insert(ordinal.clone(), message.text.clone());
                // D7: yours in primary-container without a caption, the assistant's in
                // container-high captioned with the provider (no per-message time in the topic).
                entries.push(Entry {
                    key: ordinal.clone(),
                    label: if user {
                        String::new()
                    } else {
                        provider_name(channel.state.provider).to_lowercase()
                    },
                    fill: if user {
                        "primaryContainer"
                    } else {
                        "surfaceContainerHigh"
                    }
                    .into(),
                    text: message.text.clone(),
                    markdown: !user,
                    actions: Vec::new(),
                    code_action: Some((tr("bead.panel.chat-copy-code", &[]), "code".into())),
                });
            }
        }
        let extent = self.transcript.borrow_mut().layout(&entries);

        // Follow the newest text until the reader scrolls away; clamp to the content.
        let max_scroll = f64::from((extent - transcript_h as f32).max(0.));
        let current = match stores.get("chat.scroll") {
            Some(Value::Number(n)) => *n,
            _ => 0.,
        };
        let thread = state
            .as_ref()
            .map_or(String::new(), |s| s.thread_id.clone());
        if thread != st.thread {
            st.thread = thread;
            st.follow = true;
        } else if current != st.written {
            st.follow = current >= max_scroll;
        }
        let scroll = if st.follow {
            max_scroll
        } else {
            current.clamp(0., max_scroll)
        };
        st.written = scroll;
        out.insert("chat.scroll".into(), number(scroll));

        // Only the cells that can be seen are handed to the view.
        let visible =
            self.transcript
                .borrow_mut()
                .window(scroll as f32, transcript_h as f32, &entries);
        let cells_empty = entries.is_empty();

        // Header and footer state.
        let provider = state
            .as_ref()
            .map_or(String::new(), |s| provider_name(s.provider).to_owned());
        let title = state
            .as_ref()
            .map_or_else(|| tr("bead.panel.chat-new", &[]), |s| s.title.clone());
        let subtitle = format!(
            "{} · {}",
            if provider.is_empty() {
                tr("bead.panel.chat-assistant", &[])
            } else {
                provider
            },
            tr(
                if busy {
                    "bead.panel.launcher-running"
                } else {
                    "bead.panel.launcher-idle"
                },
                &[]
            )
        );
        let trimmed = draft.trim();
        let placeholder = if in_workspace {
            "bead.panel.chat-placeholder-workspace"
        } else if busy {
            "bead.panel.chat-placeholder-busy"
        } else {
            "bead.panel.chat-placeholder"
        };
        let methods_ok = self.ready("ai.chat.send").is_some();
        let mut set = |key: &str, value: Value| {
            out.insert(key.into(), value);
        };
        set("chat.height", number(height));
        set("chat.transcript_top", number(transcript_top));
        set("chat.transcript_height", number(transcript_h));
        set("chat.extent", number(f64::from(extent)));
        set("chat.cells", Value::list(visible));
        set("chat.edge", number(m.edge));
        set("chat.button", number(m.button));
        set("chat.inner", number(f64::from(m.inner)));
        set("chat.empty", Value::Bool(cells_empty));
        set("chat.title", text(title));
        set("chat.subtitle", text(subtitle));
        set("chat.status", text(status_text));
        set("chat.status_error", Value::Bool(is_error));
        set("chat.status_top", number(status_top));
        set("chat.status_height", number(status_h));
        set("chat.question_top", number(question_top));
        set("chat.question_height", number(question_h));
        set("chat.question_rows", Value::list(rows));
        set("chat.question_extent", number(f64::from(qextent)));
        set("chat.composer_top", number(composer_top));
        set("chat.composer_height", number(composer));
        set("chat.busy", Value::Bool(busy));
        set("chat.workspace", Value::Bool(in_workspace));
        set("chat.can_new", Value::Bool(methods_ok && !busy));
        set(
            "chat.can_workspace",
            Value::Bool(
                methods_ok && !busy && state.as_ref().is_some_and(|s| !s.thread_id.is_empty()),
            ),
        );
        set(
            "chat.can_send",
            Value::Bool(methods_ok && !in_workspace && !trimmed.is_empty()),
        );
        set(
            "chat.can_reply",
            Value::Bool(methods_ok && !in_workspace && all_answered),
        );
        set("chat.can_stop", Value::Bool(methods_ok && busy));
        set("chat.placeholder", text(tr(placeholder, &[])));
        set(
            "chat.provider",
            text(state.as_ref().map_or("", |s| match s.default_provider {
                Provider::Codex => "codex",
                Provider::Claude => "claude",
            })),
        );
        set("chat.has_question", Value::Bool(question.is_some()));
        drop(st);
        *self.texts.borrow_mut() = texts;
        out
    }
}

impl Chat {
    fn state(&self) -> Option<nacre_protocol::ai_chat::AiState> {
        let mut st = self.st.borrow_mut();
        self.parsed(&mut st);
        st.parsed
            .as_ref()?
            .channel
            .as_ref()
            .map(|channel| channel.state.clone())
    }
    fn plain(&self, method: &str, extra: Json) {
        let mut params = json!({"channel": ""});
        if let (Some(base), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
            base.extend(extra.clone());
        }
        self.begin(Call::Plain, method, params, self.waker());
    }
    /// Run one view action. Stores to change and text to copy are returned; every
    /// daemon call is the published method, sent once and never retried.
    fn send(
        &self,
        draft: &str,
        state: Option<&nacre_protocol::ai_chat::AiState>,
        out: &mut Outcome,
    ) {
        if draft.trim().is_empty() || state.is_none_or(|s| s.in_workspace) {
            return;
        }
        if draft.chars().count() > nacre_protocol::ai_chat::MAX_SEND_CHARS {
            self.st.borrow_mut().message = "bead.panel.chat-too-long".into();
            return;
        }
        let params = json!({"channel": "", "text": draft, "attachments": []});
        if self.begin(Call::Send, "ai.chat.send", params, self.waker()) {
            out.stores.insert("chat.draft".into(), text(""));
            self.st.borrow_mut().follow = true;
        }
    }
    pub fn act(&self, arg: &str, stores: &Stores) -> Outcome {
        let mut out = Outcome::default();
        if stores.get("chat.menu.open") == Some(&Value::Bool(true)) {
            out.stores
                .insert("chat.menu.open".into(), Value::Bool(false));
        }
        let value = |key: &str| match stores.get(key) {
            Some(Value::Text(s)) => s.clone(),
            _ => String::new(),
        };
        let state = self.state();
        let busy = state.as_ref().is_some_and(|s| s.busy);
        let (verb, rest) = arg.split_once(':').unwrap_or((arg, ""));
        match verb {
            "launcher" => match rest {
                "refresh" => self.st.borrow_mut().launcher_asked = None,
                "new" => {
                    let provider = match value("launcher.provider").as_str() {
                        "codex" => "codex".to_owned(),
                        "claude" => "claude".to_owned(),
                        _ => self
                            .st
                            .borrow()
                            .launcher
                            .as_ref()
                            .and_then(|data| data["defaultProvider"].as_str())
                            .filter(|value| matches!(*value, "codex" | "claude"))
                            .map(str::to_owned)
                            .unwrap_or_else(|| "codex".to_owned()),
                    };
                    self.begin(
                        Call::LauncherAction,
                        "ai.launcher.new",
                        json!({"provider": provider, "project": "general-chat"}),
                        self.waker(),
                    );
                }
                "quick-chat" => {
                    out.stores.insert("chat.section".into(), text("chat"));
                    out.stores
                        .insert("launcher.quick_chat".into(), Value::Bool(true));
                    out.stores
                        .insert("bead.focused".into(), text("chat.composer"));
                    // D7: the launcher composer's text goes straight out with the chosen provider.
                    let provider = value("launcher.provider");
                    if matches!(provider.as_str(), "codex" | "claude")
                        && state.as_ref().is_some_and(|s| {
                            provider_name(s.default_provider).to_lowercase() != provider
                        })
                    {
                        self.plain("ai.chat.provider", json!({"provider": provider}));
                    }
                    if !value("chat.draft").trim().is_empty() {
                        self.send(&value("chat.draft"), state.as_ref(), &mut out);
                    }
                }
                "back" => {
                    out.stores.insert("chat.section".into(), text("launcher"));
                    out.stores
                        .insert("launcher.quick_chat".into(), Value::Bool(false));
                    out.stores
                        .insert("bead.focused".into(), text("chat.search"));
                }
                "open-selected" => {
                    let key = value("launcher.selected");
                    if !key.is_empty() {
                        self.begin(
                            Call::LauncherAction,
                            "ai.launcher.open",
                            json!({"key": key}),
                            self.waker(),
                        );
                    }
                }
                "move:up" | "move:down" | "move:-1" | "move:1" => {
                    let launcher = self
                        .st
                        .borrow()
                        .launcher
                        .clone()
                        .unwrap_or_else(|| json!({}));
                    let rows = launcher_rows(&launcher, &value("launcher.search").to_lowercase());
                    let selected = value("launcher.selected");
                    let index = rows
                        .iter()
                        .position(|row| row["key"].as_str() == Some(selected.as_str()))
                        .unwrap_or(0);
                    let up = rest == "move:up" || rest == "move:-1";
                    let next = if up {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(rows.len().saturating_sub(1))
                    };
                    if let Some(key) = rows.get(next).and_then(|row| row["key"].as_str()) {
                        out.stores.insert("launcher.selected".into(), text(key));
                    }
                }
                "brain" | "settings" => {
                    self.begin(
                        Call::LauncherAction,
                        "ai.launcher.action",
                        json!({"action": rest}),
                        self.waker(),
                    );
                }
                _ if rest.starts_with("open:") => {
                    let key = &rest[5..];
                    if !key.is_empty() {
                        self.begin(
                            Call::LauncherAction,
                            "ai.launcher.open",
                            json!({"key": key}),
                            self.waker(),
                        );
                    }
                }
                _ if rest.starts_with("provider:") => {
                    out.stores
                        .insert("launcher.provider".into(), text(&rest[9..]));
                }
                _ => {}
            },
            "pin" => {
                out.stores.insert(
                    "chat.pinned".into(),
                    Value::Bool(stores.get("chat.pinned") != Some(&Value::Bool(true))),
                );
            }
            "send" => self.send(&value("chat.draft"), state.as_ref(), &mut out),
            "stop" => self.plain("ai.chat.cancel", json!({})),
            "new" if !busy => {
                self.plain("ai.chat.new", json!({}));
                out.stores.insert("chat.section".into(), text("chat"));
                out.stores
                    .insert("bead.focused".into(), text("chat.composer"));
            }
            "workspace" if !busy => self.plain("ai.chat.workspace", json!({})),
            "provider" if matches!(rest, "codex" | "claude") => {
                self.plain("ai.chat.provider", json!({"provider": rest}));
            }
            "resume" if !busy && !rest.is_empty() => {
                self.plain("ai.chat.resume", json!({"key": rest}));
                out.stores.insert("chat.section".into(), text("chat"));
                self.st.borrow_mut().follow = true;
            }
            "reply" => {
                let Some(question) = state.and_then(|s| s.question) else {
                    return out;
                };
                let items = question["questions"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut answers = serde_json::Map::new();
                for (i, item) in items.iter().take(MAX_QUESTIONS).enumerate() {
                    let given = value(&format!("chat.answer.{i}"));
                    let id = item["id"].as_str().unwrap_or("");
                    if given.trim().is_empty() || id.is_empty() {
                        return out;
                    }
                    answers.insert(id.into(), json!({"answers": [given]}));
                }
                if items.is_empty() || items.len() > MAX_QUESTIONS {
                    return out;
                }
                let id = question["id"].as_str().unwrap_or("");
                self.plain("ai.chat.approve", json!({"id": id, "answers": answers}));
            }
            "option" => {
                let mut parts = rest.split(':').filter_map(|p| p.parse::<usize>().ok());
                if let (Some(i), Some(j), Some(question)) =
                    (parts.next(), parts.next(), state.and_then(|s| s.question))
                    && let Some(label) = question["questions"][i]["options"][j]["label"].as_str()
                    && i < MAX_QUESTIONS
                {
                    out.stores.insert(format!("chat.answer.{i}"), text(label));
                }
            }
            "copy" => out.copy = self.texts.borrow().get(rest).cloned(),
            "code" => {
                let (ordinal, index) = rest.split_once(':').unwrap_or((rest, "0"));
                out.copy = self.texts.borrow().get(ordinal).and_then(|message| {
                    markdown::code_blocks(message)
                        .into_iter()
                        .nth(index.parse().ok()?)
                });
            }
            "section" if matches!(rest, "launcher" | "chat") => {
                out.stores.insert("chat.section".into(), text(rest));
                if rest == "launcher" {
                    out.stores
                        .insert("launcher.quick_chat".into(), Value::Bool(false));
                }
            }
            _ => {}
        }
        out
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
pub(crate) mod tests;
