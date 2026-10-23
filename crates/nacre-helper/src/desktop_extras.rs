//! Native backend for the small, on-demand desktop panels.
use crate::{compositor::backend::Backend, paths::Paths, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn state(paths: &Paths) -> PathBuf {
    paths.state.join("shell/extras")
}
fn run_command(args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
    let mut command = Command::new(args.first().ok_or("Missing command")?);
    command
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn()?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or("Missing child stdin")?
            .write_all(input)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!("{} failed", args[0]).into());
    }
    Ok(output.stdout)
}
fn save(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    crate::files::atomic(path, &bytes)
}
fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn pins(path: &Path) -> Result<Vec<Value>> {
    let value = if path.is_file() {
        crate::state::read_strict(path, json!([]))?
    } else {
        json!([])
    };
    Ok(value.as_array().cloned().unwrap_or_default())
}
fn pins_for(paths: &Paths, rows: &[String]) -> Result<Vec<Value>> {
    let path = state(paths).join("pins.json");
    let original = pins(&path)?;
    let mut normalized = Vec::new();
    for pin in &original {
        if pin.is_object() {
            normalized.push(pin.clone());
        } else if let Some(id) = pin.as_str() {
            if let Some(row) = rows.iter().find(|row| row.starts_with(&format!("{id}\t"))) {
                let data = run_command(&["cliphist", "decode"], Some(row.as_bytes()))?;
                normalized.push(fingerprint(row, &data));
            }
        }
    }
    if normalized != original {
        save(&path, &json!(normalized))?;
    }
    Ok(normalized)
}
fn mime(data: &[u8]) -> &'static str {
    if data.starts_with(b"\x89PNG") {
        "image/png"
    } else if data.starts_with(&[0xff, 0xd8]) {
        "image/jpeg"
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        "image/gif"
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else {
        "text/plain;charset=utf-8"
    }
}
fn fingerprint(row: &str, data: &[u8]) -> Value {
    let label = row.split_once('\t').map(|v| v.1).unwrap_or("");
    json!({"label":digest(label.as_bytes()),"content":digest(data)})
}
fn ensure_thumbnail(directory: &Path, data: &[u8]) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let path = directory.join(format!("{}.img", digest(data)));
    if !path.exists() {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => file.write_all(data)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}
fn remove_thumbnail(directory: &Path, data: &[u8]) -> Result<()> {
    let path = directory.join(format!("{}.img", digest(data)));
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn clip_rows() -> Result<Vec<String>> {
    Ok(
        String::from_utf8_lossy(&run_command(&["cliphist", "list"], None)?)
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}
const CLIP_HISTORY_AGE: u64 = 7 * 24 * 60 * 60;
fn clip_timestamp_path(paths: &Paths) -> PathBuf {
    state(paths).join("clipboard-times.json")
}
fn cliphist_database(paths: &Paths) -> PathBuf {
    if let Some(value) = env::var_os("CLIPHIST_DB_PATH") {
        return PathBuf::from(value);
    }
    let cache = env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| paths.home.join(".cache"));
    cache.join("cliphist/db")
}
fn read_clip_timestamps(path: &Path) -> Result<BTreeMap<String, u64>> {
    if path.is_file() {
        Ok(serde_json::from_value(crate::state::read_strict(
            path,
            json!({}),
        )?)?)
    } else {
        Ok(BTreeMap::new())
    }
}
fn should_expire_clip(created: u64, now: u64, pinned: bool) -> bool {
    !pinned && now.saturating_sub(created) >= CLIP_HISTORY_AGE
}
fn record_clip_timestamp(paths: &Paths, row: &str) -> Result<()> {
    let (id, _) = row.split_once('\t').ok_or("Malformed clipboard row")?;
    if id.is_empty() || !id.bytes().all(|v| v.is_ascii_digit()) {
        return Err("Malformed clipboard ID".into());
    }
    let timestamps = clip_timestamp_path(paths);
    let _guard = crate::files::lock(&timestamps.with_extension("json.lock"))?;
    let mut values = read_clip_timestamps(&timestamps)?;
    values.insert(
        id.to_owned(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    );
    fs::create_dir_all(
        timestamps
            .parent()
            .ok_or("Missing clipboard state folder")?,
    )?;
    save(&timestamps, &json!(values))?;
    Ok(())
}
fn store_clip(paths: &Paths, content: &[u8]) -> Result<()> {
    let output = crate::process::run_bytes(
        "cliphist",
        &["store".into()],
        Some(content),
        Duration::from_secs(15),
    )?;
    if !output.status.success() {
        return Err("cliphist store failed".into());
    }
    if env::var("CLIPBOARD_STATE").is_ok_and(|value| value == "sensitive") {
        return Ok(());
    }
    let rows = clip_rows()?;
    if let Some(row) = rows.first() {
        let decoded = run_command(&["cliphist", "decode"], Some(row.as_bytes()))?;
        if decoded == content {
            record_clip_timestamp(paths, row)?;
        }
    }
    // Expire old non-pinned entries as clipboard activity continues, even if
    // the panel is never reopened. A later `clips` request also prunes.
    prune_clip_history(paths, &rows)
}
fn prune_clip_history(paths: &Paths, rows: &[String]) -> Result<()> {
    if !cliphist_database(paths).is_file() {
        return Ok(());
    }
    let pin_values = pins_for(paths, rows)?;
    let timestamps = clip_timestamp_path(paths);
    let _guard = crate::files::lock(&timestamps.with_extension("json.lock"))?;
    let mut known = read_clip_timestamps(&timestamps)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let mut live = HashSet::new();
    let mut remove = Vec::new();
    for row in rows {
        let Some((id, _)) = row.split_once('\t') else {
            continue;
        };
        if id.is_empty() || !id.bytes().all(|v| v.is_ascii_digit()) {
            continue;
        }
        live.insert(id.to_owned());
        let created = *known.entry(id.to_owned()).or_insert(now);
        if !should_expire_clip(created, now, false) {
            continue;
        }
        let decoded = match run_command(&["cliphist", "decode"], Some(row.as_bytes())) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let pin = fingerprint(row, &decoded);
        if pin_values.contains(&pin) {
            continue;
        }
        if run_command(&["cliphist", "delete"], Some(row.as_bytes())).is_ok() {
            remove_thumbnail(&state(paths).join("images"), &decoded)?;
            remove.push(id.to_owned());
        }
    }
    known.retain(|id, _| live.contains(id) && !remove.contains(id));
    fs::create_dir_all(
        timestamps
            .parent()
            .ok_or("Missing clipboard state folder")?,
    )?;
    save(&timestamps, &json!(known))?;
    Ok(())
}
fn is_image_label(label: &str) -> bool {
    label.contains("binary data")
        && ["png", "jpg", "jpeg", "gif", "webp"]
            .iter()
            .any(|x| label.to_lowercase().contains(x))
}
/// One clipboard row for the daemon topic: no content, only the shown label.
pub(crate) struct ClipRow {
    pub id: String,
    pub label: String,
    pub image: bool,
    pub created: u64,
}
/// Newest-first rows after the usual seven-day pruning (pins preserved).
pub(crate) fn clip_index(paths: &Paths, limit: usize) -> Result<Vec<ClipRow>> {
    prune_clip_history(paths, &clip_rows()?)?;
    let known = read_clip_timestamps(&clip_timestamp_path(paths))?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(clip_rows()?
        .iter()
        .filter_map(|row| {
            let (id, label) = row.split_once('\t')?;
            (!id.is_empty() && id.bytes().all(|c| c.is_ascii_digit())).then(|| ClipRow {
                id: id.to_owned(),
                label: clean(label).chars().take(200).collect(),
                image: is_image_label(label),
                created: known.get(id).copied().unwrap_or(now),
            })
        })
        .take(limit)
        .collect())
}
pub(crate) fn clip_copy(paths: &Paths, id: &str) -> Result<()> {
    clip_action(paths, "copy", id)
}
/// Delete every clip that is not pinned; returns whether anything was removed.
pub(crate) fn clip_clear(paths: &Paths) -> Result<bool> {
    let rows = clip_rows()?;
    let pinned = pins_for(paths, &rows)?;
    let mut removed = false;
    for row in &rows {
        let decoded = run_command(&["cliphist", "decode"], Some(row.as_bytes()))?;
        if pinned.contains(&fingerprint(row, &decoded)) {
            continue;
        }
        run_command(&["cliphist", "delete"], Some(row.as_bytes()))?;
        remove_thumbnail(&state(paths).join("images"), &decoded)?;
        removed = true;
    }
    prune_clip_history(paths, &clip_rows()?)?;
    Ok(removed)
}
/// N0051: the Privacy page's "clear history": every clip including pinned ones (`cliphist wipe`),
/// then the private files that only described them (thumbnails, pins, the age index). Returns
/// whether there was anything to clear.
pub(crate) fn clip_wipe(paths: &Paths) -> Result<bool> {
    let had = !clip_rows()?.is_empty();
    run_command(&["cliphist", "wipe"], None)?;
    let gone = |result: std::io::Result<()>| match result {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    };
    gone(fs::remove_dir_all(state(paths).join("images")))?;
    gone(fs::remove_file(state(paths).join("pins.json")))?;
    gone(fs::remove_file(clip_timestamp_path(paths)))?;
    Ok(had)
}
fn clips(paths: &Paths) -> Result<Value> {
    let rows = clip_rows()?;
    prune_clip_history(paths, &rows)?;
    let directory = state(paths);
    let pin_path = directory.join("pins.json");
    let mut stored = pins(&pin_path)?;
    let mut pin_set = Vec::new();
    for pin in &stored {
        if let Some(object) = pin.as_object() {
            let pair = (
                object
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                object
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            );
            if !pin_set.contains(&pair) {
                pin_set.push(pair);
            }
        } else if let Some(id) = pin.as_str() {
            if let Some(row) = rows.iter().find(|r| r.starts_with(&format!("{id}\t"))) {
                let data = run_command(&["cliphist", "decode"], Some(row.as_bytes()))?;
                let fp = fingerprint(row, &data);
                let pair = (
                    fp["label"].as_str().unwrap_or("").into(),
                    fp["content"].as_str().unwrap_or("").into(),
                );
                if !pin_set.contains(&pair) {
                    pin_set.push(pair);
                }
            }
        }
    }
    let mut recent: Vec<&String> = rows.iter().take(160).collect();
    let recent_ids: HashSet<String> = recent
        .iter()
        .filter_map(|r| r.split_once('\t').map(|x| x.0.to_owned()))
        .collect();
    for row in &rows {
        let label = row.split_once('\t').map(|x| x.1).unwrap_or("");
        if pin_set
            .iter()
            .any(|(pin_label, _)| *pin_label == digest(label.as_bytes()))
        {
            if let Some((id, _)) = row.split_once('\t') {
                if !recent_ids.contains(id) {
                    recent.push(row);
                }
            }
        }
    }
    recent.sort_by_key(|row| {
        !pin_set.iter().any(|(label, _)| {
            *label == digest(row.split_once('\t').map(|x| x.1).unwrap_or("").as_bytes())
        })
    });
    let mut result = Vec::new();
    let mut images = 0;
    for row in recent {
        let Some((id, label)) = row.split_once('\t') else {
            continue;
        };
        if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let image = is_image_label(label);
        let mut thumbnail = String::new();
        let mut data = None;
        let label_hash = digest(label.as_bytes());
        let pinned = if pin_set.iter().any(|(l, _)| l == &label_hash) {
            let decoded = run_command(&["cliphist", "decode"], Some(row.as_bytes()))?;
            let fp = fingerprint(row, &decoded);
            data = Some(decoded);
            pin_set.contains(&(
                label_hash.clone(),
                fp["content"].as_str().unwrap_or("").into(),
            ))
        } else {
            false
        };
        if image && images < 16 {
            images += 1;
            let decoded = match data {
                Some(v) => v,
                None => run_command(&["cliphist", "decode"], Some(row.as_bytes()))?,
            };
            if decoded.len() < 16 * 1024 * 1024 && mime(&decoded).starts_with("image/") {
                thumbnail = ensure_thumbnail(&directory.join("images"), &decoded)?
                    .to_string_lossy()
                    .into_owned();
            }
        }
        result.push(json!({"id":id,"title":clean(label).chars().take(200).collect::<String>(),"image":image,"thumbnail":thumbnail,"pinned":pinned}));
    }
    let live: HashSet<String> = result
        .iter()
        .filter_map(|r| r["thumbnail"].as_str())
        .map(|p| {
            Path::new(p)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let folder = directory.join("images");
    if let Ok(entries) = fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "img")
                && !live.contains(
                    &path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                )
            {
                let _ = fs::remove_file(path);
            }
        }
    }
    // Keep migrated pins in the stronger content-fingerprint format.
    let normalized: Vec<Value> = pin_set
        .into_iter()
        .map(|(label, content)| json!({"label":label,"content":content}))
        .collect();
    if normalized != stored {
        stored = normalized;
        fs::create_dir_all(&directory)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        save(&pin_path, &json!(stored))?;
    }
    Ok(Value::Array(result))
}

fn clip_action(paths: &Paths, action: &str, id: &str) -> Result<()> {
    if id.is_empty() || !id.bytes().all(|v| v.is_ascii_digit()) {
        return Err("Invalid clipboard entry".into());
    }
    let row = clip_rows()?
        .into_iter()
        .find(|r| r.starts_with(&format!("{id}\t")))
        .ok_or("Clipboard entry no longer exists")?;
    match action {
        "copy" => {
            let data = run_command(&["cliphist", "decode"], Some(row.as_bytes()))?;
            let mut command = Command::new("wl-copy");
            command
                .args(["--type", mime(&data)])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut child = command.spawn()?;
            child
                .stdin
                .take()
                .ok_or("Missing wl-copy stdin")?
                .write_all(&data)?;
            if !child.wait()?.success() {
                return Err("wl-copy failed".into());
            }
        }
        "pin" => {
            let data = run_command(&["cliphist", "decode"], Some(row.as_bytes()))?;
            let fp = fingerprint(&row, &data);
            let path = state(paths).join("pins.json");
            let mut values = pins(&path)?;
            if values.contains(&fp) {
                values.retain(|v| v != &fp);
            } else {
                values.insert(0, fp);
            }
            fs::create_dir_all(path.parent().unwrap())?;
            fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700))?;
            save(&path, &json!(values))?;
        }
        "delete" => {
            let data =
                run_command(&["cliphist", "decode"], Some(row.as_bytes())).unwrap_or_default();
            run_command(&["cliphist", "delete"], Some(row.as_bytes()))?;
            if !data.is_empty() {
                remove_thumbnail(&state(paths).join("images"), &data)?;
            }
        }
        _ => return Err("Unknown clipboard action".into()),
    }
    Ok(())
}
fn vault() -> Result<PathBuf> {
    let path = env::var_os("NACRE_BRAIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("Documents/Nacre-Brain")
        });
    Ok(fs::canonicalize(path)?)
}
fn brain_search(query: &str) -> Result<Value> {
    if query.chars().count() > 160 {
        return Ok(json!([]));
    }
    let words: Vec<String> = query
        .split_whitespace()
        .take(8)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return Ok(json!([]));
    }
    let root = vault()?;
    brain_search_at(&words, &root)
}
fn brain_search_at(words: &[String], root: &Path) -> Result<Value> {
    let start = Instant::now();
    let mut results = Vec::new();
    for (count, entry) in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .enumerate()
    {
        if count >= 10000 || start.elapsed() > Duration::from_millis(1500) {
            break;
        }
        let path = entry.path();
        if entry.file_type().is_symlink()
            || !path.is_file()
            || path.extension().is_none_or(|v| v != "md")
        {
            continue;
        }
        let relative = path.strip_prefix(root)?;
        if relative
            .components()
            .any(|p| p.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let meta = match fs::metadata(path) {
            Ok(v) if v.len() <= 512 * 1024 => v,
            _ => continue,
        };
        let text = match fs::read_to_string(path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let folded = text.to_lowercase();
        if !words.iter().all(|w| folded.contains(w)) {
            continue;
        }
        let title = text
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or_else(|| path.file_stem().and_then(|v| v.to_str()).unwrap_or(""));
        let excerpt = text
            .lines()
            .find(|line| {
                !line.starts_with('#') && words.iter().any(|w| line.to_lowercase().contains(w))
            })
            .unwrap_or(title);
        let mtime = meta
            .modified()
            .unwrap_or(UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        results.push(json!({"path":relative.to_string_lossy(),"title":clean(title).chars().take(120).collect::<String>(),"preview":clean(excerpt).chars().take(180).collect::<String>(),"score":words.iter().map(|w| folded.matches(w).count()).sum::<usize>(),"mtime":mtime}));
    }
    results.sort_by(|a, b| {
        b["score"].as_u64().cmp(&a["score"].as_u64()).then_with(|| {
            b["mtime"]
                .as_f64()
                .partial_cmp(&a["mtime"].as_f64())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    results.truncate(25);
    Ok(Value::Array(results))
}
fn menu_route(command: &str) -> String {
    if command.starts_with("grim -g ") {
        return "actions.capture.area".into();
    }
    if command.starts_with("grim ~/Pictures/Screenshots/") {
        return "actions.capture.screen".into();
    }
    if let Some(at) = command.rfind("nacre-menu ") {
        let tail = &command[at + 11..];
        if tail
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
            && !tail.is_empty()
        {
            return tail.to_owned();
        }
    }
    for (suffix, route) in [
        ("nacre-shell wallpaper", "style.picker"),
        ("nacre-shell clipboard", "actions.clipboard"),
        ("nacre-shell overview", "actions.windows"),
        ("nacre-shell keys", "learn.shortcuts"),
        ("nacre-settings", "setup.settings"),
        ("nacre-app emoji", "actions.emoji"),
        ("loginctl lock-session", "power.lock"),
        ("nacre-app terminal", "actions.terminal"),
    ] {
        if command.ends_with(suffix) {
            return route.into();
        }
    }
    String::new()
}
fn projects(paths: &Paths) -> Result<Vec<Value>> {
    let source = env::var_os("NACRE_AI_PROJECTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.home.join(".config/nacre/ai/projects.json"));
    let mut out = vec![
        json!({"id":"general-chat","label":"General chat","path":paths.home.join(".local/share/nacre/ai/chats").to_string_lossy(),"chat_directory":true}),
    ];
    if source.is_file() {
        let data: Value = crate::state::read_strict(&source, json!({}))?;
        if let Some(rows) = data["projects"].as_array() {
            out.extend(rows.iter().filter(|p| p["host"].is_null()).cloned());
        }
    }
    Ok(out)
}
fn list_source(project: &Value, agent: &str) -> Result<Vec<Value>> {
    let binary = if agent == "claude" {
        "nacre-ai-claude"
    } else {
        "nacre-ai-chat"
    };
    let account = crate::ai_core::effective_account(agent, None)?;
    let mut argv = vec![
        "list".to_owned(),
        "--project".into(),
        project["id"].as_str().unwrap_or("").into(),
        "--path".into(),
        project["path"].as_str().unwrap_or("").into(),
        "--account".into(),
        account.clone(),
    ];
    if project["chat_directory"] == true {
        argv.push("--chat-directory".into());
    }
    if agent != "claude" {
        argv.push("--all-accounts".into());
    }
    let mut command = Command::new(binary);
    command.args(&argv);
    let delta = if agent == "claude" {
        crate::ai_core::environment_delta("claude", Some(&account))?
    } else {
        crate::ai_core::local_environment_delta(Some(&account))?
    };
    if let Some(set) = delta["set"].as_object() {
        for (key, value) in set {
            if let Some(value) = value.as_str() {
                command.env(key, value);
            }
        }
    }
    if let Some(unset) = delta["unset"].as_array() {
        for key in unset.iter().filter_map(Value::as_str) {
            command.env_remove(key);
        }
    }
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let output = crate::process::capture(command, None, Duration::from_secs(30))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap_or_default();
    Ok(rows
        .into_iter()
        .map(|mut row| {
            row["project"] = project.clone();
            row["agent"] = json!(agent);
            if row.get("account").is_none() {
                row["account"] = json!(account);
            }
            row
        })
        .collect())
}
/// Newest saved chats (at most 30); also refreshes the key table `load` resolves.
pub(crate) fn chat_list(paths: &Paths) -> Result<Value> {
    let mut records = Vec::new();
    for project in projects(paths)? {
        for agent in ["codex", "claude"] {
            if let Ok(mut rows) = list_source(&project, agent) {
                records.append(&mut rows);
            }
        }
    }
    records.sort_by(|a, b| {
        b["updated_at"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(&a["updated_at"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut cached = serde_json::Map::new();
    for mut item in records.into_iter().take(30) {
        let id = item["id"].as_str().unwrap_or("");
        let agent = item["agent"].as_str().unwrap_or("codex");
        let account = item["account"].as_str().unwrap_or("");
        let cwd = item["cwd"].as_str().unwrap_or("");
        let encoded = serde_json::to_vec(&json!([id, agent, account, cwd]))?;
        let key = digest(&encoded)[..24].to_owned();
        item["title"] = json!(clean(item["title"].as_str().unwrap_or(""))
            .chars()
            .take(100)
            .collect::<String>());
        cached.insert(key, item);
    }
    let value = Value::Object(cached);
    save(&state(paths).join("chats.json"), &value)?;
    Ok(Value::Array(value.as_object().unwrap().iter().map(|(key,item)|json!({"key":key,"id":item["id"],"title":item["title"],"agent":item["agent"],"account":item["account"].as_str().filter(|s|!s.is_empty()).unwrap_or("Default"),"state":item["state"].as_str().unwrap_or("saved")})).collect()))
}
fn resume(paths: &Paths, key: &str, terminal: bool) -> Result<()> {
    let records = crate::state::read_strict(&state(paths).join("chats.json"), json!({}))?;
    let item = records.get(key).ok_or("Saved chat no longer exists")?;
    if !terminal {
        let title = clean(item["title"].as_str().unwrap_or("Nacre chat"));
        Command::new("systemd-run")
            .args([
                "--user",
                "--scope",
                "--collect",
                "--quiet",
                "env",
                "-u",
                "LD_LIBRARY_PATH",
                "-u",
                "QML_IMPORT_PATH",
                "-u",
                "QML2_IMPORT_PATH",
                "-u",
                "QT_PLUGIN_PATH",
                "-u",
                "NACRE_LIB_DIR",
                "kitty",
                "--class",
                "nacre-ai-task",
                "--title",
                &title,
                "--",
                "nacre-desktop-extras",
                "resume-terminal",
                key,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        return Ok(());
    }
    let project = &item["project"];
    let agent = item["agent"].as_str().unwrap_or("codex");
    if agent == "claude" {
        let account = item["account"].as_str().unwrap_or("");
        let mut command = Command::new("nacre-ai-claude");
        command.args([
            "resume",
            "--project",
            project["id"].as_str().unwrap_or(""),
            "--path",
            item["cwd"]
                .as_str()
                .or(project["path"].as_str())
                .unwrap_or(""),
            "--account",
            account,
            "--thread-id",
            item["id"].as_str().unwrap_or(""),
        ]);
        let delta = crate::ai_core::environment_delta("claude", Some(account))?;
        if let Some(set) = delta["set"].as_object() {
            for (k, v) in set {
                if let Some(v) = v.as_str() {
                    command.env(k, v);
                }
            }
        }
        Err(command.exec().into())
    } else {
        let account = item["account"].as_str().unwrap_or("");
        let home = crate::ai_core::account_home("codex", Some(account))?;
        let executable = paths.home.join(".local/bin/codex");
        let mut command = Command::new(executable);
        command
            .args([
                "resume",
                item["id"].as_str().unwrap_or(""),
                "--sandbox",
                "danger-full-access",
                "--ask-for-approval",
                "never",
                "-C",
                item["cwd"]
                    .as_str()
                    .or(project["path"].as_str())
                    .unwrap_or(""),
                "--add-dir",
            ])
            .arg(paths.home.join("Documents/Nacre-Brain"));
        if !account.is_empty() {
            command.env("CODEX_HOME", home);
        }
        Err(command.exec().into())
    }
}
pub(crate) fn bindings(paths: &Paths) -> Result<Value> {
    let mut backend = Backend::new(paths.clone(), env::vars().collect::<BTreeMap<_, _>>())?;
    let files = backend.module_call(
        "binding_source_files",
        &[json!(paths.home.to_string_lossy())],
        &json!({"timeout":12}),
    )?;
    let mut source = BTreeMap::<(String, String), String>::new();
    if let Some(files) = files.as_array() {
        for file in files {
            if let Some(file) = file.as_str() {
                if let Ok(rows) = backend.module_call(
                    "capture_bindings",
                    &[json!(file), json!("bindings")],
                    &json!({"timeout":12}),
                ) {
                    if let Some(rows) = rows.as_array() {
                        for row in rows {
                            let keys = row["keys"]
                                .as_str()
                                .unwrap_or("")
                                .replace(' ', "")
                                .replace("space", "SPACE");
                            let action = row["action"]["method"]
                                .as_str()
                                .unwrap_or("")
                                .replace(['.', '_'], " ");
                            let args = row["action"]["args"]
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            let desc = row["flags"]["description"]
                                .as_str()
                                .and_then(|v| v.strip_prefix("Nacre:"))
                                .unwrap_or(if args.is_empty() {
                                    &action
                                } else {
                                    Path::new(args.split_whitespace().next().unwrap_or(args))
                                        .file_stem()
                                        .and_then(|v| v.to_str())
                                        .unwrap_or(&action)
                                });
                            source.insert(
                                (row["submap"].as_str().unwrap_or("").into(), keys),
                                desc.into(),
                            );
                        }
                    }
                }
            }
        }
    }
    let live = backend.module_call("query", &[json!("bindings")], &json!({"timeout":12}))?;
    let records = live.as_array().ok_or("Malformed compositor bindings")?;
    let mut result = Vec::new();
    for binding in records {
        let mut modifiers = Vec::new();
        let mask = binding["modmask"].as_u64().unwrap_or(0);
        for (flag, name) in [(64, "SUPER"), (4, "CTRL"), (8, "ALT"), (1, "SHIFT")] {
            if mask & flag != 0 {
                modifiers.push(name);
            }
        }
        let key = binding["key"].as_str().unwrap_or("");
        let key = if key.eq_ignore_ascii_case("space") {
            "SPACE"
        } else {
            key
        };
        let combo = if modifiers.is_empty() {
            key.to_owned()
        } else {
            format!("{}+{key}", modifiers.join("+"))
        };
        let submap = binding["submap"].as_str().unwrap_or("");
        let mut description = source
            .get(&(submap.into(), combo.clone()))
            .cloned()
            .unwrap_or_else(|| {
                binding["description"]
                    .as_str()
                    .or(binding["dispatcher"].as_str())
                    .unwrap_or("")
                    .into()
            });
        if !submap.is_empty() {
            description = format!("{submap} · {description}");
        }
        let route = menu_route(binding["arg"].as_str().unwrap_or(""));
        result.push(json!({"key":combo,"description":description,"submap":submap,"menu":route}));
    }
    result.sort_by(|a, b| {
        (
            !a["key"].as_str().unwrap_or("").starts_with("SUPER"),
            a["key"].as_str().unwrap_or(""),
        )
            .cmp(&(
                !b["key"].as_str().unwrap_or("").starts_with("SUPER"),
                b["key"].as_str().unwrap_or(""),
            ))
    });
    Ok(Value::Array(result))
}
pub fn run(args: &[String]) -> Result<i32> {
    let Some(action) = args.first().map(String::as_str) else {
        return Err("Expected a desktop-extras operation".into());
    };
    let paths = Paths::environment()?;
    let directory = state(&paths);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let input_limit = if action == "store" {
        16 * 1024 * 1024
    } else {
        1024 * 1024
    };
    let mut input = Vec::new();
    std::io::stdin()
        .take((input_limit + 1) as u64)
        .read_to_end(&mut input)?;
    if input.len() > input_limit {
        return Err("Desktop extras request too large".into());
    }
    if action == "store" {
        store_clip(&paths, &input)?;
        return Ok(0);
    }
    let payload: Value = if input.iter().all(u8::is_ascii_whitespace) {
        json!({})
    } else {
        serde_json::from_slice(&input)?
    };
    let result: Result<Value> = (|| match action {
        "clips" => clips(&paths),
        "copy" | "pin" | "delete" => {
            clip_action(&paths, action, payload["id"].as_str().unwrap_or("")).map(|_| json!(true))
        }
        "brain" => brain_search(payload["query"].as_str().unwrap_or("")),
        "explore" | "note" => {
            let root = vault()?;
            let path = payload["path"].as_str().unwrap_or("");
            let file = fs::canonicalize(root.join(path));
            match file {
                Ok(file)
                    if file.starts_with(&root)
                        && file.extension().is_some_and(|v| v == "md")
                        && file.is_file() =>
                {
                    if action == "note" {
                        Command::new("xdg-open")
                            .arg(file)
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()?;
                    } else {
                        let token = uuid::Uuid::new_v4().simple().to_string();
                        save(
                            &directory.join("brain-focus.json"),
                            &json!({"path":file.strip_prefix(&root)?.to_string_lossy(),"token":token,"created":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64()}),
                        )?;
                        Command::new("nacre-shell")
                            .arg("brain")
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()?;
                    }
                    Ok(json!(true))
                }
                _ => Err("Invalid note".into()),
            }
        }
        "keys" => bindings(&paths),
        "capture" => {
            let text = payload["text"].as_str().unwrap_or("").trim();
            if text.is_empty() || text.chars().count() > 16000 {
                Err("Capture must contain 1–16000 characters".into())
            } else {
                let title = text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(80)
                    .collect::<String>();
                let mut command = Command::new("nacre-ai-memory");
                command
                    .args([
                        "--title",
                        &title,
                        "--source",
                        "User capture from desktop brain drawer",
                        "--confidence",
                        "reported",
                        "--world",
                        "Personal",
                        "--topic",
                        "Quick captures",
                    ])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null());
                let mut child = command.spawn()?;
                child
                    .stdin
                    .take()
                    .ok_or("Missing capture stdin")?
                    .write_all(text.as_bytes())?;
                if child.wait()?.success() {
                    Ok(json!(true))
                } else {
                    Err("Could not save capture".into())
                }
            }
        }
        "chats" => chat_list(&paths),
        "resume" | "resume-terminal" => resume(
            &paths,
            payload["key"]
                .as_str()
                .or_else(|| args.get(1).map(String::as_str))
                .unwrap_or(""),
            action == "resume-terminal",
        )
        .map(|_| json!(true)),
        _ => Err("Unknown desktop operation".into()),
    })();
    let (value, code) = match result {
        Ok(result) => (
            json!({"action":action,"result":result,"query":payload["query"].as_str().unwrap_or("")}),
            0,
        ),
        Err(_error) => (
            json!({"action":action,"error":format!("Could not complete {action}. Check the selected item or try again."),"query":payload["query"].as_str().unwrap_or("")}),
            0,
        ),
    };
    println!("{}", crate::json::encode(&value)?);
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn menu_routes_keep_the_existing_desktop_targets() {
        assert_eq!(
            menu_route("grim -g 0,0 1x1 - | wl-copy"),
            "actions.capture.area"
        );
        assert_eq!(
            menu_route("/usr/bin/nacre-menu setup.settings"),
            "setup.settings"
        );
        assert_eq!(menu_route("loginctl lock-session"), "power.lock");
        assert_eq!(menu_route("unrelated command"), "");
    }

    #[test]
    fn pins_store_fingerprints_without_clipboard_content() {
        let value = fingerprint("12\tprivate clipboard label", b"private clipboard body");
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert!(!value.to_string().contains("private clipboard"));
    }

    #[test]
    fn clipboard_retention_is_seven_days_and_keeps_pins() {
        let now = 1_800_000_000;
        assert!(!should_expire_clip(now - CLIP_HISTORY_AGE + 1, now, false));
        assert!(should_expire_clip(now - CLIP_HISTORY_AGE, now, false));
        assert!(!should_expire_clip(now - CLIP_HISTORY_AGE * 4, now, true));
    }

    #[test]
    fn thumbnails_are_private_and_removed_with_their_clip() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let data = b"\x89PNG\r\nexample";
        let path = ensure_thumbnail(folder.path(), data)?;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read(&path)?, data);
        remove_thumbnail(folder.path(), data)?;
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn brain_search_requires_every_term_and_skips_hidden_files_and_symlinks() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let root = folder.path();
        fs::write(root.join("visible.md"), "# Visible\nalpha beta\n")?;
        fs::create_dir(root.join(".hidden"))?;
        fs::write(root.join(".hidden/secret.md"), "alpha beta")?;
        std::os::unix::fs::symlink(root.join("visible.md"), root.join("alias.md"))?;
        let rows = brain_search_at(&["alpha".into(), "beta".into()], root)?;
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["path"], "visible.md");
        assert!(brain_search_at(&["alpha".into(), "missing".into()], root)?
            .as_array()
            .unwrap()
            .is_empty());
        Ok(())
    }
}
