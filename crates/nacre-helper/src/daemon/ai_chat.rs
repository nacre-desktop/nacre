//! Daemon host for the sidebar chat. Each open channel is a nice-10 worker
//! process (`sidebar-chat worker`, the N0023 session manager on stdio); the
//! daemon only reduces its events into the bounded `ai.chat` snapshot and
//! forwards commands. A worker stops 30 s after it is idle, or at once when
//! its pipe closes, so no provider child outlives the daemon.
use super::hub::{failure, Shared};
use crate::Result;
use nacre_protocol::{
    ai_chat::{
        AiApprove, AiChannel, AiChannelRef, AiChat, AiMessage, AiProvider, AiResume, AiSend,
        AiState, Provider, Role, EVENT_SEND_FAILED, MAX_CHANNELS, MAX_MESSAGES, MAX_MESSAGES_JSON,
        MAX_QUESTION_JSON, MAX_TEXT_JSON, TOPIC,
    },
    ErrorCode, Failure, Method,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, process::Stdio, rc::Rc, time::Duration};
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
    nacre_protocol::ai_chat::methods()
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
        }
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
    let (channel, command) = match method {
        "ai.chat.open" => {
            let request = checked(parse::<AiChannelRef>(params), AiChannelRef::validate)?;
            host.open(&hub, &request.channel).await?;
            return Ok(json!({"channel": request.channel}));
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

#[cfg(test)]
mod tests {
    use super::super::hub::{Hub, Notice};
    use super::*;

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
