//! Native Codex chat title, history and MCP naming helper.
use crate::{ai_core, ai_rpc, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{BufRead, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const HELP: &str = r#"usage: nacre-ai-chat [-h] [--thread-id THREAD_ID] [--project PROJECT]
                     [--path PATH] [--chat-directory] [--account ACCOUNT]
                     [--all-accounts] [--history-home HISTORY_HOME]
                     {title,list,resume,serve,configure} [name]

Read chat titles and name the current chat through Codex's local protocol.

positional arguments:
  {title,list,resume,serve,configure}
  name

options:
  -h, --help            show this help message and exit
  --thread-id THREAD_ID
  --project PROJECT
  --path PATH
  --chat-directory
  --account ACCOUNT
  --all-accounts
  --history-home HISTORY_HOME
"#;

#[derive(Parser)]
struct Args {
    command: String,
    name: Option<String>,
    #[arg(long)]
    thread_id: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(long)]
    chat_directory: bool,
    #[arg(long, default_value = "")]
    account: String,
    #[arg(long)]
    all_accounts: bool,
    #[arg(long)]
    history_home: Option<PathBuf>,
}
fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?))
}
fn timed_output(command: Command, timeout: Duration) -> Result<std::process::Output> {
    crate::process::capture(command, None, timeout)
}
fn expand_home(path: &Path, base: &Path) -> PathBuf {
    if path == Path::new("~") {
        base.to_path_buf()
    } else if path.starts_with("~/") {
        base.join(path.strip_prefix("~/").unwrap_or(path))
    } else {
        path.to_path_buf()
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn label(input: &str) -> String {
    use unicode_general_category::{get_general_category, GeneralCategory::*};
    input
        .chars()
        .filter(|c| {
            !matches!(
                get_general_category(*c),
                Control | Format | Surrogate | PrivateUse | Unassigned
            )
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(100)
        .collect()
}
fn account_env(account: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let delta = ai_core::environment_delta("codex", (!account.is_empty()).then_some(account))?;
    if let Some(map) = delta.get("set").and_then(Value::as_object) {
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                out.insert(k.clone(), s.to_owned());
            }
        }
    }
    if !out.contains_key("CODEX_HOME") {
        if let Ok(value) = env::var("CODEX_HOME") {
            out.insert("CODEX_HOME".into(), value);
        }
    }
    Ok(out)
}
fn codex() -> Result<PathBuf> {
    Ok(home()?.join(".local/bin/codex"))
}
type Rpc = ai_rpc::Peer;
fn rpc_new(envs: &BTreeMap<String, String>, history: Option<&Path>) -> Result<Rpc> {
    ai_core::prepare_provider_launch();
    let mut command = Command::new(codex()?);
    let inherited_history = env::var("NACRE_AI_HISTORY_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let history = history.or(inherited_history.as_deref());
    if let Some(path) = history {
        command.args([
            "-c",
            &format!(
                "sqlite_home={}",
                serde_json::to_string(&path.display().to_string())?
            ),
        ]);
    }
    command.args(["app-server", "--stdio"]);
    for (key, value) in envs {
        command.env(key, value);
    }
    Rpc::start(command, Duration::from_secs(22))
}
fn rpc_start(rpc: &mut Rpc) -> Result<()> {
    rpc.call_with_params(
        "initialize",
        json!({"clientInfo":{"name":"nacre_workflow","version":"1.0","title":"Nacre AI"}}),
    )?;
    rpc.notify("initialized", json!({}))
}
fn cache_title_at(base: &Path, id: &str, title: &str, cwd: Option<&str>) -> Result<()> {
    if !valid_id(id) {
        return Err("Invalid title cache ID".into());
    }
    let root = base.join(".local/state/nacre/ai/chat-titles");
    ai_core::reject_symlink_chain(&root)?;
    fs::create_dir_all(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let mut record = json!({"title":label(title),"version":1});
    if let Some(c) = cwd.filter(|c| !c.is_empty()) {
        record["cwd"] = json!(c)
    }
    crate::state::write(&root.join(format!("{id}.json")), &record)?;
    if let Some(c) = cwd {
        let dir = Path::new(c);
        let chats = base.join(".local/share/nacre/ai/chats");
        if dir.parent() == Some(chats.as_path())
            && dir.file_name().and_then(|x| x.to_str()).is_some_and(|s| {
                s.len() == 32
                    && s.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
        {
            let index = root.join("directories");
            ai_core::reject_symlink_chain(&index)?;
            fs::create_dir_all(&index)?;
            fs::set_permissions(&index, fs::Permissions::from_mode(0o700))?;
            crate::state::write(
                &index.join(format!(
                    "{}.json",
                    dir.file_name().unwrap().to_string_lossy()
                )),
                &json!({"threadId":id,"version":1}),
            )?
        }
    }
    Ok(())
}
fn cache_title(id: &str, title: &str, cwd: Option<&str>) -> Result<()> {
    cache_title_at(&home()?, id, title, cwd)
}
fn cache_named(rpc: &mut Rpc, id: &str, title: &str) -> Result<()> {
    let cwd = rpc
        .call_with_params("thread/read", json!({"threadId":id,"includeTurns":false}))
        .ok()
        .and_then(|v| {
            v.get("thread")
                .and_then(|t| t.get("cwd"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    cache_title(id, title, cwd.as_deref())
}
fn set_title(id: &str, title: &str, envs: &BTreeMap<String, String>) -> Result<()> {
    if !valid_id(id) {
        return Err("A valid chat ID is required".into());
    }
    let name = label(title);
    if name.is_empty() {
        return Err("A title is required".into());
    }
    let mut rpc = rpc_new(envs, None)?;
    rpc_start(&mut rpc)?;
    rpc.call_with_params("thread/name/set", json!({"threadId":id,"name":name}))?;
    cache_named(&mut rpc, id, &name)
}
fn running(project: &str, account: &str) -> Result<BTreeMap<String, String>> {
    let tag = if account.is_empty() {
        "default".into()
    } else {
        format!("profile-{account}")
    };
    let prefix = format!("ai-{project}-{tag}-");
    let mut command = Command::new("tmux");
    command.args([
        "list-panes",
        "-a",
        "-F",
        "#{session_name}\t#{pane_current_path}",
    ]);
    let output = match timed_output(command, Duration::from_secs(4)) {
        Ok(output) => output,
        Err(_) => return Ok(BTreeMap::new()),
    };
    let mut rows = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((name, path)) = line.split_once('\t') {
            let tail = name.strip_prefix(&prefix).unwrap_or("");
            let suffix = tail.starts_with("resume-")
                && tail.len() > 7
                && tail[7..]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                || tail.split_once('-').is_some_and(|(stamp, n)| {
                    stamp.len() == 15
                        && stamp.as_bytes()[8] == b'T'
                        && stamp.bytes().enumerate().all(|(i, b)| {
                            if i == 8 {
                                b == b'T'
                            } else {
                                b.is_ascii_digit()
                            }
                        })
                        && !n.is_empty()
                        && n.bytes().all(|b| b.is_ascii_digit())
                });
            if suffix {
                rows.insert(name.into(), path.into());
            }
        }
    }
    Ok(rows)
}
fn chat_records(threads: &[Value], sessions: &BTreeMap<String, String>) -> Vec<Value> {
    let mut out = Vec::new();
    let mut matched = BTreeSet::new();
    for t in threads {
        let cwd = t.get("cwd").and_then(Value::as_str);
        let session = sessions
            .iter()
            .find(|(n, p)| Some(p.as_str()) == cwd && !matched.contains(*n))
            .map(|(n, _)| n.clone());
        if let Some(s) = &session {
            matched.insert(s.clone());
        }
        out.push(json!({"id":t.get("id"),"title":label(t.get("name").and_then(Value::as_str).filter(|value|!value.is_empty()).or_else(||t.get("preview").and_then(Value::as_str).filter(|value|!value.is_empty())).unwrap_or("New task")),"state":if session.is_some(){"running"}else{"saved"},"session":session,"updated_at":t.get("updatedAt").unwrap_or(&json!(0)),"cwd":t.get("cwd")}));
    }
    for (s, p) in sessions {
        if !matched.contains(s) {
            out.push(json!({"id":null,"title":"New task","state":"running","session":s,"updated_at":0,"cwd":p}));
        }
    }
    out
}
fn history_paths(path: &Path, chat: bool) -> Result<Vec<String>> {
    if chat {
        if !path.is_dir() {
            return Ok(vec![]);
        }
        let mut result = vec![path.display().to_string()];
        for e in fs::read_dir(path)? {
            let e = e?;
            let meta = e.file_type()?;
            let n = e.file_name().to_string_lossy().into_owned();
            if meta.is_dir()
                && !meta.is_symlink()
                && n.len() == 32
                && n.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                result.push(e.path().display().to_string())
            }
        }
        return Ok(result);
    }
    let mut command = Command::new("git");
    command
        .args(["-C"])
        .arg(path)
        .args(["worktree", "list", "--porcelain"]);
    let output = timed_output(command, Duration::from_secs(5))?;
    if !output.status.success() {
        return Err("Could not list project worktrees".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.strip_prefix("worktree ").map(str::to_owned))
        .collect())
}
fn collect_pages(mut page: impl FnMut(Value) -> Result<Value>) -> Result<Vec<Value>> {
    let (mut cursor, mut rows) = (Value::Null, Vec::new());
    loop {
        let data = page(cursor.clone())?;
        rows.extend(
            data.get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        let next = data.get("nextCursor").cloned().unwrap_or(Value::Null);
        if next.is_null() || next == json!("") {
            return Ok(rows);
        }
        if next == cursor {
            return Err("Codex returned a repeated history cursor".into());
        }
        cursor = next
    }
}
fn list_threads(
    envs: &BTreeMap<String, String>,
    paths: &[String],
    history: Option<&Path>,
) -> Result<Vec<Value>> {
    let mut rpc = rpc_new(envs, history)?;
    rpc_start(&mut rpc)?;
    collect_pages(|cursor| {
        rpc.call_with_params("thread/list",json!({"cwd":paths,"limit":100,"cursor":cursor,"sortKey":"updated_at","modelProviders":[],"useStateDbOnly":true}))
    })
}
fn account_homes() -> Result<Vec<(String, PathBuf)>> {
    let h = home()?;
    let mut rows = vec![(String::new(), h.join(".codex"))];
    let root = h.join(".local/share/nacre/ai/accounts");
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if ft.is_dir()
                && !ft.is_symlink()
                && !name.is_empty()
                && name.len() <= 48
                && name.as_bytes()[0].is_ascii_alphanumeric()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                rows.push((name, entry.path()));
            }
        }
    }
    rows[1..].sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows)
}
type ThreadOrigin = (Value, String, PathBuf, bool);
fn merge_thread(
    records: &mut BTreeMap<String, ThreadOrigin>,
    thread: Value,
    account: String,
    path: PathBuf,
    is_shared: bool,
) {
    let id = thread
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let newer = records.get(&id).is_none_or(|(old, _, _, _)| {
        thread.get("updatedAt").and_then(Value::as_i64).unwrap_or(0)
            > old.get("updatedAt").and_then(Value::as_i64).unwrap_or(0)
    });
    if newer {
        records.insert(id, (thread, account, path, is_shared));
    }
}
fn shared_list(paths: &[String], project: &str) -> Result<Vec<Value>> {
    let shared = ai_core::shared_history_home()?;
    let base = home()?;
    let mut sources = vec![(String::new(), shared.clone())];
    for (account, profile) in account_homes()? {
        let mut candidates = vec![profile.clone()];
        let registry = profile.join("nacre-history-sources.json");
        if registry.is_file() {
            if let Ok(v) = crate::state::read_strict(&registry, json!([])) {
                candidates.extend(
                    v.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(|p| expand_home(Path::new(p), &base)),
                );
            }
        }
        for candidate in candidates {
            let candidate = fs::canonicalize(&candidate).unwrap_or(candidate);
            if candidate != shared
                && fs::read_dir(&candidate).ok().is_some_and(|entries| {
                    entries.flatten().any(|e| {
                        let n = e.file_name().to_string_lossy().into_owned();
                        n.starts_with("state_") && n.ends_with(".sqlite")
                    })
                })
                && !sources.iter().any(|(_, p)| *p == candidate)
            {
                sources.push((account.clone(), candidate));
            }
        }
    }
    let mut records = BTreeMap::<String, (Value, String, PathBuf, bool)>::new();
    let mut order = Vec::new();
    for (account, path) in sources {
        let mut e = account_env(&account)?;
        if !account.is_empty() {
            e.insert(
                "CODEX_HOME".into(),
                home()?
                    .join(".local/share/nacre/ai/accounts")
                    .join(&account)
                    .display()
                    .to_string(),
            );
        }
        for thread in list_threads(&e, paths, Some(&path))? {
            if let Some(id) = thread.get("id").and_then(Value::as_str) {
                if !records.contains_key(id) {
                    order.push(id.to_owned());
                }
            }
            merge_thread(
                &mut records,
                thread,
                account.clone(),
                path.clone(),
                path == shared,
            );
        }
    }
    let mut sessions = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for (account, _) in account_homes()? {
        for (name, path) in running(project, &account)? {
            owners.insert(name.clone(), account.clone());
            sessions.insert(name, path);
        }
    }
    let threads = order
        .iter()
        .filter_map(|id| records.get(id))
        .map(|(t, _, _, _)| t.clone())
        .collect::<Vec<_>>();
    let mut result = chat_records(&threads, &sessions);
    for row in &mut result {
        if let Some(id) = row.get("id").and_then(Value::as_str) {
            if let Some((_, account, path, is_shared)) = records.get(id) {
                row["account"] = json!(account);
                row["history_home"] = json!(path.display().to_string());
                row["shared_history"] = json!(is_shared);
            }
        }
        if let Some(session) = row.get("session").and_then(Value::as_str) {
            if let Some(account) = owners.get(session) {
                row["account"] = json!(account);
            }
        }
    }
    Ok(result)
}
fn tools() -> Value {
    let basic = |name: &str,
                 description: &str,
                 props: Value,
                 required: Vec<&str>,
                 annotations: Value| json!({"name":name,"description":description,"annotations":annotations,"inputSchema":{"type":"object","properties":props,"required":required,"additionalProperties":false}});
    json!([
basic("set_chat_title","Name this Codex chat with a concise 3–7 word title. Read CODEX_THREAD_ID from your shell for thread_id.",json!({"thread_id":{"type":"string","pattern":"^[a-zA-Z0-9-]{1,100}$"},"title":{"type":"string","minLength":1,"maxLength":100}}),vec!["thread_id","title"],json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})),
basic("resolve_context","Find relevant registered project paths and build hosts from conversation. This does not execute or select a project. Clarify only genuine ambiguity.",json!({"query":{"type":"string"}}),vec!["query"],json!({"readOnlyHint":true})),
basic("find_knowledge","Search shared private evidence for a subject before relying on it or adding memory. Read INDEX.md and relevant wiki pages first.",json!({"query":{"type":"string"}}),vec!["query"],json!({"readOnlyHint":true})),
basic("remember","Save meaningful durable personal knowledge, decisions, research or verified work at milestones. No raw transcripts, secrets, repetitive mentions or speculation as facts. Worlds are stable subjects, not necessarily projects. Topics connect subjects. Search first; distinguish reported/verified/unverified.",json!({"title":{"type":"string"},"source":{"type":"string"},"body":{"type":"string"},"kind":{"type":"string","enum":["inbox","personal","projects","infrastructure","devices","runbooks","decisions","checkpoints"]},"confidence":{"type":"string","enum":["reported","verified","unverified"]},"worlds":{"type":"array","items":{"type":"string"}},"topics":{"type":"array","items":{"type":"string"}}}),vec!["title","source","body","confidence"],json!({"destructiveHint":false}))])
}
fn knowledge(name: &str, args: &Value, helper: &Path) -> Result<String> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    if name != "remember" && query.trim().is_empty() {
        return Err("A nonempty query is required".into());
    }
    if name == "remember"
        && ["title", "source", "body", "confidence"].iter().any(|k| {
            args.get(*k)
                .and_then(Value::as_str)
                .is_none_or(|v| v.trim().is_empty())
        })
    {
        return Err("Title, source, body and confidence are required".into());
    }
    let mut cmd = Command::new(helper);
    let subcommand = match name {
        "resolve_context" => "nacre-ai-context",
        "find_knowledge" => "nacre-brain",
        "remember" => "nacre-ai-memory",
        _ => return Err("Unknown tool".into()),
    };
    cmd.arg(subcommand);
    if name == "resolve_context" {
        cmd.arg(query);
    } else if name == "find_knowledge" {
        cmd.args(["search", query]);
    } else {
        for (k, flag) in [
            ("title", "--title"),
            ("source", "--source"),
            ("confidence", "--confidence"),
        ] {
            if let Some(v) = args.get(k).and_then(Value::as_str) {
                cmd.args([flag, v]);
            }
        }
        cmd.arg("--kind")
            .arg(args.get("kind").and_then(Value::as_str).unwrap_or("inbox"));
        for (k, flag) in [("worlds", "--world"), ("topics", "--topic")] {
            for v in args
                .get(k)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                cmd.args([flag, v]);
            }
        }
    }
    let body = if name == "remember" {
        Some(
            args.get("body")
                .and_then(Value::as_str)
                .ok_or("Body is required")?
                .as_bytes(),
        )
    } else {
        None
    };
    let out = crate::process::capture(cmd, body, Duration::from_secs(25))?;
    let status = out.status;
    let stdout = out.stdout;
    let stderr = out.stderr;
    if !status.success() && !(name == "find_knowledge" && status.code() == Some(1)) {
        let message = String::from_utf8_lossy(&stderr);
        return Err(if message.trim().is_empty() {
            "Knowledge request failed".into()
        } else {
            message.trim().to_owned().into()
        });
    }
    let text = String::from_utf8_lossy(&stdout);
    Ok(if text.is_empty() {
        "No matching evidence".into()
    } else {
        text.chars().take(50000).collect()
    })
}
fn serve_io<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    envs: &BTreeMap<String, String>,
    helper: &Path,
) -> Result<i32> {
    let mut line = String::new();
    loop {
        line.clear();
        let count = input.by_ref().take(1_048_577).read_line(&mut line)?;
        if count == 0 {
            break;
        }
        if count > 1_048_576 {
            return Err("MCP request exceeds the workflow limit".into());
        }
        if serde_json::from_str::<Value>(&line)
            .ok()
            .is_some_and(|request| request.get("id").is_none())
        {
            continue;
        }
        let response = (|| -> Result<Value> {
            let req: Value = serde_json::from_str(&line)?;
            let id = req.get("id").cloned().unwrap_or(Value::Null);
            let method = req.get("method").and_then(Value::as_str).unwrap_or("");
            let p = req.get("params").cloned().unwrap_or(json!({}));
            let result = match method {
                "initialize" => {
                    json!({"protocolVersion":p.get("protocolVersion").unwrap_or(&json!("2024-11-05")),"capabilities":{"tools":{}},"serverInfo":{"name":"nacre-workflow","version":"1.0"}})
                }
                "ping" => json!({}),
                "tools/list" => json!({"tools":tools()}),
                "tools/call" => {
                    let name = p.get("name").and_then(Value::as_str).unwrap_or("");
                    if ![
                        "set_chat_title",
                        "resolve_context",
                        "find_knowledge",
                        "remember",
                    ]
                    .contains(&name)
                    {
                        return Ok(
                            json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unknown method"}}),
                        );
                    }
                    let a = p.get("arguments").unwrap_or(&Value::Null);
                    let result = if name == "set_chat_title" {
                        set_title(
                            a.get("thread_id").and_then(Value::as_str).unwrap_or(""),
                            a.get("title").and_then(Value::as_str).unwrap_or(""),
                            envs,
                        )
                        .map(|_| "Chat title saved".into())
                    } else {
                        knowledge(name, a, helper)
                    };
                    match result {
                        Ok(text) => json!({"content":[{"type":"text","text":text}]}),
                        Err(e) => {
                            json!({"isError":true,"content":[{"type":"text","text":e.to_string()}]})
                        }
                    }
                }
                _ => {
                    return Ok(
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unknown method"}}),
                    )
                }
            };
            Ok(json!({"jsonrpc":"2.0","id":id,"result":result}))
        })();
        let response = match response {
            Ok(v) => v,
            Err(_) => {
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid request"}})
            }
        };
        serde_json::to_writer(&mut output, &response)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(0)
}
fn serve(envs: &BTreeMap<String, String>) -> Result<i32> {
    serve_io(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        envs,
        &env::current_exe()?,
    )
}
fn configure_at(base: &Path, selected: &Path, shared: &Path) -> Result<i32> {
    ai_core::reject_symlink_chain(selected)?;
    fs::create_dir_all(selected)?;
    ai_core::reject_symlink_chain(selected)?;
    if !fs::metadata(selected)?.is_dir() {
        return Err("Codex account home is not a directory".into());
    }
    fs::set_permissions(selected, fs::Permissions::from_mode(0o700))?;
    let config = selected.join("config.toml");
    let lock = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(selected.join("nacre-config.lock"))?;
    let metadata = lock.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err("Linked or non-regular chat configuration lock".into());
    }
    lock.set_permissions(fs::Permissions::from_mode(0o600))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)?;
    let old = ai_core::read_codex_config(selected)?.unwrap_or_default();
    let parsed: toml::Value = toml::from_str(&old).map_err(|_| "Invalid Codex configuration")?;
    let is_account =
        selected.parent() == Some(base.join(".local/share/nacre/ai/accounts").as_path());
    let previous_store = parsed
        .get("sqlite_home")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    let boundary = old
        .lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .map(|line| line.len() + 1)
        .sum::<usize>()
        .min(old.len());
    let policy_keys = regex::Regex::new(r"^\s*(sandbox_mode|approval_policy)\s*=")?;
    let sqlite_key = regex::Regex::new(r"^\s*sqlite_home\s*=")?;
    let mut top = old[..boundary]
        .lines()
        .filter(|line| !policy_keys.is_match(line) && !(is_account && sqlite_key.is_match(line)))
        .collect::<Vec<_>>()
        .join("\n");
    if is_account {
        top = format!(
            "sqlite_home = {}\n{top}",
            serde_json::to_string(&shared.display().to_string())?
        );
    }
    top = format!("sandbox_mode = \"danger-full-access\"\napproval_policy = \"never\"\n{top}\n");
    let rest = &old[boundary..];
    let mut updated = format!("{top}{rest}");
    let parsed: toml::Value = toml::from_str(&updated)?;
    let command = base.join(".local/bin/nacre-ai-chat");
    if let Some(server) = parsed
        .get("mcp_servers")
        .and_then(|v| v.get("nacre_workflow"))
    {
        if server.get("command").and_then(toml::Value::as_str)
            != Some(command.to_string_lossy().as_ref())
            || server
                .get("args")
                .and_then(toml::Value::as_array)
                .is_none_or(|args| args != &[toml::Value::String("serve".into())])
        {
            return Err("Existing nacre_workflow config differs; preserving it".into());
        }
    } else {
        updated += &format!(
            "\n[mcp_servers.nacre_workflow]\ncommand = {}\nargs = [\"serve\"]\n",
            serde_json::to_string(&command.to_string_lossy().to_string())?
        );
    }
    let parsed: toml::Value = toml::from_str(&updated)?;
    if parsed
        .get("mcp_servers")
        .and_then(|v| v.get("nacre_workflow"))
        .and_then(|v| v.get("tools"))
        .and_then(|v| v.get("set_chat_title"))
        .is_none()
    {
        updated +=
            "\n[mcp_servers.nacre_workflow.tools.set_chat_title]\napproval_mode = \"approve\"\n";
    }
    let parsed: toml::Value = toml::from_str(&updated)?;
    let canonical = fs::canonicalize(selected)?;
    let server_env = parsed
        .get("mcp_servers")
        .and_then(|v| v.get("nacre_workflow"))
        .and_then(|v| v.get("env"));
    if server_env.is_some() {
        let value = server_env
            .and_then(|v| v.get("CODEX_HOME"))
            .and_then(toml::Value::as_str)
            .ok_or("Workflow tool points to another account home; preserving config")?;
        let value = expand_home(Path::new(value), base);
        if fs::canonicalize(&value).unwrap_or(value) != canonical {
            return Err("Workflow tool points to another account home; preserving config".into());
        }
    } else {
        updated += &format!(
            "\n[mcp_servers.nacre_workflow.env]\nCODEX_HOME = {}\n",
            serde_json::to_string(&canonical.display().to_string())?
        );
    }
    toml::from_str::<toml::Value>(&updated)
        .map_err(|_| "Unsupported workflow configuration; original preserved")?;
    if is_account && previous_store.as_deref() != Some(shared.to_string_lossy().as_ref()) {
        let backup_root = base.join(".local/state/nacre/ai/backups");
        ai_core::reject_symlink_chain(&backup_root)?;
        fs::create_dir_all(&backup_root)?;
        fs::set_permissions(&backup_root, fs::Permissions::from_mode(0o700))?;
        let timestamp = backup_root.join(
            chrono::Utc::now()
                .format("shared-chats-%Y%m%dT%H%M%S%fZ")
                .to_string(),
        );
        fs::create_dir_all(&timestamp)?;
        fs::set_permissions(&timestamp, fs::Permissions::from_mode(0o700))?;
        let backup = timestamp.join(selected.file_name().ok_or("Missing account name")?);
        if config.is_file() {
            fs::create_dir_all(&backup)?;
            fs::set_permissions(&backup, fs::Permissions::from_mode(0o700))?;
            fs::copy(&config, backup.join("config.toml"))?;
            fs::set_permissions(
                backup.join("config.toml"),
                fs::Permissions::from_mode(0o600),
            )?;
        }
        if let Some(previous) = previous_store {
            let previous = if previous == "~" {
                base.to_path_buf()
            } else if let Some(rest) = previous.strip_prefix("~/") {
                base.join(rest)
            } else {
                PathBuf::from(previous)
            };
            let previous = fs::canonicalize(&previous).unwrap_or(previous);
            let registry = selected.join("nacre-history-sources.json");
            let mut sources: Vec<String> = if registry.exists() {
                serde_json::from_value(crate::state::read_strict(&registry, json!([]))?)?
            } else {
                Vec::new()
            };
            if !sources.contains(&previous.display().to_string()) {
                sources.push(previous.display().to_string());
            }
            crate::state::write(&registry, &json!(sources))?;
        }
    }
    let mut file = tempfile::NamedTempFile::new_in(selected)?;
    file.write_all(updated.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(&config)?;
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600))?;
    Ok(0)
}
fn configure(envs: &BTreeMap<String, String>) -> Result<i32> {
    let base = home()?;
    let selected = envs
        .get("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| base.join(".codex"));
    let shared = ai_core::shared_history_home()?;
    configure_at(&base, &selected, &shared)
}
fn resume_argv(id: &str, history: Option<&Path>, base: &Path) -> Result<Vec<String>> {
    let mut argv = vec![
        "resume".into(),
        id.into(),
        "--sandbox".into(),
        "danger-full-access".into(),
        "--ask-for-approval".into(),
        "never".into(),
        "--add-dir".into(),
        base.join("Documents/Nacre-Brain").display().to_string(),
    ];
    if let Some(path) = history {
        argv.extend([
            "--no-daemon".into(),
            "-c".into(),
            format!(
                "sqlite_home={}",
                serde_json::to_string(&path.display().to_string())?
            ),
        ]);
    }
    Ok(argv)
}
fn run_inner(args: &[String]) -> Result<i32> {
    let mut a = Args::try_parse_from(
        std::iter::once("nacre-ai-chat".to_string()).chain(args.iter().cloned()),
    )?;
    if a.thread_id.is_none() {
        a.thread_id = env::var("CODEX_THREAD_ID").ok();
    }
    if !["title", "list", "resume", "serve", "configure"].contains(&a.command.as_str()) {
        return Err("Unknown chat command".into());
    }
    let envs = account_env(&a.account)?;
    match a.command.as_str() {
        "serve" => serve(&envs),
        "configure" => configure(&envs),
        "title" => {
            let id = a
                .thread_id
                .as_deref()
                .ok_or("Run in a Codex tool session or pass --thread-id, plus a short title")?;
            let name = label(a.name.as_deref().unwrap_or(""));
            if name.is_empty() {
                return Err(
                    "Run in a Codex tool session or pass --thread-id, plus a short title".into(),
                );
            }
            let mut rpc = rpc_new(&envs, None)?;
            rpc_start(&mut rpc)?;
            rpc.call_with_params("thread/name/set", json!({"threadId":id,"name":name}))?;
            cache_named(&mut rpc, id, &name)?;
            println!("Chat title saved");
            Ok(0)
        }
        "list" => {
            let project = a
                .project
                .as_deref()
                .ok_or("Project ID and path are required")?;
            let path = a
                .path
                .as_deref()
                .ok_or("Project ID and path are required")?;
            let paths = history_paths(path, a.chat_directory)?;
            let result = if paths.is_empty() {
                vec![]
            } else if a.all_accounts {
                shared_list(&paths, project)?
            } else {
                let rows = list_threads(&envs, &paths, a.history_home.as_deref())?;
                let sessions = running(project, &a.account)?;
                chat_records(&rows, &sessions)
            };
            println!("{}", serde_json::to_string(&result)?);
            Ok(0)
        }
        "resume" => {
            let id = a
                .thread_id
                .as_deref()
                .filter(|s| valid_id(s))
                .ok_or("A valid chat ID is required")?;
            let project = a
                .project
                .as_deref()
                .filter(|s| {
                    s.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                })
                .ok_or("A project ID is required")?;
            let tag = if a.account.is_empty() {
                "default".into()
            } else {
                format!("profile-{}", a.account)
            };
            let session = format!("ai-{project}-{tag}-resume-{id}");
            let base = home()?;
            let history = a.history_home;
            let mut argv = vec![codex()?.display().to_string()];
            argv.extend(resume_argv(id, history.as_deref(), &base)?);
            let mut assignments = Vec::new();
            for (k, v) in &envs {
                assignments.push(format!("{k}={v}"));
            }
            if !envs.contains_key("CODEX_HOME") {
                if let Some(value) = env::var_os("CODEX_HOME") {
                    assignments.push(format!("CODEX_HOME={}", value.to_string_lossy()));
                }
            }
            if let Some(history) = history {
                assignments.push(format!("NACRE_AI_HISTORY_HOME={}", history.display()));
            } else if let Ok(history) = env::var("NACRE_AI_HISTORY_HOME") {
                if !history.is_empty() {
                    assignments.push(format!("NACRE_AI_HISTORY_HOME={history}"));
                }
            }
            let shell = argv
                .iter()
                .map(|v| format!("'{}'", v.replace('\'', "'\\''")))
                .collect::<Vec<_>>()
                .join(" ");
            let command = if assignments.is_empty() {
                shell
            } else {
                format!(
                    "env {} {shell}",
                    assignments
                        .iter()
                        .map(|v| format!("'{}'", v.replace('\'', "'\\''")))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            use std::os::unix::process::CommandExt;
            let error = Command::new("tmux")
                .args(["new-session", "-A", "-s", &session, &command])
                .exec();
            Err(error.into())
        }
        _ => unreachable!(),
    }
}
pub fn run(args: &[String]) -> Result<i32> {
    // Retain the historical module-operation boundary used by frozen replays.
    // This operation publishes derived metadata and never launches a provider.
    if args.first().is_some_and(|action| action == "cache_title") {
        let request: Value = serde_json::from_str(args.get(1).ok_or("Missing cache request")?)?;
        let argument = |index: usize, key: &str| {
            request["args"]
                .get(index)
                .or_else(|| request["kwargs"].get(key))
        };
        let id = argument(0, "thread_id")
            .and_then(Value::as_str)
            .ok_or("Missing chat ID")?;
        let title = argument(1, "title")
            .and_then(Value::as_str)
            .ok_or("Missing chat title")?;
        let cwd = argument(2, "cwd").and_then(Value::as_str);
        cache_title(id, title, cwd)?;
        println!("null");
        return Ok(0);
    }
    ai_core::prepare_provider_launch();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print!("{HELP}");
        return Ok(0);
    }
    run_inner(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_remove_controls_and_bound_length() {
        assert_eq!(label("hello\r\nworld\u{1b}"), "helloworld");
        assert_eq!(label(&"x".repeat(200)).len(), 100);
    }
    #[test]
    fn title_cache_rejects_paths_and_writes_private_files() {
        let temp = tempfile::tempdir().unwrap();
        assert!(cache_title_at(temp.path(), "../escape", "bad", None).is_err());
        cache_title_at(temp.path(), "thread-one", "A title\u{1b}\n", None).unwrap();
        let p = temp
            .path()
            .join(".local/state/nacre/ai/chat-titles/thread-one.json");
        let v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
        assert_eq!(v["title"], "A title");
        assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
    }
    #[test]
    fn directory_cache_keeps_thread_identity_and_version() {
        let t = tempfile::tempdir().unwrap();
        let dir = t
            .path()
            .join(".local/share/nacre/ai/chats")
            .join("a".repeat(32));
        cache_title_at(t.path(), "thread-1", "Name", Some(dir.to_str().unwrap())).unwrap();
        let index = t
            .path()
            .join(".local/state/nacre/ai/chat-titles/directories")
            .join(format!("{}.json", "a".repeat(32)));
        let value: Value = serde_json::from_slice(&fs::read(index).unwrap()).unwrap();
        assert_eq!(value, json!({"threadId":"thread-1","version":1}));
        let title: Value = serde_json::from_slice(
            &fs::read(
                t.path()
                    .join(".local/state/nacre/ai/chat-titles/thread-1.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(title["version"], 1);
    }
    #[test]
    fn records_keep_identity_and_match_running_worktree() {
        let threads = vec![
            json!({"id":"one","name":"Fix stream lag","cwd":"/repo/one"}),
            json!({"id":"two","name":"Fix stream lag","cwd":"/repo/two"}),
            json!({"id":"three","preview":"Check settings","cwd":"/repo/three"}),
        ];
        let sessions = BTreeMap::from([(
            "ai-project-default-20260930T140000-123".into(),
            "/repo/one".into(),
        )]);
        let rows = chat_records(&threads, &sessions);
        assert_eq!(rows[0]["state"], "running");
        assert_eq!(rows[1]["state"], "saved");
        assert_eq!(rows[2]["title"], "Check settings");
        assert_eq!(
            rows.iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["one", "two", "three"]
        );
    }
    #[test]
    fn chat_directory_discovery_ignores_links_and_nonchat_dirs() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("a".repeat(32));
        fs::create_dir(&d).unwrap();
        fs::create_dir(t.path().join("unrelated")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&d, t.path().join("b".repeat(32))).unwrap();
        let p = history_paths(t.path(), true).unwrap();
        assert_eq!(
            p,
            vec![t.path().display().to_string(), d.display().to_string()]
        );
    }
    #[test]
    fn tools_only_expose_title_and_knowledge() {
        let t = tools();
        let names = t
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "set_chat_title",
                "resolve_context",
                "find_knowledge",
                "remember"
            ]
        );
        assert!(names.iter().all(|n| *n != "run_command"));
    }

    #[test]
    fn configure_is_idempotent_preserves_other_and_nested_values_and_policy() {
        let t = tempfile::tempdir().unwrap();
        let profile = t.path().join("account");
        fs::create_dir_all(&profile).unwrap();
        let file = profile.join("config.toml");
        let original = r#"model = "keep"
approval_policy = "on-request"
sandbox_mode = "workspace-write"
[profiles.special]
approval_policy = "on-request"
"#;
        fs::write(&file, original).unwrap();
        configure_at(t.path(), &profile, &t.path().join(".codex")).unwrap();
        let once = fs::read_to_string(&file).unwrap();
        configure_at(t.path(), &profile, &t.path().join(".codex")).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), once);
        let data: toml::Value = toml::from_str(&once).unwrap();
        assert_eq!(data["sandbox_mode"].as_str(), Some("danger-full-access"));
        assert_eq!(data["approval_policy"].as_str(), Some("never"));
        assert_eq!(data["model"].as_str(), Some("keep"));
        assert_eq!(
            data["profiles"]["special"]["approval_policy"].as_str(),
            Some("on-request")
        );
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn configured_home_alias_is_preserved_when_it_resolves_to_selected_profile() {
        let t = tempfile::tempdir().unwrap();
        let profile = t.path().join("profile");
        fs::create_dir_all(&profile).unwrap();
        let alias = t.path().join("old-profile-alias");
        std::os::unix::fs::symlink(&profile, &alias).unwrap();
        configure_at(t.path(), &profile, &t.path().join(".codex")).unwrap();
        let config = profile.join("config.toml");
        let original = fs::read_to_string(&config).unwrap();
        let updated =
            original.replace(&profile.display().to_string(), &alias.display().to_string());
        fs::write(&config, updated.clone()).unwrap();
        configure_at(t.path(), &profile, &t.path().join(".codex")).unwrap();
        assert_eq!(fs::read_to_string(config).unwrap(), updated);
    }
    #[test]
    fn linked_config_and_incomplete_server_environment_are_preserved_or_rejected() {
        let t = tempfile::tempdir().unwrap();
        let profile = t.path().join("profile");
        fs::create_dir_all(&profile).unwrap();
        let external = t.path().join("external.toml");
        fs::write(&external, "model = \"keep\"\n").unwrap();
        std::os::unix::fs::symlink(&external, profile.join("config.toml")).unwrap();
        assert!(configure_at(t.path(), &profile, &t.path().join(".codex")).is_err());
        assert_eq!(fs::read_to_string(&external).unwrap(), "model = \"keep\"\n");
        fs::remove_file(profile.join("config.toml")).unwrap();
        let config = profile.join("config.toml");
        let original = "[mcp_servers.nacre_workflow.env]\nOTHER = \"value\"\n";
        fs::write(&config, original).unwrap();
        assert!(configure_at(t.path(), &profile, &t.path().join(".codex")).is_err());
        assert_eq!(fs::read_to_string(config).unwrap(), original);
    }
    #[test]
    fn invalid_config_is_preserved() {
        let t = tempfile::tempdir().unwrap();
        let profile = t.path().join("codex");
        fs::create_dir_all(&profile).unwrap();
        let file = profile.join("config.toml");
        fs::write(&file, "[broken").unwrap();
        assert!(configure_at(t.path(), &profile, &t.path().join(".codex")).is_err());
        assert_eq!(fs::read_to_string(file).unwrap(), "[broken");
    }
    #[test]
    fn thread_pagination_reads_many_pages_and_rejects_repeated_cursor() {
        let rows=collect_pages(|cursor|{let n=cursor.as_str().unwrap_or("0").parse::<usize>().unwrap();Ok(json!({"data":[{"id":n}],"nextCursor":if n<6{json!((n+1).to_string())}else{Value::Null}}))}).unwrap();
        assert_eq!(rows.len(), 7);
        let repeated = collect_pages(|_| Ok(json!({"data":[],"nextCursor":"same"})));
        assert!(repeated.unwrap_err().to_string().contains("repeated"));
    }
    #[test]
    fn fake_rpc_preserves_chat_params_and_handshake_cleanup() {
        let script = r#"import json,sys
for line in sys.stdin:
 q=json.loads(line)
 if q.get('method')=='initialized':
  assert 'params' not in q
  continue
 assert q['method']=='initialize' or q['params']=={'threadId':'fixture-id','name':'Fixture title'}
 print(json.dumps({'id':q['id'],'result':{'ok':True}}),flush=True)
"#;
        let mut cmd = Command::new("python3");
        cmd.args(["-c", script])
            .env_clear()
            .env("PATH", "/usr/bin:/bin");
        let mut peer = Rpc::start(cmd, Duration::from_secs(2)).unwrap();
        assert_eq!(
            peer.call_with_params(
                "initialize",
                json!({"clientInfo":{"name":"nacre_workflow","version":"1.0","title":"Nacre AI"}})
            )
            .unwrap()["ok"],
            true
        );
        peer.notify("initialized", json!({})).unwrap();
        assert_eq!(
            peer.call_with_params(
                "thread/name/set",
                json!({"threadId":"fixture-id","name":"Fixture title"})
            )
            .unwrap()["ok"],
            true
        );
    }
    #[test]
    fn mcp_ndjson_limits_tools_and_returns_errors() {
        let t = tempfile::tempdir().unwrap();
        let helper = t.path().join("fake-helper");
        fs::write(&helper, "#!/bin/sh\nprintf 'fixture knowledge result\n'\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let requests = [
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"find_knowledge","arguments":{"query":" "}}}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"run_command"}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"find_knowledge","arguments":{"query":"fixture"}}}),
            json!({"jsonrpc":"2.0","method":"ping"}),
        ];
        let input = requests
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let mut out = Vec::new();
        serve_io(
            std::io::Cursor::new(input),
            &mut out,
            &BTreeMap::new(),
            &helper,
        )
        .unwrap();
        let responses = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(responses.len(), 4);
        assert_eq!(
            responses[0]["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "set_chat_title",
                "resolve_context",
                "find_knowledge",
                "remember"
            ]
        );
        assert_eq!(responses[1]["result"]["isError"], true);
        assert_eq!(responses[2]["error"]["code"], -32601);
        assert_eq!(
            responses[3]["result"]["content"][0]["text"],
            "fixture knowledge result\n"
        );
    }

    #[test]
    fn account_history_transition_keeps_backup_and_registers_old_store() {
        let t = tempfile::tempdir().unwrap();
        let account = t.path().join(".local/share/nacre/ai/accounts/work");
        fs::create_dir_all(&account).unwrap();
        let previous = t.path().join("custom-history");
        fs::create_dir_all(&previous).unwrap();
        fs::write(previous.join("state_5.sqlite"), b"fixture marker").unwrap();
        let config = account.join("config.toml");
        let old = format!(
            "sqlite_home = {}\nmodel = \"keep\"\n",
            serde_json::to_string(&previous.display().to_string()).unwrap()
        );
        fs::write(&config, &old).unwrap();
        configure_at(t.path(), &account, &t.path().join(".codex")).unwrap();
        let parsed: toml::Value = toml::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(
            parsed["sqlite_home"].as_str(),
            Some(t.path().join(".codex").to_string_lossy().as_ref())
        );
        let registry: Value =
            serde_json::from_slice(&fs::read(account.join("nacre-history-sources.json")).unwrap())
                .unwrap();
        assert!(registry
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some(previous.to_string_lossy().as_ref())));
        let backups = t.path().join(".local/state/nacre/ai/backups");
        let backup = fs::read_dir(backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("work/config.toml");
        assert_eq!(fs::read_to_string(&backup).unwrap(), old);
        assert_eq!(
            fs::metadata(backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn duplicate_account_histories_keep_newest_thread_and_source() {
        let mut rows = BTreeMap::new();
        merge_thread(
            &mut rows,
            json!({"id":"same","name":"Old","updatedAt":1}),
            "first".into(),
            PathBuf::from("/old"),
            false,
        );
        merge_thread(
            &mut rows,
            json!({"id":"same","name":"New","updatedAt":2}),
            "second".into(),
            PathBuf::from("/new"),
            false,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows["same"].0["name"], "New");
        assert_eq!(rows["same"].1, "second");
        assert_eq!(rows["same"].2, PathBuf::from("/new"));
    }
    #[test]
    fn resume_flags_keep_full_access_and_selected_history() {
        let argv = resume_argv(
            "thread-1",
            Some(Path::new("/old-store")),
            Path::new("/tmp/home"),
        )
        .unwrap();
        assert_eq!(
            &argv[..8],
            [
                "resume",
                "thread-1",
                "--sandbox",
                "danger-full-access",
                "--ask-for-approval",
                "never",
                "--add-dir",
                "/tmp/home/Documents/Nacre-Brain"
            ]
        );
        assert_eq!(
            &argv[8..],
            ["--no-daemon", "-c", "sqlite_home=\"/old-store\""]
        );
    }
}

#[cfg(test)]
mod request_limits {
    #[test]
    fn oversized_mcp_request_is_rejected_before_parsing_or_dispatch() {
        let input = std::io::Cursor::new(vec![b'x'; 1_048_577]);
        let mut output = Vec::new();
        let error = super::serve_io(
            input,
            &mut output,
            &Default::default(),
            std::path::Path::new("/unused"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("exceeds"));
        assert!(output.is_empty());
    }
}
