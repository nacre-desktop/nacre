//! Small AI command wrappers retained outside the chat and dashboard clients.
use crate::{ai_core, Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{Cursor, Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};

const SOURCES: &str = include_str!("../../../ai/skill-sources.json");
const MAX_ARCHIVE: usize = 16 * 1024 * 1024;
const MAX_ENTRY: u64 = 4 * 1024 * 1024;
const MAX_EXPANDED: u64 = 32 * 1024 * 1024;
const MAX_FILES: usize = 4096;

fn home() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}
fn err(message: impl Into<String>) -> Error {
    message.into().into()
}
fn valid_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && value.len() <= 48
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}
fn valid_project_id(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}
pub fn tools(args: &[String]) -> Result<i32> {
    let Some(tool) = args.first() else {
        return Err(err(
            "Usage: nacre-ai-tools python|node|defuddle [arguments...]",
        ));
    };
    let root = home()?.join(".local/share/nacre/ai/skill-tools");
    let mut paths = vec![
        root.join("python/bin"),
        root.join("node/bin"),
        root.join("node_modules/.bin"),
    ];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let path = env::join_paths(paths)?;
    let mut command = Command::new(tool);
    command
        .args(&args[1..])
        .env("PATH", path)
        .env("NODE_PATH", root.join("node_modules"));
    use std::os::unix::process::CommandExt;
    Err(command.exec().into())
}

#[derive(Deserialize)]
struct SkillSource {
    repo: String,
    revision: String,
    sha256: String,
    skills: Vec<String>,
}

fn validate_source(source: &SkillSource) -> Result<()> {
    let mut repo = source.repo.split('/');
    let part = |value: Option<&str>| {
        value.is_some_and(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    };
    if !part(repo.next())
        || !part(repo.next())
        || repo.next().is_some()
        || source.revision.len() != 40
        || !source.revision.bytes().all(|b| b.is_ascii_hexdigit())
        || source.sha256.len() != 64
        || !source.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || source.skills.is_empty()
        || source.skills.iter().any(|name| !part(Some(name)))
        || source
            .skills
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != source.skills.len()
    {
        return Err(err("Invalid pinned skill source metadata"));
    }
    Ok(())
}

fn sources(repo: &Path, allow_embedded: bool) -> Result<Vec<SkillSource>> {
    let path = repo.join("ai/skill-sources.json");
    let data = if path.is_file() {
        fs::read(path)?
    } else if allow_embedded {
        SOURCES.as_bytes().to_vec()
    } else {
        return Err(err("Skill source metadata is missing"));
    };
    let sources: Vec<SkillSource> = serde_json::from_slice(&data)?;
    for source in &sources {
        validate_source(source)?;
    }
    Ok(sources)
}

fn fetch_archive(source: &SkillSource) -> Result<Vec<u8>> {
    let url = format!(
        "https://codeload.github.com/{}/zip/{}",
        source.repo, source.revision
    );
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .new_agent();
    let response = agent.get(&url).header("User-Agent", "nacre-ai").call()?;
    let bytes = response
        .into_body()
        .with_config()
        .limit(MAX_ARCHIVE as u64 + 1)
        .read_to_vec()?;
    verify_archive(&bytes, source)?;
    Ok(bytes)
}

fn verify_archive(bytes: &[u8], source: &SkillSource) -> Result<()> {
    if bytes.len() > MAX_ARCHIVE {
        return Err(err("Skill archive exceeds download size limit"));
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    if digest != source.sha256 {
        return Err(err("Skill archive digest did not match pinned SHA-256"));
    }
    Ok(())
}

fn copy_limited<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    declared_size: u64,
    expanded: &mut u64,
    max_entry: u64,
    max_total: u64,
) -> Result<()> {
    let total_limit = max_total.saturating_sub(*expanded);
    let allowed = max_entry.min(total_limit);
    if declared_size > allowed {
        return Err(err("Skill archive exceeds extraction size limit"));
    }
    let mut bytes = Vec::new();
    let written = std::io::copy(&mut reader.take(allowed.saturating_add(1)), &mut bytes)?;
    if written > allowed || written != declared_size {
        return Err(err(
            "Skill archive entry size did not match its declaration",
        ));
    }
    writer.write_all(&bytes)?;
    *expanded = (*expanded)
        .checked_add(written)
        .ok_or("Skill archive size overflow")?;
    Ok(())
}

fn safe_relative(name: &str) -> Result<PathBuf> {
    let path = Path::new(name);
    if name.is_empty()
        || name.contains('\\')
        || name.contains('\0')
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(err("Unsafe archive path"));
    }
    Ok(path.to_path_buf())
}

fn install_source(source: &SkillSource, home: &Path) -> Result<PathBuf> {
    validate_source(source)?;
    let library = home.join(".local/share/nacre/ai/skill-library");
    let canonical_home = home.canonicalize()?;
    let mut checked = canonical_home.clone();
    for part in [".local", "share", "nacre", "ai", "skill-library"] {
        checked.push(part);
        match fs::symlink_metadata(&checked) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(err(
                    "Skill library path contains a non-directory or symbolic link",
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&checked) {
                    Ok(()) => fs::set_permissions(&checked, fs::Permissions::from_mode(0o700))?,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                let metadata = fs::symlink_metadata(&checked)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(err("Skill library path contains a symbolic link"));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    let canonical_library = library.canonicalize()?;
    if !canonical_library.starts_with(&canonical_home) {
        return Err(err("Skill library escapes the home directory"));
    }
    let release = library.join(format!(
        "{}-{}",
        source.repo.replace('/', "-"),
        source.revision
    ));
    if fs::symlink_metadata(&release).is_ok() {
        let metadata = fs::symlink_metadata(&release)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(err("Cached skill release is not a directory"));
        }
        for name in &source.skills {
            let skill = release.join(name);
            let skill_meta = fs::symlink_metadata(&skill)?;
            let marker = fs::symlink_metadata(skill.join("SKILL.md"))?;
            if skill_meta.file_type().is_symlink()
                || !skill_meta.is_dir()
                || marker.file_type().is_symlink()
                || !marker.is_file()
            {
                return Err(err("Cached skill release contains an unsafe path"));
            }
        }
        return Ok(release);
    }
    let bytes = fetch_archive(source)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > MAX_FILES {
        return Err(err("Skill archive has too many entries"));
    }
    let first = archive.file_names().next().ok_or("Empty skill archive")??;
    let root_name = first
        .split('/')
        .next()
        .ok_or("Invalid skill archive root")?;
    if root_name.is_empty()
        || root_name.contains('\\')
        || root_name.contains('\0')
        || Path::new(root_name)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(err("Invalid skill archive root"));
    }
    let prefix = root_name.to_owned() + "/";
    let staging = tempfile::Builder::new()
        .prefix("skill-release-")
        .tempdir_in(&library)?;
    let stage = staging.path().join("release");
    fs::create_dir(&stage)?;
    let mut expanded = 0u64;
    let mut found = std::collections::BTreeSet::new();
    for index in 0..archive.len() {
        let mut member = archive.by_index(index)?;
        let raw = member.name()?.into_owned();
        let relative_root = raw.strip_prefix(&prefix).ok_or("Unexpected archive root")?;
        if member.is_dir() {
            continue;
        }
        let declared_size = member.size();
        if declared_size > MAX_ENTRY {
            return Err(err("Skill archive entry exceeds size limit"));
        }
        let path = safe_relative(relative_root)?;
        if member
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(err("Skill archive contains a symbolic link"));
        }
        for skill in &source.skills {
            let archive_lead = Path::new("skills").join(skill);
            if path.starts_with(&archive_lead) {
                let exposed_path = path.strip_prefix("skills")?;
                let target = stage.join(exposed_path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)?;
                copy_limited(
                    &mut member,
                    &mut output,
                    declared_size,
                    &mut expanded,
                    MAX_ENTRY,
                    MAX_EXPANDED,
                )?;
                output.sync_all()?;
                if path == archive_lead.join("SKILL.md") {
                    found.insert(skill.clone());
                }
                break;
            }
            if path.components().count() == 1
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().to_uppercase().contains("LICENSE"))
            {
                let target = stage.join(path.file_name().ok_or("Invalid license path")?);
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(target)?;
                copy_limited(
                    &mut member,
                    &mut output,
                    declared_size,
                    &mut expanded,
                    MAX_ENTRY,
                    MAX_EXPANDED,
                )?;
                break;
            }
        }
    }
    for name in &source.skills {
        if !found.contains(name) {
            return Err(err(format!("Missing skill {name}")));
        }
    }
    match fs::rename(&stage, &release) {
        Ok(()) => Ok(release),
        Err(_error) if release.is_dir() => Ok(release),
        Err(error) => Err(error.into()),
    }
}

fn expose(source: &Path, destination: &Path, owned_library: Option<&Path>) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    let mut checked = PathBuf::new();
    for component in destination.components() {
        checked.push(component.as_os_str());
        match fs::symlink_metadata(&checked) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => {
                return Err(err(
                    "Skill destination path contains a non-directory or symbolic link",
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&checked) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                let metadata = fs::symlink_metadata(&checked)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(err("Skill destination path contains a symbolic link"));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        let name = entry.file_name();
        let target = destination.join(&name);
        let skill = entry.path();
        if fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
            let existing = fs::read_link(&target)?;
            let resolved = if existing.is_absolute() {
                existing
            } else {
                target.parent().unwrap_or(destination).join(existing)
            };
            if resolved.canonicalize().ok() == skill.canonicalize().ok() {
                continue;
            }
            if let Some(library) = owned_library {
                if resolved
                    .canonicalize()
                    .ok()
                    .zip(library.canonicalize().ok())
                    .is_some_and(|(r, l)| r.starts_with(l))
                {
                    let pending = destination.join(format!(
                        ".{}.{}",
                        name.to_string_lossy(),
                        uuid::Uuid::new_v4()
                    ));
                    std::os::unix::fs::symlink(&skill, &pending)?;
                    fs::rename(pending, target)?;
                    continue;
                }
            }
        }
        if fs::symlink_metadata(&target).is_ok() {
            return Err(err(format!(
                "Existing skill preserved; resolve conflict: {}",
                target.display()
            )));
        }
        std::os::unix::fs::symlink(&skill, target)?;
    }
    Ok(())
}

fn skill_usage(args: &[String]) -> Result<i32> {
    let mut install = false;
    let mut repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let mut claude_home = None;
    let mut codex_home = None;
    let mut repo_supplied = false;
    let mut i = 0;
    while i < args.len() {
        let (option, inline_value) = args[i]
            .split_once('=')
            .map_or((args[i].as_str(), None), |(a, b)| (a, Some(b)));
        match option {
            "--install" if inline_value.is_none() => install = true,
            "--repo" | "--claude-home" | "--codex-home" => {
                let value = if let Some(value) = inline_value {
                    value
                } else if args.get(i + 1).is_some_and(|value| !value.starts_with('-')) {
                    i += 1;
                    &args[i]
                } else {
                    eprintln!("usage: nacre-ai-skills [-h] [--install] [--repo REPO]\n                       [--claude-home CLAUDE_HOME] [--codex-home CODEX_HOME]");
                    eprintln!("nacre-ai-skills: error: argument {option}: expected one argument");
                    return Ok(2);
                };
                match option {
                    "--repo" => {
                        repo = PathBuf::from(value);
                        repo_supplied = true;
                    }
                    "--claude-home" => claude_home = Some(PathBuf::from(value)),
                    _ => codex_home = Some(PathBuf::from(value)),
                }
            }
            "-h" | "--help" => {
                print!("usage: nacre-ai-skills [-h] [--install] [--repo REPO]\n                       [--claude-home CLAUDE_HOME] [--codex-home CODEX_HOME]\n\nInstall pinned shared skills, or expose the existing library to an account.\n\noptions:\n  -h, --help            show this help message and exit\n  --install\n  --repo REPO\n  --claude-home CLAUDE_HOME\n  --codex-home CODEX_HOME\n");
                return Ok(0);
            }
            _ => {
                eprintln!("usage: nacre-ai-skills [-h] [--install] [--repo REPO]\n                       [--claude-home CLAUDE_HOME] [--codex-home CODEX_HOME]");
                eprintln!("nacre-ai-skills: error: invalid or unsupported argument");
                return Ok(2);
            }
        }
        i += 1;
    }
    let home = home()?;
    if install {
        let sources = sources(&repo, !repo_supplied)?;
        let library = home.join(".local/share/nacre/ai/skill-library");
        let total_skills = sources
            .iter()
            .map(|value| value.skills.len())
            .sum::<usize>();
        for source in sources {
            let release = install_source(&source, &home)?;
            expose(&release, &home.join(".agents/skills"), Some(&library))?;
        }
        let revisions = repo.join("ai/skill-sources.json");
        let revision_source = if revisions.is_file() {
            revisions.display().to_string()
        } else {
            "embedded pinned catalog".into()
        };
        println!(
            "Installed {total_skills} pinned upstream skills; source revisions: {revision_source}"
        );
    }
    let source = home.join(".agents/skills");
    if !source.exists() {
        return Ok(0);
    }
    let mut targets = Vec::new();
    if let Some(path) = claude_home {
        targets.push(path);
    }
    if let Some(path) = codex_home {
        targets.push(path);
    }
    if targets.is_empty() {
        targets.extend([home.join(".claude"), home.join(".codex")]);
        for group in ["accounts", "claude-accounts"] {
            let parent = home.join(".local/share/nacre/ai").join(group);
            if parent.is_dir() {
                targets.extend(
                    fs::read_dir(parent)?
                        .filter_map(|entry| entry.ok())
                        .map(|e| e.path())
                        .filter(|p| p.is_dir()),
                );
            }
        }
    }
    for target in targets {
        expose(&source, &target.join("skills"), None)?;
    }
    Ok(0)
}

pub fn skills(args: &[String]) -> Result<i32> {
    skill_usage(args)
}

pub fn account(args: &[String]) -> Result<i32> {
    let Some(account) = args.first() else {
        return Err(err("Usage: nacre-ai-account ACCOUNT [Codex arguments...]"));
    };
    if !valid_name(account) {
        return Err(err("Usage: nacre-ai-account ACCOUNT [Codex arguments...]"));
    }
    let profile = ai_core::account_home("codex", Some(account))?;
    let home = home()?;
    let _ = skills(&[
        "--codex-home".into(),
        profile.to_string_lossy().into_owned(),
    ])?;
    let chat = home.join(".local/bin/nacre-ai-chat");
    ai_core::prepare_provider_launch();
    if !Command::new(chat)
        .arg("configure")
        .env("CODEX_HOME", &profile)
        .status()?
        .success()
    {
        return Err(err("Could not configure account-scoped Codex history"));
    }
    let mut command = Command::new("codex");
    command.args(&args[1..]).env("CODEX_HOME", profile);
    ai_core::prepare_provider_launch();
    use std::os::unix::process::CommandExt;
    Err(command.exec().into())
}

fn configured_codex_policy_at(home: &Path) -> &'static str {
    let text = match ai_core::read_codex_config(&home.join(".codex")) {
        Ok(Some(text)) => text,
        Ok(None) => return "file (Codex upstream default)",
        Err(_) => return "unknown (config unreadable)",
    };
    let Ok(config) = toml::from_str::<toml::Value>(&text) else {
        return "unknown (config invalid)";
    };
    let mode = config
        .get("cli_auth_credentials_store")
        .and_then(toml::Value::as_str);
    match mode {
        None => "file (Codex upstream default)",
        Some("file") => "file",
        Some("keyring") => "keyring (fails if unavailable)",
        Some("auto") => "auto (keyring preferred; file fallback)",
        Some("ephemeral") => "ephemeral",
        Some(_) => "unknown (unrecognized provider setting)",
    }
}
fn secret_service_available() -> bool {
    let mut command = Command::new("busctl");
    command.args([
        "--user",
        "--timeout=1000ms",
        "call",
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        "s",
        "org.freedesktop.secrets",
    ]);
    crate::process::capture(command, None, Duration::from_secs(2)).is_ok_and(|output| {
        output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "b true"
    })
}

pub fn credential_backend_status_for(home: &Path, secret_service: bool) -> Value {
    json!({"codex":{"defaultHome":{"configured":configured_codex_policy_at(home)},"newNamedHomes":{"configured":"auto","preferred":"Secret Service keyring","fallback":"private file in CODEX_HOME"}},"claude":{"configured":"provider-managed; no configuration changed"},"secretService":{"available":secret_service,"probe":"D-Bus NameHasOwner only (1 second timeout)"},"privacy":"Provider auth files and secret payloads were not read."})
}

pub fn credential_backend_status() -> Value {
    let Ok(home) = home() else {
        return json!({"codex":{"defaultHome":{"configured":"unknown (HOME unavailable)"},"newNamedHomes":{"configured":"auto","preferred":"Secret Service keyring","fallback":"private file in CODEX_HOME"}},"claude":{"configured":"provider-managed; no configuration changed"},"secretService":{"available":false,"probe":"unavailable (HOME is not set)"},"privacy":"Provider auth files and secret payloads were not read."});
    };
    credential_backend_status_for(&home, secret_service_available())
}

pub fn set_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

pub fn initialize_new_account(home: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(home)?;
    if !meta.is_dir() || meta.file_type().is_symlink() || meta.mode() & 0o077 != 0 {
        return Err(err("New Codex account home is not a private directory"));
    }
    let path = home.join("config.toml");
    if fs::symlink_metadata(&path).is_ok() {
        return Err(err("Refusing to replace existing Codex config"));
    }
    set_private_file(&path, b"cli_auth_credentials_store = \"auto\"\n")
}

pub fn codex(args: &[String]) -> Result<i32> {
    let binary = home()?.join(".local/share/nacre/ai/codex/packages/0.159.2/bin/codex");
    let mut command = Command::new(binary);
    command.args(args);
    ai_core::prepare_provider_launch();
    use std::os::unix::process::CommandExt;
    Err(command.exec().into())
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".into();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn remote(args: &[String]) -> Result<i32> {
    let project_id = args.first().ok_or("project id required")?;
    let project_path = args.get(1).ok_or("absolute repository path required")?;
    let account = args.get(2).map(String::as_str).unwrap_or("");
    if !valid_project_id(project_id) {
        return Ok(2);
    }
    if !Path::new(project_path).is_absolute() {
        return Ok(2);
    }
    if !account.is_empty() && !valid_name(account) {
        return Ok(2);
    }
    for command in ["tmux", "codex"] {
        let found = env::split_paths(&env::var_os("PATH").unwrap_or_default()).any(|directory| {
            directory
                .join(command)
                .metadata()
                .is_ok_and(|meta| meta.is_file() && meta.mode() & 0o111 != 0)
        });
        if !found {
            return Err(err(format!("Required command unavailable: {command}")));
        }
    }
    let home = home()?;
    let repo = Command::new("git")
        .args(["-C", project_path, "rev-parse", "--show-toplevel"])
        .output()?;
    if !repo.status.success() {
        return Err(err("Could not resolve repository root"));
    }
    let repo = String::from_utf8(repo.stdout)?
        .trim_end_matches(['\r', '\n'])
        .to_owned();
    let task_id = format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S"),
        std::process::id()
    );
    let account_tag = if account.is_empty() {
        "default".to_owned()
    } else {
        format!("profile-{account}")
    };
    let session = format!("ai-{project_id}-{account_tag}-{task_id}");
    let worktree = home.join(format!(
        ".local/share/nacre/ai/worktrees/{project_id}/{task_id}"
    ));
    fs::create_dir_all(worktree.parent().ok_or("Invalid worktree path")?)?;
    let branch = format!("codex/ai-{project_id}-{task_id}");
    let status = Command::new("git")
        .args(["-C", &repo, "worktree", "add", "-b", &branch])
        .arg(&worktree)
        .arg("HEAD")
        .status()?;
    if !status.success() {
        return Err(err("Could not create task worktree"));
    }
    let vault = env::var_os("NACRE_BRAIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("Documents/Nacre-Brain"));
    fs::create_dir_all(&vault)?;
    fs::set_permissions(&vault, fs::Permissions::from_mode(0o700))?;

    let mut words = if account.is_empty() {
        vec!["codex".to_owned()]
    } else {
        vec![
            home.join(".local/bin/nacre-ai-account")
                .to_string_lossy()
                .into_owned(),
            account.to_owned(),
        ]
    };
    words.extend([
        "--sandbox".into(),
        "danger-full-access".into(),
        "--ask-for-approval".into(),
        "never".into(),
        "-C".into(),
        worktree.to_string_lossy().into_owned(),
        "--add-dir".into(),
        vault.to_string_lossy().into_owned(),
    ]);
    let invocation = words
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ");
    let directory = shell_quote(&worktree.to_string_lossy());
    let command = format!("umask 077; cd {directory} && {invocation}; result=$?; printf '\\nCodex ended (exit %s). Your files and saved conversation remain.\\n' \"$result\"; printf 'Type codex resume to reopen saved work, or exit to close this task.\\n'; exec bash -l");
    let mut tmux = Command::new("tmux");
    tmux.args(["new-session", "-s", &session, "bash", "-lc", &command]);
    use std::os::unix::process::CommandExt;
    Err(tmux.exec().into())
}

pub fn dispatch(command: &str, args: &[String]) -> Result<i32> {
    match command {
        "nacre-ai-skills" => skills(args),
        "nacre-ai-tools" => tools(args),
        "nacre-ai-account" => account(args),
        "nacre-ai-remote" => remote(args),
        "ai-credentials-status" => {
            println!("{}", serde_json::to_string(&credential_backend_status())?);
            Ok(0)
        }
        _ => Err(err("Unknown AI wrapper command")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn skill_exposure_retargets_only_owned_links_and_preserves_foreign_skills() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let library = root.join("library");
        let old = library.join("old/example");
        let new = library.join("new/example");
        fs::create_dir_all(&old)?;
        fs::create_dir_all(&new)?;
        fs::write(old.join("SKILL.md"), "old")?;
        fs::write(new.join("SKILL.md"), "new")?;
        let source = root.join("shared");
        fs::create_dir(&source)?;
        symlink(&old, source.join("example"))?;
        let target = root.join("account/skills");
        fs::create_dir_all(root.join("account"))?;
        fs::write(root.join("account/auth.json"), "fixture auth untouched")?;
        expose(&source, &target, Some(&library))?;
        fs::remove_file(source.join("example"))?;
        symlink(&new, source.join("example"))?;
        expose(&source, &target, Some(&library))?;
        assert_eq!(fs::read_to_string(target.join("example/SKILL.md"))?, "new");

        let foreign = root.join("foreign/example");
        fs::create_dir_all(&foreign)?;
        fs::write(foreign.join("SKILL.md"), "foreign")?;
        fs::remove_file(target.join("example"))?;
        symlink(&foreign, target.join("example"))?;
        assert!(expose(&source, &target, Some(&library)).is_err());
        assert_eq!(
            fs::read_to_string(target.join("example/SKILL.md"))?,
            "foreign"
        );
        assert_eq!(
            fs::read_to_string(root.join("account/auth.json"))?,
            "fixture auth untouched"
        );
        Ok(())
    }

    #[test]
    fn account_init_is_new_only_private_and_never_follows_a_config_symlink() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let home = temp.path().join("profile");
        fs::create_dir(&home)?;
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
        initialize_new_account(&home)?;
        assert_eq!(
            fs::read_to_string(home.join("config.toml"))?,
            "cli_auth_credentials_store = \"auto\"\n"
        );
        assert_eq!(
            fs::metadata(home.join("config.toml"))?.permissions().mode() & 0o777,
            0o600
        );
        assert!(initialize_new_account(&home).is_err());

        let other = temp.path().join("other");
        fs::create_dir(&other)?;
        fs::set_permissions(&other, fs::Permissions::from_mode(0o700))?;
        let sentinel = temp.path().join("sentinel");
        fs::write(&sentinel, "fixture config")?;
        symlink(&sentinel, other.join("config.toml"))?;
        assert!(initialize_new_account(&other).is_err());
        assert_eq!(fs::read_to_string(sentinel)?, "fixture config");
        Ok(())
    }

    #[test]
    fn doctor_reports_policy_without_reading_auth_or_exposing_config_values() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let codex = temp.path().join(".codex");
        fs::create_dir_all(&codex)?;
        fs::write(
            codex.join("config.toml"),
            "cli_auth_credentials_store = \"auto\"\napi_key = \"fixture-secret\"\n",
        )?;
        fs::write(codex.join("auth.json"), "fixture-auth-must-not-be-read")?;
        assert_eq!(
            configured_codex_policy_at(temp.path()),
            "auto (keyring preferred; file fallback)"
        );
        let serialized = credential_backend_status_for(temp.path(), false).to_string();
        assert!(!serialized.contains("fixture-secret"));
        assert!(!serialized.contains("fixture-auth"));
        assert!(serialized.contains("private file in CODEX_HOME"));
        fs::write(
            codex.join("config.toml"),
            "cli_auth_credentials_store = \"file\"\nprofile = \"keyring\"\n[profiles.keyring]\ncli_auth_credentials_store = \"auto\"\n",
        )?;
        assert_eq!(configured_codex_policy_at(temp.path()), "file");
        Ok(())
    }

    #[test]
    fn unsafe_archive_paths_are_rejected() {
        for path in ["../outside", "/absolute", "a/../../b", "", "."] {
            assert!(safe_relative(path).is_err(), "{path}");
        }
        assert!(safe_relative("skills/example/SKILL.md").is_ok());
    }

    #[test]
    fn pinned_archive_digest_and_download_limit_are_enforced() -> Result<()> {
        let bytes = b"public archive fixture";
        let source = SkillSource {
            repo: "example/skills".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            skills: vec!["demo".into()],
        };
        validate_source(&source)?;
        verify_archive(bytes, &source)?;
        let mut wrong = source;
        wrong.sha256 = "0".repeat(64);
        assert!(verify_archive(bytes, &wrong).is_err());
        assert!(verify_archive(&vec![0; MAX_ARCHIVE + 1], &wrong).is_err());
        wrong.repo = "../../escape".into();
        assert!(validate_source(&wrong).is_err());
        Ok(())
    }

    #[test]
    fn extraction_limits_bound_actual_entry_and_total_writes() -> Result<()> {
        let mut expanded = 0;
        let mut output = Vec::new();
        assert!(copy_limited(
            &mut Cursor::new(b"12345"),
            &mut output,
            5,
            &mut expanded,
            4,
            8
        )
        .is_err());
        assert!(output.is_empty());
        assert!(copy_limited(
            &mut Cursor::new(b"12345"),
            &mut output,
            4,
            &mut expanded,
            4,
            8
        )
        .is_err());
        assert!(output.is_empty());
        copy_limited(
            &mut Cursor::new(b"1234"),
            &mut output,
            4,
            &mut expanded,
            4,
            6,
        )?;
        assert_eq!(expanded, 4);
        let mut next = Vec::new();
        assert!(copy_limited(&mut Cursor::new(b"789"), &mut next, 3, &mut expanded, 4, 6).is_err());
        assert!(next.is_empty());
        assert_eq!(expanded, 4);
        Ok(())
    }
}
