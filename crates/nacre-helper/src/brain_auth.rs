//! Restart-safe sessions and one-time bootstrap for the Brain loopback server.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const BOOTSTRAP_TTL: u64 = 300;
const SESSION_TTL: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
struct SessionFile {
    cookie: String,
    bearer: String,
    expires: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct BootstrapFile {
    token: String,
    expires: u64,
}

/// Authentication state for one listener port. Session credentials survive restarts;
/// bootstrap credentials are rotated at startup and expire after five minutes.
/// Browser cookies are still host-scoped across ports; HTTP enforces same-origin
/// Fetch Metadata on protected APIs rather than treating the name as port isolation.
pub struct BrowserAuth {
    directory: PathBuf,
    port: u16,
    cookie: String,
    bearer: String,
    cookie_expires: u64,
    bootstrap: Option<BootstrapFile>,
}

impl BrowserAuth {
    pub fn open(state: &Path, port: u16) -> io::Result<Self> {
        let directory = state.join("auth").join("ports");
        secure_state_dirs(state, &directory)?;
        let _lock = auth_lock(&directory, port)?;
        let session = active_session(&directory, port)?;
        let cookie = session.cookie;
        let bearer = session.bearer;
        let cookie_expires = session.expires;
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
        Ok(Self {
            directory,
            port,
            cookie,
            bearer,
            cookie_expires,
            bootstrap: Some(bootstrap),
        })
    }

    /// A distinct session lookup key; browser cookie transport itself is host-scoped.
    pub fn cookie_name(&self) -> String {
        format!("nacre_brain_{}", self.port)
    }
    pub fn cookie_value(&self) -> &str {
        &self.cookie
    }
    pub fn bearer_value(&self) -> &str {
        &self.bearer
    }
    pub fn cookie_max_age(&self) -> u64 {
        self.cookie_expires.saturating_sub(now().unwrap_or(0))
    }
    pub fn bootstrap_token(&self) -> Option<&str> {
        self.bootstrap.as_ref().map(|value| value.token.as_str())
    }

    pub fn cookie_valid(&self, candidate: &str) -> bool {
        self.cookie_expires > now().unwrap_or(0)
            && constant_time_eq(self.cookie.as_bytes(), candidate.as_bytes())
    }

    pub fn bearer_valid(&self, candidate: &str) -> bool {
        self.cookie_expires > now().unwrap_or(0)
            && constant_time_eq(self.bearer.as_bytes(), candidate.as_bytes())
    }

    /// Consume and persistently revoke the startup token on its first successful use.
    pub fn consume_bootstrap(&mut self, candidate: &str) -> bool {
        let Ok(_lock) = auth_lock(&self.directory, self.port) else {
            return false;
        };
        self.bootstrap =
            read_bootstrap(&self.directory.join(format!("{}.bootstrap.json", self.port))).ok();
        let Some(value) = self.bootstrap.as_ref() else {
            return false;
        };
        if value.expires <= now().unwrap_or(0)
            || !constant_time_eq(value.token.as_bytes(), candidate.as_bytes())
        {
            return false;
        }
        // A trusted launch can renew an expired session without restarting the
        // compatibility watcher. Only redeeming its fresh bootstrap adopts it.
        let session_path = self.directory.join(format!("{}.json", self.port));
        let Ok(bytes) = read_private(&session_path, 4096) else {
            return false;
        };
        let Ok(session) = serde_json::from_slice::<SessionFile>(&bytes) else {
            return false;
        };
        if session.expires <= now().unwrap_or(0)
            || !valid_secret(&session.cookie)
            || !valid_secret(&session.bearer)
        {
            return false;
        }
        self.cookie = session.cookie;
        self.bearer = session.bearer;
        self.cookie_expires = session.expires;
        self.bootstrap = None;
        let path = self.directory.join(format!("{}.bootstrap.json", self.port));
        if fs::remove_file(path).is_err() {
            return false;
        }
        let marker = self.directory.join(format!("{}.browser.json", self.port));
        let digest = format!(
            "{:x}",
            Sha256::digest(format!("{}\0{}", self.cookie, self.bearer).as_bytes())
        );
        to_json(&serde_json::json!({"session": digest}))
            .and_then(|bytes| write_private(&marker, &bytes))
            .is_ok()
    }
}

/// Rotate bootstrap and materialize a handoff page for an explicit browser launch.
pub fn refresh_bootstrap(state: &Path, port: u16, handoff: bool) -> io::Result<PathBuf> {
    let directory = state.join("auth").join("ports");
    secure_state_dirs(state, &directory)?;
    let _lock = auth_lock(&directory, port)?;
    active_session(&directory, port)?;
    let bootstrap = BootstrapFile {
        token: random_hex()?,
        expires: now()?.saturating_add(BOOTSTRAP_TTL),
    };
    write_private(
        &directory.join(format!("{port}.bootstrap.json")),
        &to_json(&bootstrap)?,
    )?;
    write_handoff(state, port, &bootstrap.token, handoff)?;
    let path = state
        .join("auth")
        .join(if handoff { "handoff.html" } else { "open.html" });
    Ok(path)
}

fn active_session(directory: &Path, port: u16) -> io::Result<SessionFile> {
    let path = directory.join(format!("{port}.json"));
    let existing = match read_private(&path, 4096) {
        Ok(bytes) => serde_json::from_slice::<SessionFile>(&bytes).ok(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if let Some(session) = existing.filter(|v| {
        v.expires > now().unwrap_or(0) && valid_secret(&v.cookie) && valid_secret(&v.bearer)
    }) {
        return Ok(session);
    }
    let session = SessionFile {
        cookie: random_hex()?,
        bearer: random_hex()?,
        expires: now()?.saturating_add(SESSION_TTL),
    };
    write_private(&path, &to_json(&session)?)?;
    Ok(session)
}

fn write_handoff(state: &Path, port: u16, token: &str, handoff: bool) -> io::Result<()> {
    let url = format!(
        "http://127.0.0.1:{port}/?token={token}&handoff={}",
        u8::from(handoff)
    );
    let html = format!(
        "<!doctype html><meta charset=\"utf-8\"><script>location.replace({})</script>",
        serde_json::to_string(&url).map_err(io::Error::other)?
    );
    let path = state
        .join("auth")
        .join(if handoff { "handoff.html" } else { "open.html" });
    write_private(&path, html.as_bytes())
}

/// True when a browser has completed bootstrap for the persistent session on port.
pub fn browser_ready(state: &Path, port: u16) -> io::Result<bool> {
    let directory = state.join("auth").join("ports");
    secure_state_dirs(state, &directory)?;
    let _lock = auth_lock(&directory, port)?;
    let session = directory.join(format!("{port}.json"));
    let marker = directory.join(format!("{port}.browser.json"));
    refuse_symlink(&session)?;
    refuse_symlink(&marker)?;
    let session: SessionFile = match read_private(&session, 4096) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if session.expires <= now()? {
        return Ok(false);
    }
    let stored: serde_json::Value = match read_private(&marker, 4096) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let expected = format!(
        "{:x}",
        Sha256::digest(format!("{}\0{}", session.cookie, session.bearer).as_bytes())
    );
    Ok(stored
        .get("session")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| constant_time_eq(value.as_bytes(), expected.as_bytes())))
}

fn read_bootstrap(path: &Path) -> io::Result<BootstrapFile> {
    serde_json::from_slice(&read_private(path, 4096)?).map_err(io::Error::other)
}

fn read_private(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    refuse_symlink(path)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != nix::unistd::getuid().as_raw()
        || metadata.len() > limit
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe authentication state file",
        ));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "authentication state file exceeds limit",
        ));
    }
    Ok(bytes)
}

fn secure_state_dirs(state: &Path, ports: &Path) -> io::Result<()> {
    create_private_dirs(ports)?;
    fs::set_permissions(state, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(state.join("auth"), fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(ports, fs::Permissions::from_mode(0o700))
}

fn auth_lock(directory: &Path, port: u16) -> io::Result<std::fs::File> {
    let path = directory.join(format!("{port}.lock"));
    refuse_symlink(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(&path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != nix::unistd::getuid().as_raw()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe authentication lock",
        ));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)?;
    Ok(file)
}

fn to_json<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    serde_json::to_vec(value).map_err(io::Error::other)
}

fn valid_secret(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn now() -> io::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs())
}

fn random_hex() -> io::Result<String> {
    let mut bytes = [0_u8; 32];
    OpenOptions::new()
        .read(true)
        .open("/dev/urandom")?
        .read_exact(&mut bytes)?;
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
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "linked authentication state ancestor",
                ));
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
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "unsafe authentication state directory",
                    ));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    refuse_symlink(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
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
        let refreshed = content
            .split("token=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        assert!(first.consume_bootstrap(&refreshed));
        let expired_bootstrap = BootstrapFile {
            token: "stale".into(),
            expires: now().unwrap() - 1,
        };
        write_private(
            &dir.path().join("auth/ports/17843.bootstrap.json"),
            &to_json(&expired_bootstrap).unwrap(),
        )
        .unwrap();
        assert!(!first.consume_bootstrap("stale"));
        let mut restarted = BrowserAuth::open(dir.path(), 17843).unwrap();
        assert!(!restarted.consume_bootstrap(&refreshed));
        assert!(restarted.cookie_valid(&cookie));
        let expired = SessionFile {
            cookie: restarted.cookie_value().to_owned(),
            bearer: restarted.bearer_value().to_owned(),
            expires: now().unwrap() - 1,
        };
        write_private(
            &dir.path().join("auth/ports/17843.json"),
            &to_json(&expired).unwrap(),
        )
        .unwrap();
        let rotated = BrowserAuth::open(dir.path(), 17843).unwrap();
        assert_ne!(rotated.cookie_value(), cookie);
        assert!(!rotated.cookie_valid(&cookie));
        let other_port = BrowserAuth::open(dir.path(), 17844).unwrap();
        assert_ne!(rotated.cookie_name(), other_port.cookie_name());
        assert!(!other_port.cookie_valid(&cookie));
        assert_eq!(
            fs::metadata(dir.path().join("auth/ports/17843.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(dir.path().join("auth"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.path().join("auth/ports"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.path().join("auth/ports/17843.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        for filename in ["open.html", "handoff.html"] {
            assert_eq!(
                fs::metadata(dir.path().join("auth").join(filename))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
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

    #[test]
    fn authentication_state_rejects_hardlinks_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let ports = dir.path().join("auth/ports");
        fs::create_dir_all(&ports).unwrap();
        let session = ports.join("17843.json");
        let linked_source = dir.path().join("source.json");
        fs::write(&linked_source, br#"{"cookie":"0123456789012345678901234567890123456789012345678901234567890123","bearer":"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789","expires":4102444800}"#).unwrap();
        fs::set_permissions(&linked_source, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&linked_source, &session).unwrap();
        assert!(BrowserAuth::open(dir.path(), 17843).is_err());

        fs::remove_file(&session).unwrap();
        fs::remove_file(&linked_source).unwrap();
        fs::write(&session, vec![b'x'; 4097]).unwrap();
        fs::set_permissions(&session, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(BrowserAuth::open(dir.path(), 17843).is_err());
    }

    #[test]
    fn repairs_permissions_on_existing_state_directories() {
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join("auth");
        let ports = auth.join("ports");
        fs::create_dir(&auth).unwrap();
        fs::create_dir(&ports).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&auth, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&ports, fs::Permissions::from_mode(0o755)).unwrap();
        let _server_auth = BrowserAuth::open(dir.path(), 17843).unwrap();
        for path in [dir.path(), auth.as_path(), ports.as_path()] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }

    #[test]
    fn concurrent_bootstrap_refresh_leaves_page_and_token_in_sync() {
        let dir = tempfile::tempdir().unwrap();
        let mut auth = BrowserAuth::open(dir.path(), 17843).unwrap();
        let mut workers = Vec::new();
        for _ in 0..4 {
            let state = dir.path().to_path_buf();
            workers.push(std::thread::spawn(move || {
                for _ in 0..3 {
                    refresh_bootstrap(&state, 17843, false).unwrap();
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        let page = fs::read_to_string(dir.path().join("auth/open.html")).unwrap();
        let token = page
            .split("token=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        assert!(auth.consume_bootstrap(&token));
    }

    #[test]
    fn fifo_authentication_state_is_rejected_without_waiting_for_a_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        nix::unistd::mkfifo(
            &path,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        assert!(read_private(&path, 4096).is_err());
    }

    #[test]
    fn explicit_launch_renews_expired_session_without_restarting_server() {
        let dir = tempfile::tempdir().unwrap();
        let mut auth = BrowserAuth::open(dir.path(), 17843).unwrap();
        let old_cookie = auth.cookie.clone();
        let old_bearer = auth.bearer.clone();
        auth.cookie_expires = now().unwrap() - 1;
        write_private(
            &dir.path().join("auth/ports/17843.json"),
            &to_json(&SessionFile {
                cookie: old_cookie.clone(),
                bearer: old_bearer.clone(),
                expires: auth.cookie_expires,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(!auth.cookie_valid(&old_cookie));
        let page = refresh_bootstrap(dir.path(), 17843, false).unwrap();
        let page = fs::read_to_string(page).unwrap();
        let token = page
            .split("token=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap();
        assert!(auth.consume_bootstrap(token));
        assert!(!auth.cookie_valid(&old_cookie));
        assert!(!auth.bearer_valid(&old_bearer));
        assert!(auth.cookie_valid(auth.cookie_value()));
        assert!(auth.bearer_valid(auth.bearer_value()));
        assert!(auth.cookie_max_age() > 0);
    }
}
