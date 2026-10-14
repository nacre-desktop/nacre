//! Shared atomic private file publication and advisory locks.
use crate::{json, Result};
use nix::fcntl::{Flock, FlockArg};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
};

pub fn atomic(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid state path")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(content)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}
pub fn json_write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    atomic(path, json::encode(value)?.as_bytes())
}
pub fn lock(path: &Path) -> Result<Flock<File>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, error)| error.into())
}
