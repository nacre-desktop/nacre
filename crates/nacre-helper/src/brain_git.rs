//! Private git history for a Brain vault. All git output that could contain note
//! contents is kept out of diagnostics; guard failures name only path and rule.
use crate::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

const LIMIT: u64 = 5 * 1024 * 1024;
const IDENT_NAME: &str = "siverteh";
const IDENT_EMAIL: &str = "92440237+Siverteh@users.noreply.github.com";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct QueuedPath {
    path: String,
    agent: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Queue {
    entries: Vec<QueuedPath>,
    #[serde(default)]
    updated: Option<i64>,
    #[serde(default)]
    sweep_signature: Option<String>,
    #[serde(default)]
    sweep_updated: Option<i64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Status {
    last_commit: Option<String>,
    last_push: Option<String>,
    pending_paths: Vec<String>,
    error: Option<String>,
    #[serde(default)]
    owned_commits: Vec<String>,
    #[serde(default)]
    conflict: bool,
    #[serde(default)]
    blocked: Vec<BlockedPath>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct BlockedPath {
    path: String,
    rule: String,
}
#[derive(Clone, Debug)]
struct Candidate {
    path: String,
}

fn git(root: &Path, args: &[&str]) -> Result<Output> {
    Ok(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_AUTHOR_NAME", IDENT_NAME)
        .env("GIT_AUTHOR_EMAIL", IDENT_EMAIL)
        .env("GIT_COMMITTER_NAME", IDENT_NAME)
        .env("GIT_COMMITTER_EMAIL", IDENT_EMAIL)
        .output()?)
}
fn git_network(root: &Path, args: &[&str]) -> Result<Output> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    command
        .env("GIT_AUTHOR_NAME", IDENT_NAME)
        .env("GIT_AUTHOR_EMAIL", IDENT_EMAIL)
        .env("GIT_COMMITTER_NAME", IDENT_NAME)
        .env("GIT_COMMITTER_EMAIL", IDENT_EMAIL);
    crate::process::capture(command, None, Duration::from_secs(30))
}
fn success(root: &Path, args: &[&str]) -> Result<String> {
    let out = git(root, args)?;
    if !out.status.success() {
        return Err(format!("git {} failed", args.first().copied().unwrap_or("command")).into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
fn state(root: &Path) -> PathBuf {
    root.join(".brain-state")
}
fn queue_file(root: &Path) -> PathBuf {
    state(root).join("git-queue.json")
}
fn status_file(root: &Path) -> PathBuf {
    state(root).join("git-status.json")
}

/// Holds both locks used by note capture and ordinary Brain writes.
pub struct VaultLock {
    _locks: Vec<File>,
}
pub fn acquire_vault_lock(root: &Path) -> Result<VaultLock> {
    let state_dir = state(root);
    if fs::symlink_metadata(&state_dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Symlinked vault state directory".into());
    }
    fs::create_dir_all(&state_dir)?;
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700))?;
    let mut locks = Vec::new();
    for name in ["lock", "capture.lock"] {
        let path = state(root).join(name);
        let mut opts = OpenOptions::new();
        opts.create(true)
            .read(true)
            .append(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW);
        let file = opts.open(path)?;
        let meta = file.metadata()?;
        if !meta.is_file() || meta.nlink() != 1 {
            return Err("Linked or non-regular vault lock".into());
        }
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)?;
        locks.push(file);
    }
    Ok(VaultLock { _locks: locks })
}

fn read_queue(root: &Path) -> Result<Queue> {
    let path = queue_file(root);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Queue::default()),
        Err(error) => return Err(error.into()),
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() || meta.nlink() != 1 => {
            return Err("Unsafe Brain git queue file".into());
        }
        Ok(_) => (),
    }
    serde_json::from_slice(&fs::read(path)?).map_err(|_| "Corrupt Brain git queue JSON".into())
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let temp = path.with_file_name(format!(".git-state-{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path)?;
    Ok(())
}
fn safe_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    if fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_file() || m.nlink() != 1)
    {
        return Err(format!("{}: unsafe-existing-file", path.display()).into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.nlink() != 1 {
        return Err("unsafe Brain git metadata file".into());
    }
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn ensure_safe_existing(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_file() || m.nlink() != 1)
    {
        return Err(format!("{}: unsafe-existing-file", path.display()).into());
    }
    Ok(())
}
fn set_error(root: &Path, message: Option<String>) -> Result<()> {
    let mut status: Status = fs::read(status_file(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    status.error = message;
    write_json(&status_file(root), &status)
}

pub fn status_json(root: &Path) -> Result<String> {
    let mut status: Status = fs::read(status_file(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    status.pending_paths = read_queue(root)?
        .entries
        .into_iter()
        .map(|e| e.path)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if status.last_commit.is_none() && root.join(".git").exists() {
        status.last_commit = success(root, &["rev-parse", "HEAD"]).ok();
    }
    Ok(serde_json::to_string_pretty(&status)?)
}

/// Enqueue a vault-relative path. Paths are coalesced and queue state is private.
pub fn enqueue(root: &Path, path: &Path, agent: &str) -> Result<()> {
    let _lock = acquire_vault_lock(root)?;
    enqueue_locked(root, path, agent)
}
/// Queue a write while the caller already holds `acquire_vault_lock`.
pub fn enqueue_locked(root: &Path, path: &Path, agent: &str) -> Result<()> {
    let rel = path.strip_prefix(root).unwrap_or(path);
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("Brain git queue path must be vault-relative and normal".into());
    }
    let path = rel
        .to_str()
        .ok_or("Brain git paths must be UTF-8")?
        .to_owned();
    if under_forbidden(&path) {
        return Ok(());
    }
    fs::create_dir_all(state(root))?;
    let mut q = read_queue(root)?;
    if let Some(entry) = q.entries.iter_mut().find(|e| e.path == path) {
        entry.agent = agent.to_owned();
    } else {
        q.entries.push(QueuedPath {
            path,
            agent: agent.to_owned(),
        });
    }
    q.updated = Some(chrono::Utc::now().timestamp());
    write_json(&queue_file(root), &q)?;
    let mut status: Status = fs::read(status_file(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    status.conflict = false;
    write_json(&status_file(root), &status)
}

fn ignored() -> &'static str {
    "/credentials/\n/.brain-state/\n/.obsidian/workspace*\n/.obsidian/cache\n**/.pending-*\n**/.note-*\n*.tmp\n"
}
fn under_forbidden(path: &str) -> bool {
    path == "credentials"
        || path.starts_with("credentials/")
        || path == ".brain-state"
        || path.starts_with(".brain-state/")
}
fn path_bytes(root: &Path, path: &str) -> Result<Vec<u8>> {
    let full = root.join(path);
    let mut current = root.to_path_buf();
    let components = Path::new(path).components().collect::<Vec<_>>();
    for component in &components[..components.len().saturating_sub(1)] {
        current.push(component.as_os_str());
        if fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!("{path}: symlink-parent").into());
        }
    }
    let meta = fs::symlink_metadata(&full)?;
    if meta.file_type().is_symlink() {
        return Err(format!("{path}: symlink").into());
    }
    if !meta.is_file() {
        return Err(format!("{path}: non-regular-file").into());
    }
    if meta.len() > LIMIT {
        return Err(format!("{path}: over-5-MB").into());
    }
    Ok(fs::read(full)?)
}
struct RemoveOnDrop(PathBuf);
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn rules() -> Vec<(&'static str, Regex)> {
    [
        ("private-key", r"-----BEGIN (?:OPENSSH |RSA |EC |DSA |PGP |ENCRYPTED )?PRIVATE KEY"),
        ("openai-style-sk", r"\bsk-(?:proj-|svcacct-|ant-)?[A-Za-z0-9_-]{20,}"),
        ("github-token", r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{30,})"),
        ("aws-access-key", r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"),
        ("slack-token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
        ("google-api-key", r"\bAIza[0-9A-Za-z_-]{35}\b"),
        ("jwt", r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}"),
        ("secret-assignment", r"(?im)^\s*(?:password|passwd|pwd|api[_ -]?key|secret|access[_ -]?token|refresh[_ -]?token|client[_ -]?secret)\s*[:=]\s*\S{6,}"),
        ("url-with-credentials", r"\b[a-z][a-z0-9+.-]*://[^\s/:@]+:[^\s/@]{4,}@"),
    ].into_iter().map(|(name, pattern)| (name, Regex::new(pattern).expect("valid secret rule"))).collect()
}
fn scan_text(path: &str, bytes: &[u8]) -> Result<()> {
    let text = String::from_utf8_lossy(bytes);
    for (name, regex) in rules() {
        if regex.is_match(&text) {
            return Err(format!("{path}: {name}").into());
        }
    }
    Ok(())
}

/// Check exactly the index that would be committed, without printing blob data.
pub fn guard_staged(root: &Path) -> Result<()> {
    guard_index(root, None)
}
fn guard_index(root: &Path, index: Option<&Path>) -> Result<()> {
    let mut changed = Command::new("git");
    changed
        .arg("-C")
        .arg(root)
        .args(["diff", "--cached", "--no-renames", "--name-only", "-z"]);
    if let Some(index) = index {
        changed.env("GIT_INDEX_FILE", index);
    }
    let changed = changed.output()?;
    if !changed.status.success() {
        return Err("unable to inspect staged Brain changes".into());
    }
    for path in changed
        .stdout
        .split(|b| *b == 0)
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8_lossy(path);
        if under_forbidden(&path) {
            return Err(format!("{path}: forbidden-path").into());
        }
    }
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--stage", "-z"]);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let listing = command.output()?;
    if !listing.status.success() {
        return Err("unable to inspect staged Brain paths".into());
    }
    for record in listing.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let record = String::from_utf8_lossy(record);
        let Some((head, path)) = record.split_once('\t') else {
            continue;
        };
        if under_forbidden(path) {
            return Err(format!("{path}: forbidden-path").into());
        }
        guard_blob(root, path, head)?;
    }
    Ok(())
}

fn guard_blob(root: &Path, path: &str, stage: &str) -> Result<()> {
    if under_forbidden(path) {
        return Err(format!("{path}: forbidden-path").into());
    }
    let mode = stage.split_whitespace().next().unwrap_or("");
    if mode == "120000" {
        return Err(format!("{path}: symlink").into());
    }
    if !mode.starts_with("100") {
        return Err(format!("{path}: non-regular-file").into());
    }
    let oid = stage.split_whitespace().nth(1).unwrap_or("");
    let size = success(root, &["cat-file", "-s", oid])?
        .parse::<u64>()
        .unwrap_or(u64::MAX);
    if size > LIMIT {
        return Err(format!("{path}: over-5-MB").into());
    }
    let blob = git(root, &["cat-file", "blob", oid])?;
    if !blob.status.success() {
        return Err(format!("{path}: unreadable-staged-blob").into());
    }
    if blob.stdout.len() as u64 > LIMIT {
        return Err(format!("{path}: over-5-MB").into());
    }
    scan_text(path, &blob.stdout)
}

fn guard_index_path(root: &Path, index: &Path, path: &str) -> Result<()> {
    if under_forbidden(path) {
        return Err(format!("{path}: forbidden-path").into());
    }
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--stage", "-z", "--", path])
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_INDEX_FILE", index);
    let listing = command.output()?;
    if !listing.status.success() {
        return Err("unable to inspect staged Brain candidate".into());
    }
    for record in listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|r| !r.is_empty())
    {
        let Some(separator) = record.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let (stage, stored_path) = record.split_at(separator);
        let stored_path = &stored_path[1..];
        let stored_path = String::from_utf8_lossy(stored_path);
        if stored_path == path {
            let stage = String::from_utf8_lossy(stage);
            guard_blob(root, path, &stage)?;
        }
    }
    Ok(())
}

fn reset_index_path(root: &Path, index: &Path, path: &str) -> Result<()> {
    let mut reset = Command::new("git");
    reset
        .arg("-C")
        .arg(root)
        .args(["reset", "--quiet", "HEAD", "--", path])
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_INDEX_FILE", index);
    if !reset.output()?.status.success() {
        return Err(format!("unable to reset private Brain index for {path}").into());
    }
    Ok(())
}

fn porcelain_candidates(root: &Path) -> Result<Vec<Candidate>> {
    let output = git(
        root,
        &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
    )?;
    if !output.status.success() {
        return Err("unable to inspect Brain working tree".into());
    }
    let records = output.stdout.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut result = BTreeMap::<String, Candidate>::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        let mut add = |bytes: &[u8]| {
            let path = String::from_utf8_lossy(bytes).into_owned();
            result.entry(path.clone()).or_insert(Candidate { path });
        };
        match record[0] {
            b'?' => add(record.get(2..).unwrap_or_default()),
            b'1' | b'u' => {
                let fields = if record[0] == b'1' {
                    record.splitn(9, |byte| *byte == b' ').collect::<Vec<_>>()
                } else {
                    record.splitn(11, |byte| *byte == b' ').collect::<Vec<_>>()
                };
                let path_index = if record[0] == b'1' { 8 } else { 10 };
                let Some(path) = fields.get(path_index) else {
                    continue;
                };
                add(path);
            }
            b'2' => {
                let fields = record.splitn(10, |byte| *byte == b' ').collect::<Vec<_>>();
                let Some(path) = fields.get(9) else { continue };
                add(path);
                if index < records.len() {
                    add(records[index]);
                    index += 1;
                }
            }
            _ => (),
        }
    }
    Ok(result.into_values().collect())
}

fn candidate_signature(root: &Path, candidates: &[Candidate]) -> String {
    let mut rows = candidates
        .iter()
        .map(|candidate| {
            let full = root.join(&candidate.path);
            let stamp = fs::symlink_metadata(&full)
                .map(|meta| {
                    format!(
                        "{}:{}:{}:{}:{}:{}:{}",
                        meta.len(),
                        meta.mtime(),
                        meta.mtime_nsec(),
                        meta.ctime(),
                        meta.ctime_nsec(),
                        meta.ino(),
                        meta.mode()
                    )
                })
                .unwrap_or_else(|_| "deleted".to_owned());
            format!("{}\0{}", candidate.path, stamp)
        })
        .collect::<Vec<_>>();
    rows.sort();
    if rows.is_empty() {
        return String::new();
    }
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(rows.join("\n").as_bytes()))
}

fn validate_candidate(root: &Path, path: &str) -> Result<()> {
    if under_forbidden(path) {
        return Err(format!("{path}: forbidden-path").into());
    }
    match fs::symlink_metadata(root.join(path)) {
        Ok(meta) if meta.file_type().is_symlink() => Err(format!("{path}: symlink").into()),
        Ok(meta) if !meta.is_file() => Err(format!("{path}: non-regular-file").into()),
        Ok(_) => {
            let bytes = path_bytes(root, path)?;
            scan_text(path, &bytes)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn is_recent(root: &Path, path: &str) -> bool {
    fs::symlink_metadata(root.join(path))
        .ok()
        .and_then(|meta| {
            let modified = meta.mtime() as i128 * 1_000_000_000 + meta.mtime_nsec() as i128;
            let now = chrono::Utc::now().timestamp_nanos_opt()? as i128;
            Some(now.saturating_sub(modified) < 10_000_000_000)
        })
        .unwrap_or(false)
}

fn blocked_issue(path: &str, error: &str) -> BlockedPath {
    BlockedPath {
        path: path.to_owned(),
        rule: error
            .strip_prefix(&format!("{path}: "))
            .unwrap_or("unsafe-file")
            .to_owned(),
    }
}

pub fn init(root: &Path, remote: Option<&str>) -> Result<()> {
    fs::create_dir_all(root)?;
    let lock = acquire_vault_lock(root)?;
    if fs::symlink_metadata(root.join(".git"))
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir())
    {
        return Err("Vault .git must be a real directory".into());
    }
    if !root.join(".git").exists() {
        success(root, &["init", "-b", "main"])?;
    }
    let ignore = root.join(".gitignore");
    ensure_safe_existing(&ignore)?;
    let mut ignore_text = match fs::read_to_string(&ignore) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    for line in ignored().lines() {
        if !ignore_text.lines().any(|existing| existing == line) {
            ignore_text.push_str(line);
            ignore_text.push('\n');
        }
    }
    safe_write(&ignore, ignore_text.as_bytes(), 0o600)?;
    let hooks = root.join(".githooks");
    if fs::symlink_metadata(&hooks).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir()) {
        return Err("Vault .githooks must be a real directory".into());
    }
    fs::create_dir_all(&hooks)?;
    let hook = hooks.join("pre-commit");
    safe_write(&hook, b"#!/bin/sh\nNACRE_BRAIN=$(git rev-parse --show-toplevel) || exit 1\nexport NACRE_BRAIN\nexec nacre-brain guard --staged\n", 0o755)?;
    success(root, &["config", "core.hooksPath", ".githooks/"])?;
    if let Some(url) = remote {
        let remotes = success(root, &["remote"])?;
        if remotes.lines().any(|r| r == "origin") {
            success(root, &["remote", "set-url", "origin", url])?;
        } else {
            success(root, &["remote", "add", "origin", url])?;
        }
    }
    if fs::symlink_metadata(root.join("raw"))
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir())
    {
        return Err("Vault raw must be a real directory".into());
    }
    if !root.join("raw").exists() {
        fs::create_dir_all(root.join("raw"))?;
    }
    let manifest = root.join("raw/MANIFEST.md");
    ensure_safe_existing(&manifest)?;
    if !manifest.exists() {
        safe_write(
            &manifest,
            b"# Raw files over 5 MiB\n\nsha256 | size | origin\n--- | ---: | ---\n",
            0o600,
        )?;
    }
    // Large raw files stay out of git and receive a committed manifest entry.
    let mut large = Vec::new();
    for entry in walkdir::WalkDir::new(root.join("raw"))
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() || entry.path() == manifest {
            continue;
        }
        let md = entry.metadata()?;
        if md.len() > LIMIT {
            let mut f = File::open(entry.path())?;
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = f.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            let hash = hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            large.push((
                entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .to_string(),
                hash,
                md.len(),
            ));
        }
    }
    let mut manifest_text = fs::read_to_string(&manifest)?;
    for (p, hash, size) in large {
        if p.contains(['\n', '\r']) {
            return Err("Raw filenames cannot contain line breaks".into());
        }
        let escaped = p.chars().fold(String::new(), |mut value, character| {
            if matches!(character, '\\' | '*' | '?' | '[' | ']' | ' ') {
                value.push('\\');
            }
            value.push(character);
            value
        });
        let pattern = format!("/{escaped}");
        let mut ignores = fs::read_to_string(&ignore)?;
        if !ignores.lines().any(|l| l == pattern) {
            ignores.push_str(&format!("{pattern}\n"));
            safe_write(&ignore, ignores.as_bytes(), 0o600)?;
        }
        let row = format!("{hash} | {size} | vault:{p}\n");
        if !manifest_text.contains(&row) {
            manifest_text.push_str(&row);
        }
    }
    safe_write(&manifest, manifest_text.as_bytes(), 0o600)?;
    let staged = git(root, &["ls-files", "--stage", "-z"])?;
    if !staged.status.success() {
        return Err("unable to inspect existing Brain index".into());
    }
    if staged
        .stdout
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .any(|record| {
            String::from_utf8_lossy(record)
                .split_once('\t')
                .is_some_and(|(_, p)| under_forbidden(p))
        })
    {
        drop(lock);
        return Err(
            "Refusing baseline: credentials/ or .brain-state/ paths are already staged".into(),
        );
    }
    if success(root, &["rev-parse", "--verify", "HEAD"]).is_err() {
        success(root, &["add", "-A"])?;
        guard_staged(root)?;
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args([
                "-c",
                "user.name=siverteh",
                "-c",
                "user.email=92440237+Siverteh@users.noreply.github.com",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "-m",
                "baseline import",
            ])
            .env("GIT_AUTHOR_NAME", IDENT_NAME)
            .env("GIT_AUTHOR_EMAIL", IDENT_EMAIL)
            .env("GIT_COMMITTER_NAME", IDENT_NAME)
            .env("GIT_COMMITTER_EMAIL", IDENT_EMAIL)
            .output()?;
        if !out.status.success() {
            return Err("unable to create Brain baseline commit".into());
        }
    }
    let mut status: Status = fs::read(status_file(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if status.last_commit.is_none() {
        status.last_commit = success(root, &["rev-parse", "HEAD"]).ok();
    }
    write_json(&status_file(root), &status)?;
    drop(lock);
    Ok(())
}

fn commit_selected(
    root: &Path,
    selected: &BTreeSet<String>,
    entries: &[QueuedPath],
    sweep: bool,
    blocked: &mut Vec<BlockedPath>,
    handled: &mut BTreeSet<String>,
) -> Result<Option<String>> {
    let head = success(root, &["rev-parse", "HEAD"])?;
    let index = root
        .join(".git")
        .join(format!("brain-index-{}", uuid::Uuid::new_v4()));
    let _index_cleanup = RemoveOnDrop(index.clone());
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(root)
        .args(["read-tree", "HEAD"])
        .env("GIT_INDEX_FILE", &index);
    if !cmd.output()?.status.success() {
        return Err("unable to prepare Brain index".into());
    }
    for path in selected {
        if let Err(error) = validate_candidate(root, path) {
            blocked.push(blocked_issue(path, &error.to_string()));
            continue;
        }
        let full = root.join(path);
        let mut stage = Command::new("git");
        stage
            .arg("-C")
            .arg(root)
            .env("GIT_LITERAL_PATHSPECS", "1")
            .env("GIT_INDEX_FILE", &index);
        if full.exists() || fs::symlink_metadata(&full).is_ok() {
            stage.args(["add", "--", path]);
        } else {
            stage.args(["rm", "--cached", "--ignore-unmatch", "--", path]);
        }
        if !stage.output()?.status.success() {
            blocked.push(BlockedPath {
                path: path.clone(),
                rule: "changed-during-stage".to_owned(),
            });
            continue;
        }
        if is_recent(root, path) {
            reset_index_path(root, &index, path)?;
            continue;
        }
        // Validate the actual staged blob as well, closing the race with writers
        // that do not cooperate with the vault lock.
        if let Err(error) = guard_index_path(root, &index, path) {
            reset_index_path(root, &index, path)?;
            let description = error.to_string();
            if description.starts_with(&format!("{path}: ")) {
                blocked.push(blocked_issue(path, &description));
                continue;
            }
            return Err(error);
        }
        handled.insert(path.clone());
    }
    let tree = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("write-tree")
        .env("GIT_INDEX_FILE", &index)
        .output()?;
    let _ = fs::remove_file(&index);
    if !tree.status.success() {
        return Err("unable to write Brain tree".into());
    }
    let tree = String::from_utf8_lossy(&tree.stdout).trim().to_owned();
    if success(root, &["diff-tree", "--quiet", &head, &tree]).is_ok() {
        return Ok(None);
    }
    let changed = git(
        root,
        &["diff", "--no-renames", "--name-only", "-z", &head, &tree],
    )?;
    if !changed.status.success() {
        return Err("unable to count Brain commit paths".into());
    }
    let changed_paths = changed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect::<BTreeSet<_>>();
    let agents = entries
        .iter()
        .filter(|entry| changed_paths.contains(&entry.path))
        .map(|e| e.agent.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let counts = changed_paths.len();
    let summary = if sweep { "(sweep)" } else { "(queued)" };
    let message = if agents.is_empty() {
        format!("brain: {counts} files {summary}")
    } else {
        format!(
            "brain: {counts} files {summary}\n\nAgent: {}",
            agents.join(", ")
        )
    };
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=siverteh",
            "-c",
            "user.email=92440237+Siverteh@users.noreply.github.com",
            "commit-tree",
            &tree,
            "-p",
            &head,
            "-m",
            &message,
        ])
        .env("GIT_AUTHOR_NAME", IDENT_NAME)
        .env("GIT_AUTHOR_EMAIL", IDENT_EMAIL)
        .env("GIT_COMMITTER_NAME", IDENT_NAME)
        .env("GIT_COMMITTER_EMAIL", IDENT_EMAIL)
        .output()?;
    if !out.status.success() {
        return Err("unable to create Brain commit".into());
    }
    let commit = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let mut status: Status = fs::read(status_file(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    status.owned_commits.push(commit.clone());
    status.last_commit = Some(commit.clone());
    write_json(&status_file(root), &status)?;
    let branch = success(root, &["symbolic-ref", "HEAD"])?;
    success(root, &["update-ref", &branch, &commit, &head])?;
    Ok(Some(commit))
}

#[cfg(test)]
fn commit_paths(root: &Path, entries: &[QueuedPath]) -> Result<Option<String>> {
    let selected = entries.iter().map(|entry| entry.path.clone()).collect();
    commit_selected(
        root,
        &selected,
        entries,
        false,
        &mut Vec::new(),
        &mut BTreeSet::new(),
    )
}

pub fn flush(root: &Path) -> Result<()> {
    if !root.join(".git").exists() {
        return Ok(());
    }
    let _lock = acquire_vault_lock(root)?;
    let mut q = read_queue(root)?;
    let working = porcelain_candidates(root)?;
    let signature = candidate_signature(root, &working);
    let now = chrono::Utc::now().timestamp();
    if signature.is_empty() {
        q.sweep_signature = None;
        q.sweep_updated = None;
    } else if q.sweep_signature.as_deref() != Some(signature.as_str()) {
        q.sweep_signature = Some(signature);
        q.sweep_updated = Some(now);
        write_json(&queue_file(root), &q)?;
        return Ok(());
    }
    if !q.entries.is_empty() && q.updated.is_some_and(|at| now - at < 120) {
        return Ok(());
    }
    if q.sweep_signature.is_some() && q.sweep_updated.is_some_and(|at| now - at < 120) {
        return Ok(());
    }
    let mut initial_status: Status = fs::read(status_file(root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if initial_status.conflict {
        return Ok(());
    }
    let result = (|| -> Result<()> {
        let staged_output = git(
            root,
            &["diff", "--cached", "--no-renames", "--name-only", "-z"],
        )?;
        if !staged_output.status.success() {
            return Err("unable to inspect existing staged paths".into());
        }
        let staged_before = staged_output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| String::from_utf8_lossy(path).into_owned())
            .collect::<BTreeSet<_>>();
        let mut by_path = working
            .iter()
            .map(|candidate| candidate.path.clone())
            .collect::<BTreeSet<_>>();
        let mut blocked = Vec::new();
        let mut selected = BTreeSet::new();
        for entry in &q.entries {
            by_path.insert(entry.path.clone());
        }
        for path in by_path {
            if is_recent(root, &path) {
                continue;
            }
            selected.insert(path);
        }
        let mut status: Status = fs::read(status_file(root))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        status.blocked = blocked.clone();
        status.error = None;
        write_json(&status_file(root), &status)?;
        let attributed = q
            .entries
            .iter()
            .filter(|entry| selected.contains(&entry.path))
            .cloned()
            .collect::<Vec<_>>();
        let mut handled = BTreeSet::new();
        let committed = if selected.is_empty() {
            None
        } else {
            commit_selected(
                root,
                &selected,
                &attributed,
                true,
                &mut blocked,
                &mut handled,
            )?
        };
        let mut status: Status = fs::read(status_file(root))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        status.blocked = blocked;
        write_json(&status_file(root), &status)?;
        // Only advance clean paths in the ordinary index. Paths staged by the
        // owner keep their exact index entries while our private index commits.
        for path in handled.difference(&staged_before) {
            git(root, &["reset", "--quiet", "HEAD", "--", path])?;
        }
        initial_status = fs::read(status_file(root))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if let Some(ref commit) = committed {
            let mut status: Status = fs::read(status_file(root))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            status.last_commit = Some(commit.clone());
            status.error = None;
            write_json(&status_file(root), &status)?;
            // The separate index leaves the owner's staged work intact.
        }
        let ahead = success(root, &["rev-list", "--count", "origin/main..HEAD"])
            .unwrap_or_else(|_| "1".to_owned());
        if ahead == "0" {
            q.entries.retain(|entry| !handled.contains(&entry.path));
            q.updated = (!q.entries.is_empty()).then_some(now);
            write_json(&queue_file(root), &q)?;
            return Ok(());
        }
        let push = git_network(root, &["push", "origin", "HEAD:main"])?;
        if push.status.success() {
            let mut status: Status = fs::read(status_file(root))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            status.last_commit = Some(success(root, &["rev-parse", "HEAD"])?);
            status.last_push = Some(chrono::Utc::now().to_rfc3339());
            status.error = None;
            status.conflict = false;
            status.owned_commits.clear();
            write_json(&status_file(root), &status)?;
            q.entries.retain(|entry| !handled.contains(&entry.path));
            q.updated = (!q.entries.is_empty()).then_some(now);
            write_json(&queue_file(root), &q)?;
            return Ok(());
        }
        let fetch = git_network(root, &["fetch", "origin", "main"])?;
        if !fetch.status.success() {
            return Err("push rejected and fetch failed".into());
        }
        let upstream = success(root, &["rev-parse", "origin/main"])?;
        let ownbase = success(root, &["merge-base", "HEAD", &upstream])?;
        let unpublished = success(root, &["rev-list", &format!("{ownbase}..HEAD")])?;
        if unpublished.lines().any(|hash| {
            !initial_status
                .owned_commits
                .iter()
                .any(|owned| owned == hash)
        }) {
            return Err("push rejected; unpublished commits are not recorded Brain commits".into());
        }
        let branch = success(root, &["symbolic-ref", "--short", "HEAD"])?;
        let rebase = git(
            root,
            &["rebase", "--onto", "origin/main", &ownbase, &branch],
        )?;
        if !rebase.status.success() {
            let _ = git(root, &["rebase", "--abort"]);
            initial_status.conflict = true;
            initial_status.error = Some("push conflict; local commits retained".into());
            let _ = write_json(&status_file(root), &initial_status);
            return Err("push conflict; local commits retained".into());
        }
        initial_status.owned_commits = success(root, &["rev-list", "origin/main..HEAD"])?
            .lines()
            .map(str::to_owned)
            .collect();
        write_json(&status_file(root), &initial_status)?;
        let retry = git_network(root, &["push", "origin", "HEAD:main"])?;
        if !retry.status.success() {
            return Err("push retry failed".into());
        }
        let mut status: Status = fs::read(status_file(root))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        status.last_commit = Some(success(root, &["rev-parse", "HEAD"])?);
        status.last_push = Some(chrono::Utc::now().to_rfc3339());
        status.error = None;
        status.conflict = false;
        status.owned_commits.clear();
        write_json(&status_file(root), &status)?;
        q.entries.retain(|entry| !handled.contains(&entry.path));
        q.updated = (!q.entries.is_empty()).then_some(now);
        write_json(&queue_file(root), &q)?;
        Ok(())
    })();
    if let Err(ref e) = result {
        let _ = set_error(root, Some(e.to_string()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn command(root: &Path, args: &[&str]) -> Output {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap()
    }
    fn checked(root: &Path, args: &[&str]) -> String {
        let output = command(root, args);
        assert!(output.status.success(), "git command failed: {:?}", args);
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
    fn remote_vault(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
        let root = temp.path().join("vault");
        let bare = temp.path().join("remote.git");
        let peer = temp.path().join("peer");
        fs::create_dir_all(&root).unwrap();
        assert!(Command::new("git")
            .args(["init", "--bare", "-b", "main"])
            .arg(&bare)
            .output()
            .unwrap()
            .status
            .success());
        fs::write(root.join("shared.md"), "base\n").unwrap();
        init(&root, Some(bare.to_str().unwrap())).unwrap();
        checked(&root, &["push", "-u", "origin", "main"]);
        assert!(Command::new("git")
            .arg("--git-dir")
            .arg(&bare)
            .args(["symbolic-ref", "HEAD", "refs/heads/main"])
            .output()
            .unwrap()
            .status
            .success());
        (root, bare, peer)
    }
    fn clone_remote(bare: &Path, peer: &Path) {
        assert!(Command::new("git")
            .arg("clone")
            .arg(bare)
            .arg(peer)
            .output()
            .unwrap()
            .status
            .success());
    }
    fn peer_commit(peer: &Path, path: &str, contents: &str) {
        fs::write(peer.join(path), contents).unwrap();
        assert!(command(peer, &["add", "--", path]).status.success());
        assert!(command(
            peer,
            &[
                "-c",
                "user.name=peer",
                "-c",
                "user.email=peer@example.invalid",
                "commit",
                "-m",
                "peer edit"
            ]
        )
        .status
        .success());
        assert!(command(peer, &["push", "origin", "main"]).status.success());
    }
    fn enqueue_ready(root: &Path, path: &str, contents: &str) {
        fs::write(root.join(path), contents).unwrap();
        set_old_mtime(&root.join(path));
        enqueue(root, &root.join(path), "codex:test").unwrap();
        let mut queue = read_queue(root).unwrap();
        queue.updated = Some(chrono::Utc::now().timestamp() - 121);
        write_json(&queue_file(root), &queue).unwrap();
        sweep_ready(root);
    }
    fn set_old_mtime(path: &Path) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - Duration::from_secs(15))
            .unwrap();
    }
    fn sweep_ready(root: &Path) {
        let candidates = porcelain_candidates(root).unwrap();
        let queue = read_queue(root).unwrap();
        let mut queue = queue;
        queue.sweep_signature = Some(candidate_signature(root, &candidates));
        queue.sweep_updated = Some(chrono::Utc::now().timestamp() - 121);
        write_json(&queue_file(root), &queue).unwrap();
    }
    fn vault() -> (TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("vault");
        fs::create_dir_all(&root).unwrap();
        (t, root)
    }
    #[test]
    fn init_is_idempotent_and_excludes_private_state() {
        let (_t, root) = vault();
        fs::create_dir_all(root.join("credentials")).unwrap();
        fs::write(root.join("credentials/x"), "x").unwrap();
        fs::write(root.join("note.md"), "safe").unwrap();
        init(&root, None).unwrap();
        init(&root, None).unwrap();
        let files = success(&root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap();
        assert!(files.contains("note.md"));
        assert!(!files.contains("credentials/"));
        assert!(!files.contains(".brain-state"));
    }
    #[test]
    fn init_refuses_forced_staged_forbidden_paths() {
        for path in ["credentials/key", ".brain-state/private"] {
            let (_temp, root) = vault();
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            fs::write(root.join(path), "private").unwrap();
            checked(&root, &["init", "-b", "main"]);
            checked(&root, &["add", "-f", path]);
            assert!(init(&root, None)
                .unwrap_err()
                .to_string()
                .contains("paths are already staged"));
        }
    }
    #[test]
    fn init_manifests_and_ignores_large_raw_files() {
        let (_temp, root) = vault();
        fs::create_dir_all(root.join("raw")).unwrap();
        fs::write(root.join("raw/source.bin"), vec![7u8; LIMIT as usize + 1]).unwrap();
        init(&root, None).unwrap();
        let files = success(&root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap();
        assert!(!files.contains("raw/source.bin"));
        assert!(files.contains("raw/MANIFEST.md"));
        let manifest = fs::read_to_string(root.join("raw/MANIFEST.md")).unwrap();
        assert!(manifest.contains("raw/source.bin"));
        assert!(fs::read_to_string(root.join(".gitignore"))
            .unwrap()
            .contains("/raw/source.bin"));
    }
    #[test]
    fn guard_rejects_deleted_forbidden_paths() {
        let (_temp, root) = vault();
        init(&root, None).unwrap();
        fs::create_dir_all(root.join("credentials")).unwrap();
        fs::write(root.join("credentials/reference"), "safe reference").unwrap();
        success(&root, &["add", "-f", "credentials/reference"]).unwrap();
        success(
            &root,
            &["-c", "core.hooksPath=/dev/null", "commit", "-m", "fixture"],
        )
        .unwrap();
        fs::remove_file(root.join("credentials/reference")).unwrap();
        success(&root, &["add", "-u"]).unwrap();
        assert!(guard_staged(&root)
            .unwrap_err()
            .to_string()
            .contains("forbidden-path"));
    }
    #[test]
    fn queued_paths_are_literal_and_preserve_unrelated_files() {
        let (_temp, root) = vault();
        init(&root, None).unwrap();
        fs::create_dir_all(root.join("inbox")).unwrap();
        fs::write(root.join("inbox/[a].md"), "queued").unwrap();
        set_old_mtime(&root.join("inbox/[a].md"));
        fs::write(root.join("inbox/a.md"), "unrelated").unwrap();
        commit_paths(
            &root,
            &[QueuedPath {
                path: "inbox/[a].md".into(),
                agent: "test".into(),
            }],
        )
        .unwrap();
        assert_eq!(
            success(&root, &["show", "HEAD:inbox/[a].md"]).unwrap(),
            "queued"
        );
        assert!(success(&root, &["show", "HEAD:inbox/a.md"]).is_err());
    }
    #[test]
    fn queue_preserves_literal_backslashes_in_unix_filenames() {
        let (_temp, root) = vault();
        init(&root, None).unwrap();
        fs::create_dir_all(root.join("wiki")).unwrap();
        let path = "wiki/back\\slash.md";
        fs::write(root.join(path), "queued").unwrap();
        set_old_mtime(&root.join(path));
        enqueue(&root, &root.join(path), "test").unwrap();
        let queue = read_queue(&root).unwrap();
        assert_eq!(queue.entries[0].path, path);
        commit_paths(&root, &queue.entries).unwrap();
        assert_eq!(
            success(&root, &["show", &format!("HEAD:{path}")]).unwrap(),
            "queued"
        );
    }
    #[test]
    fn guard_rejects_secret_without_echoing_it() {
        let (_t, root) = vault();
        init(&root, None).unwrap();
        let fake = ["sk-", "proj-", "abcdefghijklmnopqrstuv"].concat();
        fs::write(root.join("bad.md"), fake).unwrap();
        success(&root, &["add", "bad.md"]).unwrap();
        let error = guard_staged(&root).unwrap_err().to_string();
        assert!(error.contains("openai-style-sk"));
        assert!(!error.contains("abcdefghijklmnopqrstuv"));
    }
    #[test]
    fn staged_guard_covers_all_secret_rules() {
        let (_t, root) = vault();
        init(&root, None).unwrap();
        let fixtures = [
            (
                "private-key",
                ["-----BEGIN ", "RSA ", "PRIVATE KEY"].concat(),
            ),
            (
                "openai-style-sk",
                ["sk-", "proj-", "abcdefghijklmnopqrstuv"].concat(),
            ),
            (
                "github-token",
                ["gh", "p_", "abcdefghijklmnopqrstuv"].concat(),
            ),
            ("aws-access-key", ["AKIA", "ABCDEFGHIJKLMNOP"].concat()),
            ("slack-token", ["xox", "b-", "abcdefghijklm"].concat()),
            ("google-api-key", format!("AIza{}", "A".repeat(35))),
            (
                "jwt",
                [
                    "eyJ",
                    "abcdefghijklm",
                    ".eyJ",
                    "abcdefghijklm",
                    ".abcdefghijklm",
                ]
                .concat(),
            ),
            (
                "secret-assignment",
                ["password", "= ", "abcdefghi"].concat(),
            ),
            (
                "url-with-credentials",
                ["https://user", ":", "abcdef@host"].concat(),
            ),
        ];
        for (i, (rule, value)) in fixtures.into_iter().enumerate() {
            let name = format!("rule-{i}.md");
            fs::write(root.join(&name), value).unwrap();
            success(&root, &["add", &name]).unwrap();
            let error = guard_staged(&root).unwrap_err().to_string();
            assert!(error.contains(rule), "expected {rule}, got {error}");
            success(&root, &["reset", "--", &name]).unwrap();
        }
    }
    #[test]
    fn rejects_forbidden_staged_paths_and_large_files() {
        let (_t, root) = vault();
        init(&root, None).unwrap();
        fs::create_dir_all(root.join("credentials")).unwrap();
        fs::write(root.join("credentials/x"), "safe").unwrap();
        success(&root, &["add", "-f", "credentials/x"]).unwrap();
        assert!(guard_staged(&root)
            .unwrap_err()
            .to_string()
            .contains("forbidden-path"));
        success(&root, &["reset"]).unwrap();
        fs::write(root.join("large.md"), vec![b'x'; LIMIT as usize + 1]).unwrap();
        success(&root, &["add", "large.md"]).unwrap();
        assert!(guard_staged(&root)
            .unwrap_err()
            .to_string()
            .contains("over-5-MB"));
    }
    #[test]
    fn guard_rejects_staged_symlink() {
        use std::os::unix::fs::symlink;
        let (_t, root) = vault();
        init(&root, None).unwrap();
        fs::write(root.join("target.md"), "safe").unwrap();
        symlink("target.md", root.join("link.md")).unwrap();
        success(&root, &["add", "link.md"]).unwrap();
        assert!(guard_staged(&root)
            .unwrap_err()
            .to_string()
            .contains("symlink"));
    }
    #[test]
    fn remote_rejection_rebases_only_our_queued_commit() {
        let temp = tempfile::tempdir().unwrap();
        let (root, bare, peer) = remote_vault(&temp);
        clone_remote(&bare, &peer);
        peer_commit(&peer, "peer.md", "peer\n");
        enqueue_ready(&root, "local.md", "local\n");
        flush(&root).unwrap();
        assert!(checked(&root, &["show", "origin/main:peer.md"]).contains("peer"));
        assert!(checked(&root, &["show", "origin/main:local.md"]).contains("local"));
        let status: serde_json::Value = serde_json::from_str(&status_json(&root).unwrap()).unwrap();
        assert!(status["error"].is_null());
    }

    #[test]
    fn sweep_commits_unqueued_owner_edits_additions_and_deletions() {
        let temp = tempfile::tempdir().unwrap();
        let (root, bare, _) = remote_vault(&temp);
        fs::write(root.join("shared.md"), "owner edit\n").unwrap();
        set_old_mtime(&root.join("shared.md"));
        fs::write(root.join("old-writer.md"), "old agent\n").unwrap();
        set_old_mtime(&root.join("old-writer.md"));
        fs::write(root.join("to-delete.md"), "remove me\n").unwrap();
        set_old_mtime(&root.join("to-delete.md"));
        success(&root, &["add", "to-delete.md"]).unwrap();
        success(
            &root,
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        )
        .unwrap();
        success(&root, &["push", "origin", "HEAD:main"]).unwrap();
        fs::remove_file(root.join("to-delete.md")).unwrap();
        sweep_ready(&root);
        flush(&root).unwrap();
        let files = checked(&root, &["ls-tree", "-r", "--name-only", "origin/main"]);
        assert!(files.contains("old-writer.md"));
        assert!(files.contains("shared.md"));
        assert!(!files.contains("to-delete.md"));
        assert_eq!(
            checked(&root, &["show", "origin/main:shared.md"]),
            "owner edit"
        );
        assert_eq!(
            checked(&root, &["show", "origin/main:old-writer.md"]),
            "old agent"
        );
        assert!(checked(&root, &["log", "-1", "--format=%s", "origin/main"]).contains("(sweep)"));
        assert!(checked(&root, &["status", "--porcelain"]).is_empty());
        fs::write(root.join("old-writer.md"), "later owner edit\n").unwrap();
        set_old_mtime(&root.join("old-writer.md"));
        sweep_ready(&root);
        flush(&root).unwrap();
        assert_eq!(
            checked(&root, &["show", "origin/main:old-writer.md"]),
            "later owner edit"
        );
        assert!(Command::new("git")
            .arg("--git-dir")
            .arg(&bare)
            .args(["cat-file", "-e", "refs/heads/main:old-writer.md"])
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn sweep_blocks_secret_and_commits_other_safe_files() {
        let temp = tempfile::tempdir().unwrap();
        let (root, _, _) = remote_vault(&temp);
        let secret = ["sk-", "proj-", "abcdefghijklmnopqrstuv"].concat();
        fs::write(root.join("blocked.md"), &secret).unwrap();
        set_old_mtime(&root.join("blocked.md"));
        fs::write(root.join("safe.md"), "safe note\n").unwrap();
        set_old_mtime(&root.join("safe.md"));
        sweep_ready(&root);
        flush(&root).unwrap();
        assert_eq!(
            checked(&root, &["show", "origin/main:safe.md"]),
            "safe note"
        );
        assert!(success(&root, &["show", "origin/main:blocked.md"]).is_err());
        let status: serde_json::Value = serde_json::from_str(&status_json(&root).unwrap()).unwrap();
        assert_eq!(status["blocked"][0]["path"], "blocked.md");
        assert_eq!(status["blocked"][0]["rule"], "openai-style-sk");
        assert!(!status.to_string().contains(&secret));
    }

    #[test]
    fn sweep_waits_for_recent_file_then_commits_it() {
        let temp = tempfile::tempdir().unwrap();
        let (root, _, _) = remote_vault(&temp);
        fs::write(root.join("writing.md"), "still writing\n").unwrap();
        sweep_ready(&root);
        let before = checked(&root, &["rev-parse", "HEAD"]);
        flush(&root).unwrap();
        assert_eq!(checked(&root, &["rev-parse", "HEAD"]), before);
        set_old_mtime(&root.join("writing.md"));
        sweep_ready(&root);
        flush(&root).unwrap();
        assert_eq!(
            checked(&root, &["show", "HEAD:writing.md"]),
            "still writing"
        );
    }

    #[test]
    fn sweep_preserves_the_owners_staged_index_and_handles_nul_paths() {
        let temp = tempfile::tempdir().unwrap();
        let (root, _, _) = remote_vault(&temp);
        success(&root, &["mv", "shared.md", "staged.md"]).unwrap();
        fs::write(root.join("staged.md"), "owner staged\n").unwrap();
        success(&root, &["add", "staged.md"]).unwrap();
        fs::write(root.join("staged.md"), "working tree version\n").unwrap();
        set_old_mtime(&root.join("staged.md"));
        fs::write(root.join("shared.md"), "recreated after staged rename\n").unwrap();
        set_old_mtime(&root.join("shared.md"));
        fs::write(root.join("working.md"), "sweep me\n").unwrap();
        set_old_mtime(&root.join("working.md"));
        let odd = "strange name\nwith-tab\t.md";
        fs::write(root.join(odd), "odd path\n").unwrap();
        set_old_mtime(&root.join(odd));
        sweep_ready(&root);
        flush(&root).unwrap();
        let staged = checked(&root, &["diff", "--cached", "--name-only"]);
        assert!(staged.contains("staged.md"));
        assert!(!staged.contains("working.md"));
        assert_eq!(
            checked(&root, &["show", &format!("HEAD:{odd}")]),
            "odd path"
        );
        assert_eq!(checked(&root, &["show", "HEAD:working.md"]), "sweep me");
        assert_eq!(
            checked(&root, &["show", "HEAD:staged.md"]),
            "working tree version"
        );
        assert_eq!(checked(&root, &["show", ":staged.md"]), "owner staged");
        assert!(success(&root, &["show", ":shared.md"]).is_err());
        assert_eq!(
            checked(&root, &["show", "HEAD:shared.md"]),
            "recreated after staged rename"
        );
    }

    #[test]
    fn sweep_parses_rename_records_and_commits_both_names() {
        let temp = tempfile::tempdir().unwrap();
        let (root, _, _) = remote_vault(&temp);
        success(&root, &["mv", "shared.md", "renamed note.md"]).unwrap();
        set_old_mtime(&root.join("renamed note.md"));
        let candidates = porcelain_candidates(&root).unwrap();
        assert!(candidates
            .iter()
            .any(|candidate| candidate.path == "shared.md"));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.path == "renamed note.md"));
        sweep_ready(&root);
        flush(&root).unwrap();
        assert_eq!(
            checked(&root, &["show", "origin/main:renamed note.md"]),
            "base"
        );
        assert!(success(&root, &["show", "origin/main:shared.md"]).is_err());
    }

    #[test]
    fn sweep_snapshot_resets_when_same_size_note_changes() {
        let temp = tempfile::tempdir().unwrap();
        let (root, _, _) = remote_vault(&temp);
        fs::write(root.join("owner.md"), "first\n").unwrap();
        set_old_mtime(&root.join("owner.md"));
        flush(&root).unwrap();
        let before = checked(&root, &["rev-parse", "HEAD"]);
        let mut queue = read_queue(&root).unwrap();
        queue.sweep_updated = Some(0);
        write_json(&queue_file(&root), &queue).unwrap();
        fs::write(root.join("owner.md"), "other\n").unwrap();
        set_old_mtime(&root.join("owner.md"));
        flush(&root).unwrap();
        assert_eq!(checked(&root, &["rev-parse", "HEAD"]), before);
        sweep_ready(&root);
        flush(&root).unwrap();
        assert_eq!(checked(&root, &["show", "HEAD:owner.md"]), "other");
    }
    #[test]
    fn conflicting_remote_edit_aborts_and_records_status() {
        let temp = tempfile::tempdir().unwrap();
        let (root, bare, peer) = remote_vault(&temp);
        clone_remote(&bare, &peer);
        peer_commit(&peer, "shared.md", "peer edit\n");
        enqueue_ready(&root, "shared.md", "local edit\n");
        assert!(flush(&root).is_err());
        assert_eq!(checked(&root, &["show", "HEAD:shared.md"]), "local edit");
        assert!(checked(&root, &["show", "origin/main:shared.md"]).contains("peer edit"));
        let status: serde_json::Value = serde_json::from_str(&status_json(&root).unwrap()).unwrap();
        assert_eq!(status["conflict"], true);
        assert!(status["error"].as_str().unwrap().contains("conflict"));
    }
}
