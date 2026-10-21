//! Shared private Markdown vault implementation.
use crate::{Error, Result};
use chrono::Utc;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::LazyLock,
};

const KINDS: &[&str] = &[
    "inbox",
    "personal",
    "projects",
    "infrastructure",
    "devices",
    "runbooks",
    "decisions",
    "credentials",
    "wiki",
    "raw",
    "checkpoints",
];
const ROOT_FILES: &[&str] = &["README.md", "AGENTS.md", "CLAUDE.md", "INDEX.md"];
const README: &str = "# Nacre Brain\n\nThis private local folder is the durable knowledge bank shared by your Codex and Claude accounts.\nOpen this folder as an Obsidian vault, or use \x60nacre-brain search WORDS\x60.\n\n| Folder | Content |\n| --- | --- |\n| inbox | New facts and task handoffs awaiting organization |\n| personal | Personal preferences and context |\n| projects | Project summaries and pointers to canonical repository instructions |\n| infrastructure | Dated host inventory, access methods, environment boundaries |\n| devices | Device identity, last verified address, hardware evidence |\n| runbooks | Repeatable operational procedures |\n| decisions | Reasons behind durable choices |\n| credentials | Secret-store references only; never secret values |\n\nSearch before adding a note. Record a source and verification date; distinguish\nobserved facts from user reports and unverified historical assumptions. Recheck\nhost identity before acting on a saved address. Keep project rules in the project.\n\nNotes belong to your OS user, not your ChatGPT account. They are not automatically\navailable to ordinary ChatGPT web chats. Codex reads them through your global\ninstructions and personal skills. The public OS repository contains only the\ntools and templates, never this folder or Codex account state.\n\nUse \x60nacre-brain note --help\x60 to save a unique note without concurrent writers\noverwriting one another. Use \x60nacre-brain store-credential NAME\x60 to put a secret\nin the OS keyring, then document only its reference. Keyring secrets are not\nautomatically copied to other hosts. This folder is not a backup by itself.\n";

fn error(s: impl Into<String>) -> Error {
    s.into().into()
}
fn vault() -> Result<PathBuf> {
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?);
    let raw = env::var_os("NACRE_BRAIN").filter(|x| !x.is_empty());
    let path = if let Some(value) = raw {
        let text = value.to_string_lossy();
        if text == "~" {
            home.clone()
        } else if let Some(rest) = text.strip_prefix("~/") {
            home.join(rest)
        } else {
            PathBuf::from(value)
        }
    } else {
        home.join("Documents/Nacre-Brain")
    };
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };
    if let Ok(real) = fs::canonicalize(&absolute) {
        return Ok(real);
    }
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                normalized.push(component.as_os_str())
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(v) => normalized.push(v),
        }
    }
    for ancestor in normalized.ancestors() {
        if let Ok(real) = fs::canonicalize(ancestor) {
            return Ok(real.join(normalized.strip_prefix(ancestor)?));
        }
    }
    Ok(normalized)
}
fn reject_links(path: &Path) -> Result<()> {
    for part in path.ancestors() {
        if fs::symlink_metadata(part).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("Symlinked vault paths are not supported"));
        }
    }
    Ok(())
}
fn private_dir(path: &Path) -> Result<()> {
    reject_links(path)?;
    if !path.exists() {
        fs::create_dir_all(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn initialize() -> Result<PathBuf> {
    let root = vault()?;
    reject_links(&root)?;
    fs::create_dir_all(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    for kind in KINDS {
        private_dir(&root.join(kind))?;
    }
    let path = root.join("README.md");
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(error("Symlinked vault paths are not supported"));
    }
    if !path.exists() {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(README.as_bytes())?;
    }
    Ok(root)
}
fn safe_note(s: &str) -> Result<()> {
    static PRIVATE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"-----BEGIN (?:OPENSSH |RSA |EC )?PRIVATE KEY-----").unwrap()
    });
    static TOKEN: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"\b(?:sk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{20,})",
        )
        .unwrap()
    });
    static ASSIGNMENT: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::RegexBuilder::new(r"(?m)^\s*(?:password|passwd|api[_ -]?key|access[_ -]?token|refresh[_ -]?token)\s*[:=]\s*\S+").case_insensitive(true).build().unwrap()
    });
    if PRIVATE.is_match(s) || TOKEN.is_match(s) || ASSIGNMENT.is_match(s) {
        return Err(error("Possible credential found. Store the secret in the OS keyring and save only its reference."));
    }
    Ok(())
}
fn note_path(root: &Path, name: &str) -> Result<PathBuf> {
    reject_links(root)?;
    let p = Path::new(name);
    let parts: Vec<_> = name.split('/').collect();
    if p.is_absolute()
        || parts.iter().any(|p| *p == ".." || p.starts_with('.'))
        || p.components().count() != parts.len()
    {
        return Err(error("Invalid vault path"));
    }
    let suffix = p
        .extension()
        .and_then(|x| x.to_str())
        .map(|x| format!(".{x}"));
    if !ROOT_FILES.contains(&name)
        && !(parts.len() >= 2
            && KINDS.contains(&parts[0])
            && [".md", ".txt", ".base", ".canvas"].contains(&suffix.as_deref().unwrap_or("")))
    {
        return Err(error(
            "Unsupported vault file; use Markdown sources and source links",
        ));
    }
    let mut at = root.to_path_buf();
    for part in parts {
        at.push(part);
        if fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("Symlinked vault paths are not supported"));
        }
    }
    Ok(at)
}
fn notes(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut paths: Vec<PathBuf> = ROOT_FILES.iter().map(|n| root.join(n)).collect();
    for kind in KINDS {
        let dir = root.join(kind);
        if fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("Symlinked vault directories are not supported"));
        }
        if dir.exists() {
            for entry in walkdir::WalkDir::new(&dir).follow_links(false) {
                let entry = entry.map_err(|e| error(e.to_string()))?;
                let p = entry.path();
                if p != dir && fs::symlink_metadata(p)?.file_type().is_symlink() {
                    return Err(error("Symlinked vault files are not supported"));
                }
                paths.push(p.to_path_buf());
            }
        }
    }
    paths.sort();
    let mut result = BTreeMap::new();
    for p in paths {
        let relative = p.strip_prefix(root)?;
        if relative
            .components()
            .any(|x| x.as_os_str().to_string_lossy().starts_with('.'))
            || !["md", "txt", "base", "canvas"]
                .contains(&p.extension().and_then(|x| x.to_str()).unwrap_or(""))
            || !p.is_file()
        {
            continue;
        }
        let name = relative.to_string_lossy().into_owned();
        let body = fs::read_to_string(note_path(root, &name)?)?;
        safe_note(&body)?;
        result.insert(name, body);
    }
    Ok(result)
}
fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn digest(notes: &BTreeMap<String, String>) -> Result<String> {
    Ok(sha(crate::json::dumps(notes, true)?.as_bytes()))
}
fn atomic(path: &Path, text: &str) -> Result<()> {
    reject_links(path)?;
    if fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.nlink() != 1) {
        return Err(error("Linked or non-regular vault files are not supported"));
    }
    if crate::state::write_text(path, text)? {
        return Ok(());
    }
    let parent = path.parent().ok_or("Missing parent directory")?;
    if !parent.exists() {
        private_dir(parent)?;
    }
    let mut f = tempfile::Builder::new()
        .prefix(".pending-")
        .tempfile_in(parent)?;
    f.write_all(text.as_bytes())?;
    f.as_file().sync_all()?;
    f.persist(path)?;
    Ok(())
}
struct VaultLock(File);
impl VaultLock {
    fn acquire(root: &Path) -> Result<Self> {
        let state = root.join(".brain-state");
        private_dir(&state)?;
        let path = state.join("lock");
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("Symlinked vault paths are not supported"));
        }
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&path)?;
        if !f.metadata()?.is_file() || f.metadata()?.nlink() != 1 {
            return Err(error("Linked or non-regular vault locks are not supported"));
        }
        f.set_permissions(fs::Permissions::from_mode(0o600))?;
        rustix::fs::flock(&f, rustix::fs::FlockOperation::LockExclusive)?;
        Ok(Self(f))
    }
}
impl Drop for VaultLock {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.0, rustix::fs::FlockOperation::Unlock);
    }
}
fn backup(root: &Path, provided: Option<&BTreeMap<String, String>>) -> Result<PathBuf> {
    let ns = provided.cloned().map(Ok).unwrap_or_else(|| notes(root))?;
    let key = digest(&ns)?;
    let folder = root.join(".brain-state/backups");
    private_dir(&folder)?;
    let p = folder.join(format!("{key}.json"));
    if !p.exists() {
        let data = json!({"version":1,"created":Utc::now().format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string(),"sha256":key,"files":ns});
        atomic(&p, &format!("{}\n", crate::json::pretty(&data)?))?;
    }
    Ok(p)
}
fn conflict(root: &Path, name: &str, local: Option<&String>, incoming: &str) -> Result<()> {
    let value = json!({"path":name,"local":local,"incoming":incoming});
    let id = sha(crate::json::dumps(&value, true)?.as_bytes());
    let dir = root.join(".brain-state/conflicts");
    private_dir(&dir)?;
    let p = dir.join(format!("{id}.json"));
    if !p.exists() {
        atomic(&p, &format!("{}\n", crate::json::pretty(&value)?))?;
    }
    Ok(())
}
fn merge(
    root: &Path,
    incoming: &Map<String, Value>,
    base: &Map<String, Value>,
    excluded: &[String],
) -> Result<(Vec<String>, Vec<String>)> {
    let mut batch = Vec::new();
    for (n, v) in incoming {
        let p = note_path(root, n)?;
        let body = v.as_str().ok_or("Invalid vault content")?;
        safe_note(body)?;
        batch.push((n.clone(), p, body.to_owned()));
    }
    let current = notes(root)?;
    backup(root, Some(&current))?;
    let (mut changed, mut conflicts) = (Vec::new(), Vec::new());
    for (n, p, body) in batch {
        if excluded.contains(&n) {
            conflict(root, &n, current.get(&n), &body)?;
            continue;
        }
        let old = current.get(&n);
        if old == Some(&body) {
            continue;
        }
        let old_base = base.get(&n).and_then(Value::as_str);
        if old.is_none() || old_base.is_some_and(|b| old.is_some_and(|o| o == b)) {
            atomic(&p, &body)?;
            changed.push(n)
        } else if old_base == Some(body.as_str()) {
            continue;
        } else {
            conflict(root, &n, old, &body)?;
            conflicts.push(n)
        }
    }
    backup(root, None)?;
    Ok((changed, conflicts))
}
fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

pub fn run(args: &[String]) -> Result<i32> {
    let Some(tool) = args.first().map(String::as_str) else {
        return Err(error("Expected brain command"));
    };
    if tool == "nacre-brain"
        && (args.len() == 1
            || args
                .get(1)
                .is_some_and(|a| ["open", "brain", "capture"].contains(&a.as_str())))
    {
        return brain(&args[1..]);
    }
    if tool == "nacre-brain-sync"
        && args
            .get(1)
            .is_some_and(|a| a == "snapshot" || a == "note_path")
    {
        if let Some(input) = args.get(2) {
            let result = serde_json::from_str::<Value>(input)
                .map_err(|e| error(e.to_string()))
                .and_then(|request| api(tool, &args[1], &request));
            match result {
                Ok(value) => {
                    println!("{}", crate::json::encode(&value)?);
                    return Ok(0);
                }
                Err(failure) => {
                    println!(
                        "{}",
                        crate::json::encode(
                            &json!({"error":failure.to_string(),"type":"ValueError"})
                        )?
                    );
                    return Ok(1);
                }
            }
        }
    }
    let arguments = match crate::brain_arguments::parse(tool, &args[1..])? {
        crate::brain_arguments::Parsed::Arguments(arguments) => arguments,
        crate::brain_arguments::Parsed::Exit(code) => return Ok(code),
    };
    let result = match tool {
        "nacre-brain" => brain(&arguments),
        "nacre-brain-sync" => sync(&arguments),
        "nacre-brain-maintain" => maintain(&arguments),
        _ => Err(error("Unsupported brain command")),
    };
    match result {
        Ok(code) => Ok(code),
        Err(failure) => {
            if tool == "nacre-brain-sync" && !args.iter().any(|a| a == "--exchange") {
                if let Ok(path) = sync_status_path() {
                    let mut previous = read_status(&path);
                    if !["authentication", "protocol", "failed"]
                        .contains(&previous["state"].as_str().unwrap_or(""))
                    {
                        if let Err(status_error) = report_status(
                            &path,
                            &mut previous,
                            "failed",
                            "Local sync failed; inspect the sync journal",
                        ) {
                            eprintln!("{status_error}")
                        }
                    }
                }
            }
            eprintln!("{tool}: {failure}");
            Ok(1)
        }
    }
}
fn brain(args: &[String]) -> Result<i32> {
    if args.is_empty() || ["open", "brain", "capture"].contains(&args[0].as_str()) {
        let a = args.first().map(String::as_str).unwrap_or("brain");
        let a = if a == "open" { "brain" } else { a };
        use std::os::unix::process::CommandExt;
        return Err(Command::new(
            PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?)
                .join(".local/bin/nacre-brain-ui"),
        )
        .arg(a)
        .exec()
        .into());
    }
    match args[0].as_str() {
        "-h" | "--help" => {
            println!("usage: nacre-brain <command>");
            Ok(0)
        }
        "path" if args.len() == 1 => {
            println!("{}", vault()?.display());
            Ok(0)
        }
        "init" if args.len() == 1 => {
            println!("{}", initialize()?.display());
            Ok(0)
        }
        "path" | "init" if args.len() != 1 => {
            eprintln!("nacre-brain: error: unrecognized arguments");
            Ok(2)
        }
        "search" => {
            let root = initialize()?;
            if args.len() < 2 {
                eprintln!("nacre-brain search: error: the following arguments are required: query");
                return Ok(2);
            }
            let q = args.get(1..).unwrap_or(&[]).join(" ");
            let s = Command::new("rg")
                .args(["-n", "-i", "-F", "--glob", "*.md", "--", &q])
                .arg(root)
                .status()?;
            Ok(s.code().unwrap_or(1))
        }
        "note" => {
            let _root = initialize()?;
            brain_note(&args[1..])
        }
        "store-credential" => {
            let _root = initialize()?;
            store_credential(&args[1..])
        }
        "run-with-credential" => {
            let _root = initialize()?;
            run_credential(&args[1..])
        }
        _ => Err(error("invalid choice")),
    }
}
fn brain_note(args: &[String]) -> Result<i32> {
    let (mut kind, mut title, mut source, mut confidence) =
        ("inbox".to_string(), None, None, "reported".to_string());
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                i += 1;
                kind = args.get(i).ok_or("expected kind")?.clone()
            }
            "--title" => {
                i += 1;
                title = Some(args.get(i).ok_or("expected title")?.clone())
            }
            "--source" => {
                i += 1;
                source = Some(args.get(i).ok_or("expected source")?.clone())
            }
            "--confidence" => {
                i += 1;
                confidence = args.get(i).ok_or("expected confidence")?.clone()
            }
            _ => return Err(error("unrecognized argument")),
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
        "credentials",
        "checkpoints",
    ]
    .contains(&kind.as_str())
    {
        return Err(error("invalid kind"));
    }
    if !["verified", "reported", "unverified"].contains(&confidence.as_str()) {
        return Err(error("invalid confidence"));
    }
    let title = title.ok_or("the following arguments are required: --title")?;
    let source = source.ok_or("the following arguments are required: --source")?;
    let mut body = String::new();
    std::io::stdin().read_to_string(&mut body)?;
    safe_note(&format!("{title}\n{source}\n{body}"))?;
    if body.trim().is_empty() {
        return Err(error("Note is empty"));
    }
    let root = vault()?;
    let slug = title
        .to_lowercase()
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
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = slug.chars().take(70).collect::<String>();
    let slug = if slug.is_empty() { "note" } else { &slug };
    let now = Utc::now();
    let text = format!(
        "# {}\n\nRecorded: {}\nConfidence: {}\nSource: {}\n\n{}\n",
        title.replace('\n', " "),
        now.format("%Y-%m-%dT%H:%M:%S%.6f+00:00"),
        confidence,
        source.replace('\n', " "),
        body.trim_end()
    );
    let p = root.join(&kind).join(format!(
        "{}-{}-{}.md",
        now.format("%Y%m%dT%H%M%SZ"),
        slug,
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ));
    let parent = p.parent().ok_or("missing note directory")?;
    let pending = tempfile::Builder::new()
        .prefix(".note-")
        .tempfile_in(parent)?;
    let name = pending.path().to_path_buf();
    fs::write(&name, &text)?;
    fs::hard_link(&name, &p)?;
    fs::remove_file(name)?;
    println!("{}", p.display());
    Ok(0)
}
struct PromptTerminal {
    tty: File,
    original: rustix::termios::Termios,
}
impl Drop for PromptTerminal {
    fn drop(&mut self) {
        let _ = rustix::termios::tcsetattr(
            &self.tty,
            rustix::termios::OptionalActions::Drain,
            &self.original,
        );
    }
}
fn store_credential(args: &[String]) -> Result<i32> {
    let name = args.first().ok_or("missing credential name")?;
    eprint!("Secret for {name}: ");
    std::io::stderr().flush()?;
    let signals = crate::signals::Signals::new()?;
    let tty = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(tty) => tty,
        Err(_) => {
            drop(signals);
            eprintln!("Warning: Password input may be echoed.");
            let mut value = String::new();
            std::io::stdin().read_line(&mut value)?;
            return store_secret(name, value);
        }
    };
    let original = rustix::termios::tcgetattr(&tty)?;
    let terminal = PromptTerminal { tty, original };
    let mut hidden = terminal.original.clone();
    hidden.local_modes.remove(rustix::termios::LocalModes::ECHO);
    rustix::termios::tcsetattr(
        &terminal.tty,
        rustix::termios::OptionalActions::Drain,
        &hidden,
    )?;
    use rustix::event::{poll, PollFd, PollFlags};
    loop {
        let mut fds = [
            PollFd::new(&terminal.tty, PollFlags::IN),
            PollFd::new(&signals.fd, PollFlags::IN),
        ];
        poll(&mut fds, None)?;
        if fds[1].revents().contains(PollFlags::IN) {
            if let Some(signal) = signals.fd.read_signal()? {
                return Ok(
                    if signal.ssi_signo == nix::sys::signal::Signal::SIGTERM as u32 {
                        143
                    } else {
                        130
                    },
                );
            }
        }
        if fds[0].revents().intersects(PollFlags::IN | PollFlags::HUP) {
            break;
        }
    }
    let mut value = String::new();
    let read = std::io::BufRead::read_line(&mut std::io::BufReader::new(&terminal.tty), &mut value);
    drop(terminal);
    drop(signals);
    read?;
    eprintln!();
    store_secret(name, value)
}
fn store_secret(name: &str, mut value: String) -> Result<i32> {
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    if value.is_empty() {
        return Err(error("Secret is empty"));
    }
    let mut c = Command::new("secret-tool")
        .args([
            "store",
            "--label",
            &format!("Nacre Brain: {name}"),
            "service",
            "nacre-brain",
            "name",
            name,
        ])
        .stdin(Stdio::piped())
        .spawn()?;
    c.stdin
        .take()
        .ok_or("missing stdin")?
        .write_all(value.as_bytes())?;
    if !c.wait()?.success() {
        return Err(error("secret-tool failed"));
    }
    println!("Saved reference: secret-service://nacre-brain/{name}");
    Ok(0)
}
fn run_credential(args: &[String]) -> Result<i32> {
    let env_pos = args
        .iter()
        .position(|arg| arg == "--env")
        .ok_or("--env is required")?;
    let key = args.get(env_pos + 1).ok_or("--env is required")?;
    if !valid_env(key) {
        return Err(error("Invalid environment variable name"));
    }
    let separator = args.iter().position(|arg| arg == "--");
    let stop = separator.unwrap_or(args.len());
    let positional = args
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < stop && *i != env_pos && *i != env_pos + 1)
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let name = positional.first().ok_or("missing credential name")?;
    let program_args = if let Some(index) = separator {
        args[index + 1..].to_vec()
    } else {
        positional.iter().skip(1).cloned().collect::<Vec<_>>()
    };
    let program = program_args.first().ok_or("Supply a command after --")?;
    let out = Command::new("secret-tool")
        .args(["lookup", "service", "nacre-brain", "name", name])
        .output()?;
    let secret = String::from_utf8_lossy(&out.stdout)
        .trim_end_matches(char::from(10))
        .to_owned();
    if !out.status.success() || secret.is_empty() {
        return Err(error("Credential not found or keyring is locked"));
    }
    use std::os::unix::process::CommandExt;
    Err(Command::new(program)
        .args(&program_args[1..])
        .env(key, secret)
        .exec()
        .into())
}
fn valid_env(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|x| x == '_' || x.is_ascii_alphabetic())
        && c.all(|x| x == '_' || x.is_ascii_alphanumeric())
}
fn read_status(path: &Path) -> Value {
    read_json_file(path)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}
fn sync_status_path() -> Result<PathBuf> {
    Ok(crate::paths::Paths::environment()?
        .state
        .join("brain-sync.json"))
}
fn read_json_file(path: &Path) -> Result<Value> {
    reject_links(path)?;
    if !path.exists() {
        return Err(
            std::io::Error::new(std::io::ErrorKind::NotFound, "State file not found").into(),
        );
    }
    let fallback = crate::state::schema(path)
        .map(|schema| schema.defaults.clone())
        .unwrap_or(Value::Null);
    crate::state::read(path, fallback)
}
fn report_status(path: &Path, previous: &mut Value, state: &str, message: &str) -> Result<()> {
    report_status_at(
        path,
        previous,
        state,
        message,
        Utc::now().timestamp_micros() as f64 / 1e6,
    )
}
fn report_status_at(
    path: &Path,
    previous: &mut Value,
    state: &str,
    message: &str,
    now: f64,
) -> Result<()> {
    previous["state"] = json!(state);
    previous["message"] = json!(message);
    previous["lastAttempt"] = json!(now);
    if state == "success" || state == "conflicts" {
        previous["lastSuccess"] = json!(now);
        previous["failures"] = json!(0);
        previous["nextRetry"] = json!(0);
    } else if state == "offline" {
        let n = previous["failures"].as_u64().unwrap_or(0).saturating_add(1);
        previous["failures"] = json!(n);
        previous["nextRetry"] = json!(
            now + ((120u64.saturating_mul(2u64.pow(n.saturating_sub(1).min(5) as u32))).min(3600))
                as f64
        );
    }
    if let Some(parent) = path.parent() {
        private_dir(parent)?;
    }
    atomic(path, &crate::json::encode(previous)?)
}
fn exchange_error(code: i32, text: &str) -> (&'static str, &'static str) {
    let low = text.to_lowercase();
    if low.contains("permission denied") || low.contains("host key verification failed") {
        return (
            "authentication",
            "Remote SSH authentication failed; check the existing SSH identity or host key",
        );
    }
    if code == 255
        && [
            "timed out",
            "connection refused",
            "no route",
            "resolve hostname",
            "network is unreachable",
            "connection closed",
        ]
        .iter()
        .any(|x| low.contains(x))
    {
        return (
            "offline",
            "Remote host unavailable; local notes are safe and synchronization will retry",
        );
    }
    if low.contains("protocol") || code == 127 {
        return (
            "protocol",
            "Remote sync helper is missing or incompatible; both hosts need protocol version 2",
        );
    }
    (
        "failed",
        "Remote exchange failed; check the remote sync journal",
    )
}
pub fn api(helper: &str, operation: &str, request: &Value) -> Result<Value> {
    let args = request["args"].as_array().cloned().unwrap_or_default();
    let root = args
        .first()
        .and_then(|v| v.get("$path"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or("Expected a tagged Path")?;
    match (helper, operation) {
        ("nacre-brain-sync", "snapshot") => Ok(json!(notes(&root)?)),
        ("nacre-brain-sync", "note_path") => {
            let name = args
                .get(1)
                .and_then(Value::as_str)
                .ok_or("Missing argument: name")?;
            Ok(json!(note_path(&root, name)?.to_string_lossy()))
        }
        _ => Err(error("Unsupported native module operation")),
    }
}

fn restore(archive: &Path, destination: &Path) -> Result<()> {
    let record = read_json_file(archive)?;
    let files = record
        .get("files")
        .and_then(Value::as_object)
        .ok_or("Invalid backup files")?;
    let mut map = BTreeMap::new();
    for (k, v) in files {
        map.insert(
            k.clone(),
            v.as_str().ok_or("Invalid backup content")?.to_owned(),
        );
    }
    if record["version"] != 1 || digest(&map)? != record["sha256"].as_str().unwrap_or("") {
        return Err(error("Backup checksum mismatch"));
    }
    if destination.exists() {
        return Err(error("Restore destination must not exist"));
    }
    for (k, v) in &map {
        note_path(destination, k)?;
        safe_note(v)?
    }
    private_dir(destination)?;
    for (k, v) in map {
        atomic(&note_path(destination, &k)?, &v)?
    }
    Ok(())
}

fn check(root: &Path) -> Result<Vec<String>> {
    let ns = notes(root)?;
    let mut findings = Vec::new();
    let stems = ns
        .keys()
        .map(|n| {
            Path::new(n)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    let fenced = regex::Regex::new(r"(?s)\x60\x60\x60.*?\x60\x60\x60").unwrap();
    let scheme = regex::Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*:").unwrap();
    let wiki_link = regex::Regex::new(r"\[\[([^\]]+)\]\]")?;
    let relative_link = regex::Regex::new(r"\]\(([^)]+)\)")?;
    let reviewed = regex::Regex::new(r"(?m)^Reviewed: (\d{4}-\d{2}-\d{2})")?;
    for (name, body) in &ns {
        if !name.ends_with(".md") || ROOT_FILES.contains(&name.as_str()) {
            continue;
        }
        let fields: &[&str] = if name.starts_with("wiki/") {
            &["Source:", "Reviewed:", "Status:"]
        } else if name.starts_with("raw/") {
            &[]
        } else {
            &["Source:", "Confidence:", "Recorded:"]
        };
        for field in fields {
            if !body.contains(field) {
                findings.push(format!("{name}: missing {field}"));
            }
        }
        let clean = fenced.replace_all(body, "");
        for captures in wiki_link.captures_iter(&clean) {
            let link = &captures[1];
            let target = link
                .split('|')
                .next()
                .unwrap_or("")
                .split('#')
                .next()
                .unwrap_or("");
            if !target.is_empty()
                && !stems.contains(
                    Path::new(target)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .as_ref(),
                )
            {
                findings.push(format!("{name}: missing wiki link {target}"));
            }
        }
        for captures in relative_link.captures_iter(&clean) {
            let target = captures[1]
                .trim_matches(['<', '>'])
                .split('#')
                .next()
                .unwrap_or("");
            let decoded = percent_decode(target);
            if decoded.is_empty() || decoded.starts_with('/') || scheme.is_match(&decoded) {
                continue;
            }
            if !root
                .join(Path::new(name).parent().unwrap_or(Path::new("")))
                .join(&decoded)
                .exists()
            {
                findings.push(format!("{name}: missing relative link {decoded}"));
            }
        }
        if let Some(date) = reviewed.captures(body) {
            let date = chrono::NaiveDate::parse_from_str(&date[1], "%Y-%m-%d")?;
            if (chrono::Local::now().date_naive() - date).num_days() > 30 {
                findings.push(format!(
                    "{name}: review older than 30 days; verify time-sensitive claims"
                ));
            }
        }
    }
    let dir = root.join(".brain-state/conflicts");
    if dir.exists() {
        let mut items = fs::read_dir(dir)?
            .map(|e| e.map(|x| x.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        items.sort();
        for p in items {
            if p.extension().is_some_and(|x| x == "json") {
                let row = read_json_file(&p)?;
                findings.push(format!(
                    "Unresolved sync conflict: {} ({})",
                    row["path"].as_str().unwrap_or(""),
                    p.file_stem().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }
    Ok(findings)
}
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                result.push((a * 16 + b) as u8);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn sync(args: &[String]) -> Result<i32> {
    let mut exchange = false;
    let mut force = false;
    let default = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?)
        .join(".config/nacre/brain-sync.json");
    let mut config = default;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--exchange" => exchange = true,
            "--force" => force = true,
            "--config" => {
                i += 1;
                config = PathBuf::from(
                    args.get(i)
                        .ok_or("argument --config: expected one argument")?,
                );
            }
            "--help" | "-h" => {}
            _ => return Err(error("unrecognized argument")),
        }
        i += 1;
    }
    if !exchange && !config.exists() {
        return Ok(0);
    }
    if exchange {
        let root = initialize()?;
        let _guard = VaultLock::acquire(&root)?;
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        let req: Value = crate::json::parse_value(&input)?;
        if req["version"] != 2 {
            return Err(error("Both hosts require sync protocol version 2"));
        }
        let incoming = req
            .get("notes")
            .and_then(Value::as_object)
            .ok_or("Invalid sync notes")?;
        let base = req
            .get("base")
            .and_then(Value::as_object)
            .ok_or("Invalid sync baseline")?;
        let (changed, conflicts) = merge(&root, incoming, base, &[])?;
        let ns = notes(&root)?;
        println!(
            "{}",
            crate::json::encode(
                &json!({"version":2,"notes":ns,"changed":changed,"conflicts":conflicts})
            )?
        );
        return Ok(0);
    }
    let status_path = sync_status_path()?;
    reject_links(&status_path)?;
    let mut status = read_status(&status_path);
    if !force
        && status["nextRetry"].as_f64().unwrap_or(0.0) > Utc::now().timestamp_micros() as f64 / 1e6
    {
        return Ok(0);
    }
    let root = initialize()?;
    let _guard = VaultLock::acquire(&root)?;
    let conf = read_json_file(&config)?;
    let host = conf["host"].as_str().ok_or("missing host")?;
    if host.is_empty()
        || !host
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_@".contains(c))
    {
        return Err(error("Invalid SSH alias"));
    }
    let before = notes(&root)?;
    let base_path = root
        .join(".brain-state/peers")
        .join(format!("{}.json", sha(host.as_bytes())));
    let mut base = if base_path.exists() {
        reject_links(&base_path)?;
        let value = read_json_file(&base_path)?;
        value.as_object().cloned().ok_or("Invalid peer baseline")?
    } else {
        Map::new()
    };
    if status.get("lastSuccess").is_none() && base_path.exists() {
        status["lastSuccess"] = json!(fs::metadata(&base_path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs_f64());
    }
    backup(&root, Some(&before))?;
    let request = json!({"version":2,"notes":before,"base":base});
    let ssh_args = vec![
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        host.into(),
        "exec \"$HOME/.local/bin/nacre-brain-sync\" --exchange".into(),
    ];
    let request_text = crate::json::encode(&request)?;
    let output = match crate::process::run(
        "ssh",
        &ssh_args,
        Some(&request_text),
        std::time::Duration::from_secs(60),
    ) {
        Ok(output) => output,
        Err(error) if error.to_string().contains("timed out") => {
            report_status(
                &status_path,
                &mut status,
                "offline",
                "Remote exchange timed out; synchronization will retry",
            )?;
            eprintln!("Remote exchange timed out; synchronization will retry");
            return Ok(75);
        }
        Err(error) => return Err(error),
    };
    let exit = output.status;
    let stdout = output.stdout;
    let stderr = output.stderr;
    if !exit.success() {
        let (category, message) =
            exchange_error(exit.code().unwrap_or(1), &String::from_utf8_lossy(&stderr));
        report_status(&status_path, &mut status, category, message)?;
        if category == "offline" {
            eprintln!("{message}");
            return Ok(75);
        }
        return Err(error(message));
    }
    let reply: Value = serde_json::from_slice(&stdout)?;
    if reply["version"] != 2 {
        return Err(error("Unexpected sync protocol"));
    }
    let incoming = reply
        .get("notes")
        .and_then(Value::as_object)
        .ok_or("Invalid sync notes")?;
    let sent = reply
        .get("changed")
        .and_then(Value::as_array)
        .ok_or("Invalid sync changes")?;
    let excluded = reply
        .get("conflicts")
        .and_then(Value::as_array)
        .ok_or("Invalid sync conflicts")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or("Invalid sync conflict")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (changed, mut conflicts) = merge(&root, incoming, &obj(&json!(before)), &excluded)?;
    conflicts.extend(excluded);
    conflicts.sort();
    conflicts.dedup();
    let after = notes(&root)?;
    for (name, body) in &after {
        if incoming.get(name).and_then(Value::as_str) == Some(body) {
            base.insert(name.clone(), json!(body));
        }
    }
    let peer_dir = root.join(".brain-state/peers");
    private_dir(&peer_dir)?;
    atomic(&base_path, &format!("{}\n", crate::json::encode(&base)?))?;
    println!(
        "Brain sync: {} received, {} sent, {} conflicts",
        changed.len(),
        sent.len(),
        conflicts.len()
    );
    status["received"] = json!(changed.len());
    status["sent"] = json!(sent.len());
    status["conflicts"] = json!(conflicts.len());
    report_status(
        &status_path,
        &mut status,
        if conflicts.is_empty() {
            "success"
        } else {
            "conflicts"
        },
        if conflicts.is_empty() {
            "Knowledge synchronized"
        } else {
            "Sync completed with conflicts to reconcile"
        },
    )?;
    if !conflicts.is_empty() {
        eprintln!("Conflicts preserved under .brain-state/conflicts; reconcile deliberately.");
        return Ok(2);
    }
    Ok(0)
}

fn maintain(args: &[String]) -> Result<i32> {
    let action = args
        .first()
        .ok_or("the following arguments are required: command")?
        .as_str();
    let a = &args[1..];
    if action == "restore" {
        if a.len() != 2 {
            return Err(error("restore requires archive and destination"));
        }
        let destination = PathBuf::from(&a[1]);
        restore(Path::new(&a[0]), &destination)?;
        println!("{}", destination.display());
        return Ok(0);
    }
    let root = initialize()?;
    let _guard = VaultLock::acquire(&root)?;
    match action {
        "backup" => {
            println!("{}", backup(&root, None)?.display());
            Ok(0)
        }
        "setup" => {
            let home = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?);
            let installed = home.join(".local/share/nacre/brain/templates");
            for name in ["AGENTS.md", "CLAUDE.md", "INDEX.md"] {
                let p = root.join(name);
                if !p.exists() {
                    let body = if installed.is_dir() {
                        fs::read_to_string(installed.join(name))?
                    } else {
                        match name {
                            "AGENTS.md" => include_str!("../../../ai/brain/AGENTS.md"),
                            "CLAUDE.md" => include_str!("../../../ai/brain/CLAUDE.md"),
                            _ => include_str!("../../../ai/brain/INDEX.md"),
                        }
                        .to_owned()
                    };
                    atomic(&p, &body)?
                }
            }
            backup(&root, None)?;
            Ok(0)
        }
        "read" => {
            let name = a.first().ok_or("read requires path")?;
            let p = note_path(&root, name)?;
            let body = fs::read_to_string(p)?;
            println!("SHA256: {}\n{}", sha(body.as_bytes()), body);
            Ok(0)
        }
        "update" => {
            let name = a.first().ok_or("update requires path")?;
            if !name.starts_with("wiki/") && !ROOT_FILES.contains(&name.as_str()) {
                return Err(error(
                    "Only wiki pages and root instructions/index may be updated; preserve evidence",
                ));
            }
            let pos = a
                .iter()
                .position(|x| x == "--expected-sha256")
                .ok_or("--expected-sha256 is required")?;
            let expected = a.get(pos + 1).ok_or("--expected-sha256 is required")?;
            let p = note_path(&root, name)?;
            let old = if p.exists() {
                Some(fs::read_to_string(&p)?)
            } else {
                None
            };
            let actual = old.as_ref().map_or("new".into(), |b| sha(b.as_bytes()));
            if &actual != expected {
                return Err(error("Page changed; reread and reconcile before updating"));
            }
            let mut body = String::new();
            std::io::stdin().read_to_string(&mut body)?;
            safe_note(&body)?;
            if body.trim().is_empty() {
                return Err(error("Empty page"));
            }
            backup(&root, None)?;
            atomic(&p, &body)?;
            backup(&root, None)?;
            println!("{}", p.display());
            Ok(0)
        }
        "resolve" => {
            let id = a.first().ok_or("resolve requires conflict id")?;
            if id.len() != 64
                || !id
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(error("Invalid conflict ID"));
            }
            let p = root
                .join(".brain-state/conflicts")
                .join(format!("{id}.json"));
            let d = root.join(".brain-state/resolved");
            private_dir(&d)?;
            fs::rename(p, d.join(format!("{id}.json")))?;
            println!("Conflict evidence archived; run sync to verify convergence");
            Ok(0)
        }
        "check" => {
            let findings = check(&root)?;
            if findings.is_empty() {
                println!("Brain check passed")
            } else {
                println!("{}", findings.join("\n"))
            }
            Ok(if findings.is_empty() { 0 } else { 1 })
        }
        _ => Err(error("invalid choice")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_note_rejects_credentials_and_allows_refs() {
        for s in [
            "password: secret",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "sk-proj-xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        ] {
            assert!(safe_note(s).is_err())
        }
        assert!(safe_note("Credential reference: secret-service://nacre-brain/development").is_ok())
    }
    #[test]
    fn paths_reject_traversal_hidden_and_symlink() {
        let root = std::env::temp_dir().join(format!("nacre-brain-path-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        assert!(note_path(&root, "../outside.md").is_err());
        assert!(note_path(&root, ".codex/auth.json").is_err());
        assert!(note_path(&root, "wiki/page.md").is_ok());
        let outside = root.with_extension("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("wiki")).unwrap();
        assert!(note_path(&root, "wiki/page.md").is_err());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
    #[test]
    fn merges_preserve_divergence_and_ignore_the_shared_baseline() {
        let root = std::env::temp_dir().join(format!("nacre-brain-merge-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut a = Map::new();
        a.insert("wiki/p.md".into(), json!("local"));
        merge(&root, &a, &Map::new(), &[]).unwrap();
        let mut remote = Map::new();
        remote.insert("wiki/p.md".into(), json!("remote"));
        let mut base = Map::new();
        base.insert("wiki/p.md".into(), json!("base"));
        let (changed, conflicts) = merge(&root, &remote, &base, &[]).unwrap();
        assert!(changed.is_empty());
        assert_eq!(conflicts, vec!["wiki/p.md"]);
        assert_eq!(fs::read_to_string(root.join("wiki/p.md")).unwrap(), "local");
        assert_eq!(
            fs::read_dir(root.join(".brain-state/conflicts"))
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json"))
                .count(),
            1
        );
        let mut unchanged = Map::new();
        unchanged.insert("wiki/p.md".into(), json!("base"));
        let (changed, conflicts) = merge(&root, &unchanged, &base, &[]).unwrap();
        assert!(changed.is_empty() && conflicts.is_empty());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn snapshots_ignore_pending_and_unmanaged_files() {
        let root =
            std::env::temp_dir().join(format!("nacre-brain-snapshot-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("inbox")).unwrap();
        fs::write(root.join("inbox/.note-pending"), "partial").unwrap();
        fs::write(root.join("inbox/note.md"), "published").unwrap();
        fs::write(root.join("inbox/file.pdf"), "pdf").unwrap();
        assert_eq!(
            notes(&root).unwrap().get("inbox/note.md").unwrap(),
            "published"
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn incoming_batch_is_validated_before_any_write() {
        let root =
            std::env::temp_dir().join(format!("nacre-brain-validation-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        for (name, body) in [
            ("../outside.md", "x"),
            (".codex/auth.json", "x"),
            ("inbox/private.md", "password: secret"),
        ] {
            let mut data = Map::new();
            data.insert("inbox/first.md".into(), json!("valid"));
            data.insert(name.into(), json!(body));
            assert!(merge(&root, &data, &Map::new(), &[]).is_err());
            assert!(!root.join("inbox/first.md").exists());
        }
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn conflict_reply_preserves_local_text() {
        let root =
            std::env::temp_dir().join(format!("nacre-brain-excluded-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut first = Map::new();
        first.insert("wiki/p.md".into(), json!("mine"));
        merge(&root, &first, &Map::new(), &[]).unwrap();
        let mut reply = Map::new();
        reply.insert("wiki/p.md".into(), json!("theirs"));
        merge(&root, &reply, &Map::new(), &["wiki/p.md".into()]).unwrap();
        assert_eq!(fs::read_to_string(root.join("wiki/p.md")).unwrap(), "mine");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn one_sided_edit_propagates_and_old_backup_restores() {
        let root =
            std::env::temp_dir().join(format!("nacre-brain-restore-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut input = Map::new();
        input.insert("wiki/p.md".into(), json!("old"));
        merge(&root, &input, &Map::new(), &[]).unwrap();
        let archive = backup(&root, None).unwrap();
        let mut updated = Map::new();
        updated.insert("wiki/p.md".into(), json!("new"));
        let mut baseline = Map::new();
        baseline.insert("wiki/p.md".into(), json!("old"));
        assert_eq!(
            merge(&root, &updated, &baseline, &[]).unwrap(),
            (vec!["wiki/p.md".to_string()], Vec::<String>::new())
        );
        assert_eq!(notes(&root).unwrap()["wiki/p.md"], "new");
        let dest = root.with_extension("restore");
        restore(&archive, &dest).unwrap();
        assert_eq!(notes(&dest).unwrap()["wiki/p.md"], "old");
        assert!(restore(&archive, &dest).is_err());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(dest);
    }
    #[test]
    fn network_errors_keep_auth_protocol_and_offline_distinct() {
        assert_eq!(exchange_error(255, "Connection timed out").0, "offline");
        assert_eq!(
            exchange_error(255, "Permission denied (publickey)").0,
            "authentication"
        );
        assert_eq!(exchange_error(127, "not found").0, "protocol");
    }
    #[test]
    fn offline_backoff_preserves_success_and_increases_retry() {
        let root =
            std::env::temp_dir().join(format!("nacre-brain-status-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("status.json");
        let mut status = json!({});
        report_status_at(&path, &mut status, "success", "done", 1000.0).unwrap();
        report_status_at(&path, &mut status, "offline", "unavailable", 1000.0).unwrap();
        let first = status["nextRetry"].as_f64().unwrap();
        report_status_at(&path, &mut status, "offline", "unavailable", 1000.0).unwrap();
        assert_eq!(status["lastSuccess"], 1000.0);
        assert!(status["nextRetry"].as_f64().unwrap() > first);
        status["failures"] = json!(u64::MAX);
        report_status_at(&path, &mut status, "offline", "unavailable", 1000.0).unwrap();
        assert_eq!(status["failures"], u64::MAX);
        assert_eq!(status["nextRetry"], 4600.0);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn maintenance_check_reports_metadata_and_broken_links() {
        let root = std::env::temp_dir().join(format!("nacre-brain-check-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/p.md"), "[[missing]] [broken](absent.md)").unwrap();
        let found = check(&root).unwrap();
        assert!(found.iter().any(|x| x.contains("missing Source:")));
        assert!(found.iter().any(|x| x.contains("wiki link")));
        assert!(found.iter().any(|x| x.contains("relative link")));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn checkpoint_with_evidence_metadata_passes_check() {
        let root =
            std::env::temp_dir().join(format!("nacre-brain-checkpoint-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("checkpoints")).unwrap();
        fs::write(
            root.join("checkpoints/task.md"),
            "# Task\nRecorded: 2026-10-01\nConfidence: verified\nSource: test\nCompleted task.",
        )
        .unwrap();
        assert!(check(&root).unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn pending_notes_and_other_formats_are_ignored_in_snapshot() {
        let root = std::env::temp_dir().join(format!("nacre-brain-files-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("inbox")).unwrap();
        fs::write(root.join("inbox/.note-pending"), "partial").unwrap();
        fs::write(root.join("inbox/article.pdf"), "not managed").unwrap();
        fs::write(root.join("inbox/kept.md"), "body").unwrap();
        assert_eq!(
            notes(&root).unwrap(),
            BTreeMap::from([("inbox/kept.md".into(), "body".into())])
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn extended_managed_formats_and_root_files_survive_empty_incoming() {
        let root =
            std::env::temp_dir().join(format!("nacre-brain-formats-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let mut data = Map::new();
        for (name, body) in [
            ("wiki/views/current.base", "views: []"),
            ("wiki/map.canvas", "{}"),
            ("raw/article.txt", "source"),
            ("AGENTS.md", "rules"),
            ("wiki/back\\slash.md", "literal filename"),
        ] {
            data.insert(name.into(), json!(body));
        }
        merge(&root, &data, &Map::new(), &[]).unwrap();
        merge(&root, &Map::new(), &data, &[]).unwrap();
        assert_eq!(
            notes(&root).unwrap(),
            BTreeMap::from([
                ("wiki/views/current.base".into(), "views: []".into()),
                ("wiki/map.canvas".into(), "{}".into()),
                ("raw/article.txt".into(), "source".into()),
                ("AGENTS.md".into(), "rules".into()),
                ("wiki/back\\slash.md".into(), "literal filename".into())
            ])
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn linked_state_and_lock_targets_are_preserved() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("vault");
        fs::create_dir_all(root.join(".brain-state")).unwrap();
        let outside = temporary.path().join("outside");
        fs::write(&outside, "untouched").unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o644)).unwrap();
        let target = root.join(".brain-state/lock");
        fs::hard_link(&outside, &target).unwrap();
        assert!(VaultLock::acquire(&root).is_err());
        assert_eq!(fs::metadata(&outside).unwrap().mode() & 0o777, 0o644);
        assert!(atomic(&target, "changed").is_err());
        fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink(&outside, &target).unwrap();
        assert!(VaultLock::acquire(&root).is_err());
        assert!(atomic(&target, "changed").is_err());
        assert_eq!(fs::read_to_string(outside).unwrap(), "untouched");
    }
    #[test]
    fn linked_ancestors_and_root_paths_are_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let outside = temporary.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let link = temporary.path().join("linked");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(atomic(&link.join("note.md"), "changed").is_err());
        assert!(note_path(&link, "wiki/note.md").is_err());
        assert!(!outside.join("note.md").exists());
    }
    #[test]
    fn checks_decode_links_and_ignore_incomplete_markup_and_code() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/target.txt"), "evidence").unwrap();
        fs::write(root.join("wiki/page.md"), "Source: test\nReviewed: 2099-01-01\nStatus: current\n[valid](target%2Etxt)\n[absent](missing%2Etxt)\n```[[ignored]]```\n[[unfinished").unwrap();
        assert_eq!(
            check(root).unwrap(),
            vec!["wiki/page.md: missing relative link missing.txt"]
        );
        fs::write(
            root.join("wiki/page.md"),
            "Source: test\nReviewed: 2026-99-99\nStatus: current\n",
        )
        .unwrap();
        assert!(check(root).is_err());
    }
}
