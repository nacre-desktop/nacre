//! Private Unix listener, validated systemd activation and same-user clients.
use crate::Result;
use nix::{
    fcntl::{Flock, FlockArg},
    sys::socket::{
        connect, getsockopt, socket, sockopt, AddressFamily, SockFlag, SockType, UnixAddr,
    },
};
use std::{
    env,
    fs::{self, File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::UnixListener,
        },
    },
    path::{Component, Path, PathBuf},
};

fn validate_listener(socket: &impl std::os::fd::AsFd) -> Result<()> {
    if !getsockopt(socket, sockopt::AcceptConn)? {
        return Err("Activated Unix socket is not listening".into());
    }
    Ok(())
}

pub fn path() -> Result<PathBuf> {
    path_from(
        env::var_os("NACRE_DAEMON_SOCKET").as_deref(),
        env::var_os("XDG_RUNTIME_DIR").as_deref(),
    )
}
pub(crate) fn path_from(
    selected: Option<&std::ffi::OsStr>,
    runtime: Option<&std::ffi::OsStr>,
) -> Result<PathBuf> {
    let path = if let Some(path) = selected {
        PathBuf::from(path)
    } else {
        PathBuf::from(runtime.ok_or("XDG_RUNTIME_DIR is required")?).join("nacre/daemon.sock")
    };
    if !path.is_absolute()
        || path.as_os_str().as_bytes().len() > 107
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("Invalid daemon socket path".into());
    }
    Ok(path)
}
fn directory(path: &Path, create: bool) -> Result<()> {
    // A user namespace can expose filesystem-root ownership as an unmapped UID.
    // Trust that anchor for ancestors only; the leaf must still have our exact UID.
    let root_uid = fs::symlink_metadata("/")?.uid();
    let mut part = PathBuf::new();
    for component in path.components() {
        part.push(component.as_os_str());
        match fs::symlink_metadata(&part) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err("Refusing linked or non-directory socket ancestry".into())
            }
            Ok(meta) if !ancestor_uid(meta.uid(), nix::unistd::getuid().as_raw(), root_uid) => {
                return Err("Refusing foreign socket ancestry".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                fs::DirBuilder::new().mode(0o700).create(&part)?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.uid() != nix::unistd::getuid().as_raw() || meta.mode() & 0o777 != 0o700 {
        return Err("Daemon socket folder must be owned by this user with mode 0700".into());
    }
    Ok(())
}
fn ancestor_uid(uid: u32, own: u32, root: u32) -> bool {
    uid == 0 || uid == own || uid == root
}
pub fn validate_endpoint(path: &Path) -> Result<()> {
    directory(path.parent().ok_or("Invalid daemon socket parent")?, false)?;
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_socket()
        || meta.uid() != nix::unistd::getuid().as_raw()
        || meta.mode() & 0o777 != 0o600
    {
        return Err("Daemon socket must be owned by this user with mode 0600".into());
    }
    Ok(())
}
pub fn check_uid(uid: u32) -> Result<()> {
    if uid != nix::unistd::getuid().as_raw() {
        Err("Daemon peer UID mismatch".into())
    } else {
        Ok(())
    }
}
pub struct Listener {
    listener: Option<UnixListener>,
    pub path: PathBuf,
    _lock: Flock<File>,
    cleanup: Option<(u64, u64)>,
}
impl Listener {
    pub fn acquire(path: PathBuf) -> Result<Self> {
        let parent = path.parent().ok_or("Invalid daemon socket parent")?;
        directory(parent, true)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(parent.join("daemon.lock"))?;
        let meta = lock.metadata()?;
        if !meta.is_file()
            || meta.uid() != nix::unistd::getuid().as_raw()
            || meta.nlink() != 1
            || meta.mode() & 0o777 != 0o600
        {
            return Err("Refusing unsafe daemon lock".into());
        }
        let lock = Flock::lock(lock, FlockArg::LockExclusiveNonblock)
            .map_err(|_| "Another daemon owns this socket folder")?;
        let inherited = env::var("LISTEN_FDS").ok().filter(|value| value != "0");
        let (listener, cleanup) = if let Some(count) = inherited {
            if count != "1"
                || env::var("LISTEN_PID").ok().as_deref()
                    != Some(std::process::id().to_string().as_str())
            {
                return Err("Invalid socket activation environment".into());
            }
            let mut fds = listenfd::ListenFd::from_env();
            let listener = fds
                .take_unix_listener(0)?
                .ok_or("Missing activated Unix listener")?;
            validate_listener(&listener)?;
            if listener.local_addr()?.as_pathname() != Some(path.as_path()) {
                return Err("Activated socket path differs from daemon endpoint".into());
            }
            validate_endpoint(&path)?;
            (listener, None)
        } else {
            match fs::symlink_metadata(&path) {
                Ok(meta) => {
                    if !meta.file_type().is_socket() || meta.uid() != nix::unistd::getuid().as_raw()
                    {
                        return Err("Refusing occupied or foreign daemon endpoint".into());
                    }
                    let probe = socket(
                        AddressFamily::Unix,
                        SockType::Stream,
                        SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
                        None,
                    )?;
                    match connect(probe.as_raw_fd(), &UnixAddr::new(&path)?) {
                        Ok(()) => return Err("Another server owns the daemon endpoint".into()),
                        Err(nix::errno::Errno::ECONNREFUSED) => fs::remove_file(&path)?,
                        Err(nix::errno::Errno::ENOENT) => {}
                        Err(_) => return Err("Daemon endpoint is active or unavailable".into()),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            let listener = UnixListener::bind(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            let meta = fs::symlink_metadata(&path)?;
            (listener, Some((meta.dev(), meta.ino())))
        };
        nix::fcntl::fcntl(
            &listener,
            nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC),
        )?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener: Some(listener),
            path,
            _lock: lock,
            cleanup,
        })
    }
    pub fn take(&mut self) -> Result<UnixListener> {
        self.listener
            .take()
            .ok_or_else(|| "Daemon listener was already taken".into())
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        if let Some((device, inode)) = self.cleanup {
            match fs::symlink_metadata(&self.path) {
                Ok(meta) if meta.dev() == device && meta.ino() == inode => {
                    if fs::remove_file(&self.path).is_err() {
                        tracing::warn!("Daemon socket cleanup failed");
                    }
                }
                Ok(_) => tracing::warn!("Daemon endpoint changed before cleanup"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => tracing::warn!("Daemon socket cleanup inspection failed"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_requires_a_listener_not_a_connected_unix_stream() -> Result<()> {
        let home = tempfile::tempdir()?;
        let listener = UnixListener::bind(home.path().join("listener"))?;
        validate_listener(&listener)?;
        let (connected, _peer) = std::os::unix::net::UnixStream::pair()?;
        assert!(validate_listener(&connected).is_err());
        Ok(())
    }
    #[test]
    fn namespace_root_is_allowed_only_as_an_ancestor() {
        assert!(ancestor_uid(42, 1000, 42));
        assert!(ancestor_uid(0, 1000, 42));
        assert!(!ancestor_uid(1001, 1000, 42));
    }
    #[test]
    fn private_modes_link_refusal_uid_and_duplicate_owner_are_enforced() -> Result<()> {
        let home = tempfile::tempdir()?;
        let endpoint = home.path().join("private/daemon.sock");
        let owner = Listener::acquire(endpoint.clone())?;
        validate_endpoint(&endpoint)?;
        assert!(Listener::acquire(endpoint.clone()).is_err());
        assert!(check_uid(nix::unistd::getuid().as_raw()).is_ok());
        assert!(check_uid(nix::unistd::getuid().as_raw().wrapping_add(1)).is_err());
        drop(owner);
        assert!(!endpoint.exists());
        std::os::unix::fs::symlink(home.path(), home.path().join("linked"))?;
        assert!(Listener::acquire(home.path().join("linked/daemon.sock")).is_err());
        fs::create_dir(home.path().join("insecure"))?;
        fs::set_permissions(
            home.path().join("insecure"),
            fs::Permissions::from_mode(0o755),
        )?;
        assert!(Listener::acquire(home.path().join("insecure/daemon.sock")).is_err());
        Ok(())
    }
    #[test]
    fn stale_owned_socket_is_replaced_but_regular_files_are_not() -> Result<()> {
        let home = tempfile::tempdir()?;
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
        let endpoint = home.path().join("daemon.sock");
        drop(UnixListener::bind(&endpoint)?);
        let listener = Listener::acquire(endpoint.clone())?;
        drop(listener);
        fs::write(&endpoint, "preserved")?;
        assert!(Listener::acquire(endpoint.clone()).is_err());
        assert_eq!(fs::read_to_string(endpoint)?, "preserved");
        Ok(())
    }
}
