//! Shared account-independent primitives and small AI command wrappers.
use crate::{Error, Result};
use chrono::{DateTime, Local, Utc};
use clap::Parser;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};
use unicode_casefold::UnicodeCaseFold;

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?))
}
fn error(message: impl Into<String>) -> Error {
    message.into().into()
}

pub fn slug(value: &str) -> String {
    let slug = value
        .case_fold()
        .collect::<String>()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    slug.chars().take(70).collect()
}

pub fn one(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect()
}

/// Resolve projects whose id, label, or aliases occur as whole words in a query.
pub fn resolve_context(query: &str, registry: &Value) -> Value {
    let folded = query.case_fold().collect::<String>();
    let projects: Vec<Value> = registry
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|project| {
            let aliases = [project.get("id"), project.get("label")]
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .chain(
                    project
                        .get("aliases")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                )
                .chain(
                    project
                        .get("brain")
                        .and_then(|b| b.get("aliases"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                );
            let matches: Vec<String> = aliases
                .filter(|alias| whole_word_match(&folded, &alias.case_fold().collect::<String>()))
                .map(str::to_owned)
                .collect();
            let mut row = serde_json::Map::new();
            for key in [
                "id",
                "label",
                "path",
                "host",
                "build_host",
                "build_path",
                "task_sources",
            ] {
                if let Some(value) = project.get(key) {
                    row.insert(key.into(), value.clone());
                }
            }
            row.insert("matches".into(), json!(matches));
            Value::Object(row)
        })
        .collect();
    let matched: Vec<Value> = projects
        .iter()
        .filter(|p| {
            p.get("matches")
                .and_then(Value::as_array)
                .is_some_and(|m| !m.is_empty())
        })
        .cloned()
        .collect();
    json!({"matched":matched,"projects":projects,"ambiguous":matched.len()>1,"instruction":"Read repository instructions before editing. Use isolated local worktrees; heavy builds use build_host/build_path and exact changes in isolated remote tmux worktrees. No match is not permission to invent a repository. Multiple matches may be legitimate cross-project work; clarify only if target is unclear."})
}
fn whole_word_match(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(start, text)| {
        let end = start + text.len();
        let boundary = |c: Option<char>| c.is_none_or(|v| !v.is_alphanumeric() && v != '_');
        boundary(haystack[..start].chars().next_back()) && boundary(haystack[end..].chars().next())
    })
}
/// The owner opt-in is true only for a literal JSON boolean true.
pub fn full_access() -> bool {
    let Ok(home) = home() else { return false };
    crate::state::read(&home.join(".config/nacre/profile.json"), json!({}))
        .ok()
        .and_then(|profile| profile.get("aiFullAccess").and_then(Value::as_bool))
        == Some(true)
}

fn valid_account(account: &str) -> bool {
    let mut chars = account.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && account.len() <= 48
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Return the selected provider home, repairing its directory mode without inspecting its contents.
pub fn account_home(agent: &str, account: Option<&str>) -> Result<PathBuf> {
    let home = home()?;
    let account = account.unwrap_or("");
    if !account.is_empty() && !valid_account(account) {
        return Err(error("Invalid account"));
    }
    let path = match (agent, account.is_empty()) {
        ("codex", true) => home.join(".codex"),
        ("codex", false) => home.join(".local/share/nacre/ai/accounts").join(account),
        ("claude", true) => home.join(".claude"),
        ("claude", false) => home
            .join(".local/share/nacre/ai/claude-accounts")
            .join(account),
        _ => return Err(error("Invalid agent")),
    };
    if !account.is_empty() {
        reject_symlink_chain(&path)?;
        fs::create_dir_all(&path)?;
        reject_symlink_chain(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(path)
}

fn reject_symlink_chain(path: &std::path::Path) -> Result<()> {
    for component in path.ancestors() {
        if fs::symlink_metadata(component).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(error("Symlinked AI account paths are not supported"));
        }
    }
    Ok(())
}

/// Apply the private creation mask immediately before launching an AI provider.
pub fn prepare_provider_launch() {
    let _ = nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
}

/// Resolve Codex's configured shared SQLite history directory without reading databases.
pub fn shared_history_home() -> Result<PathBuf> {
    let home = home()?;
    let config = fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default();
    let parsed: toml::Value = toml::from_str(&config)?;
    let found = parsed
        .get("sqlite_home")
        .and_then(toml::Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    let path = found
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let path = if path == std::path::Path::new("~") {
        home.clone()
    } else if path.starts_with("~/") {
        home.join(path.strip_prefix("~/").unwrap_or(&path))
    } else if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };
    Ok(fs::canonicalize(&path).unwrap_or(path))
}

/// Return only environment deltas; inherited values (which may contain secrets) are never serialized.
pub fn environment_delta(agent: &str, account: Option<&str>) -> Result<Value> {
    let home = account_home(agent, account)?;
    let mut set = BTreeMap::new();
    match agent {
        "codex" => {
            if account.is_some_and(|a| !a.is_empty()) {
                set.insert("CODEX_HOME", home.display().to_string());
            }
        }
        "claude" => {
            set.insert("CLAUDE_CONFIG_DIR", home.display().to_string());
        }
        _ => return Err(error("Invalid agent")),
    }
    Ok(json!({"set":set,"unset":[]}))
}

fn projects_path() -> Result<PathBuf> {
    Ok(env::var_os("NACRE_AI_PROJECTS")
        .map(PathBuf::from)
        .unwrap_or(home()?.join(".config/nacre/ai/projects.json")))
}
fn context(args: &[String]) -> Result<i32> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print!("usage: nacre-ai-context [-h] [query ...]\n\nResolve conversation context without selecting or executing a project.\n\npositional arguments:\n  query\n\noptions:\n  -h, --help  show this help message and exit\n");
        return Ok(0);
    }
    if args.iter().any(|arg| arg.starts_with('-')) {
        return Ok(2);
    }
    let query = args.join(" ");
    let path = projects_path()?;
    let registry = if path.exists() {
        crate::state::read_as(&path, Some(crate::state::by_id("ai-projects")?), json!({}))?
    } else {
        json!({})
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&resolve_context(&query, &registry))?
    );
    Ok(0)
}
fn memory(args: &[String]) -> Result<i32> {
    let (mut kind, mut title, mut source, mut confidence) =
        ("inbox".to_owned(), None, None, "reported".to_owned());
    let (mut worlds, mut topics) = (Vec::new(), Vec::new());
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" | "--title" | "--source" | "--confidence" | "--world" | "--topic" => {
                let option = args[i].as_str();
                i += 1;
                let Some(value) = args.get(i).cloned() else {
                    return Ok(2);
                };
                match option {
                    "--kind" => kind = value,
                    "--title" => title = Some(value),
                    "--source" => source = Some(value),
                    "--confidence" => confidence = value,
                    "--world" => worlds.push(value),
                    "--topic" => topics.push(value),
                    _ => unreachable!(),
                }
            }
            "slug" => {
                let request: Value =
                    serde_json::from_str(args.get(i + 1).ok_or("missing slug value")?)?;
                let value = request
                    .get("args")
                    .and_then(Value::as_array)
                    .and_then(|values| values.first())
                    .and_then(Value::as_str)
                    .ok_or("missing slug value")?;
                println!("{}", json!(slug(value)));
                return Ok(0);
            }
            "-h" | "--help" => {
                print!("usage: nacre-ai-memory [-h] --title TITLE --source SOURCE\n                       [--kind {{inbox,personal,projects,infrastructure,devices,runbooks,decisions,checkpoints}}]\n                       [--confidence {{verified,reported,unverified}}]\n                       [--world WORLD] [--topic TOPIC]\n\nStore concise evidence and create discoverable, cross-project brain entities.\n\noptions:\n  -h, --help            show this help message and exit\n  --title TITLE\n  --source SOURCE\n  --kind {{inbox,personal,projects,infrastructure,devices,runbooks,decisions,checkpoints}}\n  --confidence {{verified,reported,unverified}}\n  --world WORLD\n  --topic TOPIC\n");
                return Ok(0);
            }
            _ => return Ok(2),
        }
        i += 1;
    }
    if ![
        "inbox",
        "personal",
        "projects",
        "infrastructure",
        "devices",
        "runbooks",
        "decisions",
        "checkpoints",
    ]
    .contains(&kind.as_str())
        || !["verified", "reported", "unverified"].contains(&confidence.as_str())
    {
        return Ok(2);
    }
    let Some(title) = title else {
        return Ok(2);
    };
    let Some(source) = source else {
        return Ok(2);
    };
    let mut body = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut body)?;
    let mut raw_metadata = vec![title.as_str(), source.as_str(), body.as_str()];
    raw_metadata.extend(worlds.iter().map(String::as_str));
    raw_metadata.extend(topics.iter().map(String::as_str));
    crate::brain_data::validate_note(&raw_metadata.join("\n"))?;
    if body.trim().is_empty() {
        return Err(error("Memory is empty"));
    }
    let worlds = stable_unique(
        worlds
            .into_iter()
            .map(|value| one(&value))
            .filter(|value| !slug(value).is_empty())
            .collect(),
    );
    let topics = stable_unique(
        topics
            .into_iter()
            .map(|value| one(&value).replace(',', " "))
            .filter(|value| !slug(value).is_empty())
            .collect(),
    );
    let normalized_title = one(&title);
    let normalized_source = one(&source);
    let normalized_topics = topics;
    let world_json = python_json_array(&worlds)?;
    let topic_json = python_json_array(&normalized_topics)?;
    let signature_data = format!(
        "[{}, {}, {}, {}, {}, {}, {}]",
        serde_json::to_string(&kind)?,
        serde_json::to_string(&normalized_title)?,
        serde_json::to_string(&normalized_source)?,
        serde_json::to_string(&confidence)?,
        world_json,
        topic_json,
        serde_json::to_string(body.trim())?
    );
    let signature = format!("{:x}", Sha256::digest(signature_data.as_bytes()));
    let root = crate::brain_data::root_path()?;
    let state = root.join(".brain-state");
    if fs::symlink_metadata(&state).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(error("Symlinked vault paths are not supported"));
    }
    fs::create_dir_all(&state)?;
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700))?;
    let lock_path = state.join("capture.lock");
    if fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(error("Symlinked vault paths are not supported"));
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&lock_path)?;
    if !lock.metadata()?.is_file() || lock.metadata()?.nlink() != 1 {
        return Err(error("Linked or non-regular memory lock"));
    }
    lock.set_permissions(fs::Permissions::from_mode(0o600))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)?;
    let record = state.join(format!("capture-{signature}.json"));
    if fs::symlink_metadata(&record).is_ok_and(|metadata| {
        metadata.file_type().is_symlink() || !metadata.is_file() || metadata.nlink() != 1
    }) {
        return Err(error("Linked or non-regular memory record"));
    }
    let note = if record.exists() {
        let data: Value = serde_json::from_slice(&fs::read(&record)?)?;
        let stored = PathBuf::from(
            data.get("path")
                .and_then(Value::as_str)
                .ok_or("Invalid memory record")?,
        );
        let relative = stored
            .strip_prefix(&root)
            .map_err(|_| error("Memory record escapes the vault"))?;
        let validated = crate::brain_data::validate_note_path(&relative.to_string_lossy())?;
        let metadata = fs::symlink_metadata(&validated)?;
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
            return Err(error("Linked, non-regular, or non-private memory note"));
        }
        validated
    } else {
        let metadata = format!(
            "Worlds: {}\nTopics: {}\n\n",
            worlds.join(", "),
            normalized_topics.join(", ")
        );
        let note = crate::brain_data::write_note(
            &kind,
            &normalized_title,
            &normalized_source,
            &confidence,
            &(metadata + body.trim()),
        )?;
        crate::files::atomic(&record, &serde_json::to_vec(&json!({"path":note}))?)?;
        fs::set_permissions(&record, fs::Permissions::from_mode(0o600))?;
        note
    };
    for world in &worlds {
        ensure_world_page(&root, &note, world)?;
    }
    println!(
        "{}",
        json!({"path":note,"worlds":worlds,"topics":normalized_topics})
    );
    Ok(0)
}
fn python_json_array(values: &[String]) -> Result<String> {
    Ok(format!(
        "[{}]",
        values
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    ))
}

fn stable_unique(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}
fn ensure_world_page(root: &std::path::Path, note: &std::path::Path, world: &str) -> Result<()> {
    let ident = slug(world);
    if ident.is_empty() {
        return Ok(());
    }
    let wiki = root.join("wiki");
    if fs::symlink_metadata(&wiki).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(error("Symlinked vault directories are not supported"));
    }
    if wiki.is_dir() {
        for entry in fs::read_dir(&wiki)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink()
                || !entry.file_type()?.is_file()
                || entry.path().extension().is_none_or(|ext| ext != "md")
            {
                continue;
            }
            let text = fs::read_to_string(entry.path())?;
            let mut names = vec![entry
                .path()
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()];
            for line in text.lines() {
                let lower = line.to_ascii_lowercase();
                if lower.starts_with("name:") {
                    names.push(line[5..].trim().to_owned());
                }
                if lower.starts_with("aliases:") {
                    names.extend(line[8..].split(',').map(|v| v.trim().to_owned()));
                }
            }
            if names.iter().any(|name| slug(name) == ident) {
                return Ok(());
            }
        }
    }
    let relative = note
        .strip_prefix(root)
        .unwrap_or(note)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");
    let content = format!("# {world}\n\nEntity: world\nName: {world}\nAliases: {ident}\nReviewed: {}\nStatus: Evidence-backed subject; initial descriptor\nSource: [[{relative}]]\n\nThis subject was recorded from conversation or work. It does not imply a registered repository.\n\n## Evidence\n\n- [[{relative}]]\n", Local::now().date_naive());
    crate::brain_data::create_page(&format!("wiki/entity-{ident}.md"), &content)?;
    Ok(())
}

pub fn number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite) && !value.is_boolean()
}
pub fn usage_snapshot(data: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in ["five_hour", "seven_day"] {
        if let Some(window) = data
            .get("rate_limits")
            .and_then(|v| v.get(key))
            .and_then(Value::as_object)
        {
            let clean: serde_json::Map<String, Value> = window
                .iter()
                .filter(|(k, v)| {
                    ["used_percentage", "resets_at"].contains(&k.as_str()) && number(v)
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            result.insert(key.into(), Value::Object(clean));
        }
    }
    let session: serde_json::Map<String, Value> = data
        .get("cost")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(k, v)| ["total_cost_usd", "total_duration_ms"].contains(&k.as_str()) && number(v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    result.insert("session".into(), Value::Object(session));
    result.insert("observed_at".into(), json!(Utc::now().to_rfc3339()));
    Value::Object(result)
}
fn read_private_json(path: &std::path::Path) -> Option<Value> {
    reject_symlink_chain(path).ok()?;
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > 1_048_576 {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn codex_usage(account: &str) -> Result<(Value, Value)> {
    let mut command = std::process::Command::new(home()?.join(".local/bin/codex"));
    command.args(["app-server", "--stdio"]);
    command.env(
        "CODEX_HOME",
        account_home("codex", (!account.is_empty()).then_some(account))?,
    );
    prepare_provider_launch();
    crate::ai_rpc::usage(command, std::time::Duration::from_secs(8))
}
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn reset_time(value: &Value) -> String {
    value
        .as_f64()
        .and_then(|seconds| DateTime::from_timestamp(seconds as i64, 0))
        .map(|d| d.with_timezone(&Local).format("%a %H:%M").to_string())
        .unwrap_or_else(|| "unknown".into())
}

#[derive(Parser)]
#[command(
    disable_help_flag = true,
    disable_version_flag = true,
    args_override_self = true
)]
struct UsageArguments {
    #[arg(value_parser = ["read", "capture"])]
    command: String,
    #[arg(long, default_value = "codex", value_parser = ["codex", "claude"])]
    agent: String,
    #[arg(long, default_value = "")]
    account: String,
    #[arg(long, default_value = "")]
    delegate: String,
}

fn usage(args: &[String]) -> Result<i32> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("usage: nacre-ai-usage [-h] {{read,capture}} [--agent {{codex,claude}}] [--account ACCOUNT] [--delegate DELEGATE]\n\nRead provider-supported quota data; capture Claude's native status-line data.\n\npositional arguments:\n  {{read,capture}}\n\noptions:\n  -h, --help            show this help message and exit\n  --agent {{codex,claude}}\n  --account ACCOUNT\n  --delegate DELEGATE\n");
        return Ok(0);
    }
    if args.first().is_some_and(|command| command == "number") {
        let request: Value = serde_json::from_str(args.get(1).ok_or("missing numeric value")?)?;
        let value = request
            .get("args")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .cloned()
            .unwrap_or(Value::Null);
        println!("{}", number(&value));
        return Ok(0);
    }
    let parsed = match UsageArguments::try_parse_from(
        std::iter::once("nacre-ai-usage").chain(args.iter().map(String::as_str)),
    ) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(2),
    };
    let command = parsed.command.as_str();
    let agent = parsed.agent.as_str();
    let account = parsed.account.as_str();
    if !account.is_empty() && !valid_account(account) {
        if command == "capture" {
            return Ok(1);
        }
        println!("{}", json!({"rows":["Usage unavailable"]}));
        return Ok(0);
    }
    let root = home()?.join(".local/state/nacre/ai/usage");
    let path = root.join(format!(
        "claude-{}.json",
        if account.is_empty() {
            "default"
        } else {
            account
        }
    ));
    if command == "capture" {
        let mut input = String::new();
        let mut stdin = std::io::stdin().take(1_048_577);
        std::io::Read::read_to_string(&mut stdin, &mut input)?;
        if input.len() > 1_048_576 {
            return Err(error("Usage input exceeds 1 MiB"));
        }
        let snapshot = usage_snapshot(&serde_json::from_str(&input)?);
        reject_symlink_chain(&root)?;
        fs::create_dir_all(&root)?;
        reject_symlink_chain(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        use std::os::unix::fs::OpenOptionsExt;
        let temp = root.join(format!(
            ".usage-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let bytes = serde_json::to_vec(&snapshot)?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?
            .write_all(&bytes)?;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
        fs::rename(temp, path)?;
        if !parsed.delegate.is_empty() {
            let _ = crate::ai_rpc::delegate(
                &parsed.delegate,
                input.as_bytes(),
                std::time::Duration::from_secs(5),
            );
            return Ok(0);
        }
        let windows = ["five_hour", "seven_day"]
            .iter()
            .filter_map(|key| {
                snapshot
                    .get(key)
                    .and_then(|v| v.get("used_percentage"))
                    .and_then(Value::as_f64)
                    .map(|pct| format!("{} {}%", key.replace('_', " "), pct))
            })
            .collect::<Vec<_>>();
        println!(
            "Nacre · {}",
            if windows.is_empty() {
                "Claude".into()
            } else {
                windows.join(" · ")
            }
        );
        return Ok(0);
    }
    let label = if agent == "codex" {
        "Codex"
    } else {
        "Claude Code"
    };
    let mut rows = vec![format!(
        "{label} · {}",
        if account.is_empty() {
            "Default"
        } else {
            account
        }
    )];
    if agent == "claude" {
        let data = read_private_json(&path);
        let Some(data) = data else {
            rows.push("Not yet reported — available after a Claude response".into());
            println!("{}", json!({"rows":rows}));
            return Ok(0);
        };
        for (key, label) in [("five_hour", "5 hour"), ("seven_day", "7 day")] {
            if let Some(w) = data.get(key) {
                if let Some(pct) = w
                    .get("used_percentage")
                    .filter(|v| number(v))
                    .and_then(Value::as_f64)
                {
                    rows.push(format!(
                        "{label}: {}% used · resets {}",
                        pct,
                        reset_time(&w["resets_at"])
                    ));
                }
            }
        }
        if rows.len() == 1 {
            rows.push("Quota not exposed by this Claude session/account".into());
        }
        if let Some(cost) = data
            .pointer("/session/total_cost_usd")
            .filter(|v| number(v))
            .and_then(Value::as_f64)
        {
            rows.push(format!(
                "Session estimate: ${:.2} (not subscription billing)",
                cost
            ));
        }
        rows.push(format!(
            "Cached report: {} UTC",
            data.get("observed_at")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .chars()
                .take(19)
                .collect::<String>()
        ));
    } else {
        match codex_usage(account) {
            Ok((data, summary)) => {
                let buckets = data
                    .get("rateLimitsByLimitId")
                    .filter(|v| !v.is_null())
                    .cloned()
                    .unwrap_or_else(
                        || json!({"codex":data.get("rateLimits").cloned().unwrap_or(Value::Null)}),
                    );
                if let Some(buckets) = buckets.as_object() {
                    for (ident, bucket) in buckets {
                        for key in ["primary", "secondary"] {
                            if let Some(window) = bucket.get(key) {
                                if let Some(pct) = window
                                    .get("usedPercent")
                                    .filter(|v| number(v))
                                    .and_then(Value::as_f64)
                                {
                                    let duration = window
                                        .get("windowDurationMins")
                                        .filter(|v| number(v))
                                        .and_then(Value::as_f64)
                                        .map(|m| format!("{}h", m / 60.0))
                                        .unwrap_or_else(|| key.into());
                                    rows.push(format!(
                                        "{ident} · {duration}: {pct}% used · resets {}",
                                        reset_time(&window["resetsAt"])
                                    ));
                                }
                            }
                        }
                    }
                }
                if rows.len() == 1 {
                    rows.push("Quota unavailable for this account/authentication mode".into());
                }
                if let Some(tokens) = summary
                    .pointer("/summary/lifetimeTokens")
                    .filter(|v| number(v))
                    .and_then(Value::as_u64)
                {
                    rows.push(format!("Reported lifetime tokens: {}", grouped(tokens)));
                }
            }
            Err(_) => rows.push("Usage unavailable — check login or try Refresh".into()),
        }
    }
    println!("{}", json!({"rows":rows}));
    Ok(0)
}

pub fn run(args: &[String]) -> Result<i32> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(error("Expected an AI command"));
    };
    if command == "ai-core" {
        let mut input = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
        let request: Value = serde_json::from_str(&input)?;
        let operation = request
            .get("operation")
            .and_then(Value::as_str)
            .ok_or("missing operation")?;
        let result = match operation {
            "full-access" => json!({"full_access":full_access()}),
            "environment" => environment_delta(
                request
                    .get("agent")
                    .and_then(Value::as_str)
                    .unwrap_or("codex"),
                request.get("account").and_then(Value::as_str),
            )?,
            "shared-history-home" => json!({"path":shared_history_home()?.display().to_string()}),
            _ => return Err(error("Unknown AI core operation")),
        };
        println!("{}", serde_json::to_string(&result)?);
        return Ok(0);
    }
    match command {
        "nacre-ai-context" => context(&args[1..]),
        "nacre-ai-memory" => memory(&args[1..]),
        "nacre-ai-usage" => usage(&args[1..]),
        _ => Err(error("Unknown AI command")),
    }
}
