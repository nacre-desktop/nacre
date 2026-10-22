//! Native transport and session manager for the sidebar chat QML contract.
//!
//! The wire format deliberately remains newline-delimited JSON.  This module
//! owns only per-channel metadata; provider transcripts remain in their native
//! stores and are never copied into Nacre's state file.
use crate::{ai_core, Result};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    os::fd::AsFd,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex, Weak,
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;
use wait_timeout::ChildExt;

mod actions;

const MAX_FRAME: usize = 1_048_576;
const MAX_PROVIDER_FRAME: usize = 16 * 1_048_576;
const MAX_PRIVATE_STATE: usize = 16 * 1_048_576;
const MAX_CLIENTS: usize = 32;
const INSTRUCTIONS: &str = "You are Nacre AI in an ongoing desktop sidebar conversation. It is a full assistant workspace, not a short-answer mode. The user can change desktop workspaces without changing this conversation. Read ~/.codex/AGENTS.md and the shared personal workflow at ~/.agents/skills/nacre-project-work/SKILL.md. Use the private shared brain and project registry to resolve context; inspect each project's own instructions before work. Preserve other chats, accounts and active sessions. Use isolated worktrees and the registered build host for heavy tests. Never copy authentication or log secrets. Record useful verified knowledge using the existing brain workflow. Name this conversation with a concise title once its subject is clear.";

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?))
}
fn require_controller(agent: &str) -> Result<()> {
    let name = if agent == "claude" {
        "nacre-ai-claude"
    } else {
        "nacre-ai"
    };
    fs::metadata(home()?.join(".local/bin").join(name))?;
    Ok(())
}
fn channel_paths(channel: &str) -> Result<(PathBuf, PathBuf)> {
    if !channel.is_empty()
        && (channel.len() > 48
            || !channel
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'))
    {
        return Err(crate::i18n::tr(
            "helper.sidebar-chat.invalid-assistant-view-channel",
            "Invalid assistant view channel",
        )
        .into());
    }
    let state = home()?.join(".local/state/nacre/shell/sidebar");
    let state = if channel.is_empty() {
        state
    } else {
        state.join("views").join(channel)
    };
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid().as_raw())));
    let socket = runtime.join(if channel.is_empty() {
        "nacre-sidebar-chat.sock".into()
    } else {
        format!("nacre-chat-{channel}.sock")
    });
    Ok((state, socket))
}
fn private_dir(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("Private directory path must be absolute".into());
    }
    let mut current = PathBuf::from("/");
    for component in path.components().skip(1) {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            Ok(_) => return Err("Unsafe private directory path".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new().mode(0o700).create(&current)?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
fn private_runtime(socket: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(socket.parent().ok_or("Missing runtime directory")?)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err("Sidebar runtime directory must be owned and private".into());
    }
    Ok(())
}
fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    private_dir(path.parent().ok_or("Missing parent")?)?;
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.nlink() == 1 => {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?
        }
        Ok(_) => return Err("Refusing linked or non-regular private file".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    crate::state::write(path, value)
}
fn read_private(path: &Path) -> Result<Value> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.uid() != nix::unistd::getuid().as_raw()
        || meta.len() as usize > MAX_PRIVATE_STATE
    {
        return Err("Unsafe or oversized sidebar state".into());
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take((MAX_PRIVATE_STATE + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PRIVATE_STATE {
        return Err("Oversized sidebar state".into());
    }
    let value = crate::state::read_as(path, crate::state::schema(path), json!({}))?;
    if !value.is_object() {
        return Err("Invalid sidebar metadata".into());
    }
    Ok(value)
}
fn friendly_error_text(error: &str) -> (&'static str, &'static str) {
    let e = error.to_ascii_lowercase();
    if ["sign in", "login", "authentication", "unauthorized"]
        .iter()
        .any(|s| e.contains(s))
    {
        (
            "helper.sidebar-chat.sign-in-to-the-selected-assistant-account-in-ai-settings",
            "Sign in to the selected assistant account in AI settings.",
        )
    } else if ["usage limit", "rate limit", "quota"]
        .iter()
        .any(|s| e.contains(s))
    {
        (
            "helper.sidebar-chat.the-selected-assistant-account-has-reached-its-current-usage-limi",
            "The selected assistant account has reached its current usage limit.",
        )
    } else {
        (
            "helper.sidebar-chat.the-assistant-could-not-finish-this-request-try-again-or-open-ai",
            "The assistant could not finish this request. Try again or open AI settings.",
        )
    }
}
fn friendly_error(error: &str) -> String {
    match friendly_error_text(error).0 {
        "helper.sidebar-chat.sign-in-to-the-selected-assistant-account-in-ai-settings" => crate::i18n::tr(
            "helper.sidebar-chat.sign-in-to-the-selected-assistant-account-in-ai-settings",
            "Sign in to the selected assistant account in AI settings.",
        ),
        "helper.sidebar-chat.the-selected-assistant-account-has-reached-its-current-usage-limi" => crate::i18n::tr(
            "helper.sidebar-chat.the-selected-assistant-account-has-reached-its-current-usage-limi",
            "The selected assistant account has reached its current usage limit.",
        ),
        _ => crate::i18n::tr(
            "helper.sidebar-chat.the-assistant-could-not-finish-this-request-try-again-or-open-ai",
            "The assistant could not finish this request. Try again or open AI settings.",
        ),
    }
}
fn text_content(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter(|x| x["type"] == "text")
            .filter_map(|x| x["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
pub fn model_input(text: &str, files: &[Value]) -> Vec<Value> {
    let mut inputs = vec![json!({"type":"text","text":text})];
    inputs.extend(
        files
            .iter()
            .filter(|f| f["image"] == true)
            .filter_map(|f| f["path"].as_str())
            .map(|path| json!({"type":"localImage","path":path})),
    );
    inputs
}
fn read_frame<R: BufRead>(reader: &mut R, line: &mut Vec<u8>) -> std::io::Result<usize> {
    read_frame_limit(reader, line, MAX_FRAME)
}
fn read_frame_limit<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<usize> {
    line.clear();
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(line.len());
        }
        let end = buf
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n + 1)
            .unwrap_or(buf.len());
        if line.len() + end > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "oversized line frame",
            ));
        }
        line.extend_from_slice(&buf[..end]);
        reader.consume(end);
        if line.last() == Some(&b'\n') {
            return Ok(line.len());
        }
    }
}

fn provider_frame<R: Read + AsFd>(
    reader: &mut BufReader<R>,
    line: &mut Vec<u8>,
    timeout: Duration,
) -> std::io::Result<usize> {
    line.clear();
    let deadline = Instant::now() + timeout;
    loop {
        if reader.buffer().is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            let timeout = rustix::event::Timespec::try_from(left)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
            let mut fds = [rustix::event::PollFd::new(
                reader.get_ref(),
                rustix::event::PollFlags::IN,
            )];
            match rustix::event::poll(&mut fds, Some(&timeout)) {
                Ok(0) => return Err(std::io::ErrorKind::TimedOut.into()),
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
                _ => {}
            }
        }
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            return Ok(line.len());
        }
        let end = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |index| index + 1);
        if line.len() + end > MAX_PROVIDER_FRAME {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "oversized provider frame",
            ));
        }
        line.extend_from_slice(&bytes[..end]);
        reader.consume(end);
        if line.last() == Some(&b'\n') {
            return Ok(line.len());
        }
    }
}

fn provider_write(input: &mut impl Write, bytes: &[u8]) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut offset = 0;
    while offset < bytes.len() {
        if Instant::now() >= deadline {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        match input.write(&bytes[offset..]) {
            Ok(0) => return Err(std::io::ErrorKind::BrokenPipe.into()),
            Ok(count) => offset += count,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

struct Shared {
    state: PathBuf,
    meta: Value,
    messages: Vec<Value>,
    clients: Vec<(u64, UnixStream)>,
    next_client: u64,
    busy: bool,
    status: String,
    error: String,
    question: Option<Value>,
    async_questions: Vec<Value>,
    pending_rpc: Option<(String, Value, mpsc::Sender<Value>)>,
    provider_pid: Option<i32>,
    peer: Option<Arc<CodexPeer>>,
    connect_lock: Arc<Mutex<()>>,
    send_lock: Arc<Mutex<()>>,
    submissions: usize,
    generation: u64,
    pending: Vec<String>,
}
impl Shared {
    fn emit(&mut self, msg: Value) {
        let bytes = match serde_json::to_vec(&msg) {
            Ok(v) => v,
            Err(_) => return,
        };
        let mut frame = bytes;
        frame.push(b'\n');
        self.clients
            .retain_mut(|(_, c)| c.write_all(&frame).is_ok());
    }
    fn history(&mut self) {
        self.emit(json!({"type":"history","messages":self.messages}));
    }
    fn update(&mut self) {
        let q = self.question.clone();
        let cwd = self.meta["cwd"].as_str().unwrap_or("");
        let key = self.meta["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| hex(&Sha256::digest(cwd.as_bytes())));
        self.emit(json!({"type":"state","defaultProvider":ai_core::default_assistant().unwrap_or_else(|_|"codex".into()),"inWorkspace":self.meta["in_workspace"].as_bool().unwrap_or(false),"provider":self.meta["agent"].as_str().unwrap_or("codex"),"account":self.meta["account"].as_str().filter(|s|!s.is_empty()).unwrap_or("Default"),"title":self.meta["title"].as_str().unwrap_or("New chat"),"threadId":self.meta["id"].as_str().unwrap_or(""),"model":self.meta["model"].as_str().unwrap_or(""),"features":["attachments"],"composerKey":key,"busy":self.busy,"status":self.status,"error":self.error,"question":q}));
    }
    fn save(&self) -> Result<()> {
        atomic_json(&self.state, &self.meta)
    }
    fn finish(&mut self, error: &str) {
        self.busy = false;
        self.status.clear();
        self.error = error.into();
        self.question = self.async_questions.first().cloned();
        self.update();
    }
    fn submission_failed(&mut self, error: &str) {
        let active = self
            .peer
            .as_ref()
            .is_some_and(|peer| peer.turn.lock().is_ok_and(|turn| !turn.is_empty()));
        if active {
            self.error = error.to_owned();
            self.update();
        } else {
            self.finish(error);
        }
    }
}
fn new_meta(state: &Path, fresh: bool) -> Result<Value> {
    let agent = ai_core::default_assistant()?;
    let account = ai_core::effective_account(&agent, None)?;
    require_controller("codex")?;
    let hash = hex(&Sha256::digest(format!("{agent}|{account}").as_bytes()));
    let path = state.join(format!("{}.json", &hash[..20]));
    if path.exists() && !fresh {
        let meta = read_private(&path)?;
        if meta.as_object().is_some_and(|object| !object.is_empty()) {
            validate_metadata(&meta)?;
            return Ok(meta);
        }
    }
    let chats = home()?
        .join(".local/share/nacre/ai/chats")
        .join(Uuid::new_v4().simple().to_string());
    private_dir(&chats)?;
    let guidance = home()?.join(".codex/AGENTS.md");
    let linked = chats.join("AGENTS.md");
    if guidance.is_file() && !linked.exists() {
        std::os::unix::fs::symlink(guidance, linked)?;
    }
    let meta = json!({"agent":agent,"account":account,"id":"","cwd":chats,"title":"New chat","model":"","started":false});
    atomic_json(&path, &meta)?;
    Ok(meta)
}
fn validate_metadata(meta: &Value) -> Result<()> {
    let agent = meta["agent"]
        .as_str()
        .filter(|agent| matches!(*agent, "codex" | "claude"))
        .ok_or("Invalid sidebar provider")?;
    let account = meta["account"].as_str().ok_or("Invalid sidebar account")?;
    ai_core::effective_account(agent, Some(account))?;
    if !meta["cwd"]
        .as_str()
        .is_some_and(|cwd| Path::new(cwd).is_absolute())
        || !meta["title"].is_string()
    {
        return Err("Invalid sidebar metadata".into());
    }
    for key in ["id", "model"] {
        if meta.get(key).is_some_and(|value| !value.is_string()) {
            return Err("Invalid sidebar metadata".into());
        }
    }
    for key in ["started", "in_workspace"] {
        if meta.get(key).is_some_and(|value| !value.is_boolean()) {
            return Err("Invalid sidebar metadata".into());
        }
    }
    if meta.get("answered_questions").is_some_and(|value| {
        !value
            .as_array()
            .is_some_and(|rows| rows.iter().all(Value::is_string))
    }) {
        return Err("Invalid sidebar metadata".into());
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn state_path_for(state: &Path, meta: &Value) -> PathBuf {
    let hash = hex(&Sha256::digest(
        format!(
            "{}|{}",
            meta["agent"].as_str().unwrap_or("codex"),
            meta["account"].as_str().unwrap_or("")
        )
        .as_bytes(),
    ));
    state.join(format!("{}.json", &hash[..20]))
}

/// Run the native server or QML stdio client. Called by argv0 dispatch.
pub fn run(args: &[String]) -> Result<i32> {
    let Some(action) = args.first().map(String::as_str) else {
        return Err("Expected server or client".into());
    };
    if action == "api" || ["model_input", "preferences", "configure_channel"].contains(&action) {
        let (operation, payload) = if action == "api" {
            (
                args.get(1).map(String::as_str).unwrap_or(""),
                args.get(2).map(String::as_str).unwrap_or("{}"),
            )
        } else {
            (action, args.get(1).map(String::as_str).unwrap_or("{}"))
        };
        let request: Value = serde_json::from_str(payload)?;
        let (value, code) = api(operation, &request);
        println!("{}", serde_json::to_string(&value)?);
        return Ok(code);
    }
    let mut channel = "";
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--channel" {
            channel = args.get(i + 1).map(String::as_str).unwrap_or("");
            i += 2;
        } else {
            return Err("Invalid sidebar-chat arguments".into());
        }
    }
    if !["server", "client"].contains(&action) {
        return Err("Expected server or client".into());
    }
    channel_paths(channel)?;
    if action == "server" {
        server(channel)?;
    } else {
        client(channel)?;
    }
    Ok(0)
}
fn api(operation: &str, request: &Value) -> (Value, i32) {
    let args = request["args"].as_array().cloned().unwrap_or_default();
    match operation {
        "model_input" => {
            if args.len() < 2 {
                return (
                    json!({"error":"model_input requires text and files","type":"TypeError"}),
                    1,
                );
            }
            let Some(text) = args[0].as_str() else {
                return (
                    json!({"error":"text must be a string","type":"TypeError"}),
                    1,
                );
            };
            let Some(files) = args[1].as_array() else {
                return (
                    json!({"error":"files must be an array","type":"TypeError"}),
                    1,
                );
            };
            (json!(model_input(text, files)), 0)
        }
        "configure_channel" => {
            let channel = args.first().and_then(Value::as_str).unwrap_or("");
            match channel_paths(channel) {
                Ok(_) => (Value::Null, 0),
                Err(_) => (
                    json!({"error":crate::i18n::tr("helper.sidebar-chat.invalid-assistant-view-channel","Invalid assistant view channel"),"type":"ValueError"}),
                    1,
                ),
            }
        }
        "preferences" => {
            let controller = match home() {
                Ok(h) => h.join(".local/bin/nacre-ai"),
                Err(e) => return (json!({"error":e.to_string(),"type":"OSError"}), 1),
            };
            if fs::metadata(&controller).is_err() {
                return (
                    json!({"error":format!("[Errno 2] No such file or directory: '{}'",controller.display()),"type":"FileNotFoundError"}),
                    1,
                );
            }
            let agent = match ai_core::default_assistant() {
                Ok(v) => v,
                Err(error) => return (json!({"error":error.to_string(),"type":"OSError"}), 1),
            };
            let account = match ai_core::effective_account(&agent, None) {
                Ok(v) => v,
                Err(e) => return (json!({"error":e.to_string(),"type":"ValueError"}), 1),
            };
            (json!([agent, account]), 0)
        }
        _ => (
            json!({"error":"Unknown sidebar chat API operation","type":"ValueError"}),
            1,
        ),
    }
}
fn start_server(channel: &str) -> Result<()> {
    if channel.is_empty() {
        let status = Command::new("systemctl")
            .args(["--user", "start", "nacre-sidebar-ai.service"])
            .stdout(Stdio::null())
            .status()?;
        if !status.success() {
            return Err("Could not start sidebar chat service".into());
        }
    } else {
        let unit = format!("nacre-sidebar-ai-view-{channel}");
        let active = Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", &unit])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !active {
            let exe = env::current_exe()?;
            let unit_arg = format!("--unit={unit}");
            let status = Command::new("systemd-run")
                .args([
                    "--user",
                    "--quiet",
                    "--collect",
                    &unit_arg,
                    "--property=Type=exec",
                ])
                .arg(exe)
                .args(["sidebar-chat", "server", "--channel", channel])
                .stdout(Stdio::null())
                .status()?;
            if !status.success() {
                return Err("Could not start sidebar chat service".into());
            }
        }
    }
    Ok(())
}
fn server(channel: &str) -> Result<()> {
    let (root, path) = channel_paths(channel)?;
    private_runtime(&path)?;
    ai_core::prepare_provider_launch();
    if !channel.is_empty() {
        let views = home()?.join(".local/state/nacre/shell/sidebar/views");
        private_dir(views.parent().ok_or("Missing sidebar state directory")?)?;
        private_dir(&views)?;
    }
    private_dir(&root)?;
    let lock_path = path.with_extension("lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&lock_path)?;
    let lm = lock.metadata()?;
    if !lm.is_file() || lm.nlink() != 1 || lm.uid() != nix::unistd::getuid().as_raw() {
        return Err("Unsafe sidebar server lock".into());
    }
    lock.set_permissions(fs::Permissions::from_mode(0o600))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| "Sidebar chat server is already running")?;
    if let Ok(m) = fs::symlink_metadata(&path) {
        if m.file_type().is_symlink()
            || !m.file_type().is_socket()
            || m.uid() != nix::unistd::getuid().as_raw()
        {
            return Err("Refusing unsafe sidebar socket path".into());
        }
        match UnixStream::connect(&path) {
            Ok(_) => return Err("Sidebar chat server is already running".into()),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                fs::remove_file(&path)?
            }
            Err(e) => return Err(e.into()),
        }
    }
    let listener = UnixListener::bind(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let meta = new_meta(&root, false)?;
    let state = state_path_for(&root, &meta);
    let shared = Arc::new(Mutex::new(Shared {
        state,
        meta,
        messages: vec![],
        clients: vec![],
        next_client: 0,
        busy: false,
        status: String::new(),
        error: String::new(),
        question: None,
        async_questions: vec![],
        pending_rpc: None,
        provider_pid: None,
        peer: None,
        connect_lock: Arc::new(Mutex::new(())),
        send_lock: Arc::new(Mutex::new(())),
        submissions: 0,
        generation: 0,
        pending: vec![],
    }));
    let mut idle = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                idle = Instant::now();
                let mut s = shared.lock().map_err(|_| "Chat lock poisoned")?;
                if s.clients.len() >= MAX_CLIENTS {
                    drop(stream);
                    continue;
                }
                let mut w = stream.try_clone()?;
                w.set_write_timeout(Some(Duration::from_secs(3)))?;
                w.write_all(
                    &(serde_json::to_vec(&json!({"type":"history","messages":s.messages}))?),
                )
                .and_then(|_| w.write_all(b"\n"))?;
                let id = s.next_client;
                s.next_client = s.next_client.wrapping_add(1);
                s.clients.push((id, w));
                s.update();
                let connect = s.meta["agent"] == "codex"
                    && !s.meta["id"].as_str().unwrap_or("").is_empty()
                    && s.meta["in_workspace"] != true
                    && s.peer.is_none();
                let restore_claude = s.meta["agent"] == "claude"
                    && s.meta["started"] == true
                    && s.messages.is_empty();
                let connection_generation = s.generation;
                drop(s);
                let state = Arc::clone(&shared);
                thread::spawn(move || serve_client(stream, state, id));
                if connect {
                    let state = Arc::clone(&shared);
                    thread::spawn(move || {
                        if let Err(error) = ensure_peer(&state) {
                            if let Ok(mut s) = state.lock() {
                                if s.generation == connection_generation {
                                    let message = friendly_error(&error);
                                    s.finish(&message);
                                }
                            }
                        }
                    });
                } else if restore_claude {
                    let state = Arc::clone(&shared);
                    thread::spawn(move || {
                        let _ = restore_claude_history(&state);
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
                let s = shared.lock().map_err(|_| "Chat lock poisoned")?;
                if s.clients.is_empty() && !s.busy {
                    if !channel.is_empty() && idle.elapsed() > Duration::from_secs(90) {
                        let _ = s.save();
                        break;
                    }
                } else {
                    idle = Instant::now();
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    let peer = shared.lock().ok().and_then(|mut state| state.peer.take());
    if let Some(peer) = peer {
        peer.close();
    }
    drop(listener);
    fs::remove_file(path)?;
    Ok(())
}
fn serve_client(stream: UnixStream, shared: Arc<Mutex<Shared>>, client_id: u64) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let clone = match stream.try_clone() {
        Ok(v) => v,
        Err(_) => return,
    };
    let mut reader = BufReader::new(clone);
    let mut line = Vec::new();
    loop {
        match read_frame(&mut reader, &mut line) {
            Ok(0) => break,
            Ok(_) => {
                let cmd: Value = match serde_json::from_slice(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if !cmd.is_object() {
                    continue;
                }
                handle(cmd, &shared);
            }
            _ => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                break;
            }
        }
    }
    if let Ok(mut s) = shared.lock() {
        s.clients.retain(|(id, _)| *id != client_id);
    }
}
fn handle(cmd: Value, shared: &Arc<Mutex<Shared>>) {
    match cmd["action"].as_str().unwrap_or("") {
        "send" => handle_send(cmd, shared),
        "stop" => {
            if let Ok(s) = shared.lock() {
                if let Some(peer) = s.peer.clone() {
                    thread::spawn(move || {
                        let _ = peer.stop();
                    });
                } else if let Some(raw) = s.provider_pid {
                    let _ = nix::sys::signal::killpg(
                        nix::unistd::Pid::from_raw(raw),
                        nix::sys::signal::Signal::SIGINT,
                    );
                }
            }
        }
        "new" => start_new_chat(shared),
        "answer" => answer(cmd, shared),
        "provider" => {
            if let Some(p) = cmd["provider"]
                .as_str()
                .filter(|p| *p == "codex" || *p == "claude")
            {
                if set_default_provider(p).is_err() {
                    if let Ok(mut s) = shared.lock() {
                        s.error = "Could not update assistant settings.".into();
                        s.update();
                    }
                } else if let Ok(mut s) = shared.lock() {
                    s.update();
                }
            }
        }
        "load" | "workspace" => actions::handle(&cmd, shared),
        _ => {}
    }
}
fn handle_send(cmd: Value, shared: &Arc<Mutex<Shared>>) {
    let text = cmd["text"].as_str().unwrap_or("").trim().to_owned();
    if text.is_empty() || text.chars().count() > 64000 {
        return;
    }
    if cmd
        .get("attachments")
        .is_some_and(|value| !value.is_array())
    {
        return;
    }
    let files = cmd["attachments"].as_array().cloned().unwrap_or_default();
    let mut s = match shared.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    if s.meta["in_workspace"] == true {
        return;
    }
    if s.submissions >= MAX_CLIENTS {
        s.emit(json!({"type":"sendFailed","text":text,"attachments":files}));
        s.error = "Too many chat submissions are waiting.".into();
        s.update();
        return;
    }
    s.submissions += 1;
    s.busy = true;
    s.error.clear();
    s.status = "Connecting".into();
    s.update();
    drop(s);
    let shared = Arc::clone(shared);
    thread::spawn(move || send_message(shared, text, files));
}
struct SubmissionGuard(Arc<Mutex<Shared>>);
impl Drop for SubmissionGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = self.0.lock() {
            s.submissions = s.submissions.saturating_sub(1);
        }
    }
}
fn start_new_chat(shared: &Arc<Mutex<Shared>>) {
    let (root, old, generation, was_in_workspace) = {
        let mut s = match shared.lock() {
            Ok(v) => v,
            Err(_) => return,
        };
        if s.busy {
            return;
        }
        let Some(root) = s.state.parent().map(Path::to_path_buf) else {
            return;
        };
        s.generation = s.generation.wrapping_add(1);
        let was_in_workspace = s.meta["in_workspace"].as_bool().unwrap_or(false);
        s.meta["in_workspace"] = json!(true);
        s.busy = true;
        s.status = "Connecting".into();
        s.update();
        let old = s.peer.take();
        s.provider_pid = None;
        (root, old, s.generation, was_in_workspace)
    };
    if let Some(peer) = old {
        peer.close();
    }
    let meta = match new_meta(&root, true) {
        Ok(v) => v,
        Err(e) => {
            if let Ok(mut s) = shared.lock() {
                if s.generation != generation {
                    return;
                }
                s.meta["in_workspace"] = json!(was_in_workspace);
                let message = friendly_error(&e.to_string());
                s.finish(&message);
            }
            return;
        }
    };
    if let Ok(mut s) = shared.lock() {
        if s.generation != generation {
            return;
        }
        s.meta = meta;
        s.busy = false;
        s.state = state_path_for(&root, &s.meta);
        s.messages.clear();
        s.question = None;
        s.async_questions.clear();
        s.pending.clear();
        s.pending_rpc = None;
        s.error.clear();
        s.status.clear();
        let _ = s.save();
        s.history();
        s.update();
    }
}
fn allow_once() -> String {
    crate::i18n::tr("helper.sidebar-chat.allow-once", "Allow once")
}
fn answer(cmd: Value, shared: &Arc<Mutex<Shared>>) {
    let mut s = match shared.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    let Some(q) = s.question.clone() else { return };
    if cmd["id"].as_str().unwrap_or("") != q["id"].as_str().unwrap_or("") {
        return;
    }
    let answers = cmd["answers"].clone();
    let Some(rows) = q["questions"].as_array() else {
        return;
    };
    for row in rows {
        let id = row["id"].as_str().unwrap_or("");
        if answers[id]["answers"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .is_none_or(|v| v.trim().is_empty())
        {
            return;
        }
    }
    if q["kind"] == "async" {
        let text = rows
            .iter()
            .map(|row| {
                let id = row["id"].as_str().unwrap_or("");
                format!(
                    "{}\nAnswer: {}",
                    row["question"].as_str().unwrap_or(""),
                    answers[id]["answers"][0].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        s.async_questions.retain(|item| item["id"] != q["id"]);
        if !s.meta["answered_questions"].is_array() {
            s.meta["answered_questions"] = json!([]);
        }
        s.meta["answered_questions"]
            .as_array_mut()
            .unwrap()
            .push(q["id"].clone());
        let _ = s.save();
        s.question = s.async_questions.first().cloned();
        s.update();
        drop(s);
        handle(json!({"action":"send","text":text}), shared);
        return;
    }
    let result = if q["kind"] == "approval" {
        json!({"decision":if answers["approval"]["answers"][0]==allow_once(){"accept"}else{"decline"}})
    } else {
        json!({"answers":answers})
    };
    if let Some((_, _, tx)) = s.pending_rpc.take() {
        let _ = tx.send(result);
    }
    s.question = s.async_questions.first().cloned();
    s.status = "Working".into();
    s.update();
}
fn add_async_question(s: &mut Shared, item: &Value) {
    let id = format!("async-{}", item["id"].as_str().unwrap_or(""));
    if s.meta["answered_questions"]
        .as_array()
        .is_some_and(|a| a.contains(&json!(id)))
        || s.async_questions.iter().any(|q| q["id"] == id)
    {
        return;
    }
    let rows=item["questions"].as_array().into_iter().flatten().enumerate().map(|(i,q)|json!({"id":i.to_string(),"question":q["title"],"options":q["options"].as_array().cloned().unwrap_or_default().iter().map(|v|if v.is_string(){json!({"label":v})}else{v.clone()}).collect::<Vec<_>>()})).collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }
    let q = json!({"id":id,"questions":rows,"kind":"async"});
    s.async_questions.push(q.clone());
    if s.question.is_none() {
        s.question = Some(q);
    }
    s.update();
}
struct CodexPeer {
    shared: Weak<Mutex<Shared>>,
    generation: u64,
    pid: i32,
    child: Mutex<CommandChild>,
    input: Mutex<std::process::ChildStdin>,
    pending: Mutex<HashMap<String, mpsc::Sender<Value>>>,
    sequence: AtomicU64,
    thread_id: Mutex<String>,
    turn: Mutex<String>,
}
struct ConnectingPeer(Arc<CodexPeer>, bool);
impl Drop for ConnectingPeer {
    fn drop(&mut self) {
        if self.1 {
            self.0.close();
        }
    }
}
struct CommandChild(std::process::Child);
fn stop_owned_child(child: &mut std::process::Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM);
    match child.wait_timeout(Duration::from_secs(8)) {
        Ok(Some(_)) => {}
        _ => {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Drop for CommandChild {
    fn drop(&mut self) {
        stop_owned_child(&mut self.0);
    }
}
impl std::ops::Deref for CommandChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for CommandChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl CodexPeer {
    fn is_current(&self, state: &Shared) -> bool {
        state.generation == self.generation && state.provider_pid == Some(self.pid)
    }
    fn write(&self, value: &Value) -> std::result::Result<(), String> {
        let mut frame = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        frame.push(b'\n');
        if frame.len() > MAX_FRAME {
            return Err("Oversized Codex request".into());
        }
        let mut input = self.input.lock().map_err(|_| "Codex input lock poisoned")?;
        provider_write(&mut *input, &frame).map_err(|error| error.to_string())
    }
    fn call(&self, method: &str, params: Value) -> std::result::Result<Value, String> {
        let id = json!(self.sequence.fetch_add(1, Ordering::Relaxed));
        let key = id.to_string();
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| "Codex pending lock poisoned")?
            .insert(key.clone(), tx);
        if let Err(e) = self.write(&json!({"id":id,"method":method,"params":params})) {
            if let Ok(mut p) = self.pending.lock() {
                p.remove(&key);
            }
            return Err(e);
        }
        let response = match rx.recv_timeout(Duration::from_secs(45)) {
            Ok(v) => v,
            Err(_) => {
                if let Ok(mut p) = self.pending.lock() {
                    p.remove(&key);
                }
                return Err(format!("{method} timed out"));
            }
        };
        if response.get("error").is_some() {
            Err(response["error"]["message"]
                .as_str()
                .unwrap_or("Request failed")
                .into())
        } else {
            Ok(response["result"].clone())
        }
    }
    fn start(
        shared: &Arc<Mutex<Shared>>,
        meta: &Value,
        generation: u64,
    ) -> std::result::Result<Arc<Self>, String> {
        require_controller("codex").map_err(|e| e.to_string())?;
        let home = home().map_err(|e| e.to_string())?;
        let mut c = Command::new(home.join(".local/bin/codex"));
        c.args(["app-server", "--stdio"]);
        command_env(&mut c, "codex", meta["account"].as_str().unwrap_or(""))
            .map_err(|e| e.to_string())?;
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        crate::process::restore_child_signals(&mut c);
        let mut child = CommandChild(c.spawn().map_err(|e| e.to_string())?);
        let input = child.stdin.take().ok_or("Missing Codex stdin")?;
        let output = child.stdout.take().ok_or("Missing Codex output")?;
        let flags = OFlag::from_bits_truncate(
            fcntl(input.as_fd(), FcntlArg::F_GETFL).map_err(|e| e.to_string())?,
        );
        fcntl(input.as_fd(), FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
            .map_err(|e| e.to_string())?;
        let pid = child.id() as i32;
        let peer = Arc::new(Self {
            shared: Arc::downgrade(shared),
            generation,
            pid,
            child: Mutex::new(child),
            input: Mutex::new(input),
            pending: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(1),
            thread_id: Mutex::new(String::new()),
            turn: Mutex::new(String::new()),
        });
        let mut connecting = ConnectingPeer(Arc::clone(&peer), true);
        if let Ok(mut s) = shared.lock() {
            if s.generation != generation {
                return Err("Chat changed while connecting".into());
            }
            s.provider_pid = Some(pid);
        }
        thread::spawn({
            let weak = Arc::downgrade(&peer);
            move || Self::read_loop(weak, output)
        });
        peer.call("initialize",json!({"clientInfo":{"name":"nacre_sidebar","title":"Nacre AI Sidebar","version":"1.0"},"capabilities":{"experimentalApi":true}}))?;
        peer.write(&json!({"method":"initialized"}))?;
        let resumed = !meta["id"].as_str().unwrap_or("").is_empty();
        let result = if resumed {
            peer.call("thread/resume", json!({"threadId":meta["id"]}))?
        } else {
            peer.call("thread/start",json!({"cwd":meta["cwd"],"approvalPolicy":"never","sandbox":if ai_core::full_access(){"danger-full-access"}else{"workspace-write"},"developerInstructions":INSTRUCTIONS}))?
        };
        let thread_id = result["thread"]["id"]
            .as_str()
            .ok_or("Missing Codex thread")?
            .to_owned();
        // Store the native thread ID on the manager metadata before exposing
        // the connection to any send operation.
        if let Ok(mut id) = peer.thread_id.lock() {
            *id = thread_id.clone();
        }
        if let Ok(mut s) = shared.lock() {
            if !peer.is_current(&s) {
                return Err("Chat changed while connecting".into());
            }
            s.meta["id"] = json!(thread_id);
            if let Some(title) = result["thread"]["name"]
                .as_str()
                .filter(|title| !title.is_empty())
            {
                s.meta["title"] = json!(title);
            }
            if let Some(model) = result["model"].as_str() {
                s.meta["model"] = json!(model);
            } else {
                s.meta["model"] = json!("");
            }
            let _ = s.save();
            if resumed {
                restore_turns(&mut s, &result["thread"]["turns"]);
            }
            s.update();
        }
        connecting.1 = false;
        Ok(peer)
    }
    fn read_loop(weak: Weak<Self>, output: std::process::ChildStdout) {
        let mut reader = BufReader::new(output);
        loop {
            let mut line = Vec::new();
            let read = match provider_frame(&mut reader, &mut line, Duration::from_secs(300)) {
                Ok(n) => n,
                Err(_) => break,
            };
            if read == 0 {
                break;
            }
            let event: Value = match serde_json::from_slice(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(peer) = weak.upgrade() else { break };
            if event.get("id").is_some() {
                if let Some(method) = event["method"].as_str() {
                    let _ = rpc_question_peer(&peer, event["id"].clone(), method, &event["params"]);
                } else if let Ok(mut p) = peer.pending.lock() {
                    if let Some(tx) = p.remove(&event["id"].to_string()) {
                        let _ = tx.send(event);
                    }
                }
                continue;
            }
            peer_event(&peer, &event);
        }
        if let Some(peer) = weak.upgrade() {
            if let Some(shared) = peer.shared.upgrade() {
                if let Ok(mut s) = shared.lock() {
                    if peer.is_current(&s) {
                        if s.busy || s.pending_rpc.is_some() {
                            s.finish("The assistant connection ended; your conversation is saved.");
                        }
                        s.pending_rpc = None;
                        if s.peer.as_ref().is_some_and(|p| Arc::ptr_eq(p, &peer)) {
                            s.peer = None;
                        }
                        s.provider_pid = None;
                    }
                }
            }
        }
    }
    fn send(&self, text: &str, files: &[Value], steer: bool) -> std::result::Result<(), String> {
        let inputs = model_input(text, files);
        let thread_id = self
            .thread_id
            .lock()
            .map_err(|_| "Codex thread lock poisoned")?
            .clone();
        if steer {
            let turn = self
                .turn
                .lock()
                .map_err(|_| "Codex turn lock poisoned")?
                .clone();
            if !turn.is_empty() {
                match self.call(
                    "turn/steer",
                    json!({"threadId":thread_id,"expectedTurnId":turn,"input":inputs}),
                ) {
                    Ok(_) => return Ok(()),
                    Err(e)
                        if e.to_ascii_lowercase().contains("no active turn")
                            || self.turn.lock().is_ok_and(|turn| turn.is_empty()) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        let result = self.call("turn/start", json!({"threadId":thread_id,"input":inputs}))?;
        if let Some(id) = result["turn"]["id"].as_str() {
            *self.turn.lock().map_err(|_| "Codex turn lock poisoned")? = id.to_owned();
        }
        Ok(())
    }
    fn stop(&self) -> std::result::Result<(), String> {
        let turn = self
            .turn
            .lock()
            .map_err(|_| "Codex turn lock poisoned")?
            .clone();
        if turn.is_empty() {
            return Ok(());
        }
        let thread_id = self
            .thread_id
            .lock()
            .map_err(|_| "Codex thread lock poisoned")?
            .clone();
        self.call(
            "turn/interrupt",
            json!({"threadId":thread_id,"turnId":turn}),
        )
        .map(|_| ())
    }
    fn close(&self) {
        if let Ok(mut child) = self.child.lock() {
            stop_owned_child(&mut child.0);
        }
    }
}
fn restore_turns(s: &mut Shared, turns: &Value) {
    let mut rows = Vec::new();
    let mut pending = Vec::new();
    for turn in turns.as_array().into_iter().flatten() {
        for item in turn["items"].as_array().into_iter().flatten() {
            if item["type"] == "userMessage" {
                pending.clear();
            } else if item["type"] == "agentMessage" && item["questions"].as_array().is_some() {
                pending.push(item.clone());
            }
            let role = match item["type"].as_str().unwrap_or("") {
                "userMessage" => "user",
                "agentMessage" => "assistant",
                _ => continue,
            };
            let body = if role == "user" {
                text_content(&item["content"])
            } else {
                item["text"].as_str().unwrap_or("").to_owned()
            };
            if role == "user" || !body.is_empty() {
                rows.push(json!({"id":item["id"],"role":role,"text":body}));
            }
        }
    }
    s.messages = rows;
    s.history();
    for item in pending {
        add_async_question(s, &item);
    }
}
fn rpc_question_peer(
    peer: &Arc<CodexPeer>,
    id: Value,
    method: &str,
    params: &Value,
) -> std::result::Result<(), String> {
    let Some(shared) = peer.shared.upgrade() else {
        return Err("Chat ended".into());
    };
    let (kind, questions) = if method == "item/tool/requestUserInput" {
        (
            "input",
            params["questions"].as_array().cloned().unwrap_or_default(),
        )
    } else if method == "item/commandExecution/requestApproval"
        || method == "item/fileChange/requestApproval"
    {
        (
            "approval",
            vec![
                json!({"id":"approval","header":"Permission","question":params["reason"].as_str().unwrap_or("Allow this operation?"),"options":[{"label":allow_once()},{"label":crate::i18n::tr("helper.sidebar-chat.decline","Decline")}]}),
            ],
        )
    } else {
        return peer.write(&json!({"id":id,"error":{"code":-32601,"message":crate::i18n::tr("helper.sidebar-chat.this-client-does-not-support-that-interaction","This client does not support that interaction.")}}));
    };
    let rid = id
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| id.to_string());
    let (tx, rx) = mpsc::channel();
    {
        let mut s = shared.lock().map_err(|_| "Chat lock poisoned")?;
        if !peer.is_current(&s) {
            return Err("Stale chat generation".into());
        }
        if questions.is_empty() {
            return peer.write(
                &json!({"id":id,"error":{"code":-32602,"message":"Invalid interaction questions"}}),
            );
        }
        let rows = questions;
        s.question = Some(json!({"id":rid.clone(),"questions":rows,"kind":kind}));
        s.pending_rpc = Some((kind.into(), id.clone(), tx));
        s.status = "Waiting for your answer".into();
        s.update();
    }
    let peer = Arc::clone(peer);
    thread::spawn(move || match rx.recv_timeout(Duration::from_secs(300)) {
        Ok(result) => {
            let _ = peer.write(&json!({"id":id,"result":result}));
        }
        Err(_) => {
            if let Some(shared) = peer.shared.upgrade() {
                if let Ok(mut s) = shared.lock() {
                    if peer.is_current(&s)
                        && s.peer.as_ref().is_some_and(|p| Arc::ptr_eq(p, &peer))
                        && s.question.as_ref().is_some_and(|q| q["id"] == rid)
                    {
                        s.question = None;
                        s.pending_rpc = None;
                        s.finish("Timed out waiting for your answer.");
                    }
                }
            }
        }
    });
    Ok(())
}
fn peer_event(peer: &CodexPeer, event: &Value) {
    let Some(shared) = peer.shared.upgrade() else {
        return;
    };
    if let Ok(mut s) = shared.lock() {
        if !peer.is_current(&s) {
            return;
        }
        let p = &event["params"];
        let method = event["method"].as_str().unwrap_or("");
        let thread_id = peer.thread_id.lock().map(|v| v.clone()).unwrap_or_default();
        if !thread_id.is_empty() && p["threadId"].as_str().is_some_and(|id| id != thread_id) {
            return;
        }
        match method {
            "turn/started" => {
                if let Some(id) = p["turn"]["id"].as_str() {
                    if let Ok(mut turn) = peer.turn.lock() {
                        *turn = id.into();
                    }
                }
                s.busy = true;
                s.status = "Working".into();
                s.update();
            }
            "item/started" => {
                let kind = p["item"]["type"].as_str().unwrap_or("");
                if kind == "agentMessage" {
                    if p["item"]["questions"].is_array() {
                        add_async_question(&mut s, &p["item"]);
                    }
                    return;
                }
                s.status = match kind {
                    "commandExecution" => "Running a command",
                    "fileChange" => "Editing files",
                    "mcpToolCall" => "Using a tool",
                    "webSearch" => "Searching the web",
                    _ => return,
                }
                .into();
                s.update();
            }
            "item/agentMessage/delta" => {
                let id = p["itemId"].as_str().unwrap_or("reply");
                let delta = p["delta"].as_str().unwrap_or("");
                if delta.is_empty() {
                    return;
                }
                if let Some(m) = s.messages.iter_mut().find(|m| m["id"] == id) {
                    m["text"] = json!(format!("{}{}", m["text"].as_str().unwrap_or(""), delta));
                } else {
                    s.messages
                        .push(json!({"id":id,"role":"assistant","text":delta}));
                }
                s.emit(json!({"type":"delta","id":id,"text":delta}));
            }
            "item/completed" => {
                let item = &p["item"];
                if item["type"] == "userMessage" {
                    let body = text_content(&item["content"]);
                    if let Some(index) = s.pending.iter().position(|p| p == &body) {
                        s.pending.remove(index);
                    } else {
                        s.messages
                            .push(json!({"id":item["id"],"role":"user","text":body}));
                        s.emit(json!({"type":"message","id":item["id"],"role":"user","text":body}));
                    }
                } else if item["type"] == "agentMessage" {
                    let id = item["id"].as_str().unwrap_or("reply");
                    let body = item["text"].as_str().unwrap_or("");
                    if !body.is_empty() {
                        if let Some(m) = s.messages.iter_mut().find(|m| m["id"] == id) {
                            m["text"] = json!(body);
                        } else {
                            s.messages
                                .push(json!({"id":id,"role":"assistant","text":body}));
                        }
                        s.emit(json!({"type":"message","id":id,"role":"assistant","text":body}));
                    }
                    if item["questions"].as_array().is_some() {
                        add_async_question(&mut s, item);
                    }
                }
            }
            "turn/completed" => {
                if let Ok(mut turn) = peer.turn.lock() {
                    *turn = String::new();
                }
                let error = if p["turn"]["status"] == "failed" {
                    friendly_error(p["turn"]["error"]["message"].as_str().unwrap_or(""))
                } else {
                    String::new()
                };
                s.finish(&error);
            }
            "thread/name/updated" => {
                if let Some(title) = p["threadName"].as_str().or_else(|| p["name"].as_str()) {
                    s.meta["title"] = json!(title);
                    let _ = s.save();
                    s.update();
                }
            }
            "error" if p["willRetry"] != true => {
                let error = friendly_error(p["error"]["message"].as_str().unwrap_or(""));
                s.finish(&error);
            }
            _ => {}
        }
    };
}
fn ensure_peer(shared: &Arc<Mutex<Shared>>) -> std::result::Result<Arc<CodexPeer>, String> {
    let connect = {
        let s = shared.lock().map_err(|_| "Chat lock poisoned")?;
        Arc::clone(&s.connect_lock)
    };
    let _guard = connect.lock().map_err(|_| "Chat connect lock poisoned")?;
    let (meta, generation) = {
        let mut s = shared.lock().map_err(|_| "Chat lock poisoned")?;
        if let Some(peer) = &s.peer {
            return Ok(Arc::clone(peer));
        }
        if s.meta["in_workspace"] == true {
            return Err("This chat is open in the workspace.".into());
        }
        s.status = "Connecting".into();
        s.update();
        (s.meta.clone(), s.generation)
    };
    let peer = match CodexPeer::start(shared, &meta, generation) {
        Ok(peer) => peer,
        Err(error) => {
            if let Ok(mut s) = shared.lock() {
                if s.generation == generation && s.peer.is_none() {
                    s.provider_pid = None;
                    s.question = s.async_questions.first().cloned();
                    s.pending_rpc = None;
                }
            }
            return Err(error);
        }
    };
    let mut s = shared.lock().map_err(|_| "Chat lock poisoned")?;
    if !peer.is_current(&s) {
        drop(s);
        peer.close();
        return Err("Chat changed while connecting".into());
    }
    s.peer = Some(Arc::clone(&peer));
    s.status.clear();
    s.update();
    Ok(peer)
}
fn set_default_provider(provider: &str) -> Result<()> {
    if !["codex", "claude"].contains(&provider) {
        return Err("Unknown assistant".into());
    }
    let path = env::var_os("NACRE_AI_SETTINGS")
        .map(PathBuf::from)
        .unwrap_or(home()?.join(".config/nacre/ai/settings.json"));
    let schema = crate::state::by_id("ai-settings")?;
    crate::state::update_as(&path, schema, |mut data| {
        data["default_assistant"] = json!(provider);
        Ok(data)
    })
}
fn send_message(shared: Arc<Mutex<Shared>>, mut text: String, files: Vec<Value>) {
    let _submission_count = SubmissionGuard(Arc::clone(&shared));
    let send_lock = match shared.lock() {
        Ok(s) => Arc::clone(&s.send_lock),
        Err(_) => return,
    };
    let _submission = match send_lock.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let original_text = text.clone();
    let raw_files = files;
    let files = match crate::composer::attachments(&json!(raw_files)) {
        Ok(f) => f,
        Err(e) => {
            if let Ok(mut s) = shared.lock() {
                s.history();
                s.emit(json!({"type":"sendFailed","text":original_text,"attachments":raw_files}));
                s.submission_failed(&friendly_error(&e));
            }
            return;
        }
    };
    let (meta, generation) = {
        let mut s = match shared.lock() {
            Ok(v) => v,
            Err(_) => return,
        };
        if s.meta["agent"] == "claude" && s.meta["id"].as_str().unwrap_or("").is_empty() {
            s.meta["id"] = json!(Uuid::new_v4().to_string());
            s.meta["started"] = json!(false);
            let _ = s.save();
            s.update();
        }
        s.busy = true;
        s.status = "Connecting".into();
        s.update();
        (s.meta.clone(), s.generation)
    };
    // Attachments are validated by the native composer command before reaching
    // this socket. The paths are passed as explicit context, preserving the
    // established desktop contract without reading their contents here.
    let paths = files
        .iter()
        .filter_map(|f| f["path"].as_str())
        .collect::<Vec<_>>();
    if !paths.is_empty() {
        text.push_str("\n\nUser-selected local files:\n");
        text.push_str(&paths.join("\n"));
    }
    // Restore a resumed transcript before inserting the optimistic user row.
    // Otherwise a late history reply would replace the new message.
    let ready = if meta["agent"] == "codex" {
        ensure_peer(&shared).map(|_| ())
    } else {
        require_controller("claude")
            .and_then(|_| restore_claude_history(&shared))
            .map_err(|error| error.to_string())
    };
    if let Err(error) = ready {
        if let Ok(mut s) = shared.lock() {
            s.history();
            s.emit(json!({"type":"sendFailed","text":original_text,"attachments":files}));
            s.submission_failed(&friendly_error(&error));
        }
        return;
    }
    let title_update = if let Ok(mut state) = shared.lock() {
        if state.meta["title"] == "New chat" {
            let title = original_text
                .split_whitespace()
                .take(7)
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(80)
                .collect::<String>();
            state.meta["title"] = json!(title);
            let _ = state.save();
            state
                .peer
                .clone()
                .map(|peer| (peer, title, state.meta["id"].clone()))
        } else {
            None
        }
    } else {
        None
    };
    if let Some((peer, title, id)) = title_update {
        if let Err(error) = peer.call("thread/name/set", json!({"threadId":id,"name":title})) {
            if let Ok(mut state) = shared.lock() {
                state.history();
                state.emit(json!({"type":"sendFailed","text":original_text,"attachments":files}));
                state.submission_failed(&friendly_error(&error));
            }
            return;
        }
    }
    let user_id = format!("user-{}", Uuid::new_v4().simple());
    if let Ok(mut s) = shared.lock() {
        if s.generation != generation {
            return;
        }
        if meta["agent"] == "codex" {
            s.pending.push(text.clone());
        }
        let display = if files.is_empty() {
            original_text.clone()
        } else {
            format!(
                "{original_text}\n\nAttached: {}",
                files
                    .iter()
                    .filter_map(|file| file["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let item = json!({"id":user_id,"role":"user","text":display});
        s.messages.push(item.clone());
        s.emit(json!({"type":"message","id":user_id,"role":"user","text":display}));
        s.busy = true;
        s.status = "Working".into();
        s.update();
    }
    let cwd = meta["cwd"].as_str().unwrap_or("");
    let result = if meta["agent"] == "claude" {
        send_claude(&meta, &text, cwd, &shared)
    } else {
        send_codex(&meta, &text, cwd, &shared, &files)
    };
    if let Ok(mut s) = shared.lock() {
        if s.generation != generation {
            return;
        }
        match result {
            Ok((id, model)) => {
                if s.meta["id"].as_str().unwrap_or("").is_empty() {
                    s.meta["id"] = json!(id);
                }
                if !model.is_empty() {
                    s.meta["model"] = json!(model);
                }
                let _ = s.save();
                if meta["agent"] == "claude" {
                    s.finish("");
                } else {
                    // Codex returns the turn ID before streaming completes.
                    // Its notification reader owns the busy -> idle transition.
                    s.update();
                }
            }
            Err(e)
                if meta["agent"] == "claude"
                    && (e.starts_with("claude-result:")
                        || e == "Response interrupted"
                        || e == "claude-failed") =>
            {
                let message = if e == "Response interrupted" {
                    e
                } else {
                    friendly_error(e.strip_prefix("claude-result:").unwrap_or(&e))
                };
                s.finish(&message);
            }
            Err(e) => {
                s.pending.retain(|p| p != &text);
                s.messages.retain(|message| message["id"] != user_id);
                s.history();
                s.emit(json!({"type":"sendFailed","text":original_text,"attachments":files}));
                let message = friendly_error(&e);
                s.submission_failed(&message);
            }
        }
    }
}
fn command_env(command: &mut Command, agent: &str, account: &str) -> Result<()> {
    let delta = if agent == "codex" {
        ai_core::local_environment_delta(Some(account))?
    } else {
        ai_core::environment_delta(agent, (!account.is_empty()).then_some(account))?
    };
    if let Some(values) = delta["set"].as_object() {
        for (k, v) in values {
            if let Some(v) = v.as_str() {
                command.env(k, v);
            }
        }
    }
    if let Some(values) = delta["unset"].as_array() {
        for k in values.iter().filter_map(Value::as_str) {
            command.env_remove(k);
        }
    }
    ai_core::prepare_provider_launch();
    if agent == "codex" && !account.is_empty() {
        // Keep the existing account-scoped MCP/history setup boundary. It
        // receives only the explicit environment delta, never copied auth.
        let mut configure = Command::new(home()?.join(".local/bin/nacre-ai-chat"));
        configure.arg("configure").stdin(Stdio::null());
        for (name, value) in command.get_envs() {
            if let Some(value) = value {
                configure.env(name, value);
            } else {
                configure.env_remove(name);
            }
        }
        let output = crate::process::capture(configure, None, Duration::from_secs(20))?;
        if !output.status.success() {
            return Err("Could not configure the selected assistant account".into());
        }
    }
    Ok(())
}
fn restore_claude_history(shared: &Arc<Mutex<Shared>>) -> Result<()> {
    let connect = shared
        .lock()
        .map_err(|_| "Chat lock poisoned")?
        .connect_lock
        .clone();
    let _connection = connect.lock().map_err(|_| "Chat connect lock poisoned")?;
    let (meta, generation) = {
        let s = shared.lock().map_err(|_| "Chat lock poisoned")?;
        if !s.messages.is_empty() || s.meta["in_workspace"] == true {
            return Ok(());
        }
        (s.meta.clone(), s.generation)
    };
    if meta["agent"] != "claude" || meta["started"] != true {
        return Ok(());
    }
    let sdk = home()?.join(".local/share/nacre/ai/claude-sdk/bin/python");
    if !sdk.exists() {
        return Ok(());
    }
    let script="import json,sys\nfrom claude_agent_sdk import get_session_messages\nrows=[]\nfor item in get_session_messages(sys.argv[1],directory=sys.argv[2]):\n if item.type not in ('user','assistant'): continue\n content=item.message.get('content',[])\n text=content if isinstance(content,str) else chr(10).join(block.get('text','') for block in content if block.get('type')=='text')\n if text: rows.append(dict(id=item.uuid,role=item.type,text=text))\nprint(json.dumps(rows[-300:]))\n";
    let mut cmd = Command::new(sdk);
    cmd.args([
        "-c",
        script,
        meta["id"].as_str().unwrap_or(""),
        meta["cwd"].as_str().unwrap_or(""),
    ])
    .stderr(Stdio::null());
    command_env(&mut cmd, "claude", meta["account"].as_str().unwrap_or(""))?;
    let output = crate::process::capture(cmd, None, Duration::from_secs(20))?;
    if !output.status.success() {
        return Ok(());
    }
    let rows: Value = serde_json::from_slice(&output.stdout)?;
    let Some(rows) = rows.as_array() else {
        return Ok(());
    };
    let mut s = shared.lock().map_err(|_| "Chat lock poisoned")?;
    if s.generation != generation || !s.messages.is_empty() {
        return Ok(());
    }
    s.messages = rows
        .iter()
        .filter(|m| {
            m.is_object()
                && m["id"].is_string()
                && matches!(m["role"].as_str(), Some("user" | "assistant"))
                && m["text"].is_string()
        })
        .cloned()
        .collect();
    s.history();
    Ok(())
}
fn send_claude(
    meta: &Value,
    text: &str,
    cwd: &str,
    shared: &Arc<Mutex<Shared>>,
) -> std::result::Result<(String, String), String> {
    require_controller("claude").map_err(|e| e.to_string())?;
    if meta["started"] == true && shared.lock().is_ok_and(|s| s.messages.is_empty()) {
        let _ = restore_claude_history(shared);
    }
    let home = home().map_err(|e| e.to_string())?;
    let mut c = Command::new(home.join(".local/bin/claude"));
    c.current_dir(cwd).args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
    ]);
    if ai_core::full_access() {
        c.arg("--dangerously-skip-permissions");
    }
    c.args(["--append-system-prompt", INSTRUCTIONS]);
    let session = meta["id"].as_str().unwrap_or("");
    if meta["started"] == true {
        c.args(["--resume", session]);
    } else {
        c.args(["--session-id", session]);
    }
    command_env(&mut c, "claude", meta["account"].as_str().unwrap_or(""))
        .map_err(|e| e.to_string())?;
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    crate::process::restore_child_signals(&mut c);
    let mut child = CommandChild(c.spawn().map_err(|e| e.to_string())?);
    if let Ok(mut s) = shared.lock() {
        s.provider_pid = Some(child.id() as i32);
    }
    let mut stdin = child.stdin.take().ok_or("Missing Claude stdin")?;
    let flags = OFlag::from_bits_truncate(
        fcntl(stdin.as_fd(), FcntlArg::F_GETFL).map_err(|e| e.to_string())?,
    );
    fcntl(stdin.as_fd(), FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
        .map_err(|e| e.to_string())?;
    if let Err(error) = provider_write(&mut stdin, text.as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        if let Ok(mut s) = shared.lock() {
            s.provider_pid = None;
        }
        return Err(error.to_string());
    }
    drop(stdin);
    let stdout = child.stdout.take().ok_or("Missing Claude stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut ident = format!("claude-{}", Uuid::new_v4().simple());
    let mut streamed = false;
    let mut session_id = session.to_owned();
    let mut provider_error = None;
    loop {
        let mut line = Vec::new();
        match provider_frame(&mut reader, &mut line, Duration::from_secs(300)) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                provider_error = Some(error.to_string());
                let _ = child.kill();
                break;
            }
        }
        let event: Value = match serde_json::from_slice(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if event["type"] == "stream_event" {
            let ev = &event["event"];
            if ev["type"] == "message_start" {
                ident = ev["message"]["id"].as_str().unwrap_or(&ident).into();
                streamed = false;
            }
            if ev["delta"]["type"] == "text_delta" {
                let delta = ev["delta"]["text"].as_str().unwrap_or("");
                if !delta.is_empty() {
                    if let Ok(mut s) = shared.lock() {
                        let item = s.messages.iter_mut().find(|m| m["id"] == ident);
                        if let Some(item) = item {
                            let old = item["text"].as_str().unwrap_or("").to_owned();
                            item["text"] = json!(format!("{old}{delta}"));
                        } else {
                            s.messages
                                .push(json!({"id":ident,"role":"assistant","text":delta}));
                        }
                        s.emit(json!({"type":"delta","id":ident,"text":delta}));
                    }
                }
                streamed = true;
            }
        } else if event["type"] == "assistant" {
            let body = text_content(&event["message"]["content"]);
            if !body.is_empty() && !streamed {
                if let Ok(mut s) = shared.lock() {
                    s.messages
                        .push(json!({"id":ident,"role":"assistant","text":body}));
                    s.emit(json!({"type":"message","id":ident,"role":"assistant","text":body}));
                }
            }
            if let Some(blocks) = event["message"]["content"].as_array() {
                for block in blocks {
                    if block["type"] == "tool_use" {
                        if let Ok(mut s) = shared.lock() {
                            s.status =
                                format!("Using {}", block["name"].as_str().unwrap_or("a tool"));
                            s.update();
                        }
                    }
                }
            }
            if let Some(id) = event["session_id"].as_str() {
                session_id = id.into();
            }
        } else if event["type"] == "result" {
            if event["is_error"] == true {
                provider_error = Some(
                    event["result"]
                        .as_str()
                        .unwrap_or("Provider error")
                        .to_owned(),
                );
            }
            if let Some(id) = event["session_id"].as_str() {
                session_id = id.into();
            }
            if let Ok(mut s) = shared.lock() {
                s.meta["started"] = json!(true);
                s.meta["id"] = json!(session_id);
                let _ = s.save();
            }
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if let Ok(mut s) = shared.lock() {
        s.provider_pid = None;
    }
    if let Some(error) = provider_error {
        return Err(format!("claude-result:{error}"));
    }
    if !status.success() {
        use std::os::unix::process::ExitStatusExt;
        return Err(if status.signal() == Some(nix::libc::SIGINT) {
            "Response interrupted".into()
        } else {
            "claude-failed".into()
        });
    }
    if let Ok(mut s) = shared.lock() {
        s.meta["started"] = json!(true);
        s.meta["id"] = json!(session_id);
    }
    Ok((session_id, String::new()))
}
fn send_codex(
    _meta: &Value,
    text: &str,
    _cwd: &str,
    shared: &Arc<Mutex<Shared>>,
    files: &[Value],
) -> std::result::Result<(String, String), String> {
    let peer = ensure_peer(shared)?;
    let steer = !peer
        .turn
        .lock()
        .map_err(|_| "Codex turn lock poisoned")?
        .is_empty();
    peer.send(text, files, steer)?;
    let id = peer
        .thread_id
        .lock()
        .map_err(|_| "Codex thread lock poisoned")?
        .clone();
    Ok((id, String::new()))
}
fn client(channel: &str) -> Result<()> {
    let (_, path) = channel_paths(channel)?;
    private_runtime(&path)?;
    start_server(channel)?;
    // Keep stdin independent so cancellation and follow-up actions continue
    // to reach the server while provider events stream back.
    let (tx, rx) = std::sync::mpsc::sync_channel::<Option<Vec<u8>>>(32);
    thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut source = BufReader::new(stdin.lock());
        let mut line = Vec::new();
        loop {
            match read_frame(&mut source, &mut line) {
                Ok(0) => {
                    let _ = tx.send(None);
                    break;
                }
                Ok(_) => {
                    if tx.send(Some(line.clone())).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(None);
                    break;
                }
            }
        }
    });
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut buffer = vec![0; 65536];
    loop {
        let mut stream = None;
        for _ in 0..100 {
            match UnixStream::connect(&path) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    thread::sleep(Duration::from_millis(100))
                }
                Err(e) => return Err(e.into()),
            }
        }
        let Some(mut socket) = stream else {
            let frame = serde_json::to_vec(
                &json!({"type":"connection","connected":false,"error":crate::i18n::tr("helper.sidebar-chat.reopen-the-drawer-to-reconnect","Reopen the drawer to reconnect.")}),
            )?;
            out.write_all(&frame)?;
            out.write_all(b"\n")?;
            out.flush()?;
            return Ok(());
        };
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        socket.set_write_timeout(Some(Duration::from_millis(100)))?;
        let mut pending = Vec::new();
        let mut lost = false;
        loop {
            loop {
                match rx.try_recv() {
                    Ok(Some(line)) => {
                        if socket.write_all(&line).is_err() {
                            lost = true;
                            break;
                        }
                    }
                    Ok(None) => return Ok(()),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(_) => return Ok(()),
                }
            }
            if lost {
                break;
            }
            match socket.read(&mut buffer) {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    pending.extend_from_slice(&buffer[..n]);
                    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                        out.write_all(&pending[..=end])?;
                        out.flush()?;
                        pending.drain(..=end);
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => {
                    break;
                }
            }
        }
        let _ = socket.shutdown(std::net::Shutdown::Both);
        // An interrupted frame belongs to the old connection. Never join it
        // to the reconnect event or the next server's initial history.
        let frame = serde_json::to_vec(
            &json!({"type":"connection","connected":false,"error":crate::i18n::tr("helper.sidebar-chat.reconnecting","Reconnecting…")}),
        )?;
        out.write_all(&frame)?;
        out.write_all(b"\n")?;
        out.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_or_linked_runtime_directories_are_refused_without_chmod() {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let socket = directory.path().join("chat.sock");
        assert!(private_runtime(&socket).is_ok());
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_runtime(&socket).is_err());
        assert_eq!(
            fs::metadata(directory.path()).unwrap().mode() & 0o777,
            0o755
        );
        let linked = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(directory.path(), linked.path().join("runtime")).unwrap();
        assert!(private_runtime(&linked.path().join("runtime/chat.sock")).is_err());
    }
    #[test]
    fn malformed_saved_metadata_is_rejected_before_provider_launch() {
        let metadata = json!({"agent":"codex","account":"","cwd":"/fixture/chat","title":"Title"});
        assert!(validate_metadata(&metadata).is_ok());
        for (key, value) in [
            ("agent", json!("other")),
            ("account", json!("../escape")),
            ("cwd", json!("relative")),
            ("id", json!([])),
            ("started", json!("true")),
        ] {
            let mut invalid = metadata.clone();
            invalid[key] = value;
            assert!(validate_metadata(&invalid).is_err(), "{key}");
        }
    }
    #[test]
    fn partial_provider_frames_have_an_absolute_deadline() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(b"{\"partial\":").unwrap();
        let mut reader = BufReader::new(reader);
        let start = Instant::now();
        let error =
            provider_frame(&mut reader, &mut Vec::new(), Duration::from_millis(30)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn friendly_errors_are_generic_and_classify_limits() {
        assert_eq!(
            friendly_error_text("Invalid secret_key sensitive-value").1,
            "The assistant could not finish this request. Try again or open AI settings."
        );
        assert!(friendly_error_text("usage limit reached")
            .1
            .contains("usage limit"));
        assert!(friendly_error_text("please sign in").1.contains("Sign in"));
    }
    #[test]
    fn text_content_preserves_text_blocks_only() {
        assert_eq!(
            text_content(
                &json!([{"type":"text","text":"A"},{"type":"image"},{"type":"text","text":"B"}])
            ),
            "A\nB"
        );
        assert_eq!(text_content(&json!("plain")), "plain");
    }
    #[test]
    fn model_input_matches_frozen_text_case_and_only_passes_images() {
        assert_eq!(
            model_input("Public fixture", &[]),
            vec![json!({"type":"text","text":"Public fixture"})]
        );
        assert_eq!(
            model_input(
                "prompt",
                &[
                    json!({"path":"/fixture/a.png","image":true}),
                    json!({"path":"/fixture/a.txt","image":false})
                ]
            ),
            vec![
                json!({"type":"text","text":"prompt"}),
                json!({"type":"localImage","path":"/fixture/a.png"})
            ]
        );
    }
}
