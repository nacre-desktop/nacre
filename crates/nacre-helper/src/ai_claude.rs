//! Native Claude Code launcher and bounded reader for its local session JSONL.
use crate::{ai_core, Error, Result};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use std::os::unix::{fs::OpenOptionsExt, process::CommandExt};

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_LINE: usize = 4 * 1024 * 1024;
const MAX_SESSIONS: usize = 500;

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
fn read_file(path: &Path) -> Result<Option<Vec<u8>>> {
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
    if metadata.len() > MAX_FILE {
        return Err(err("Claude session data exceeds the size limit"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| err("Could not read Claude session data"))?;
    if bytes.len() as u64 > MAX_FILE { return Err(err("Claude session data exceeds the size limit")); }
    Ok(Some(bytes))
}
fn jsonl(path: &Path) -> Vec<Value> {
    let Ok(Some(bytes)) = read_file(path) else { return Vec::new() };
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty() && line.len() <= MAX_LINE)
        .filter_map(|line| serde_json::from_slice(line).ok())
        .filter(Value::is_object)
        .collect()
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

/// Return the final 300 user/assistant messages from one validated session.
/// `directory` is the Claude project cwd used by the SDK's sanitized layout.
pub fn messages(config: &Path, session_id: &str, directory: &Path) -> Result<Vec<Value>> {
    if !valid_id(session_id) || !directory.is_absolute() {
        return Err(err("Invalid Claude session reference"));
    }
    let project = config.join("projects").join(sanitized(&directory.to_string_lossy()));
    let path = project.join(format!("{session_id}.jsonl"));
    let rows = jsonl(&path)
        .into_iter()
        .filter_map(|item| {
            let role = item.get("type")?.as_str()?;
            if role != "user" && role != "assistant" {
                return None;
            }
            let text = content_text(item.get("message")?);
            (!text.is_empty()).then(|| {
                json!({"id":item.get("uuid").and_then(Value::as_str).unwrap_or(""),"role":role,"text":text})
            })
        })
        .collect::<Vec<_>>();
    Ok(rows.into_iter().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect())
}

fn index_entries(project: &Path) -> Result<Option<Vec<Value>>> {
    let Some(bytes) = read_file(&project.join("sessions-index.json"))? else { return Ok(None) };
    let index: Value = serde_json::from_slice(&bytes).map_err(|_| err("Malformed Claude session index"))?;
    let version = index.get("version").and_then(Value::as_u64);
    if version.is_some_and(|v| v != 1) {
        return Err(err("Unsupported Claude session index version"));
    }
    let entries = index
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| err("Malformed Claude session index"))?;
    Ok(Some(entries.clone()))
}
fn session_ids(project: &Path) -> Result<Vec<String>> {
    if let Some(entries) = index_entries(project)? {
        return Ok(entries
            .iter()
            .filter_map(|entry| entry.get("sessionId").and_then(Value::as_str))
            .filter(|id| valid_id(id))
            .map(str::to_owned)
            .collect());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(project).map_err(|_| err("Could not list Claude sessions"))? {
        let entry = entry.map_err(|_| err("Could not list Claude sessions"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str().and_then(|n| n.strip_suffix(".jsonl")) else { continue };
        if valid_id(name) { ids.push(name.into()); }
    }
    Ok(ids)
}
fn running_sessions(project: &str, account: &str) -> std::collections::BTreeMap<String, String> {
    if executable("tmux").is_none() { return Default::default(); }
    let tag = if account.is_empty() { "default".into() } else { format!("profile-{account}") };
    let prefix = format!("ai-{project}-claude-{tag}-");
    let Ok(output) = Command::new("tmux").args(["list-panes", "-a", "-F", "#{session_name}\t#{pane_current_path}"]).output() else { return Default::default() };
    String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| {
        let (name, cwd) = line.split_once('\t')?;
        name.starts_with(&prefix).then(|| (name.to_owned(), cwd.to_owned()))
    }).collect()
}
fn history(config: &Path, root: &Path, chat_directory: bool, project_id: &str, account: &str) -> Result<Vec<Value>> {
    if !config.join("projects").is_dir() { return Ok(Vec::new()) }
    let roots = if chat_directory && root.is_dir() {
        let mut paths = vec![root.to_path_buf()];
        if let Ok(entries) = fs::read_dir(root) {
            paths.extend(entries.flatten().filter_map(|entry| {
                let name = entry.file_name();
                (entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && name.to_str().is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())))
                    .then(|| entry.path())
            }));
        }
        paths
    } else { vec![root.to_path_buf()] };
    let mut output = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for directory in roots {
        let project = config.join("projects").join(sanitized(&directory.to_string_lossy()));
        let ids = match session_ids(&project) {
            Ok(ids) => ids,
            Err(error) if error.to_string().contains("Unsupported Claude session index version") => return Err(error),
            Err(_) => continue,
        };
        for id in ids.into_iter().take(MAX_SESSIONS) {
            if !seen.insert(id.clone()) { continue }
            let path = project.join(format!("{id}.jsonl"));
            let meta = match fs::metadata(&path) { Ok(m) if m.is_file() => m, _ => continue };
            let records = jsonl(&path);
            if records.first().is_some_and(|r| r.get("isSidechain").and_then(Value::as_bool) == Some(true)) { continue }
            let mut custom = None;
            let mut summary = None;
            let mut first = None;
            let mut cwd = None;
            for record in &records {
                if let Some(value) = record.get("customTitle").and_then(Value::as_str) { custom = Some(value.to_owned()); }
                if let Some(value) = record.get("aiTitle").and_then(Value::as_str) { summary = Some(value.to_owned()); }
                if cwd.is_none() { cwd = record.get("cwd").and_then(Value::as_str).map(str::to_owned); }
                if record.get("type").and_then(Value::as_str) == Some("user") && first.is_none() {
                    let text = content_text(record.get("message").unwrap_or(&Value::Null)).replace('\n', " ");
                    if !text.trim().is_empty() && !text.starts_with('<') && !text.starts_with("<command-name>") { first = Some(text.chars().take(200).collect::<String>()); }
                }
                if let Some(value) = record.get("lastPrompt").and_then(Value::as_str).or_else(|| record.get("summary").and_then(Value::as_str)) { summary = Some(value.to_owned()); }
            }
            let title = custom.or(summary).or(first).unwrap_or_else(|| "New chat".into());
            let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            output.push(json!({"id":id,"title":title,"updated_at":modified,"cwd":cwd.unwrap_or_else(|| directory.to_string_lossy().into_owned()),"state":"saved","session":null,"agent":"claude"}));
        }
    }
    let running = running_sessions(project_id, account);
    let mut matched = std::collections::BTreeSet::new();
    for row in &mut output {
        let cwd = row["cwd"].as_str().unwrap_or("");
        if let Some((name, _)) = running.iter().find(|(name, path)| path == cwd && !matched.contains(*name)) {
            row["state"] = json!("running");
            row["session"] = json!(name);
            matched.insert(name.clone());
        }
    }
    output.sort_by(|a,b| b["updated_at"].as_f64().unwrap_or(0.0).total_cmp(&a["updated_at"].as_f64().unwrap_or(0.0)));
    Ok(output)
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
            "-h" | "--help" => { println!("usage: nacre-ai-claude new|resume|list|login [--project ID] [--path ABS] [--account NAME] [--thread-id UUID] [--chat-directory] [--tmux]"); return Ok(0); }
            other => return Err(err(format!("Unknown Claude option: {other}"))),
        }
        i += 1;
    }
    if !path.is_absolute() || project.is_empty() || !project.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(err("A valid project ID and absolute path are required"));
    }
    let config = ai_core::account_home("claude", Some(&account))?;
    if command == "list" {
        let chats = history(&config, &path, chat_directory, &project, &account).unwrap_or_default();
        println!("{}", serde_json::to_string(&chats)?);
        return Ok(0);
    }
    let binary = executable("claude").ok_or_else(|| err("Claude Code is not installed on this host"))?;
    let mut envs = env::vars_os().collect::<std::collections::HashMap<_, _>>();
    envs.insert("CLAUDE_CONFIG_DIR".into(), config.as_os_str().into());
    let envs: Vec<_> = envs.into_iter().collect();
    let mut skills = Command::new(home()?.join(".local/bin/nacre-ai-skills"));
    skills.args(["--claude-home", &config.to_string_lossy()]);
    if !skills.status()?.success() { return Err(err("Could not prepare Claude skills")); }
    if command == "login" {
        use std::os::unix::process::CommandExt;
        let error = Command::new(binary).args(["auth", "login"]).envs(envs).exec();
        return Err(error.into());
    }
    if tmux {
        let tag = if account.is_empty() { "default".into() } else { format!("profile-{account}") };
        let suffix = thread.clone().unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());
        let name = format!("ai-{project}-claude-{tag}-{suffix}");
        let exe = env::current_exe()?;
        let mut child = format!("{} nacre-ai-claude {} --project {} --path {} --account {}", shell_quote(&exe.to_string_lossy()), command, shell_quote(&project), shell_quote(&path.to_string_lossy()), shell_quote(&account));
        if let Some(id) = &thread { child.push_str(&format!(" --thread-id {}", shell_quote(id))); }
        if chat_directory { child.push_str(" --chat-directory"); }
        let error = Command::new("tmux").args(["new-session", "-A", "-s", &name, &child]).envs(envs).exec();
        return Err(error.into());
    }
    let directory = if command == "resume" { path.clone() } else { prepare_directory(&path, &project, chat_directory)? };
    if !directory.is_dir() { return Err(err("The saved workspace no longer exists")); }
    env::set_current_dir(&directory)?;
    let vault = env::var_os("NACRE_BRAIN").map(PathBuf::from).unwrap_or(home()?.join("Documents/Nacre-Brain"));
    fs::create_dir_all(&vault)?;
    let mut argv = Vec::<String>::new();
    if ai_core::full_access() { argv.push("--dangerously-skip-permissions".into()); }
    argv.extend(["--append-system-prompt".into(), guidance(), "--add-dir".into(), vault.to_string_lossy().into_owned()]);
    let settings = [config.join("settings.json"), directory.join(".claude/settings.json"), directory.join(".claude/settings.local.json")];
    let mut status = Value::Object(Default::default());
    for file in settings { if let Some(bytes) = read_file(&file)? { if let Ok(data) = serde_json::from_slice::<Value>(&bytes) { if let Some(line) = data.get("statusLine") { if line.is_object() { status = line.clone(); } } } } }
    let mut usage = vec![home()?.join(".local/bin/nacre-ai-usage").to_string_lossy().into_owned(), "capture".into(), "--agent".into(), "claude".into(), "--account".into(), account.clone()];
    if let Some(cmd) = status.get("command").and_then(Value::as_str) { usage.extend(["--delegate".into(), cmd.into()]); }
    argv.extend(["--settings".into(), json!({"statusLine":{"type":"command","command":usage.iter().map(|s| shell_quote(s)).collect::<Vec<_>>().join(" ")}}).to_string()]);
    if command == "resume" {
        let id = thread.as_deref().filter(|id| valid_id(id)).ok_or_else(|| err("A valid Claude session ID is required"))?;
        if let Some(agent) = background_session(&binary, &config, id, &envs) { let error = Command::new(binary).args(["attach", &agent]).envs(envs).exec(); return Err(error.into()); }
        argv.extend(["--resume".into(), id.into()]);
    }
    let error = Command::new(binary).args(argv).envs(envs).exec();
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
    let mut cmd = Command::new("git");
    cmd.args(["-C", &root.to_string_lossy(), "worktree", "add"]);
    if !has_head { cmd.arg("--orphan"); }
    if !cmd.args(["-b", &format!("claude/{task}"), &directory.to_string_lossy()]).status()?.success() { return Err(err("Could not create Claude worktree")); }
    Ok(directory)
}
fn background_session(binary: &Path, config: &Path, thread: &str, envs: &[(std::ffi::OsString, std::ffi::OsString)]) -> Option<String> {
    let output = Command::new(binary).args(["agents", "--json"]).envs(envs).output().ok()?;
    let agents: Vec<Value> = serde_json::from_slice(&output.stdout).ok()?;
    if let Some(id) = agents.iter().find(|a| a.get("kind").and_then(Value::as_str) == Some("background") && a.get("sessionId").and_then(Value::as_str) == Some(thread)).and_then(|a| a.get("id")).and_then(Value::as_str) { return Some(id.into()); }
    let roster: Value = serde_json::from_slice(&fs::read(config.join("daemon/roster.json")).ok()?).ok()?;
    agents.iter().find(|agent| {
        agent.get("kind").and_then(Value::as_str) == Some("background")
            && roster.pointer(&format!("/workers/{}/dispatch/launch/sessionId", agent.get("id").and_then(Value::as_str).unwrap_or(""))).and_then(Value::as_str).is_some_and(|p| Path::new(p).file_stem().and_then(|s| s.to_str()) == Some(thread))
    }).and_then(|a| a.get("id")).and_then(Value::as_str).map(str::to_owned)
}
fn shell_quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\\''")) }
fn guidance() -> String { "This session was opened with Nacre AI. Read ~/.codex/AGENTS.md for shared personal workflow, and ~/.config/nacre/ai/projects.json for project/build-host locations. Read project AGENTS.md and CLAUDE.md before editing. Use isolated worktrees. Search and save durable findings through nacre-brain; do not store transcripts or turn suggestions into facts. Never copy AI credentials between accounts. A General chat folder is for conversation; resolve projects later using nacre-ai-context QUERY.".into() }

pub fn run(args: &[String]) -> Result<i32> {
    ai_core::prepare_provider_launch();
    match run_inner(args) { Ok(code) => Ok(code), Err(error) => { eprintln!("nacre-ai-claude: {error}"); Ok(1) } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn reads_previous_and_current_index_schemas_with_unknown_fields() {
        let current: Value = serde_json::from_str(include_str!("../tests/fixtures/claude/current/sessions-index.json")).unwrap();
        let previous: Value = serde_json::from_str(include_str!("../tests/fixtures/claude/previous/sessions-index.json")).unwrap();
        assert_eq!(current["version"], 1);
        assert_eq!(current["entries"][0]["sessionId"], "7b7db928-7582-4a1e-91b2-e6b601ca8a2d");
        assert!(current["entries"][0].get("newUpstreamField").is_some());
        assert!(previous.get("version").is_none());
        assert_eq!(previous["entries"][0]["sessionId"], "9563ba8c-801e-494d-9446-ed9417ebd6e5");
        assert_eq!(content_text(&json!({"content":"old string form"})), "old string form");
        assert_eq!(content_text(&json!({"content":[{"type":"text","text":"new block form"},{"type":"image"}]})), "new block form");
    }

    #[test]
    fn history_reader_uses_only_synthetic_home_and_degrades_on_bad_data() {
        let temp = tempdir().unwrap();
        let config = temp.path().join("claude");
        let cwd = temp.path().join("repo");
        let project = config.join("projects").join(sanitized(&cwd.to_string_lossy()));
        fs::create_dir_all(&project).unwrap();
        let id = "7b7db928-7582-4a1e-91b2-e6b601ca8a2d";
        fs::write(project.join("sessions-index.json"), include_str!("../tests/fixtures/claude/current/sessions-index.json")).unwrap();
        fs::write(project.join(format!("{id}.jsonl")), include_str!("../tests/fixtures/claude/current/session.jsonl")).unwrap();
        let rows = messages(&config, id, &cwd).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["text"], "Fixture current prompt");
        assert_eq!(rows[1]["role"], "assistant");
        assert!(session_ids(&project).unwrap().contains(&id.to_owned()));
        fs::write(project.join("sessions-index.json"), r#"{"version":999,"entries":[]}"#).unwrap();
        assert!(session_ids(&project).unwrap_err().to_string().contains("Unsupported"));
        fs::write(project.join("sessions-index.json"), "{bad json").unwrap();
        assert!(session_ids(&project).is_err());
    }
}
