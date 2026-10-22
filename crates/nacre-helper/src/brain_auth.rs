//! Restart-safe sessions and one-time bootstrap for the Brain loopback server.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const BOOTSTRAP_TTL: u64 = 300;
const SESSION_TTL: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
struct SessionFile {
    cookie: String,
    expires: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct BootstrapFile {
    token: String,
    expires: u64,
}

/// Authentication state for one listener port. Session credentials survive restarts;
/// bootstrap credentials are rotated at startup and expire after five minutes.
pub struct BrowserAuth {
    directory: PathBuf,
    port: u16,
    cookie: String,
    cookie_expires: u64,
    bootstrap: Option<BootstrapFile>,
}

impl BrowserAuth {
    pub fn open(state: &Path, port: u16) -> io::Result<Self> {
        let directory = state.join("auth").join("ports");
        create_private_dirs(&directory)?;
        fs::set_permissions(state.join("auth"), fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        let session_path = directory.join(format!("{port}.json"));
        refuse_symlink(&session_path)?;
        let existing = if session_path.exists() {
            let contents = read_private(&session_path, 4096)?;
            serde_json::from_slice::<SessionFile>(&contents)
                .ok()
                .filter(|value| value.expires > now().unwrap_or(0) && value.cookie.len() == 64 && value.cookie.bytes().all(|b| b.is_ascii_hexdigit()))
        } else {
            None
        };
        let cookie = match &existing {
            Some(value) => value.cookie.clone(),
            None => random_hex()?,
        };
        let cookie_expires = match existing {
            Some(value) => value.expires,
            None => now()?.saturating_add(SESSION_TTL),
        };
        write_private(&session_path, &to_json(&SessionFile { cookie: cookie.clone(), expires: cookie_expires })?)?;
        let bootstrap = BootstrapFile {
            token: random_hex()?,
            expires: now()?.saturating_add(BOOTSTRAP_TTL),
        };
        write_private(
            &directory.join(format!("{port}.bootstrap.json")),
            &to_json(&bootstrap)?,
        )?;
        write_handoff(state, port, &bootstrap.token, false)?;
        write_handoff(state, port, &bootstrap.token, true)?;
        Ok(Self { directory, port, cookie, cookie_expires, bootstrap: Some(bootstrap) })
    }

    pub fn cookie_name(&self) -> String { format!("nacre_brain_{}", self.port) }
    pub fn cookie_value(&self) -> &str { &self.cookie }
    pub fn cookie_max_age(&self) -> u64 { self.cookie_expires.saturating_sub(now().unwrap_or(0)) }
    pub fn bootstrap_token(&self) -> Option<&str> { self.bootstrap.as_ref().map(|value| value.token.as_str()) }

    pub fn cookie_valid(&self, candidate: &str) -> bool {
        self.cookie_expires >= now().unwrap_or(0) && constant_time_eq(self.cookie.as_bytes(), candidate.as_bytes())
    }

    /// Consume and persistently revoke the startup token on its first successful use.
    pub fn consume_bootstrap(&mut self, candidate: &str) -> bool {
        self.bootstrap = read_bootstrap(&self.directory.join(format!("{}.bootstrap.json", self.port))).ok();
        let Some(value) = self.bootstrap.as_ref() else { return false };
        if value.expires <= now().unwrap_or(0) || !constant_time_eq(value.token.as_bytes(), candidate.as_bytes()) {
            return false;
        }
        self.bootstrap = None;
        let path = self.directory.join(format!("{}.bootstrap.json", self.port));
        if fs::remove_file(path).is_err() { return false; }
        let marker = self.directory.join(format!("{}.browser.json", self.port));
        let digest = format!("{:x}", Sha256::digest(self.cookie.as_bytes()));
        to_json(&serde_json::json!({"session": digest})).and_then(|bytes| write_private(&marker, &bytes)).is_ok()
    }
}

/// Rotate bootstrap and materialize a handoff page for an explicit browser launch.
pub fn refresh_bootstrap(state: &Path, port: u16, handoff: bool) -> io::Result<PathBuf> {
    let directory = state.join("auth").join("ports");
    create_private_dirs(&directory)?;
    fs::set_permissions(state.join("auth"), fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let bootstrap = BootstrapFile { token: random_hex()?, expires: now()?.saturating_add(BOOTSTRAP_TTL) };
    write_private(&directory.join(format!("{port}.bootstrap.json")), &to_json(&bootstrap)?)?;
    write_handoff(state, port, &bootstrap.token, handoff)?;
    let path = state.join("auth").join(if handoff { "handoff.html" } else { "open.html" });
    Ok(path)
}

fn write_handoff(state: &Path, port: u16, token: &str, handoff: bool) -> io::Result<()> {
    let url = format!("http://127.0.0.1:{port}/?token={token}&handoff={}", u8::from(handoff));
    let html = format!("<!doctype html><meta charset=\"utf-8\"><script>location.replace({})</script>", serde_json::to_string(&url).map_err(io::Error::other)?);
    let path = state.join("auth").join(if handoff { "handoff.html" } else { "open.html" });
    write_private(&path, html.as_bytes())
}

/// True when a browser has completed bootstrap for the persistent session on port.
pub fn browser_ready(state: &Path, port: u16) -> io::Result<bool> {
    let directory = state.join("auth").join("ports");
    let session = directory.join(format!("{port}.json"));
    let marker = directory.join(format!("{port}.browser.json"));
    refuse_symlink(&session)?;
    refuse_symlink(&marker)?;
    let session: SessionFile = match read_private(&session, 4096) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if session.expires <= now()? { return Ok(false); }
    let stored: serde_json::Value = match read_private(&marker, 4096) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let expected = format!("{:x}", Sha256::digest(session.cookie.as_bytes()));
    Ok(stored.get("session").and_then(serde_json::Value::as_str).is_some_and(|value| constant_time_eq(value.as_bytes(), expected.as_bytes())))
}

fn read_bootstrap(path: &Path) -> io::Result<BootstrapFile> {
    serde_json::from_slice(&read_private(path, 4096)?).map_err(io::Error::other)
}

fn read_private(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    refuse_symlink(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > limit {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "unsafe authentication state file"));
    }
    fs::read(path)
}

fn to_json<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(io::Error::other)
}

fn now() -> io::Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_secs())
}

fn random_hex() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    OpenOptions::new().read(true).open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let max = left.len().max(right.len());
    for index in 0..max {
        difference |= usize::from(*left.get(index).unwrap_or(&0) ^ *right.get(index).unwrap_or(&0));
    }
    difference == 0
}

fn refuse_symlink(path: &Path) -> io::Result<()> {
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "linked authentication state ancestor"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn create_private_dirs(path: &Path) -> io::Result<()> {
    refuse_symlink(path)?;
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        match fs::create_dir(&current) {
            Ok(()) => fs::set_permissions(&current, fs::Permissions::from_mode(0o700))?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(io::Error::new(io::ErrorKind::PermissionDenied, "unsafe authentication state directory"));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    refuse_symlink(path)?;
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.as_file().set_permissions(fs::Permissions::from_mode(0o600))?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn credentials_are_port_bound_restart_safe_and_bootstrap_is_one_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = BrowserAuth::open(dir.path(), 17843).unwrap();
        let cookie = first.cookie_value().to_owned();
        let token = first.bootstrap_token().unwrap().to_owned();
        assert!(first.cookie_valid(&cookie));
        assert!(!browser_ready(dir.path(), 17843).unwrap());
        assert!(first.consume_bootstrap(&token));
        assert!(browser_ready(dir.path(), 17843).unwrap());
        assert!(!first.consume_bootstrap(&token));
        let page = refresh_bootstrap(dir.path(), 17843, false).unwrap();
        let content = fs::read_to_string(page).unwrap();
        let refreshed = content.split("token=").nth(1).unwrap().split('&').next().unwrap().to_owned();
        assert!(first.consume_bootstrap(&refreshed));
        let expired_bootstrap = BootstrapFile { token: "stale".into(), expires: now().unwrap() - 1 };
        write_private(&dir.path().join("auth/ports/17843.bootstrap.json"), &to_json(&expired_bootstrap).unwrap()).unwrap();
        assert!(!first.consume_bootstrap("stale"));
        let restarted = BrowserAuth::open(dir.path(), 17843).unwrap();
        assert!(!restarted.consume_bootstrap(&refreshed));
        assert!(restarted.cookie_valid(&cookie));
        let expired = SessionFile { cookie: restarted.cookie_value().to_owned(), expires: now().unwrap() - 1 };
        write_private(&dir.path().join("auth/ports/17843.json"), &to_json(&expired).unwrap()).unwrap();
        let rotated = BrowserAuth::open(dir.path(), 17843).unwrap();
        assert_ne!(rotated.cookie_value(), cookie);
        assert!(!rotated.cookie_valid(&cookie));
        let other_port = BrowserAuth::open(dir.path(), 17844).unwrap();
        assert_ne!(rotated.cookie_name(), other_port.cookie_name());
        assert!(!other_port.cookie_valid(&cookie));
        assert_eq!(fs::metadata(dir.path().join("auth/ports/17843.json")).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(dir.path().join("auth")).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(dir.path().join("auth/ports")).unwrap().permissions().mode() & 0o777, 0o700);
        for filename in ["open.html", "handoff.html"] {
            assert_eq!(fs::metadata(dir.path().join("auth").join(filename)).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn symlinked_state_ancestor_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        let linked = dir.path().join("linked");
        symlink(&target, &linked).unwrap();
        assert!(BrowserAuth::open(&linked, 17843).is_err());
    }
}
