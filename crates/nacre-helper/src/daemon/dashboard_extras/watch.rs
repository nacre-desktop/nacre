//! Idle-free directory watcher. A missing directory is retried through its parent.
use crate::Result;
use rustix::fs::inotify::{self, CreateFlags, Reader, WatchFlags};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    mem::MaybeUninit,
    os::{fd::OwnedFd, unix::ffi::OsStrExt},
    path::PathBuf,
};
use tokio::io::unix::AsyncFd;

const IGNORED: u32 = 0x8000;

pub struct Watch {
    fd: AsyncFd<OwnedFd>,
    dirs: Vec<(PathBuf, WatchFlags)>,
    /// wd -> (directory, is a temporary parent watch for a missing directory)
    active: BTreeMap<i32, (PathBuf, bool)>,
    buffer: Vec<MaybeUninit<u8>>,
}

impl Watch {
    pub fn new(dirs: Vec<(PathBuf, WatchFlags)>) -> Result<Self> {
        let fd = inotify::init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC)?;
        // SAFETY: AsyncFd owns this descriptor throughout its registration.
        #[allow(unsafe_code)]
        let fd = unsafe { AsyncFd::register(fd) }?;
        let mut watch = Self {
            fd,
            dirs,
            active: BTreeMap::new(),
            buffer: vec![MaybeUninit::uninit(); 16384],
        };
        watch.arm();
        Ok(watch)
    }
    /// Replace the watched set (for example after a Brain refresh).
    pub fn set(&mut self, dirs: Vec<(PathBuf, WatchFlags)>) {
        let keep: Vec<_> = self
            .active
            .iter()
            .filter(|(_, (path, parent))| !parent && !dirs.iter().any(|(d, _)| d == path))
            .map(|(wd, _)| *wd)
            .collect();
        for wd in keep {
            let _ = inotify::remove_watch(self.fd.get_ref(), wd);
            self.active.remove(&wd);
        }
        self.dirs = dirs;
        self.arm();
    }
    fn arm(&mut self) {
        for (dir, flags) in self.dirs.clone() {
            if self.active.values().any(|(p, parent)| *p == dir && !parent) {
                continue;
            }
            match inotify::add_watch(self.fd.get_ref(), &dir, flags | WatchFlags::ONLYDIR) {
                Ok(wd) => {
                    self.active.insert(wd, (dir.clone(), false));
                    if let Some(parent) = self
                        .active
                        .iter()
                        .find(|(_, (p, parent))| *p == dir && *parent)
                        .map(|(wd, _)| *wd)
                    {
                        let _ = inotify::remove_watch(self.fd.get_ref(), parent);
                        self.active.remove(&parent);
                    }
                }
                Err(_) => {
                    let covered = self.active.values().any(|(p, parent)| *p == dir && *parent);
                    let Some(parent) = dir.parent().filter(|_| !covered) else {
                        continue;
                    };
                    let flags = WatchFlags::CREATE
                        | WatchFlags::MOVED_TO
                        | WatchFlags::ONLYDIR
                        | WatchFlags::MASK_ADD;
                    if let Ok(wd) = inotify::add_watch(self.fd.get_ref(), parent, flags) {
                        // A parent that is itself a real watch keeps its identity.
                        self.active.entry(wd).or_insert((dir, true));
                    }
                }
            }
        }
    }
    fn read(fd: &OwnedFd, buffer: &mut [MaybeUninit<u8>]) -> std::io::Result<Vec<OsString>> {
        let mut reader = Reader::new(fd, buffer);
        let mut names = Vec::new();
        loop {
            let event = reader.next().map_err(std::io::Error::from)?;
            if event.events().bits() & IGNORED == 0 {
                names.push(OsString::from(std::ffi::OsStr::from_bytes(
                    event.file_name().map_or(b"".as_slice(), |n| n.to_bytes()),
                )));
            }
            if reader.is_buffer_empty() {
                return Ok(names);
            }
        }
    }
    /// Wait for at least one real event; returns the changed entry names.
    pub async fn next(&mut self) -> Result<Vec<OsString>> {
        loop {
            let Self { fd, buffer, .. } = self;
            let mut ready = fd.readable().await?;
            let result = ready.try_io(|fd| Self::read(fd.get_ref(), buffer));
            drop(ready);
            if let Ok(names) = result {
                let names = names?;
                self.arm();
                if !names.is_empty() {
                    return Ok(names);
                }
            }
        }
    }
}
