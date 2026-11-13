//! Daily cleanup for Nacre-owned histories. External screenshot files and
//! provider-owned assistant databases are deliberately outside these roots.
use crate::{paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    env, fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DAY: u64 = 24 * 60 * 60;
const CHAT_SCAN_DEPTH: usize = 8;
const CHAT_SCAN_LIMIT: usize = 10_000;

fn days(paths: &Paths, key: &str, default: u64) -> u64 {
    fs::read(paths.config.join("desktop.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value[key].as_u64())
        .unwrap_or(default)
}

fn old(path: &Path, days: u64, now: SystemTime) -> bool {
    days > 0
        && path
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= Duration::from_secs(days.saturating_mul(DAY)))
}

fn safe_dir_within_home(home: &Path, path: &Path) -> bool {
    let Ok(real_home) = fs::canonicalize(home) else {
        return false;
    };
    let Ok(relative) = path.strip_prefix(home) else {
        return false;
    };
    let mut current = real_home;
    for component in relative.components() {
        let std::path::Component::Normal(part) = component else {
            return false;
        };
        current.push(part);
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            return false;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return false;
        }
    }
    true
}

fn chat_last_activity(path: &Path) -> Option<SystemTime> {
    let mut latest = None;
    for (index, item) in walkdir::WalkDir::new(path)
        .follow_links(false)
        .max_depth(CHAT_SCAN_DEPTH)
        .into_iter()
        .enumerate()
    {
        // An incomplete scan cannot establish that the chat is inactive.
        if index + 1 >= CHAT_SCAN_LIMIT {
            return None;
        }
        let entry = item.ok()?;
        if entry.depth() == CHAT_SCAN_DEPTH
            && entry.file_type().is_dir()
            && fs::read_dir(entry.path()).ok()?.next().is_some()
        {
            return None;
        }
        if entry.file_type().is_file() {
            let modified = entry.metadata().ok()?.modified().ok()?;
            latest = Some(latest.map_or(modified, |seen: SystemTime| seen.max(modified)));
        }
    }
    latest.or_else(|| fs::symlink_metadata(path).ok()?.modified().ok())
}

/// Claude Code keeps a session's transcripts in `~/.claude/projects/<cwd with every
/// non-alphanumeric byte replaced by '-'>`; chats nested in a chat folder extend that name.
fn claude_project_name(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn in_chat_project(project: &str, chat: &str) -> bool {
    project == chat || project.starts_with(&format!("{chat}-"))
}

/// The chat's own files and the transcripts of every session started in it or below
/// it: a running session's working directory files can be weeks old.
fn chat_activity(home: &Path, path: &Path) -> Option<SystemTime> {
    let mut latest = chat_last_activity(path)?;
    let chat = claude_project_name(path);
    if let Ok(projects) = fs::read_dir(home.join(".claude/projects")) {
        for project in projects.flatten() {
            if in_chat_project(&project.file_name().to_string_lossy(), &chat) {
                latest = latest.max(chat_last_activity(&project.path())?);
            }
        }
    }
    Some(latest)
}

fn old_chat(home: &Path, path: &Path, days: u64, now: SystemTime) -> bool {
    days > 0
        && chat_activity(home, path)
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= Duration::from_secs(days.saturating_mul(DAY)))
}

fn screenshot_index(paths: &Paths) -> std::path::PathBuf {
    paths.state.join("shell/screenshot-history.json")
}

#[derive(serde::Deserialize, serde::Serialize)]
struct ScreenshotRecord {
    name: String,
    dev: u64,
    ino: u64,
}

/// Called only by Nacre's screenshot writer after it reserves the output file.
pub fn record_screenshot(paths: &Paths, path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if path.parent() != Some(paths.screenshots().as_path())
        || !metadata.is_file()
        || metadata.uid() != nix::unistd::getuid().as_raw()
    {
        return Err("Refusing to record a non-Nacre screenshot".into());
    }
    let index = screenshot_index(paths);
    let mut rows: Vec<ScreenshotRecord> = fs::read(&index)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid screenshot name")?
        .to_owned();
    rows.retain(|row| row.name != name);
    rows.push(ScreenshotRecord {
        name,
        dev: metadata.dev(),
        ino: metadata.ino(),
    });
    crate::state::write(&index, &json!(rows))
}

fn desktop_trash(path: &Path) -> bool {
    matches!(
        Command::new("gio")
            .args(["trash", "--"])
            .arg(path)
            .status(),
        Ok(status) if status.success()
    )
}

fn prune_screenshots_with(
    paths: &Paths,
    now: SystemTime,
    mut trash: impl FnMut(&Path) -> bool,
) -> Result<usize> {
    let index = screenshot_index(paths);
    let mut rows: Vec<ScreenshotRecord> = fs::read(&index)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let days = days(paths, "screenshotRetention", 0);
    let mut removed = Vec::new();
    if days == 0 {
        return Ok(0);
    }
    if !safe_dir_within_home(&paths.home, &paths.screenshots()) {
        return Ok(0);
    }
    for record in &rows {
        let name = &record.name;
        if name.is_empty() || Path::new(name).components().count() != 1 {
            continue;
        }
        let path = paths.screenshots().join(name);
        if !old(&path, days, now)
            || !fs::symlink_metadata(&path).is_ok_and(|m| {
                m.is_file()
                    && m.uid() == nix::unistd::getuid().as_raw()
                    && m.dev() == record.dev
                    && m.ino() == record.ino
            })
        {
            continue;
        }
        if trash(&path) {
            removed.push(name.clone());
            tracing::info!(file=%path.display(), "Moved expired Nacre screenshot to Trash");
        } else {
            tracing::warn!(file=%path.display(), "Could not move expired Nacre screenshot to Trash");
        }
    }
    rows.retain(|record| !removed.contains(&record.name));
    crate::state::write(&index, &json!(rows))?;
    Ok(removed.len())
}

fn chat_id(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[derive(Debug, PartialEq, Eq)]
struct TmuxPane {
    name: String,
    cwd: PathBuf,
}

fn parse_tmux_panes(listing: &str) -> Option<Vec<TmuxPane>> {
    listing
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (name, cwd) = line.split_once('\t')?;
            (!name.is_empty() && !cwd.is_empty()).then(|| TmuxPane {
                name: name.to_owned(),
                cwd: PathBuf::from(cwd),
            })
        })
        .collect()
}

fn tmux_panes() -> Option<Vec<TmuxPane>> {
    let available = env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| directory.join("tmux").is_file())
    });
    if !available {
        return None;
    }
    let mut command = Command::new("tmux");
    command.args([
        "list-panes",
        "-a",
        "-F",
        "#{session_name}\t#{pane_current_path}",
    ]);
    let output = crate::process::capture(command, None, Duration::from_secs(4)).ok()?;
    if !output.status.success() {
        return String::from_utf8_lossy(&output.stderr)
            .to_ascii_lowercase()
            .contains("no server running")
            .then(Vec::new);
    }
    parse_tmux_panes(&String::from_utf8_lossy(&output.stdout))
}

/// Nacre names AI sessions `ai-<project>-<agent>-<account>-<thread id>`.
fn session_thread(name: &str) -> Option<&str> {
    let id = name.get(name.len().checked_sub(36)?..)?;
    (name.starts_with("ai-")
        && id.split('-').map(str::len).eq([8, 4, 4, 4, 12])
        && id.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit()))
    .then_some(id)
}

/// The Claude project of every live Nacre AI session, or None when one of them
/// cannot be placed (e.g. a Codex thread): then no chat can be proven closed.
fn live_session_projects(home: &Path, panes: &[TmuxPane]) -> Option<Vec<String>> {
    let projects: Vec<_> = fs::read_dir(home.join(".claude/projects"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    let mut live = Vec::new();
    for pane in panes.iter().filter(|pane| pane.name.starts_with("ai-")) {
        let thread = session_thread(&pane.name)?;
        let project = projects
            .iter()
            .find(|project| project.join(format!("{thread}.jsonl")).is_file())?;
        live.push(project.file_name()?.to_string_lossy().into_owned());
    }
    Some(live)
}

fn chat_has_live_tmux(path: &Path, panes: &[TmuxPane], live_projects: &[String]) -> bool {
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let chat = claude_project_name(path);
    panes.iter().any(|pane| {
        fs::canonicalize(&pane.cwd)
            .unwrap_or_else(|_| pane.cwd.clone())
            .starts_with(&target)
    }) || live_projects
        .iter()
        .any(|project| in_chat_project(project, &chat))
}

fn prune_ai_chats(
    paths: &Paths,
    now: SystemTime,
    trash: &mut impl FnMut(&Path) -> bool,
) -> Result<usize> {
    let root = paths.home.join(".local/share/nacre/ai/chats");
    let days = days(paths, "aiChatRetention", 0);
    if days == 0 {
        return Ok(0);
    }
    if !safe_dir_within_home(&paths.home, &root) {
        return Ok(0);
    }
    let Some(panes) = tmux_panes() else {
        tracing::warn!("Could not list tmux panes; skipping AI chat retention");
        return Ok(0);
    };
    prune_ai_chats_with(paths, now, trash, &panes)
}

fn prune_ai_chats_with(
    paths: &Paths,
    now: SystemTime,
    trash: &mut impl FnMut(&Path) -> bool,
    panes: &[TmuxPane],
) -> Result<usize> {
    let root = paths.home.join(".local/share/nacre/ai/chats");
    if !safe_dir_within_home(&paths.home, &root) {
        return Ok(0);
    }
    let days = days(paths, "aiChatRetention", 0);
    if days == 0 {
        return Ok(0);
    }
    let Some(live_projects) = live_session_projects(&paths.home, panes) else {
        tracing::warn!("A live AI session has no known chat folder; skipping AI chat retention");
        return Ok(0);
    };
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !chat_id(&entry.file_name().to_string_lossy())
            || !meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.uid() != nix::unistd::getuid().as_raw()
            || !old_chat(&paths.home, &path, days, now)
        {
            continue;
        }
        // Saved chats are user-retained; leave their Nacre-managed directory intact.
        if saved_chat_path(paths, &path) {
            continue;
        }
        if chat_has_live_tmux(&path, panes, &live_projects) {
            tracing::info!(directory=%path.display(), "Keeping open Nacre AI chat");
            continue;
        }
        if trash(&path) {
            removed += 1;
            tracing::info!(directory=%path.display(), "Moved expired Nacre AI chat to Trash");
        } else {
            tracing::warn!(directory=%path.display(), "Could not move expired Nacre AI chat to Trash");
        }
    }
    Ok(removed)
}

fn saved_chat_path(paths: &Paths, path: &Path) -> bool {
    let matches = |records: Value| {
        records.as_object().is_some_and(|rows| {
            rows.values().any(|row| {
                row["cwd"].as_str() == Some(path.to_string_lossy().as_ref())
                    && (row["state"].as_str() == Some("saved")
                        || row["pinned"].as_bool() == Some(true)
                        || row["starred"].as_bool() == Some(true))
            })
        })
    };
    let saved: Value = fs::read(paths.state.join("shell/extras/chats.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| json!({}));
    if matches(saved) {
        return true;
    }
    let sidebar = paths.state.join("ai/sidebar");
    fs::read_dir(sidebar)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            entry.path().extension().is_some_and(|ext| ext == "json")
                && fs::read(entry.path())
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .is_some_and(|record: Value| {
                        record["cwd"].as_str() == Some(path.to_string_lossy().as_ref())
                    })
        })
}

pub fn prune(paths: &Paths, now: SystemTime) -> Result<()> {
    let mut trash = desktop_trash;
    let screenshots = prune_screenshots_with(paths, now, &mut trash)?;
    let chats = prune_ai_chats(paths, now, &mut trash)?;
    if screenshots + chats > 0 {
        tracing::info!(screenshots, chats, "History retention pass completed");
    }
    Ok(())
}

fn prune_daily(paths: &Paths, now: SystemTime) -> Result<()> {
    let marker = paths.state.join("shell/history-retention-last-run");
    let now_s = now.duration_since(UNIX_EPOCH)?.as_secs();
    let previous = fs::read_to_string(&marker)
        .ok()
        .and_then(|value| value.parse::<u64>().ok());
    if previous.is_some_and(|previous| now_s.saturating_sub(previous) < DAY) {
        return Ok(());
    }
    prune(paths, now)?;
    crate::files::atomic(&marker, now_s.to_string().as_bytes())?;
    Ok(())
}

pub async fn run(paths: Paths) -> Result<()> {
    let mut tick = tokio::time::interval(Duration::from_secs(DAY));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let work = paths.clone();
        tokio::task::spawn_blocking(move || prune_daily(&work, SystemTime::now())).await??;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_keeps_old_paths_and_recent_paths_survive() -> Result<()> {
        let home = tempfile::tempdir()?;
        let _paths = Paths::new(home.path());
        let file = home.path().join("old");
        fs::write(&file, b"x")?;
        assert!(!old(
            &file,
            0,
            SystemTime::now() + Duration::from_secs(400 * DAY)
        ));
        assert!(!old(&file, 30, SystemTime::now()));
        Ok(())
    }

    #[test]
    fn screenshot_pruning_only_uses_nacre_record_and_trashes_it() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(paths.config.clone())?;
        fs::create_dir_all(paths.screenshots())?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"screenshotRetention":30}"#,
        )?;
        let owned = paths.screenshots().join("screenshot_20000101.png");
        let unrelated = paths.screenshots().join("user.png");
        fs::write(&owned, b"owned")?;
        fs::write(&unrelated, b"user")?;
        let now = SystemTime::now() + Duration::from_secs(400 * DAY);
        record_screenshot(&paths, &owned)?;
        assert_eq!(
            prune_screenshots_with(&paths, now, |path| fs::remove_file(path).is_ok())?,
            1
        );
        assert!(unrelated.exists());
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"screenshotRetention":0}"#,
        )?;
        let forever = paths.screenshots().join("screenshot_forever.png");
        fs::write(&forever, b"keep")?;
        record_screenshot(&paths, &forever)?;
        assert_eq!(
            prune_screenshots_with(&paths, now, |path| fs::remove_file(path).is_ok())?,
            0
        );
        assert!(forever.exists());
        fs::remove_file(&forever)?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"screenshotRetention":30}"#,
        )?;
        let replaced = paths.screenshots().join("screenshot_replaced.png");
        fs::write(&replaced, b"original")?;
        record_screenshot(&paths, &replaced)?;
        fs::rename(&replaced, paths.screenshots().join("original-held.png"))?;
        fs::write(&replaced, b"replacement")?;
        assert_eq!(
            prune_screenshots_with(&paths, now, |path| fs::remove_file(path).is_ok())?,
            0
        );
        assert!(replaced.exists());
        Ok(())
    }

    #[test]
    fn screenshot_pruning_refuses_a_symlinked_capture_root() -> Result<()> {
        let home = tempfile::tempdir()?;
        let external = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(paths.config.clone())?;
        fs::create_dir_all(paths.state.join("shell"))?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"screenshotRetention":1}"#,
        )?;
        let outside = external.path().join("owned.png");
        fs::write(&outside, b"outside")?;
        let outside_meta = fs::metadata(&outside)?;
        fs::write(
            screenshot_index(&paths),
            json!([{"name":"owned.png","dev":outside_meta.dev(),"ino":outside_meta.ino()}])
                .to_string(),
        )?;
        std::os::unix::fs::symlink(external.path(), home.path().join("Pictures"))?;
        let mut called = false;
        assert_eq!(
            prune_screenshots_with(
                &paths,
                SystemTime::now() + Duration::from_secs(3 * DAY),
                |_| {
                    called = true;
                    true
                }
            )?,
            0
        );
        assert!(!called);
        assert!(outside.exists());
        Ok(())
    }

    #[test]
    fn ai_pruning_only_removes_expired_nacre_uuid_directories() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(paths.config.clone())?;
        let root = paths.home.join(".local/share/nacre/ai/chats");
        fs::create_dir_all(&root)?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"aiChatRetention":30}"#,
        )?;
        let owned = root.join("0123456789abcdef0123456789abcdef");
        let unrelated = root.join("provider-history");
        fs::create_dir(&owned)?;
        fs::write(owned.join("user-notes.txt"), b"keep this user file")?;
        fs::create_dir(&unrelated)?;
        let trash = home.path().join("Trash");
        fs::create_dir(&trash)?;
        let moved = trash.join(owned.file_name().ok_or("Missing chat ID")?);
        assert_eq!(
            prune_ai_chats_with(
                &paths,
                SystemTime::now() + Duration::from_secs(400 * DAY),
                &mut |path| fs::rename(path, trash.join(path.file_name().unwrap())).is_ok(),
                &[],
            )?,
            1
        );
        assert!(unrelated.exists());
        assert!(!owned.exists());
        assert!(moved.exists());
        assert_eq!(
            fs::read(moved.join("user-notes.txt"))?,
            b"keep this user file"
        );
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"aiChatRetention":0}"#,
        )?;
        let forever = root.join("fedcba9876543210fedcba9876543210");
        fs::create_dir(&forever)?;
        assert_eq!(
            prune_ai_chats_with(
                &paths,
                SystemTime::now() + Duration::from_secs(400 * DAY),
                &mut |_| panic!("zero retention must not trash chats"),
                &[],
            )?,
            0
        );
        assert!(forever.exists());
        Ok(())
    }

    #[test]
    fn ai_pruning_refuses_symlinked_root_and_preserves_saved_pinned_and_starred_chats() -> Result<()>
    {
        let home = tempfile::tempdir()?;
        let external = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(paths.config.clone())?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"aiChatRetention":1}"#,
        )?;
        let nacre = paths.home.join(".local/share/nacre/ai");
        fs::create_dir_all(&nacre)?;
        let outside = external.path().join("0123456789abcdef0123456789abcdef");
        fs::create_dir(&outside)?;
        std::os::unix::fs::symlink(external.path(), nacre.join("chats"))?;
        assert_eq!(
            prune_ai_chats_with(
                &paths,
                SystemTime::now() + Duration::from_secs(3 * DAY),
                &mut |_| panic!("symlinked root must not trash chats"),
                &[],
            )?,
            0
        );
        assert!(outside.exists());
        fs::remove_file(nacre.join("chats"))?;
        let root = nacre.join("chats");
        fs::create_dir(&root)?;
        let mut records = serde_json::Map::new();
        for (index, field) in ["state", "pinned", "starred"].into_iter().enumerate() {
            let name = format!("{index:032x}");
            let chat = root.join(&name);
            fs::create_dir(&chat)?;
            let chat = chat.to_string_lossy();
            let row = match field {
                "state" => json!({"cwd":chat,"state":"saved"}),
                "pinned" => json!({"cwd":chat,"pinned":true}),
                _ => json!({"cwd":chat,"starred":true}),
            };
            records.insert(name, row);
        }
        let saved_file = paths.state.join("shell/extras/chats.json");
        crate::state::write(&saved_file, &Value::Object(records))?;
        let mut called = false;
        assert_eq!(
            prune_ai_chats_with(
                &paths,
                SystemTime::now() + Duration::from_secs(3 * DAY),
                &mut |_| {
                    called = true;
                    false
                },
                &[],
            )?,
            0
        );
        assert!(!called);
        Ok(())
    }

    #[test]
    fn expired_chat_trash_failure_keeps_the_working_directory() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.config)?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"aiChatRetention":1}"#,
        )?;
        let root = paths.home.join(".local/share/nacre/ai/chats");
        fs::create_dir_all(&root)?;
        let chat = root.join("0123456789abcdef0123456789abcdef");
        fs::create_dir(&chat)?;
        fs::write(chat.join("user-notes.txt"), b"keep this file")?;
        let now = SystemTime::now() + Duration::from_secs(3 * DAY);
        let mut called = false;
        assert_eq!(
            prune_ai_chats_with(
                &paths,
                now,
                &mut |_| {
                    called = true;
                    false
                },
                &[]
            )?,
            0
        );
        assert!(called);
        assert!(chat.is_dir());
        assert_eq!(fs::read(chat.join("user-notes.txt"))?, b"keep this file");
        Ok(())
    }

    #[test]
    fn active_tmux_chat_is_preserved_by_pane_cwd_or_session_transcript() -> Result<()> {
        let home = tempfile::tempdir()?;
        let chat = home.path().join("0123456789abcdef0123456789abcdef");
        let nested = chat.join("fedcba9876543210fedcba9876543210");
        fs::create_dir_all(&nested)?;
        let in_chat = parse_tmux_panes(&format!("shell\t{}", chat.display())).unwrap();
        let in_nested = parse_tmux_panes(&format!("shell\t{}", nested.display())).unwrap();
        let elsewhere = parse_tmux_panes(&format!("shell\t{}", home.path().display())).unwrap();
        assert!(chat_has_live_tmux(&chat, &in_chat, &[]));
        assert!(chat_has_live_tmux(&chat, &in_nested, &[]));
        assert!(!chat_has_live_tmux(&chat, &elsewhere, &[]));

        // A session whose pane moved to $HOME is still found through its transcript.
        let thread = "5d0352a0-9307-4a5d-b687-2cc977fcbee7";
        let project = home
            .path()
            .join(".claude/projects")
            .join(claude_project_name(&nested));
        fs::create_dir_all(&project)?;
        fs::write(project.join(format!("{thread}.jsonl")), "{}\n")?;
        let session = parse_tmux_panes(&format!(
            "ai-general-chat-claude-default-{thread}\t{}",
            home.path().display()
        ))
        .unwrap();
        let live = live_session_projects(home.path(), &session).unwrap();
        assert!(chat_has_live_tmux(&chat, &session, &live));
        assert!(!chat_has_live_tmux(
            &home.path().join("00000000000000000000000000000000"),
            &session,
            &live
        ));
        // A live session that cannot be placed blocks the whole pass.
        let unknown = parse_tmux_panes(&format!(
            "ai-demo-codex-default-0b6f5f2e-1111-4222-8333-944445555666\t{}",
            home.path().display()
        ))
        .unwrap();
        assert!(live_session_projects(home.path(), &unknown).is_none());
        assert!(parse_tmux_panes("malformed-live-pane").is_none());
        Ok(())
    }

    #[test]
    fn a_recent_transcript_keeps_a_chat_with_old_files() -> Result<()> {
        let home = tempfile::tempdir()?;
        let chat = home.path().join("0123456789abcdef0123456789abcdef");
        fs::create_dir_all(&chat)?;
        let old = fs::File::create(chat.join("notes.txt"))?;
        let week_ago = SystemTime::now() - Duration::from_secs(8 * DAY);
        old.set_modified(week_ago)?;
        fs::File::open(&chat)?.set_modified(week_ago)?;
        assert!(old_chat(home.path(), &chat, 7, SystemTime::now()));
        let project = home
            .path()
            .join(".claude/projects")
            .join(format!("{}-child", claude_project_name(&chat)));
        fs::create_dir_all(&project)?;
        fs::write(project.join("thread.jsonl"), "{}\n")?;
        assert!(!old_chat(home.path(), &chat, 7, SystemTime::now()));
        Ok(())
    }

    #[test]
    fn ai_pruning_keeps_chat_with_entries_over_the_activity_scan_limit() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.config)?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"aiChatRetention":1}"#,
        )?;
        let root = paths.home.join(".local/share/nacre/ai/chats");
        fs::create_dir_all(&root)?;
        let chat = root.join("0123456789abcdef0123456789abcdef");
        fs::create_dir(&chat)?;
        for index in 0..CHAT_SCAN_LIMIT {
            fs::write(chat.join(format!("entry-{index}")), b"")?;
        }
        let status = Command::new("touch")
            .args(["-d", "@1"])
            .arg(&chat)
            .status()?;
        assert!(status.success());
        let mut called = false;
        assert_eq!(
            prune_ai_chats_with(
                &paths,
                SystemTime::now() + Duration::from_secs(3 * DAY),
                &mut |_| {
                    called = true;
                    true
                },
                &[],
            )?,
            0
        );
        assert!(!called);
        assert!(chat.is_dir());
        Ok(())
    }

    #[test]
    fn ai_pruning_keeps_chat_with_entries_beyond_activity_scan_depth() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.config)?;
        fs::write(
            paths.config.join("desktop.json"),
            br#"{"aiChatRetention":1}"#,
        )?;
        let root = paths.home.join(".local/share/nacre/ai/chats");
        fs::create_dir_all(&root)?;
        let chat = root.join("0123456789abcdef0123456789abcdef");
        fs::create_dir(&chat)?;
        let mut deep = chat.clone();
        for index in 0..=CHAT_SCAN_DEPTH {
            deep.push(format!("level-{index}"));
            fs::create_dir(&deep)?;
        }
        fs::write(deep.join("active.txt"), b"active")?;
        let status = Command::new("touch")
            .args(["-d", "@1"])
            .arg(&chat)
            .status()?;
        assert!(status.success());
        let mut called = false;
        assert_eq!(
            prune_ai_chats_with(
                &paths,
                SystemTime::now() + Duration::from_secs(3 * DAY),
                &mut |_| {
                    called = true;
                    true
                },
                &[],
            )?,
            0
        );
        assert!(!called);
        assert!(chat.is_dir());
        Ok(())
    }

    #[test]
    fn active_chat_file_mtime_protects_an_old_directory() -> Result<()> {
        let home = tempfile::tempdir()?;
        let chat = home.path().join("chat");
        fs::create_dir(&chat)?;
        fs::write(chat.join("transcript.jsonl"), b"active")?;
        let status = Command::new("touch")
            .args(["-d", "@1"])
            .arg(&chat)
            .status()?;
        assert!(status.success());
        let now = SystemTime::now() + Duration::from_secs(29 * DAY);
        assert!(!old_chat(home.path(), &chat, 30, now));
        Ok(())
    }

    #[test]
    fn retention_pass_runs_at_most_once_per_day() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let first = UNIX_EPOCH + Duration::from_secs(200 * DAY);
        prune_daily(&paths, first)?;
        let marker = paths.state.join("shell/history-retention-last-run");
        assert_eq!(fs::read_to_string(&marker)?, (200 * DAY).to_string());
        prune_daily(&paths, first + Duration::from_secs(DAY - 1))?;
        assert_eq!(fs::read_to_string(&marker)?, (200 * DAY).to_string());
        prune_daily(&paths, first + Duration::from_secs(DAY))?;
        assert_eq!(fs::read_to_string(&marker)?, (201 * DAY).to_string());
        Ok(())
    }
}
