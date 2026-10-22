//! Native Claude Code launcher and bounded reader for its local session JSONL.
use crate::{ai_core, Error, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Duration,
};
use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};
use wait_timeout::ChildExt;

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_LINE: usize = 4 * 1024 * 1024;
const MAX_SESSIONS: usize = 500;
const MAX_OUTPUT: usize = 1024 * 1024;
const MAX_ROSTER: u64 = 1024 * 1024;
const HELP: &str = "usage: nacre-ai-claude [-h] [--project PROJECT] [--path PATH]\n                       [--account ACCOUNT] [--thread-id THREAD_ID]\n                       [--chat-directory] [--tmux]\n                       {new,resume,list,login}\n\nLaunch Claude tasks and read native session metadata using Anthropic's SDK.\n\npositional arguments:\n  {new,resume,list,login}\n\noptions:\n  -h, --help            show this help message and exit\n  --project PROJECT\n  --path PATH\n  --account ACCOUNT\n  --thread-id THREAD_ID\n  --chat-directory\n  --tmux\n";

fn err(message: impl Into<String>) -> Error {
    message.into().into()
}
fn home() -> Result<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).ok_or_else(|| err("HOME is not set"))
}
fn executable(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?).map(|path| path.join(name)).find(|path| path.is_file())
}
fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn sanitized(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}
fn reject_linked_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(err("Linked Claude session directories are not supported"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(err("Could not inspect Claude session directories")),
        }
    }
    Ok(())
}
fn read_file(path: &Path) -> Result<Option<Vec<u8>>> {
    read_file_limited(path, MAX_FILE)
}
fn read_file_limited(path: &Path, limit: u64) -> Result<Option<Vec<u8>>> {
    reject_linked_ancestors(path)?;
    let mut file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
    {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(err("Could not inspect Claude session data")),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1 {
        return Err(err("Claude session data is not a private regular file"));
    }
    if metadata.len() > limit {
        return Err(err("Claude session data exceeds the size limit"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("Could not read Claude session data"))?;
    if bytes.len() as u64 > limit { return Err(err("Claude session data exceeds the size limit")); }
    Ok(Some(bytes))
}
fn valid_schema_marker(value: &Value) -> bool {
    for marker in ["schemaVersion", "formatVersion"] {
        if let Some(version) = value.get(marker) {
            if version.as_u64() != Some(1) {
                return false;
            }
        }
    }
    // Claude's JSONL `version` is a provider release string (such as 2.1.29).
    true
}
fn jsonl(path: &Path) -> Vec<Value> {
    let Ok(Some(bytes)) = read_file(path) else { return Vec::new() };
    let mut records = Vec::new();
    for line in bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty() && line.len() <= MAX_LINE) {
        let Ok(value) = serde_json::from_slice::<Value>(line) else { continue };
        if !value.is_object() { continue; }
        if !valid_schema_marker(&value) { return Vec::new(); }
        records.push(value);
    }
    records
}
fn content_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
fn valid_session_records(records: &[Value], session_id: &str) -> bool {
    if records
        .first()
        .is_some_and(|record| record.get("isSidechain").and_then(Value::as_bool) == Some(true))
    {
        return false;
    }
    records.iter().any(|record| {
        matches!(record.get("type").and_then(Value::as_str), Some("user" | "assistant"))
            && record
                .get("sessionId")
                .and_then(Value::as_str)
                .is_none_or(|id| id == session_id)
            && record.get("isMeta").and_then(Value::as_bool) != Some(true)
            && record.get("uuid").and_then(Value::as_str).is_some_and(valid_id)
            && record.get("message").and_then(Value::as_object).is_some()
            && !content_text(&record["message"]).trim().is_empty()
    })
}
fn message_rows(records: &[Value], session_id: &str) -> Vec<Value> {
    if !valid_session_records(records, session_id) {
        return Vec::new();
    }
    records
        .iter()
        .filter_map(|record| {
            let role = record.get("type")?.as_str()?;
            if role != "user" && role != "assistant" {
                return None;
            }
            if record
                .get("sessionId")
                .and_then(Value::as_str)
                .is_some_and(|id| id != session_id)
                || record.get("isSidechain").and_then(Value::as_bool) == Some(true)
                || record.get("isMeta").and_then(Value::as_bool) == Some(true)
                || record.get("isCompactSummary").and_then(Value::as_bool) == Some(true)
            {
                return None;
            }
            let uuid = record.get("uuid")?.as_str()?;
            if !valid_id(uuid) {
                return None;
            }
            let message = record.get("message")?;
            if message
                .get("role")
                .and_then(Value::as_str)
                .is_some_and(|value| value != role)
            {
                return None;
            }
            let text = content_text(message);
            (!text.trim().is_empty()).then(|| json!({"id":uuid,"role":role,"text":text}))
        })
        .collect()
}

/// Return the final 300 user/assistant messages from one validated session.
/// `directory` is the Claude project cwd used by the SDK's sanitized layout.
pub fn messages(config: &Path, session_id: &str, directory: &Path) -> Result<Vec<Value>> {
    if !valid_id(session_id) || !directory.is_absolute() || !config.is_absolute() {
        return Err(err("Invalid Claude session reference"));
    }
    let project = config.join("projects").join(sanitized(&directory.to_string_lossy()));
    let path = project.join(format!("{session_id}.jsonl"));
    reject_linked_ancestors(&project)?;
    let records = jsonl(&path);
    if records
        .iter()
        .filter_map(|record| record.get("cwd").and_then(Value::as_str))
        .any(|cwd| !Path::new(cwd).is_absolute() || Path::new(cwd) != directory)
    {
        return Err(err("Claude session directory does not match its recorded workspace"));
    }
    let rows = message_rows(&records, session_id);
    Ok(rows.into_iter().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect())
}

fn index_entries(project: &Path) -> Result<Option<Vec<Value>>> {
    let Some(bytes) = read_file(&project.join("sessions-index.json"))? else { return Ok(None) };
    let index: Value = serde_json::from_slice(&bytes).map_err(|_| err("Malformed Claude session index"))?;
    if index.get("version").is_some_and(|version| version.as_u64() != Some(1)) {
        return Err(err("Unsupported Claude session index version"));
    }
    for marker in ["schemaVersion", "formatVersion"] {
        if index.get(marker).is_some_and(|version| version.as_u64() != Some(1)) {
            return Err(err("Unsupported Claude session index format"));
        }
    }
    let entries = index
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| err("Malformed Claude session index"))?;
    Ok(Some(entries.clone()))
}
fn session_ids(project: &Path) -> Result<Vec<String>> {
    reject_linked_ancestors(project)?;
    let mut ids = std::collections::BTreeSet::new();
    match index_entries(project) {
        Ok(Some(entries)) => ids.extend(
            entries
                .iter()
                .filter_map(|entry| entry.get("sessionId").and_then(Value::as_str))
                .filter(|id| valid_id(id))
                .map(str::to_owned),
        ),
        Ok(None) => {}
        Err(error) if error.to_string().contains("Unsupported Claude session index") => {
            return Err(error)
        }
        Err(_) => {} // An index is a cache; malformed data falls back to source files.
    }
    let entries = match fs::read_dir(project) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(ids.into_iter().collect()),
        Err(_) => return Err(err("Could not list Claude sessions")),
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str().and_then(|n| n.strip_suffix(".jsonl")) else { continue };
        if valid_id(name) { ids.insert(name.to_owned()); }
    }
    Ok(ids.into_iter().collect())
}
fn bounded_output(mut command: Command, timeout: Duration) -> Option<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().ok()?;
    match child.wait_timeout(timeout).ok()? {
        Some(_) => {
            let output = child.wait_with_output().ok()?;
            (output.stdout.len() <= MAX_OUTPUT).then_some(output)
        }
        None => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}
fn account_tag(account: &str) -> String {
    if account.is_empty() { "default".into() } else { format!("profile-{account}") }
}
fn running_sessions_for(project: &str, account: &str, listing: &str) -> std::collections::BTreeMap<String, String> {
    let prefix = format!("ai-{project}-claude-{}-", account_tag(account));
    listing.lines().filter_map(|line| {
        let (name, cwd) = line.split_once('\t')?;
        name.starts_with(&prefix).then(|| (name.to_owned(), cwd.to_owned()))
    }).collect()
}
fn running_sessions(project: &str, account: &str) -> std::collections::BTreeMap<String, String> {
    if executable("tmux").is_none() { return Default::default(); }
    let mut command = Command::new("tmux");
    command.args(["list-panes", "-a", "-F", "#{session_name}\t#{pane_current_path}"]);
    let Some(output) = bounded_output(command, Duration::from_secs(4)) else { return Default::default() };
    running_sessions_for(project, account, &String::from_utf8_lossy(&output.stdout))
}
fn project_directories(root: &Path, chat_directory: bool) -> Vec<PathBuf> {
    if chat_directory {
        let mut paths = vec![root.to_path_buf()];
        if let Ok(entries) = fs::read_dir(root) {
            paths.extend(entries.flatten().filter_map(|entry| {
                let name = entry.file_name();
                (entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && name.to_str().is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())))
                    .then(|| entry.path())
            }));
        }
        return paths;
    }
    let Some(git) = executable("git") else { return vec![root.to_path_buf()] };
    let mut command = Command::new(git);
    command.args(["-C", &root.to_string_lossy(), "worktree", "list", "--porcelain"]);
    let Some(output) = bounded_output(command, Duration::from_secs(5)) else { return vec![root.to_path_buf()] };
    let paths = parse_worktree_list(&String::from_utf8_lossy(&output.stdout));
    if paths.is_empty() { vec![root.to_path_buf()] } else { paths }
}
fn parse_worktree_list(listing: &str) -> Vec<PathBuf> {
    listing.lines().filter_map(|line| line.strip_prefix("worktree ").map(PathBuf::from)).collect()
}
fn first_prompt(records: &[Value]) -> Option<String> {
    records.iter().find_map(|record| {
        if record.get("type").and_then(Value::as_str) != Some("user")
            || record.get("isMeta").and_then(Value::as_bool) == Some(true)
            || record.get("isCompactSummary").and_then(Value::as_bool) == Some(true)
        {
            return None;
        }
        let text = content_text(record.get("message")?).replace('\n', " ");
        let text = text.trim();
        if text.is_empty() || text.starts_with('<') || text.starts_with("[Request interrupted by user") {
            return None;
        }
        Some(text.chars().take(200).collect())
    })
}
fn title_from(records: &[Value]) -> Option<String> {
    let mut custom = None;
    let mut ai_title = None;
    let mut summary = None;
    let mut last_prompt = None;
    for record in records {
        if record.get("type").and_then(Value::as_str) == Some("custom-title") {
            if let Some(value) = record.get("customTitle").and_then(Value::as_str) { custom = Some(value.to_owned()); }
        }
        if let Some(value) = record.get("aiTitle").and_then(Value::as_str) { ai_title = Some(value.to_owned()); }
        if let Some(value) = record.get("summary").and_then(Value::as_str) { summary = Some(value.to_owned()); }
        if let Some(value) = record.get("lastPrompt").and_then(Value::as_str) { last_prompt = Some(value.to_owned()); }
    }
    custom.or(ai_title).or(summary).or(last_prompt).or_else(|| first_prompt(records))
}
fn history(config: &Path, root: &Path, chat_directory: bool, running: &std::collections::BTreeMap<String, String>) -> Result<Vec<Value>> {
    if !config.join("projects").is_dir() { return Ok(Vec::new()) }
    let roots = project_directories(root, chat_directory);
    let mut output = std::collections::BTreeMap::<String, Value>::new();
    for directory in &roots {
        let project = config.join("projects").join(sanitized(&directory.to_string_lossy()));
        let ids = match session_ids(&project) {
            Ok(ids) => ids,
            Err(error) if error.to_string().contains("Unsupported Claude session index") => return Err(error),
            Err(_) => continue,
        };
        let mut matched = std::collections::BTreeSet::new();
        for id in ids.into_iter().take(MAX_SESSIONS) {
            let path = project.join(format!("{id}.jsonl"));
            let meta = match fs::symlink_metadata(&path) { Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.nlink() == 1 => m, _ => continue };
            let records = jsonl(&path);
            if !valid_session_records(&records, &id) { continue }
            if records.iter().filter_map(|record| record.get("cwd").and_then(Value::as_str())).any(|cwd| {
                !Path::new(cwd).is_absolute() || sanitized(cwd) != project.file_name().and_then(|name| name.to_str()).unwrap_or("")
            }) { continue }
            let title = title_from(&records);
            let mut cwd = None;
            for record in &records {
                if cwd.is_none() { cwd = record.get("cwd").and_then(Value::as_str).map(str::to_owned); }
            }
            let Some(title) = title else { continue };
            let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let cwd = cwd.unwrap_or_else(|| directory.to_string_lossy().into_owned());
            let session = running.iter().find_map(|(name, pane_cwd)| (pane_cwd == &cwd && !matched.contains(name)).then(|| name.clone()));
            if let Some(name) = &session { matched.insert(name.clone()); }
            let row = json!({"id":id,"title":title,"updated_at":modified,"cwd":cwd,"state":if session.is_some(){"running"}else{"saved"},"session":session,"agent":"claude"});
            if output.get(&id).and_then(|old| old["updated_at"].as_f64()).is_none_or(|old| modified > old) { output.insert(id, row); }
        }
    }
    let mut rows: Vec<_> = output.into_values().collect();
    rows.sort_by(|a,b| b["updated_at"].as_f64().unwrap_or(0.0).total_cmp(&a["updated_at"].as_f64().unwrap_or(0.0)));
    Ok(rows)
}

fn prepare_environment(
    mut inherited: std::collections::HashMap<std::ffi::OsString, std::ffi::OsString>,
    config: &Path,
) -> std::collections::HashMap<std::ffi::OsString, std::ffi::OsString> {
    inherited.insert("CLAUDE_CONFIG_DIR".into(), config.as_os_str().into());
    inherited
}
fn merged_status_line(settings: &[Value]) -> serde_json::Map<String, Value> {
    let mut merged = serde_json::Map::new();
    for data in settings {
        if let Some(line) = data.get("statusLine").and_then(Value::as_object) {
            merged.extend(line.clone());
        }
    }
    merged
}
fn launch_args(full_access: bool, status_line: &serde_json::Map<String, Value>, account: &str, resume: Option<&str>, vault: &Path, usage_binary: &Path) -> Vec<String> {
    let mut args = Vec::new();
    if full_access { args.push("--dangerously-skip-permissions".into()); }
    args.extend(["--append-system-prompt".into(), guidance(), "--add-dir".into(), vault.to_string_lossy().into_owned()]);
    let mut usage = vec![usage_binary.to_string_lossy().into_owned(), "capture".into(), "--agent".into(), "claude".into(), "--account".into(), account.into()];
    if let Some(command) = status_line.get("command").and_then(Value::as_str) { usage.extend(["--delegate".into(), command.into()]); }
    let mut status = status_line.clone();
    status.insert("type".into(), json!("command"));
    status.insert("command".into(), json!(usage.iter().map(|item| shell_quote(item)).collect::<Vec<_>>().join(" ")));
    args.extend(["--settings".into(), json!({"statusLine":status}).to_string()]);
    if let Some(id) = resume { args.extend(["--resume".into(), id.into()]); }
    args
}
fn tmux_child(exe: &str, command: &str, project: &str, path: &str, account: &str, thread: Option<&str>, chat: bool) -> String {
    let mut child = format!("{} nacre-ai-claude {} --project {} --path {} --account {}", shell_quote(exe), command, shell_quote(project), shell_quote(path), shell_quote(account));
    if let Some(id) = thread { child.push_str(&format!(" --thread-id {}", shell_quote(id))); }
    if chat { child.push_str(" --chat-directory"); }
    child
}
fn worktree_args(root: &Path, destination: &Path, task: &str, has_head: bool) -> Vec<String> {
    let mut args = vec!["-C".into(), root.to_string_lossy().into_owned(), "worktree".into(), "add".into()];
    if !has_head { args.push("--orphan".into()); }
    args.extend(["-b".into(), format!("claude/{task}"), destination.to_string_lossy().into_owned()]);
    args
}
fn background_agent_id(agents: &[Value], roster: &Value, thread: &str) -> Option<String> {
    agents.iter().find(|agent| {
        agent.get("kind").and_then(Value::as_str) == Some("background")
            && agent.get("id").and_then(Value::as_str).is_some()
            && agent.get("sessionId").and_then(Value::as_str) == Some(thread)
    }).and_then(|agent| agent.get("id")).and_then(Value::as_str).map(str::to_owned).or_else(|| {
        agents.iter().find_map(|agent| {
            if agent.get("kind").and_then(Value::as_str) != Some("background") { return None; }
            let id = agent.get("id")?.as_str()?;
            let launch_path = roster.pointer(&format!("/workers/{id}/dispatch/launch/sessionId"))?.as_str()?;
            (Path::new(launch_path).file_stem().and_then(|name| name.to_str()) == Some(thread)).then(|| id.to_owned())
        })
    })
}
fn background_session(binary: &Path, config: &Path, thread: &str, envs: &std::collections::HashMap<std::ffi::OsString, std::ffi::OsString>) -> Option<String> {
    let mut command = Command::new(binary);
    command.args(["agents", "--json"]).envs(envs);
    let output = bounded_output(command, Duration::from_secs(10))?;
    let agents: Vec<Value> = serde_json::from_slice(&output.stdout).ok()?;
    let roster = read_file_limited(&config.join("daemon/roster.json"), MAX_ROSTER).ok()??;
    let roster: Value = serde_json::from_slice(&roster).ok()?;
    background_agent_id(&agents, &roster, thread)
}

fn run_inner(args: &[String]) -> Result<i32> {
    let mut command = "new".to_owned();
    let mut project = "general-chat".to_owned();
    let mut path = home()?.join(".local/share/nacre/ai/chats");
    let mut account = String::new();
    let mut thread = None;
    let mut chat_directory = false;
    let mut tmux = false;
    let mut i = 0;
    while i < args.len() {
        let value = |offset: usize, label: &str| args.get(offset).ok_or_else(|| err(format!("{label} requires a value")));
        match args[i].as_str() {
            "new" | "resume" | "list" | "login" => command = args[i].clone(),
            "--project" => { i += 1; project = value(i, "--project")?.clone(); }
            "--path" => { i += 1; path = PathBuf::from(value(i, "--path")?); }
            "--account" => { i += 1; account = value(i, "--account")?.clone(); }
            "--thread-id" => { i += 1; thread = Some(value(i, "--thread-id")?.clone()); }
            "--chat-directory" => chat_directory = true,
            "--tmux" => tmux = true,
            "-h" | "--help" => { print!("{HELP}"); return Ok(0); }
            other => return Err(err(format!("Unknown Claude option: {other}"))),
        }
        i += 1;
    }
    if !path.is_absolute() || project.is_empty() || !project.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(err("A valid project ID and absolute path are required"));
    }
    let config = ai_core::account_home("claude", Some(&account))?;
    let envs = prepare_environment(env::vars_os().collect(), &config);
    let skills = Command::new(home()?.join(".local/bin/nacre-ai-skills"))
        .args(["--claude-home", &config.to_string_lossy()])
        .envs(&envs)
        .status()?;
    if !skills.success() { return Err(err("Could not prepare Claude skills")); }
    if command == "list" {
        let running = running_sessions(&project, &account);
        let chats = history(&config, &path, chat_directory, &running).unwrap_or_default();
        println!("{}", serde_json::to_string(&chats)?);
        return Ok(0);
    }
    let binary = executable("claude").ok_or_else(|| err("Claude Code is not installed on this host"))?;
    if command == "login" {
        let error = Command::new(binary).args(["auth", "login"]).envs(&envs).exec();
        return Err(error.into());
    }
    if tmux {
        let tag = account_tag(&account);
        let suffix = thread.clone().unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        let name = format!("ai-{project}-claude-{tag}-{suffix}");
        let exe = env::current_exe()?;
        let child = tmux_child(&exe.to_string_lossy(), &command, &project, &path.to_string_lossy(), &account, thread.as_deref(), chat_directory);
        let error = Command::new("tmux").args(["new-session", "-A", "-s", &name, &child]).envs(&envs).exec();
        return Err(error.into());
    }
    let directory = if command == "resume" { path.clone() } else { prepare_directory(&path, &project, chat_directory)? };
    if !directory.is_dir() { return Err(err("The saved workspace no longer exists")); }
    env::set_current_dir(&directory)?;
    let vault = env::var_os("NACRE_BRAIN").map(PathBuf::from).unwrap_or(home()?.join("Documents/Nacre-Brain"));
    fs::create_dir_all(&vault)?;
    let settings = [config.join("settings.json"), directory.join(".claude/settings.json"), directory.join(".claude/settings.local.json")];
    let setting_values = settings.iter().filter_map(|file| read_file(file).ok().flatten()).filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).collect::<Vec<_>>();
    let status = merged_status_line(&setting_values);
    if command == "resume" {
        let id = thread.as_deref().filter(|id| valid_id(id)).ok_or_else(|| err("A valid Claude session ID is required"))?;
        if let Some(agent) = background_session(&binary, &config, id, &envs) { let error = Command::new(binary).args(["attach", &agent]).envs(&envs).exec(); return Err(error.into()); }
        let usage = home()?.join(".local/bin/nacre-ai-usage");
        let argv = launch_args(ai_core::full_access(), &status, &account, Some(id), &vault, &usage);
        let error = Command::new(binary).args(argv).envs(&envs).exec();
        return Err(error.into());
    }
    let usage = home()?.join(".local/bin/nacre-ai-usage");
    let argv = launch_args(ai_core::full_access(), &status, &account, None, &vault, &usage);
    let error = Command::new(binary).args(argv).envs(&envs).exec();
    Err(error.into())
}

fn prepare_directory(root: &Path, project: &str, chat: bool) -> Result<PathBuf> {
    if chat {
        let directory = root.join(uuid::Uuid::new_v4().simple().to_string());
        fs::create_dir_all(&directory)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        return Ok(directory);
    }
    let task = uuid::Uuid::new_v4().simple().to_string();
    let directory = home()?.join(".local/share/nacre/ai/worktrees").join(project).join(&task);
    fs::create_dir_all(directory.parent().ok_or("Invalid Claude worktree path")?)?;
    let has_head = Command::new("git").args(["-C", &root.to_string_lossy(), "rev-parse", "--verify", "HEAD"]).stdout(Stdio::null()).stderr(Stdio::null()).status()?.success();
    if !Command::new("git").args(worktree_args(root, &directory, &task, has_head)).status()?.success() { return Err(err("Could not create Claude worktree")); }
    Ok(directory)
}
fn shell_quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\\''")) }
fn help() -> &'static str { HELP }
fn guidance() -> String { "This session was opened with Nacre AI. Read ~/.codex/AGENTS.md for shared personal workflow, and ~/.config/nacre/ai/projects.json for project/build-host locations. Read the selected project and working directory AGENTS.md as well as CLAUDE.md before editing. Follow project instructions and use isolated worktrees. Keep this conversation when the user changes project. Search the private brain with nacre-brain search and automatically save durable findings using nacre-brain note; read ~/.agents/skills/nacre-brain/SKILL.md for details. Codex-specific title tools do not apply to Claude: use Claude native session titles and /rename. Never copy Codex or Claude credentials between accounts. A General chat folder is for conversation; resolve relevant projects later using nacre-ai-context QUERY. Save meaningful findings through nacre-ai-memory with --world and --topic, including personal and research topics. Read ~/.agents/skills/nacre-brain/SKILL.md. Do not store transcripts or turn suggestions into facts.".into() }

pub fn run(args: &[String]) -> Result<i32> {
    ai_core::prepare_provider_launch();
    match run_inner(args) { Ok(code) => Ok(code), Err(error) => { eprintln!("nacre-ai-claude: {error}"); Ok(1) } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const CURRENT_ID: &str = "7b7db928-7582-4a1e-91b2-e6b601ca8a2d";
    const PREVIOUS_ID: &str = "9563ba8c-801e-494d-9446-ed9417ebd6e5";
    fn fixture(which: &str) -> (&'static str, &'static str) {
        if which == "current" {
            (
                include_str!("../tests/fixtures/claude/current/sessions-index.json"),
                include_str!("../tests/fixtures/claude/current/session.jsonl"),
            )
        } else {
            (
                include_str!("../tests/fixtures/claude/previous/sessions-index.json"),
                include_str!("../tests/fixtures/claude/previous/session.jsonl"),
            )
        }
    }
    fn put_fixture(config: &Path, cwd: &Path, which: &str, id: &str) -> PathBuf {
        let project = config.join("projects").join(sanitized(&cwd.to_string_lossy()));
        fs::create_dir_all(&project).unwrap();
        let (index, transcript) = fixture(which);
        fs::write(project.join("sessions-index.json"), index).unwrap();
        let transcript = transcript.replace("/fixture", &cwd.to_string_lossy());
        fs::write(project.join(format!("{id}.jsonl")), transcript).unwrap();
        project
    }

    #[test]
    fn frozen_help_matches_the_sdk_wording_byte_for_byte() {
        let frozen: Value = serde_json::from_str(include_str!(
            "../../../tests/parity/helpers/nacre-ai-claude/expected-help.json"
        ))
        .unwrap();
        assert_eq!(help(), frozen["stdout"].as_str().unwrap());
    }

    #[test]
    fn current_and_previous_fixtures_run_through_native_reader() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("claude");
        let current_cwd = temp.path().join("current repo");
        let old_cwd = temp.path().join("old-repo");
        let current = put_fixture(&config, &current_cwd, "current", CURRENT_ID);
        let old = put_fixture(&config, &old_cwd, "previous", PREVIOUS_ID);
        let current_rows = messages(&config, CURRENT_ID, &current_cwd).unwrap();
        let old_rows = messages(&config, PREVIOUS_ID, &old_cwd).unwrap();
        assert_eq!(current_rows[0]["text"], "Fixture current prompt");
        assert_eq!(current_rows[1]["text"], "Fixture answer");
        assert_eq!(old_rows[0]["text"], "Fixture previous prompt");
        assert_eq!(old_rows[1]["text"], "Fixture previous answer");
        assert!(session_ids(&current).unwrap().contains(&CURRENT_ID.to_owned()));
        assert!(session_ids(&old).unwrap().contains(&PREVIOUS_ID.to_owned()));
    }

    #[test]
    fn stale_index_raw_files_title_precedence_fallback_cwd_time_and_dedup() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("claude");
        let root = temp.path().join("chat-root");
        let project = put_fixture(&config, &root, "current", CURRENT_ID);
        let raw_id = "12345678-1234-4234-8234-123456789abc";
        let current = fixture("current").1.replace("/fixture", &root.to_string_lossy());
        fs::write(project.join(format!("{raw_id}.jsonl")), &current).unwrap();
        // A nested private chat folder may contain a duplicate transcript.
        let child = root.join("0123456789abcdef0123456789abcdef");
        let child_project = config.join("projects").join(sanitized(&child.to_string_lossy()));
        fs::create_dir_all(&child_project).unwrap();
        let child_current = current.replace(root.to_str().unwrap(), child.to_str().unwrap());
        fs::write(child_project.join(format!("{CURRENT_ID}.jsonl")), child_current).unwrap();
        let running = running_sessions_for(
            "chat",
            "work",
            &format!("ai-chat-claude-profile-work-a\t{}\nai-chat-claude-profile-work-b\t{}\nai-chat-claude-default-c\t/fixture\n", root.display(), child.display()),
        );
        let rows = history(&config, &root, true, &running).unwrap();
        assert_eq!(rows.iter().filter(|row| row["id"] == CURRENT_ID).count(), 1);
        let row = rows.iter().find(|row| row["id"] == CURRENT_ID).unwrap();
        assert_eq!(row["title"], "Fixture current title");
        assert!(row["cwd"].as_str().unwrap().starts_with(root.to_str().unwrap()));
        assert_eq!(row["state"], "running");
        assert!(row["updated_at"].as_f64().unwrap() > 0.0);
        assert!(rows.iter().any(|row| row["id"] == raw_id));

        let old_cwd = temp.path().join("old cwd fallback");
        let old_project = put_fixture(&config, &old_cwd, "previous", PREVIOUS_ID);
        let (_, previous) = fixture("previous");
        fs::write(old_project.join(format!("{PREVIOUS_ID}.jsonl")), previous).unwrap();
        let old = history(&config, &old_cwd, true, &BTreeMap::new()).unwrap();
        let old_cwd = old_cwd.to_string_lossy().into_owned();
        assert_eq!(old[0]["cwd"].as_str(), Some(old_cwd.as_str()));
        assert_eq!(old[0]["title"], "Fixture previous prompt");
    }

    #[test]
    fn unsupported_index_and_jsonl_schema_and_unsafe_ancestor_fail_closed() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("claude");
        let cwd = temp.path().join("repo");
        let project = put_fixture(&config, &cwd, "current", CURRENT_ID);
        fs::write(project.join("sessions-index.json"), r#"{"version":2,"entries":[]}"#).unwrap();
        assert!(session_ids(&project).unwrap_err().to_string().contains("Unsupported"));
        fs::write(project.join("sessions-index.json"), "{bad json").unwrap();
        assert!(session_ids(&project).unwrap().contains(&CURRENT_ID.to_owned()));
        fs::write(project.join(format!("{CURRENT_ID}.jsonl")), format!("{{\"schemaVersion\":99}}\n{}", fixture("current").1)).unwrap();
        assert!(messages(&config, CURRENT_ID, &cwd).unwrap().is_empty());
        fs::write(project.join(format!("{CURRENT_ID}.jsonl")), fixture("current").1).unwrap();
        let linked = temp.path().join("linked-config");
        std::os::unix::fs::symlink(&config, &linked).unwrap();
        assert!(messages(&linked, CURRENT_ID, &cwd).is_err());
    }

    #[test]
    fn sidechain_meta_and_invalid_rows_do_not_become_messages() {
        let base = json!({"type":"user","uuid":"11111111-1111-4111-8111-111111111111","message":{"content":"real prompt"}});
        assert_eq!(message_rows(std::slice::from_ref(&base), CURRENT_ID).len(), 1);
        let sidechain = json!({"type":"user","isSidechain":true,"uuid":"11111111-1111-4111-8111-111111111111","message":{"content":"hidden"}});
        assert!(message_rows(&[sidechain], CURRENT_ID).is_empty());
        let meta = json!({"type":"user","isMeta":true,"uuid":"11111111-1111-4111-8111-111111111111","message":{"content":"hidden"}});
        assert!(message_rows(&[meta], CURRENT_ID).is_empty());
        assert_eq!(message_rows(&[base.clone(), meta], CURRENT_ID).len(), 1);
        let malformed = json!({"type":"assistant","uuid":"bad","message":{"content":"invalid"}});
        assert!(message_rows(&[malformed], CURRENT_ID).is_empty());
    }

    #[test]
    fn explicit_titles_beat_recent_prompts_and_worktrees_parse_as_history_roots() {
        let prompt = json!({"type":"user","uuid":"11111111-1111-4111-8111-111111111111","message":{"content":"first prompt"}});
        let generated = json!({"type":"summary","aiTitle":"AI title","summary":"Stable summary","lastPrompt":"Recent prompt"});
        assert_eq!(title_from(&[prompt.clone(), generated.clone()]).as_deref(), Some("AI title"));
        let mut summary = generated.clone();
        summary.as_object_mut().unwrap().remove("aiTitle");
        assert_eq!(title_from(&[prompt.clone(), summary]).as_deref(), Some("Stable summary"));
        let mut recent = generated;
        recent.as_object_mut().unwrap().remove("aiTitle");
        recent.as_object_mut().unwrap().remove("summary");
        assert_eq!(title_from(&[prompt.clone(), recent]).as_deref(), Some("Recent prompt"));
        assert_eq!(title_from(&[prompt]).as_deref(), Some("first prompt"));
        assert_eq!(parse_worktree_list("worktree /repo\nHEAD abc\n\nworktree /worktree\nbranch refs/heads/task\n"), vec![PathBuf::from("/repo"), PathBuf::from("/worktree")]);
    }

    #[test]
    fn account_environment_status_overlay_launch_and_resume_contracts_hold() {
        let mut inherited = HashMap::new();
        inherited.insert("CODEX_HOME".into(), "/codex-profile".into());
        inherited.insert("PATH".into(), "/bin".into());
        let envs = prepare_environment(inherited, Path::new("/private/claude-profile"));
        assert_eq!(envs.get(std::ffi::OsStr::new("CLAUDE_CONFIG_DIR")).unwrap().to_string_lossy().as_ref(), "/private/claude-profile");
        assert_eq!(envs.get(std::ffi::OsStr::new("CODEX_HOME")).unwrap().to_string_lossy().as_ref(), "/codex-profile");

        let status = merged_status_line(&[
            json!({"statusLine":{"command":"custom","padding":1}}),
            json!({"statusLine":{"padding":2,"mode":"compact"}}),
        ]);
        assert_eq!(status["command"], "custom");
        assert_eq!(status["padding"], 2);
        assert_eq!(status["mode"], "compact");
        let vault = Path::new("/private/brain");
        let usage = Path::new("/private/bin/nacre-ai-usage");
        let resume_id = "7b7db928-7582-4a1e-91b2-e6b601ca8a2d";
        let standard = launch_args(false, &status, "work", Some(resume_id), vault, usage);
        assert!(!standard.contains(&"--dangerously-skip-permissions".into()));
        assert!(standard.windows(2).any(|pair| pair[0] == "--resume" && pair[1] == resume_id));
        let elevated = launch_args(true, &status, "", None, vault, usage);
        assert!(elevated.contains(&"--dangerously-skip-permissions".into()));
        let settings_index = elevated.iter().position(|item| item == "--settings").unwrap();
        let settings: Value = serde_json::from_str(&elevated[settings_index + 1]).unwrap();
        assert_eq!(settings["statusLine"]["padding"], 2);
        assert_eq!(settings["statusLine"]["mode"], "compact");
        assert_eq!(account_tag(""), "default");
        assert_eq!(account_tag("work"), "profile-work");
        assert!(running_sessions_for("p", "work", "ai-p-claude-profile-work-1\t/repo").contains_key("ai-p-claude-profile-work-1"));
        assert!(running_sessions_for("p", "work", "ai-p-claude-default-1\t/repo").is_empty());
        assert!(tmux_child("/tmp/helper with spaces", "new", "p", "/my repo", "work", None, true).contains("--path '/my repo'"));
    }

    #[test]
    fn handoff_exact_and_roster_fork_and_orphan_worktree_contracts_hold() {
        let original = "7b7db928-7582-4a1e-91b2-e6b601ca8a2d";
        let agents = vec![
            json!({"id":"bg-exact","kind":"background","sessionId":original}),
            json!({"id":"bg-fork","kind":"background","sessionId":"forked"}),
            json!({"id":"interactive","kind":"interactive","sessionId":original}),
        ];
        let roster = json!({"workers":{"bg-fork":{"dispatch":{"launch":{"sessionId":format!("/projects/{original}.jsonl")}}}}});
        assert_eq!(background_agent_id(&agents, &roster, original).as_deref(), Some("bg-exact"));
        assert_eq!(background_agent_id(&agents[1..], &roster, original).as_deref(), Some("bg-fork"));
        let args = worktree_args(Path::new("/repo"), Path::new("/private/tree"), "task", false);
        assert!(args.contains(&"--orphan".to_owned()));
        assert!(args.windows(2).any(|pair| pair[0] == "-b" && pair[1] == "claude/task"));
        let existing = worktree_args(Path::new("/repo"), Path::new("/private/tree"), "task", true);
        assert!(!existing.contains(&"--orphan".to_owned()));
    }
}
