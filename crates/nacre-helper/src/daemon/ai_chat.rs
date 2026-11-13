//! Daemon host for the sidebar chat. Each open channel is a nice-10 worker
//! process (`sidebar-chat worker`, the N0023 session manager on stdio); the
//! daemon only reduces its events into the bounded `ai.chat` snapshot and
//! forwards commands. A worker stops 30 s after it is idle, or at once when
//! its pipe closes, so no provider child outlives the daemon.
use super::hub::{failure, Shared};
use crate::Result;
use nacre_protocol::{
    ai_chat::{
        AiApprove, AiChannel, AiChannelRef, AiChat, AiMessage, AiProvider, AiResume, AiSavedList,
        AiSend, AiState, Provider, Role, EVENT_SEND_FAILED, MAX_CHANNELS, MAX_MESSAGES,
        MAX_MESSAGES_JSON, MAX_QUESTION_JSON, MAX_SAVED, MAX_TEXT_JSON, TOPIC,
    },
    ErrorCode, Failure, Method,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Stdio,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
    sync::{watch, Notify},
    task::JoinSet,
};

const EVENT_LINE: usize = 20 * 1024 * 1024;
const START_WAIT: Duration = Duration::from_secs(8);
const WRITE_WAIT: Duration = Duration::from_secs(3);
const COALESCE: Duration = Duration::from_millis(40);

pub fn methods() -> BTreeMap<String, Method> {
    let mut methods = nacre_protocol::ai_chat::methods();
    methods.extend(nacre_protocol::ai_launcher::methods());
    methods
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub, &BTreeMap::new())
}
fn publish(hub: &Shared, channels: &BTreeMap<String, AiChannel>) -> Result<()> {
    let chat = AiChat {
        available: true,
        channels: channels.clone(),
    };
    chat.validate()?;
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(chat)?)?;
    Ok(())
}

// ---- event reduction (the QML `accept` semantics over a bounded window) ----

fn clamp(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
fn escaped(c: char) -> usize {
    match c {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
        c if (c as u32) < 0x20 => 6,
        c => c.len_utf8(),
    }
}
fn escaped_len(text: &str) -> usize {
    text.chars().map(escaped).sum()
}
/// The longest prefix whose JSON-escaped body fits `max` bytes.
fn fit(text: &str, max: usize) -> (&str, bool) {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        used += escaped(c);
        if used > max {
            return (&text[..index], true);
        }
    }
    (text, false)
}
fn provider(value: &Value) -> Provider {
    if value == "claude" {
        Provider::Claude
    } else {
        Provider::Codex
    }
}
fn state_of(event: &Value) -> AiState {
    let text = |key: &str, max: usize| clamp(event[key].as_str().unwrap_or(""), max);
    AiState {
        default_provider: provider(&event["defaultProvider"]),
        in_workspace: event["inWorkspace"].as_bool().unwrap_or(false),
        provider: provider(&event["provider"]),
        account: text("account", 128),
        title: text("title", 1024),
        thread_id: text("threadId", 128),
        model: text("model", 128),
        features: event["features"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .take(8)
            .map(|f| clamp(f, 32))
            .collect(),
        composer_key: text("composerKey", 128),
        busy: event["busy"].as_bool().unwrap_or(false),
        status: text("status", 512),
        error: text("error", 1024),
        question: Some(&event["question"])
            .filter(|q| q.is_object() && q.to_string().len() <= MAX_QUESTION_JSON)
            .cloned(),
    }
}

fn key(id: &Value) -> String {
    clamp(
        &id.as_str().map_or_else(|| id.to_string(), str::to_owned),
        256,
    )
}
struct Entry {
    ordinal: u64,
    message: AiMessage,
    size: usize,
}
#[derive(Default)]
struct View {
    state: Option<AiState>,
    messages: Vec<Entry>,
    next: u64,
    trimmed: bool,
}
impl View {
    fn push(&mut self, id: &Value, role: Role, text: &str) {
        let (kept, cut) = fit(text, MAX_TEXT_JSON - 2);
        self.messages.push(Entry {
            ordinal: self.next,
            message: AiMessage {
                id: key(id),
                role,
                text: kept.to_owned(),
                truncated: cut,
            },
            size: escaped_len(kept) + 2,
        });
        self.next += 1;
    }
    fn find(&mut self, id: &Value) -> Option<&mut Entry> {
        let id = key(id);
        self.messages.iter_mut().rev().find(|e| e.message.id == id)
    }
    fn put(&mut self, id: &Value, role: Role, text: &str) {
        let (kept, cut) = fit(text, MAX_TEXT_JSON - 2);
        match self.find(id) {
            Some(entry) => {
                entry.message.text = kept.to_owned();
                entry.message.truncated = cut;
                entry.size = escaped_len(kept) + 2;
            }
            None => self.push(id, role, text),
        }
    }
    fn delta(&mut self, id: &Value, text: &str) {
        match self.find(id) {
            Some(entry) => {
                let (kept, cut) = fit(text, MAX_TEXT_JSON.saturating_sub(entry.size));
                entry.message.text.push_str(kept);
                entry.message.truncated |= cut;
                entry.size += escaped_len(kept);
            }
            None => self.push(id, Role::Assistant, text),
        }
    }
    fn trim(&mut self) {
        let mut total: usize = self
            .messages
            .iter()
            .map(|e| e.size + e.message.id.len())
            .sum();
        while self.messages.len() > 1
            && (self.messages.len() > MAX_MESSAGES || total > MAX_MESSAGES_JSON)
        {
            let dropped = self.messages.remove(0);
            total -= dropped.size + dropped.message.id.len();
            self.trimmed = true;
        }
    }
    /// Apply one worker event; returns the unsent text of a refused submission.
    fn apply(&mut self, event: &Value) -> Option<Value> {
        match event["type"].as_str().unwrap_or("") {
            "state" => self.state = Some(state_of(event)),
            "history" => {
                self.messages.clear();
                self.trimmed = false;
                for item in event["messages"].as_array().into_iter().flatten() {
                    let role = match item["role"].as_str() {
                        Some("user") => Role::User,
                        Some("assistant") => Role::Assistant,
                        _ => continue,
                    };
                    self.push(&item["id"], role, item["text"].as_str().unwrap_or(""));
                }
                self.trim();
            }
            "message" => {
                let role = if event["role"] == "user" {
                    Role::User
                } else {
                    Role::Assistant
                };
                self.put(&event["id"], role, event["text"].as_str().unwrap_or(""));
                self.trim();
            }
            "delta" => {
                self.delta(&event["id"], event["text"].as_str().unwrap_or(""));
                self.trim();
            }
            "sendFailed" => {
                return Some(json!({
                    "text": event["text"],
                    "attachments": event["attachments"].as_array().cloned().unwrap_or_default(),
                }));
            }
            _ => {}
        }
        None
    }
    fn channel(&self) -> Option<AiChannel> {
        Some(AiChannel {
            state: self.state.clone()?,
            messages: self
                .messages
                .iter()
                .map(|e| {
                    (
                        format!("{:010}", e.ordinal % 10_000_000_000),
                        e.message.clone(),
                    )
                })
                .collect(),
            trimmed: self.trimmed,
        })
    }
}

// ---- hosted workers ----

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Starting,
    Ready,
    Gone,
}
struct Channel {
    input: tokio::sync::Mutex<Option<ChildStdin>>,
    view: RefCell<View>,
    phase: watch::Sender<Phase>,
}
#[derive(Default)]
struct Hosted {
    channels: RefCell<BTreeMap<String, Rc<Channel>>>,
    dirty: Notify,
}
pub struct Host {
    hosted: Rc<Hosted>,
    tasks: RefCell<JoinSet<()>>,
    spawn: Box<dyn Fn(&str) -> Command>,
    /// Reads the saved-chat list (blocking); replaced by tests.
    saved: Arc<dyn Fn() -> std::result::Result<Value, String> + Send + Sync>,
    listing: Cell<bool>,
    launcher_listing: Arc<AtomicBool>,
    launcher_cancel: Arc<AtomicBool>,
    launcher_records: RefCell<BTreeMap<String, Value>>,
}
impl Default for Host {
    fn default() -> Self {
        Self::new(Box::new(|channel| {
            // Keep the daemon's exact image even if an upgrade replaced its path.
            let mut command = Command::new("/proc/self/exe");
            command.args(["sidebar-chat", "worker", "--channel", channel]);
            for name in [
                "NOTIFY_SOCKET",
                "WATCHDOG_USEC",
                "WATCHDOG_PID",
                "LISTEN_PID",
                "LISTEN_FDS",
                "LISTEN_FDNAMES",
            ] {
                command.env_remove(name);
            }
            command
        }))
    }
}
fn unavailable(message: &str) -> Failure {
    failure(ErrorCode::Unavailable, message, true).dispatch_state(false)
}
async fn bounded_line(
    reader: &mut (impl AsyncBufRead + Unpin),
    line: &mut Vec<u8>,
) -> std::io::Result<usize> {
    line.clear();
    let count = (&mut *reader)
        .take(EVENT_LINE as u64 + 1)
        .read_until(b'\n', line)
        .await?;
    if line.len() > EVENT_LINE {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    Ok(count)
}
impl Host {
    pub fn new(spawn: Box<dyn Fn(&str) -> Command>) -> Self {
        Self {
            hosted: Rc::default(),
            tasks: RefCell::default(),
            spawn,
            saved: Arc::new(|| {
                let paths = crate::paths::Paths::environment().map_err(|e| e.to_string())?;
                crate::desktop_extras::chat_list(&paths).map_err(|e| e.to_string())
            }),
            listing: Cell::new(false),
            launcher_listing: Arc::new(AtomicBool::new(false)),
            launcher_cancel: Arc::new(AtomicBool::new(false)),
            launcher_records: RefCell::new(BTreeMap::new()),
        }
    }
    /// The QML "Recent chats" list: same helpers and key table as `ai.chat.resume`.
    async fn saved(&self) -> std::result::Result<Value, Failure> {
        if self.listing.replace(true) {
            return Err(
                failure(ErrorCode::Busy, "Saved chats are being read", true).dispatch_state(false)
            );
        }
        let read = self.saved.clone();
        let rows = tokio::time::timeout(
            Duration::from_secs(40),
            tokio::task::spawn_blocking(move || read()),
        )
        .await;
        self.listing.set(false);
        let rows = match rows {
            Ok(Ok(Ok(rows))) => rows,
            _ => return Err(unavailable("Could not read saved chats")),
        };
        let list = AiSavedList {
            chats: rows
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|row| serde_json::from_value(row.clone()).ok())
                .take(MAX_SAVED)
                .collect(),
        };
        list.validate().map_err(|f| f.dispatch_state(false))?;
        serde_json::to_value(list).map_err(|_| unavailable("Could not read saved chats"))
    }
    async fn launcher_list(&self) -> std::result::Result<Value, Failure> {
        if self
            .launcher_listing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(
                failure(ErrorCode::Busy, "Launcher history is being read", true)
                    .dispatch_state(false),
            );
        }
        self.launcher_cancel.store(false, Ordering::Release);
        let active = self.launcher_listing.clone();
        let cancellation = self.launcher_cancel.clone();
        let read = tokio::task::spawn_blocking(move || {
            struct Release(Arc<AtomicBool>);
            impl Drop for Release {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Release);
                }
            }
            let _release = Release(active);
            crate::ai_dashboard::collect_launcher_history(|| cancellation.load(Ordering::Acquire))
                .map_err(|e| e.to_string())
        });
        let result = tokio::time::timeout(Duration::from_secs(45), read).await;
        let (records, warnings) = match result {
            Ok(Ok(Ok(rows))) => rows,
            _ => {
                self.launcher_cancel.store(true, Ordering::Release);
                return Err(unavailable("Could not read AI chat history"));
            }
        };
        let mut saved = BTreeMap::new();
        let mut chats = Vec::new();
        for item in records {
            if chats.len() >= 30 {
                break;
            }
            append_launcher_row(&mut saved, &mut chats, item);
        }
        self.launcher_records.replace(saved);
        let default_provider =
            crate::ai_core::default_assistant()
                .ok()
                .map_or(Provider::Codex, |value| {
                    if value == "claude" {
                        Provider::Claude
                    } else {
                        Provider::Codex
                    }
                });
        let list = nacre_protocol::ai_launcher::List {
            chats,
            projects: Vec::new(),
            default_provider,
            warning: launcher_text(&warnings.join("; "), 2048),
        };
        list.validate().map_err(|f| f.dispatch_state(false))?;
        serde_json::to_value(list).map_err(|_| unavailable("Could not encode AI chat history"))
    }
    fn start(&self, hub: &Shared, name: &str) -> std::result::Result<Rc<Channel>, Failure> {
        if self.hosted.channels.borrow().len() >= MAX_CHANNELS {
            return Err(
                failure(ErrorCode::Busy, "Too many assistant chats are open", true)
                    .dispatch_state(false),
            );
        }
        let mut command = (self.spawn)(name);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = command
            .spawn()
            .map_err(|_| unavailable("Assistant worker could not start"))?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(unavailable("Assistant worker could not start"));
        };
        let channel = Rc::new(Channel {
            input: tokio::sync::Mutex::new(Some(input)),
            view: RefCell::default(),
            phase: watch::channel(Phase::Starting).0,
        });
        self.hosted
            .channels
            .borrow_mut()
            .insert(name.to_owned(), channel.clone());
        let (hosted, hub, name) = (self.hosted.clone(), hub.clone(), name.to_owned());
        let task_channel = channel.clone();
        self.tasks.borrow_mut().spawn_local(async move {
            let channel = task_channel;
            let mut reader = BufReader::new(output);
            let mut line = Vec::new();
            while matches!(bounded_line(&mut reader, &mut line).await, Ok(1..)) {
                let Ok(event) = serde_json::from_slice::<Value>(&line) else {
                    continue;
                };
                let refused = channel.view.borrow_mut().apply(&event);
                if let Some(mut data) = refused {
                    data["channel"] = json!(name);
                    let valid = serde_json::from_value::<nacre_protocol::ai_chat::AiSendFailed>(
                        data.clone(),
                    )
                    .is_ok_and(|v| v.validate().is_ok());
                    if valid {
                        let _ = hub.borrow_mut().event(TOPIC, EVENT_SEND_FAILED, data);
                    }
                }
                if event["type"] == "state" {
                    channel.phase.send_replace(Phase::Ready);
                }
                hosted.dirty.notify_one();
            }
            // Stdout closed or oversized: stop the worker through its pipe.
            channel.input.lock().await.take();
            {
                let mut channels = hosted.channels.borrow_mut();
                if channels.get(&name).is_some_and(|c| Rc::ptr_eq(c, &channel)) {
                    channels.remove(&name);
                }
            }
            channel.phase.send_replace(Phase::Gone);
            hosted.dirty.notify_one();
            if tokio::time::timeout(Duration::from_secs(12), child.wait())
                .await
                .is_err()
            {
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
        });
        Ok(channel)
    }
    async fn open(&self, hub: &Shared, name: &str) -> std::result::Result<(), Failure> {
        let existing = self.hosted.channels.borrow().get(name).cloned();
        let channel = match existing {
            Some(channel) => {
                // Any frame refreshes the worker's idle timer.
                let _ = self.write(&channel, &json!({"action":"ping"})).await;
                channel
            }
            None => self.start(hub, name)?,
        };
        let mut phase = channel.phase.subscribe();
        let ready =
            match tokio::time::timeout(START_WAIT, phase.wait_for(|p| *p != Phase::Starting)).await
            {
                Ok(Ok(phase)) => *phase == Phase::Ready,
                _ => false,
            };
        if !ready {
            // Release the channel lock before the caller falls back to the socket server.
            channel.input.lock().await.take();
            return Err(unavailable("Assistant worker is unavailable"));
        }
        Ok(())
    }
    async fn write(&self, channel: &Channel, command: &Value) -> std::io::Result<()> {
        let mut frame = serde_json::to_vec(command)?;
        frame.push(b'\n');
        let mut input = channel.input.lock().await;
        let Some(input) = input.as_mut() else {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        };
        tokio::time::timeout(WRITE_WAIT, async {
            input.write_all(&frame).await?;
            input.flush().await
        })
        .await
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
    }
    async fn command(&self, name: &str, command: Value) -> std::result::Result<(), Failure> {
        let channel = self.hosted.channels.borrow().get(name).cloned();
        let Some(channel) = channel.filter(|c| *c.phase.borrow() == Phase::Ready) else {
            return Err(unavailable("Assistant is not connected"));
        };
        // A failed write may still have reached the worker; never replay it.
        self.write(&channel, &command).await.map_err(|_| {
            failure(ErrorCode::Unavailable, "Assistant connection failed", true)
                .dispatch_state(true)
        })
    }
    fn snapshot(&self) -> BTreeMap<String, AiChannel> {
        self.hosted
            .channels
            .borrow()
            .iter()
            .filter_map(|(name, c)| Some((name.clone(), c.view.borrow().channel()?)))
            .collect()
    }
}

/// Publishes the topic whenever a worker changes, at most once per interval.
pub async fn run(hub: Shared) -> Result<()> {
    let host = Rc::new(Host::default());
    hub.borrow_mut().ai_chat = Some(host.clone());
    hub.borrow_mut().service("ai-chat", "running", 0)?;
    loop {
        host.hosted.dirty.notified().await;
        tokio::time::sleep(COALESCE).await;
        publish(&hub, &host.snapshot())?;
    }
}

fn parse<T: DeserializeOwned>(params: Value) -> std::result::Result<T, Failure> {
    serde_json::from_value(params).map_err(|_| {
        failure(ErrorCode::Validation, "Invalid assistant request", false).dispatch_state(false)
    })
}
fn checked<T>(
    value: std::result::Result<T, Failure>,
    validate: impl Fn(&T) -> std::result::Result<(), Failure>,
) -> std::result::Result<T, Failure> {
    let value = value?;
    validate(&value).map_err(|f| f.dispatch_state(false))?;
    Ok(value)
}
pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    let host = hub
        .borrow()
        .ai_chat
        .clone()
        .ok_or_else(|| unavailable("Assistant service is unavailable"))?;
    if method.starts_with("ai.launcher.") {
        return dispatch_launcher(hub, host, method, params).await;
    }
    let (channel, command) = match method {
        "ai.chat.open" => {
            let request = checked(parse::<AiChannelRef>(params), AiChannelRef::validate)?;
            host.open(&hub, &request.channel).await?;
            return Ok(json!({"channel": request.channel}));
        }
        "ai.chat.saved" => {
            checked(parse::<AiChannelRef>(params), AiChannelRef::validate)?;
            return host.saved().await;
        }
        "ai.chat.send" => {
            let r = checked(parse::<AiSend>(params), AiSend::validate)?;
            let command = json!({"action":"send","text":r.text,"attachments":r.attachments});
            (r.channel, command)
        }
        "ai.chat.approve" => {
            let r = checked(parse::<AiApprove>(params), AiApprove::validate)?;
            let command = json!({"action":"answer","id":r.id,"answers":r.answers});
            (r.channel, command)
        }
        "ai.chat.resume" => {
            let r = checked(parse::<AiResume>(params), AiResume::validate)?;
            (r.channel, json!({"action":"load","key":r.key}))
        }
        "ai.chat.provider" => {
            let r = checked(parse::<AiProvider>(params), AiProvider::validate)?;
            let name = if r.provider == Provider::Claude {
                "claude"
            } else {
                "codex"
            };
            (r.channel, json!({"action":"provider","provider":name}))
        }
        "ai.chat.cancel" | "ai.chat.new" | "ai.chat.workspace" => {
            let r = checked(parse::<AiChannelRef>(params), AiChannelRef::validate)?;
            let action = match method {
                "ai.chat.cancel" => "stop",
                "ai.chat.new" => "new",
                _ => "workspace",
            };
            (r.channel, json!({"action":action}))
        }
        _ => return Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
    };
    host.command(&channel, command).await?;
    Ok(json!({"channel": channel}))
}

async fn dispatch_launcher(
    hub: Shared,
    host: Rc<Host>,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    use nacre_protocol::ai_launcher as launcher;
    match method {
        "ai.launcher.list" => {
            checked(parse::<launcher::Empty>(params), |_| Ok(()))?;
            host.launcher_list().await
        }
        "ai.launcher.open" => {
            let request = checked(parse::<launcher::Open>(params), launcher::Open::validate)?;
            let cached = host.launcher_records.borrow().clone();
            let (item, unique_title) = if let Some(item) = cached.get(&request.key).cloned() {
                let unique = cached
                    .values()
                    .filter(|row| row["title"] == item["title"])
                    .count()
                    == 1;
                (item, unique)
            } else {
                // A cached UI row can outlive this daemon's in-memory map.
                // Rediscover only local general chats and match the opaque key.
                let history = tokio::task::spawn_blocking(|| {
                    crate::ai_dashboard::collect_launcher_history(|| false)
                });
                let (rows, _) = match tokio::time::timeout(Duration::from_secs(45), history).await {
                    Ok(Ok(Ok(value))) => value,
                    _ => return Err(unavailable("Could not refresh local chat history")),
                };
                find_local_launcher_record(rows, &request.key).ok_or_else(|| {
                    failure(
                        ErrorCode::Validation,
                        "Chat is no longer in the launcher list",
                        false,
                    )
                    .dispatch_state(false)
                })?
            };
            open_launcher_record(hub, item, request.key, unique_title).await
        }
        "ai.launcher.new" => {
            let request = checked(parse::<launcher::New>(params), launcher::New::validate)?;
            let project = request.project;
            let provider = if request.provider == Provider::Claude {
                "claude"
            } else {
                "codex"
            };
            let provider = provider.to_owned();
            let launched = tokio::task::spawn_blocking(move || {
                crate::ai_dashboard::launcher_new(&provider, &project)
            });
            match tokio::time::timeout(Duration::from_secs(5), launched).await {
                Ok(Ok(Ok(()))) => Ok(json!({"ok":true})),
                _ => Err(unavailable("Could not start new AI chat")),
            }
        }
        "ai.launcher.latest" => {
            checked(parse::<launcher::Empty>(params), |_| Ok(()))?;
            // Resume latest stays independent of an in-flight panel refresh.
            let history = tokio::task::spawn_blocking(|| {
                crate::ai_dashboard::collect_launcher_history(|| false)
            });
            let (rows, _warnings) =
                match tokio::time::timeout(Duration::from_secs(45), history).await {
                    Ok(Ok(Ok(value))) => value,
                    _ => return Err(unavailable("Could not read latest AI chat")),
                };
            if rows.is_empty() {
                return Err(unavailable("Latest chat is unavailable"));
            }
            let unique_title = rows
                .iter()
                .filter(|row| row["title"] == rows[0]["title"])
                .count()
                == 1;
            let item = rows
                .into_iter()
                .next()
                .ok_or_else(|| unavailable("Latest chat is no longer available"))?;
            let key = launcher_key(&item);
            let _ = open_launcher_record(hub, item, key, unique_title).await?;
            Ok(json!({"ok":true}))
        }
        "ai.launcher.action" => {
            let request = checked(parse::<launcher::Action>(params), |_| Ok(()))?;
            let action = match request.action {
                launcher::LauncherAction::Brain => "brain",
                launcher::LauncherAction::Settings => "settings",
                launcher::LauncherAction::Projects => "projects",
            };
            let action = action.to_owned();
            let launched =
                tokio::task::spawn_blocking(move || crate::ai_dashboard::launcher_action(&action));
            match tokio::time::timeout(Duration::from_secs(5), launched).await {
                Ok(Ok(Ok(()))) => Ok(json!({"ok":true})),
                _ => Err(unavailable("Could not open AI launcher action")),
            }
        }
        _ => Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
    }
}

pub(super) async fn open_launcher_record(
    hub: Shared,
    item: Value,
    key: String,
    unique_title: bool,
) -> std::result::Result<Value, Failure> {
    if !launcher_record_is_local(&item) {
        return Err(failure(
            ErrorCode::Validation,
            "Remote chats are unavailable in the launcher",
            false,
        )
        .dispatch_state(false));
    }
    let snapshot = hub.borrow().snapshot("compositor");
    let existing = snapshot
        .as_ref()
        .and_then(|snapshot| existing_chat_window(snapshot, &item, &key, unique_title));
    if let Some(window) = existing {
        super::shell_actions::compositor(hub.clone(), "window.focus", json!({"window":window}))
            .await?;
        return Ok(json!({"ok":true,"focused":true}));
    }
    let opened =
        tokio::task::spawn_blocking(move || crate::ai_dashboard::launcher_open_record(&item, &key));
    match tokio::time::timeout(Duration::from_secs(5), opened).await {
        Ok(Ok(Ok(()))) => Ok(json!({"ok":true,"focused":false})),
        _ => Err(unavailable("Could not open AI chat window")),
    }
}

fn launcher_record_is_local(item: &Value) -> bool {
    item["project"]["host"].is_null()
}

fn find_local_launcher_record(rows: Vec<Value>, key: &str) -> Option<(Value, bool)> {
    let index = rows
        .iter()
        .position(|row| launcher_key(row) == key && launcher_record_is_local(row))?;
    let title = rows[index]["title"].clone();
    let unique_title = rows.iter().filter(|row| row["title"] == title).count() == 1;
    Some((rows.into_iter().nth(index)?, unique_title))
}

pub(super) fn launcher_key(item: &Value) -> String {
    let identity = item["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(|id| json!({"id":id}))
        .unwrap_or_else(|| json!({"session":item["session"]}));
    let identity = json!({
        "chat":identity, "agent":item["agent"], "account":item["account"],
        "project":item["project"]["id"], "host":item["project"]["host"],
        "path":item["project"]["path"],
    })
    .to_string();
    let hash = identity.bytes().fold(0xcbf29ce484222325u64, |state, byte| {
        (state ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("chat{hash:016x}")
}

fn append_launcher_row(
    records: &mut BTreeMap<String, Value>,
    chats: &mut Vec<nacre_protocol::ai_launcher::Chat>,
    item: Value,
) {
    let Some(title) = item["title"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
    else {
        return;
    };
    if !launcher_record_is_local(&item) {
        return;
    }
    let key = launcher_key(&item);
    if records.contains_key(&key) {
        return;
    }
    records.insert(key.clone(), item.clone());
    chats.push(nacre_protocol::ai_launcher::Chat {
        key,
        title: launcher_text(title, 256),
        // The wire type still requires this legacy field; the panel no longer
        // renders it and no project is associated with the chat.
        project: "general-chat".into(),
        provider: if item["agent"] == "claude" {
            Provider::Claude
        } else {
            Provider::Codex
        },
        state: launcher_text(item["state"].as_str().unwrap_or("saved"), 32),
        last_activity: item["updated_at"].as_f64().unwrap_or(0.0),
    });
}

fn launcher_text(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max)
        .collect()
}

fn existing_chat_window(
    snapshot: &Value,
    item: &Value,
    key: &str,
    unique_title: bool,
) -> Option<String> {
    let paths = crate::paths::Paths::environment().ok();
    existing_chat_window_with_proc(
        snapshot,
        item,
        key,
        unique_title,
        std::path::Path::new("/proc"),
        paths.as_ref(),
        nix::unistd::getuid().as_raw(),
    )
}

fn existing_chat_window_with_proc(
    snapshot: &Value,
    item: &Value,
    key: &str,
    unique_title: bool,
    proc: &std::path::Path,
    paths: Option<&crate::paths::Paths>,
    uid: u32,
) -> Option<String> {
    let windows = snapshot["windows"].as_object()?;
    let class = format!("nacre-ai-chat-{key}");
    let chat_id = item["id"].as_str().filter(|id| !id.is_empty());
    if let Some((id, _)) = windows
        .iter()
        .find(|(_, window)| window["appId"].as_str() == Some(class.as_str()))
    {
        return Some(id.clone());
    }
    let matching_titles = windows
        .iter()
        .filter(|(_, window)| {
            window["appId"].as_str() == Some("nacre-ai-task")
                && window["title"].as_str() == item["title"].as_str()
        })
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let remote_open = item["project"]["host"].as_str().is_some()
        && item["session"]
            .as_str()
            .is_some_and(|session| !session.is_empty());
    for (id, window) in windows
        .iter()
        .filter(|(_, window)| {
            let app_id = window["appId"].as_str().unwrap_or("");
            app_id == "nacre-ai-task" || app_id.starts_with("nacre-ai-chat-")
        })
        .take(64)
    {
        let Some(pid) = window["pid"].as_i64() else {
            continue;
        };
        if let Some(paths) = paths {
            let info = crate::window_chat_title::resolve_info(pid, proc, paths, uid);
            if chat_id.is_some_and(|chat_id| info.thread_id == chat_id) {
                return Some(id.clone());
            }
        }
        if item["project"]["host"].as_str().is_none()
            && matches!(item["agent"].as_str(), Some("claude" | "codex"))
            && item["session"].as_str().is_some_and(|session| {
                crate::window_chat_title::has_tmux_session(pid, proc, uid, session)
            })
        {
            return Some(id.clone());
        }
    }
    (remote_open && unique_title && matching_titles.len() == 1).then(|| matching_titles[0].clone())
}

#[cfg(test)]
mod tests {
    use super::super::hub::{Hub, Notice};
    use super::*;

    #[test]
    fn launcher_key_is_stable_and_scoped_to_provider_project_and_account() {
        let row = json!({"id":"thread-1","agent":"codex","account":"work","project":{"id":"demo","host":"builder","path":"/repo"}});
        assert_eq!(launcher_key(&row), launcher_key(&row));
        let mut other = row.clone();
        other["account"] = json!("personal");
        assert_ne!(launcher_key(&row), launcher_key(&other));
        let mut session_row = row.clone();
        session_row["id"] = Value::Null;
        session_row["session"] = json!("tmux-session");
        let mut changed_session = session_row.clone();
        changed_session["session"] = json!("another-session");
        assert_ne!(launcher_key(&session_row), launcher_key(&changed_session));
        let mut saved_row = row.clone();
        saved_row["session"] = json!("tmux-session");
        assert_eq!(launcher_key(&row), launcher_key(&saved_row));
    }

    #[test]
    fn launcher_list_deduplicates_repeated_provider_rows() {
        let row = json!({"id":"thread-1","agent":"codex","account":"work","title":"Fixture","state":"idle","updated_at":1,"project":{"id":"demo","label":"Demo"}});
        let mut records = BTreeMap::new();
        let mut chats = Vec::new();
        append_launcher_row(&mut records, &mut chats, row.clone());
        append_launcher_row(&mut records, &mut chats, row);
        assert_eq!(chats.len(), 1);
        assert_eq!(records.len(), 1);
        assert_eq!(chats[0].project, "general-chat");
    }

    #[test]
    fn launcher_list_drops_remote_history_rows() {
        let row = json!({"id":"remote-thread","title":"Fixture","project":{"id":"demo","host":"builder"}});
        let mut records = BTreeMap::new();
        let mut chats = Vec::new();
        append_launcher_row(&mut records, &mut chats, row);
        assert!(chats.is_empty());
        assert!(records.is_empty());
    }

    #[test]
    fn launcher_reopens_cached_key_after_daemon_map_is_lost() {
        let row = json!({
            "id":"local-thread","title":"Fixture","agent":"codex",
            "project":{"id":"general-chat","path":"/chat"}
        });
        let key = launcher_key(&row);
        let (found, unique) = find_local_launcher_record(vec![row.clone()], &key).unwrap();
        assert_eq!(found, row);
        assert!(unique);
    }

    #[test]
    fn launcher_relookup_never_accepts_remote_records() {
        let row = json!({
            "id":"remote-thread","title":"Fixture","agent":"codex",
            "project":{"id":"general-chat","host":"builder"}
        });
        assert!(find_local_launcher_record(vec![row.clone()], &launcher_key(&row)).is_none());
    }

    #[test]
    fn launcher_focus_matches_its_owned_window_class() {
        let row = json!({"id":"thread-1","title":"Fixture","project":{"id":"demo"}});
        let key = launcher_key(&row);
        let mut windows = serde_json::Map::new();
        for index in 0..96 {
            windows.insert(
                format!("unrelated-{index:03}"),
                json!({"appId":"browser","title":"Other"}),
            );
        }
        windows.insert(
            "niri-window:42".into(),
            json!({"appId":format!("nacre-ai-chat-{key}")}),
        );
        let snapshot = json!({"windows":windows});
        assert_eq!(
            existing_chat_window(&snapshot, &row, &key, true).as_deref(),
            Some("niri-window:42")
        );
    }

    #[test]
    fn launcher_focus_uses_unique_exact_task_title_for_remote_sessions() {
        let row = json!({"id":"remote-thread","title":"Camera work","state":"running","session":"session-1","project":{"id":"demo","host":"builder"}});
        let snapshot =
            json!({"windows":{"niri-window:43":{"appId":"nacre-ai-task","title":"Camera work"}}});
        assert_eq!(
            existing_chat_window(&snapshot, &row, "unused", true).as_deref(),
            Some("niri-window:43")
        );
        assert_eq!(existing_chat_window(&snapshot, &row, "unused", false), None);
    }

    #[test]
    fn launcher_focus_never_uses_local_same_title_without_matching_thread() {
        let row = json!({"id":"local-thread","title":"Camera work","project":{"id":"demo","path":"/repo"}});
        let snapshot =
            json!({"windows":{"niri-window:45":{"appId":"nacre-ai-task","title":"Camera work"}}});
        assert_eq!(existing_chat_window(&snapshot, &row, "unused", true), None);
    }

    #[test]
    fn launcher_focus_finds_local_tmux_clients_in_synthetic_proc_tree() -> crate::Result<()> {
        let fixture = tempfile::tempdir()?;
        let proc = fixture.path().join("proc");
        let parent = proc.join("100");
        std::fs::create_dir_all(parent.join("task/100"))?;
        std::fs::write(parent.join("task/100/children"), "101")?;
        std::fs::write(parent.join("cmdline"), b"/usr/bin/kitty\0")?;
        let client = proc.join("101");
        std::fs::create_dir_all(client.join("task/101"))?;
        std::fs::write(client.join("task/101/children"), "")?;
        std::fs::write(
            client.join("cmdline"),
            b"/usr/bin/tmux\0attach-session\0-t\0=nacre-chat-claude-1\0",
        )?;
        let item = json!({"id":"thread-claude","title":"Claude chat","state":"running","session":"nacre-chat-claude-1","agent":"claude","project":{"id":"demo","path":"/repo"}});
        let snapshot = json!({"windows":{"niri-window:46":{"appId":"nacre-ai-task","title":"Claude chat","pid":100}}});
        let paths = crate::paths::Paths::new(fixture.path());
        for (agent, state) in [
            ("claude", "running"),
            ("claude", "saved"),
            ("codex", "running"),
            ("codex", "saved"),
        ] {
            let mut item = item.clone();
            item["agent"] = json!(agent);
            item["state"] = json!(state);
            assert_eq!(
                existing_chat_window_with_proc(
                    &snapshot,
                    &item,
                    "unused",
                    true,
                    &proc,
                    Some(&paths),
                    nix::unistd::getuid().as_raw(),
                )
                .as_deref(),
                Some("niri-window:46")
            );
        }
        Ok(())
    }

    #[test]
    fn launcher_focus_ignores_ambiguous_generic_titles() {
        let row = json!({"id":"remote-thread","title":"Camera work","project":{"id":"demo","host":"builder"}});
        let snapshot = json!({"windows":{
            "niri-window:43":{"appId":"nacre-ai-task","title":"Camera work"},
            "niri-window:44":{"appId":"nacre-ai-task","title":"Camera work"}
        }});
        assert_eq!(existing_chat_window(&snapshot, &row, "unused", true), None);
    }

    fn texts(view: &View) -> Vec<(String, String)> {
        view.messages
            .iter()
            .map(|e| (e.message.id.clone(), e.message.text.clone()))
            .collect()
    }
    /// The QML `accept` semantics for message events, as the reference model.
    fn qml_model(events: &[Value]) -> Vec<(String, String)> {
        let mut rows: Vec<(String, String)> = Vec::new();
        for event in events {
            let id = event["id"].as_str().unwrap_or("").to_owned();
            let text = event["text"].as_str().unwrap_or("").to_owned();
            match event["type"].as_str().unwrap_or("") {
                "history" => {
                    rows = event["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|m| {
                            (
                                m["id"].as_str().unwrap().to_owned(),
                                m["text"].as_str().unwrap().to_owned(),
                            )
                        })
                        .collect()
                }
                "delta" => match rows.iter_mut().find(|r| r.0 == id) {
                    Some(row) => row.1.push_str(&text),
                    None => rows.push((id, text)),
                },
                "message" => match rows.iter_mut().find(|r| r.0 == id) {
                    Some(row) => row.1 = text,
                    None => rows.push((id, text)),
                },
                _ => {}
            }
        }
        rows
    }
    fn state(busy: bool) -> Value {
        json!({"type":"state","defaultProvider":"codex","inWorkspace":false,"provider":"claude","account":"Default","title":"New chat","threadId":"t","model":"","features":["attachments"],"composerKey":"k","busy":busy,"status":"","error":"","question":null})
    }

    #[test]
    fn reduced_messages_match_the_qml_accept_semantics_and_validate() {
        let events = vec![
            json!({"type":"history","messages":[{"id":"a","role":"user","text":"earlier"},{"id":"b","role":"assistant","text":"answer"}]}),
            state(true),
            json!({"type":"message","id":"u","role":"user","text":"hello"}),
            json!({"type":"delta","id":"s","text":"stre"}),
            json!({"type":"delta","id":"s","text":"am"}),
            json!({"type":"message","id":"s","role":"assistant","text":"stream done"}),
            json!({"type":"delta","id":"late","text":"x"}),
            json!({"type":"history","messages":[{"id":"z","role":"user","text":"fresh"}]}),
            json!({"type":"delta","id":"z2","text":"é\n\"q\""}),
        ];
        let mut view = View::default();
        let mut seen = Vec::new();
        for event in &events {
            seen.push(event.clone());
            view.apply(event);
            assert_eq!(texts(&view), qml_model(&seen), "{event}");
            if let Some(channel) = view.channel() {
                channel.validate().unwrap();
            }
        }
    }

    #[test]
    fn state_is_clamped_and_questions_travel_unchanged() {
        let mut view = View::default();
        let mut event = state(false);
        event["title"] = json!("t".repeat(5000));
        event["question"] = json!({"id":"approval-1","kind":"approval","questions":[]});
        view.apply(&event);
        let channel = view.channel().unwrap();
        assert_eq!(channel.state.title.len(), 1024);
        assert_eq!(channel.state.provider, Provider::Claude);
        assert_eq!(channel.state.question, Some(event["question"].clone()));
        event["question"] = json!({"x":"y".repeat(MAX_QUESTION_JSON)});
        view.apply(&event);
        assert_eq!(view.channel().unwrap().state.question, None);
        view.apply(&json!({"type":"state"}));
        view.channel().unwrap().validate().unwrap();
        assert!(View::default().channel().is_none());
    }

    #[test]
    fn window_is_bounded_by_count_and_serialized_size_and_text_is_cut_once() {
        let mut view = View::default();
        view.apply(&state(false));
        for i in 0..(MAX_MESSAGES + 10) {
            view.apply(&json!({"type":"message","id":format!("m{i}"),"role":"user","text":"x"}));
        }
        let channel = view.channel().unwrap();
        assert_eq!(channel.messages.len(), MAX_MESSAGES);
        assert!(channel.trimmed);
        assert_eq!(channel.messages.values().next().unwrap().id, "m10");
        channel.validate().unwrap();
        let mut view = View::default();
        view.apply(&state(false));
        // Quotes escape to two bytes, so the cut is measured after escaping.
        for i in 0..8 {
            view.apply(&json!({"type":"message","id":format!("q{i}"),"role":"assistant","text":"\"".repeat(30_000)}));
        }
        let channel = view.channel().unwrap();
        channel.validate().unwrap();
        assert!(channel.messages.len() < 8 && channel.trimmed);
        assert!(channel.messages.values().all(|m| m.truncated));
        // Deltas past the bound stop growing the text but keep the flag.
        view.apply(&json!({"type":"delta","id":"q7","text":"more"}));
        let last = view
            .channel()
            .unwrap()
            .messages
            .values()
            .last()
            .unwrap()
            .clone();
        assert!(last.truncated && !last.text.ends_with("more"));
        view.channel().unwrap().validate().unwrap();
    }

    #[test]
    fn refused_submissions_return_their_text_and_files() {
        let mut view = View::default();
        let refused =
            view.apply(&json!({"type":"sendFailed","text":"keep","attachments":[{"path":"/a"}]}));
        assert_eq!(
            refused,
            Some(json!({"text":"keep","attachments":[{"path":"/a"}]}))
        );
        assert!(view.apply(&state(false)).is_none());
    }

    const WORKER: &str = r#"
printf '%s\n' '{"type":"history","messages":[]}'
printf '%s\n' '{"type":"state","provider":"codex","composerKey":"k","busy":false}'
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$LOG"
  case "$line" in
    *'"send"'*) printf '%s\n' '{"type":"message","id":"u1","role":"user","text":"hello"}' ;;
    *'"load"'*) printf '%s\n' '{"type":"sendFailed","text":"unsent","attachments":[]}' ;;
    *'"stop"'*) exit 0 ;;
  esac
done
"#;
    fn fixture(script: &'static str) -> (tempfile::TempDir, Shared, Rc<Host>) {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let host = Rc::new(Host::new(Box::new(move |_| {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]).env("LOG", &log);
            command
        })));
        let hub = Hub::new("test".into(), false, Duration::from_millis(16)).shared();
        initial(&hub).unwrap();
        hub.borrow_mut().ai_chat = Some(host.clone());
        (dir, hub, host)
    }
    async fn call(
        hub: &Shared,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, Failure> {
        dispatch(hub.clone(), method, params).await
    }
    fn local() -> tokio::task::LocalSet {
        tokio::task::LocalSet::new()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_waits_for_state_then_commands_reach_the_worker_and_events_publish() {
        local()
            .run_until(async {
                let (dir, hub, host) = fixture(WORKER);
                let mut notices = hub.borrow().observe();
                call(&hub, "ai.chat.open", json!({"channel":"p1"}))
                    .await
                    .unwrap();
                assert_eq!(host.snapshot()["p1"].state.composer_key, "k");
                call(&hub, "ai.chat.send", json!({"channel":"p1","text":"hello"}))
                    .await
                    .unwrap();
                call(
                    &hub,
                    "ai.chat.resume",
                    json!({"channel":"p1","key":"saved"}),
                )
                .await
                .unwrap();
                call(
                    &hub,
                    "ai.chat.approve",
                    json!({"channel":"p1","id":"q","answers":{}}),
                )
                .await
                .unwrap();
                call(
                    &hub,
                    "ai.chat.provider",
                    json!({"channel":"p1","provider":"claude"}),
                )
                .await
                .unwrap();
                call(&hub, "ai.chat.new", json!({"channel":"p1"}))
                    .await
                    .unwrap();
                call(&hub, "ai.chat.workspace", json!({"channel":"p1"}))
                    .await
                    .unwrap();
                let event = loop {
                    if let Ok(Notice::Event { topic, name, data }) = notices.recv().await {
                        break (topic, name, data);
                    }
                };
                assert_eq!(event.0, TOPIC);
                assert_eq!(event.1, EVENT_SEND_FAILED);
                assert_eq!(
                    event.2,
                    json!({"text":"unsent","attachments":[],"channel":"p1"})
                );
                call(&hub, "ai.chat.cancel", json!({"channel":"p1"}))
                    .await
                    .unwrap();
                // The worker exits on stop, after logging every earlier command, so the
                // log is complete only once the channel has left the snapshot. Wait on
                // that event-driven; the cap only turns a hang into a failure.
                tokio::time::timeout(Duration::from_secs(60), async {
                    while !host.snapshot().is_empty() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("channel left the snapshot");
                let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
                let actions: Vec<_> = log
                    .lines()
                    .map(|l| {
                        serde_json::from_str::<Value>(l).unwrap()["action"]
                            .as_str()
                            .unwrap()
                            .to_owned()
                    })
                    .collect();
                assert_eq!(
                    actions,
                    [
                        "send",
                        "load",
                        "answer",
                        "provider",
                        "new",
                        "workspace",
                        "stop"
                    ]
                );
                assert!(host.snapshot().is_empty());
                let late = call(&hub, "ai.chat.send", json!({"channel":"p1","text":"x"}))
                    .await
                    .unwrap_err();
                assert_eq!(late.dispatched, Some(false));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refusals_happen_before_dispatch_and_a_dead_worker_reports_unavailable() {
        local()
            .run_until(async {
                let (_dir, hub, host) = fixture("exit 3");
                let failed = call(&hub, "ai.chat.open", json!({"channel":"p1"}))
                    .await
                    .unwrap_err();
                assert_eq!(
                    (failed.code, failed.dispatched),
                    (ErrorCode::Unavailable, Some(false))
                );
                assert!(host.snapshot().is_empty());
                for (method, params) in [
                    ("ai.chat.open", json!({"channel":"Bad"})),
                    ("ai.chat.send", json!({"channel":"","text":"  "})),
                    (
                        "ai.chat.send",
                        json!({"channel":"","text":"x","extra":1,"attachments":"no"}),
                    ),
                    (
                        "ai.chat.approve",
                        json!({"channel":"","id":"","answers":{}}),
                    ),
                    ("ai.chat.provider", json!({"channel":"","provider":"other"})),
                    ("ai.chat.unknown", json!({})),
                ] {
                    let error = call(&hub, method, params).await.unwrap_err();
                    assert!(
                        matches!(error.code, ErrorCode::Validation | ErrorCode::UnknownMethod),
                        "{method}"
                    );
                    assert_ne!(error.dispatched, Some(true));
                }
                let (_dir, hub, host) = fixture(WORKER);
                for index in 0..MAX_CHANNELS {
                    call(&hub, "ai.chat.open", json!({"channel":format!("c{index}")}))
                        .await
                        .unwrap();
                }
                let full = call(&hub, "ai.chat.open", json!({"channel":"extra"}))
                    .await
                    .unwrap_err();
                assert_eq!((full.code, full.dispatched), (ErrorCode::Busy, Some(false)));
                assert_eq!(host.snapshot().len(), MAX_CHANNELS);
                // Cancelling frees a slot; the published topic stays valid and bounded.
                call(&hub, "ai.chat.cancel", json!({"channel":"c0"}))
                    .await
                    .unwrap();
                publish(&hub, &host.snapshot()).unwrap();
                assert!(hub.borrow().snapshot(TOPIC).is_some());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn saved_lists_the_helper_rows_bounded_and_refuses_overlap_and_failure() {
        let rows: Vec<Value> = (0..40)
            .map(|i| json!({"key":format!("k{i}"),"id":"t","title":"Fixture","agent":"claude","account":"Default","state":"saved"}))
            .collect();
        let host = Rc::new(Host {
            saved: Arc::new(move || Ok(Value::Array(rows.clone()))),
            ..Host::default()
        });
        let hub = Hub::new("test".into(), false, Duration::from_millis(16)).shared();
        hub.borrow_mut().ai_chat = Some(host.clone());
        local()
            .run_until(async {
                let list = call(&hub, "ai.chat.saved", json!({"channel":""}))
                    .await
                    .unwrap();
                assert_eq!(list["chats"].as_array().unwrap().len(), MAX_SAVED);
                assert_eq!(list["chats"][0]["key"], "k0");
                let bad = call(&hub, "ai.chat.saved", json!({"channel":"BAD"}))
                    .await
                    .unwrap_err();
                assert_eq!(bad.code, ErrorCode::Validation);
                host.listing.set(true);
                let busy = call(&hub, "ai.chat.saved", json!({"channel":""}))
                    .await
                    .unwrap_err();
                assert_eq!(busy.code, ErrorCode::Busy);
            })
            .await;
        let host = Host {
            saved: Arc::new(|| Err("no history".into())),
            ..Host::default()
        };
        let failed = host.saved().await.unwrap_err();
        assert_eq!(failed.code, ErrorCode::Unavailable);
        assert!(!host.listing.get(), "the guard is released after a failure");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_silent_worker_that_never_reports_state_is_released_for_the_socket_fallback() {
        local()
            .run_until(async {
                tokio::time::pause();
                let (_dir, hub, host) = fixture("sleep 3");
                let error = call(&hub, "ai.chat.open", json!({"channel":""}))
                    .await
                    .unwrap_err();
                assert_eq!(error.dispatched, Some(false));
                // Its stdin was closed, so a real worker would now exit and drop the lock.
                let channel = host.hosted.channels.borrow().get("").cloned();
                assert!(channel.is_none_or(|c| c.input.try_lock().is_ok_and(|i| i.is_none())));
            })
            .await;
    }
}
