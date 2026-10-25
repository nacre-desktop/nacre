//! Exclusive ownership of the config and state roots used by the daemon.
use crate::Result;
use nix::{
    fcntl::{openat, AtFlags, Flock, FlockArg, OFlag},
    sys::stat::{fstat, fstatat, mkdirat, Mode, SFlag},
    unistd::Uid,
};
use std::{
    fs::File,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Component, Path, PathBuf},
};

const BUSY: &str = "Nacre destination busy or unsafe";

struct PinnedPath {
    path: PathBuf,
    dirs: Vec<File>,
    names: Vec<String>,
}

struct WriterRoot {
    paths: Vec<PinnedPath>,
    lock: Flock<File>,
}

pub struct WriterLease {
    config_path: PathBuf,
    state_path: PathBuf,
    roots: Vec<WriterRoot>,
}

fn check_dir(stat: &nix::sys::stat::FileStat, uid: u32, root_uid: u32) -> Result<()> {
    if stat.st_mode & SFlag::S_IFMT.bits() != SFlag::S_IFDIR.bits()
        || (stat.st_uid != uid && stat.st_uid != root_uid)
        || (stat.st_mode & 0o022 != 0
            && !(stat.st_uid == root_uid && stat.st_mode & 0o7777 == 0o1777))
    {
        return Err(BUSY.into());
    }
    Ok(())
}

fn validate_chain(
    dirs: &[File],
    names: &[String],
    uid: u32,
    root_uid: u32,
    final_private: bool,
) -> Result<()> {
    let mut anchor_next = false;
    for (index, dir) in dirs.iter().enumerate() {
        let opened = fstat(dir)?;
        if anchor_next {
            if opened.st_uid != uid || opened.st_mode & 0o7777 != 0o700 {
                return Err(BUSY.into());
            }
        } else {
            check_dir(&opened, uid, root_uid)?;
        }
        if final_private
            && index + 1 == dirs.len()
            && (opened.st_uid != uid || opened.st_mode & 0o022 != 0)
        {
            return Err(BUSY.into());
        }
        if index < names.len() {
            let named = fstatat(
                &dirs[index],
                names[index].as_str(),
                AtFlags::AT_SYMLINK_NOFOLLOW,
            )?;
            let child = fstat(&dirs[index + 1])?;
            if named.st_dev != child.st_dev || named.st_ino != child.st_ino {
                return Err(BUSY.into());
            }
        }
        anchor_next = opened.st_uid == root_uid && opened.st_mode & 0o022 != 0;
        if anchor_next && opened.st_mode & 0o7777 != 0o1777 {
            return Err(BUSY.into());
        }
    }
    if final_private && anchor_next {
        return Err(BUSY.into());
    }
    Ok(())
}

fn pin_path(path: &Path, uid: u32, intermediate_mode: Mode) -> Result<PinnedPath> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return Err(BUSY.into());
    }
    let mut dirs = vec![File::open("/")?];
    let root_uid = fstat(&dirs[0])?.st_uid;
    let names = path
        .components()
        .filter_map(|part| match part {
            Component::RootDir => None,
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut anchor_next = false;
    for (index, name) in names.iter().enumerate() {
        // Recheck the complete pinned ancestry immediately before any mkdir/open.
        validate_chain(&dirs, &names[..index], uid, root_uid, false)?;
        let parent = dirs.last().ok_or(BUSY)?;
        let parent_stat = fstat(parent)?;
        anchor_next = parent_stat.st_uid == root_uid && parent_stat.st_mode & 0o022 != 0;
        let child_fd = match openat(
            parent,
            name.as_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(nix::errno::Errno::ENOENT) if !anchor_next => {
                validate_chain(&dirs, &names[..index], uid, root_uid, false)?;
                let create_mode = if index + 1 == names.len() {
                    Mode::from_bits_truncate(0o700)
                } else {
                    intermediate_mode
                };
                mkdirat(parent, name.as_str(), create_mode)?;
                openat(
                    parent,
                    name.as_str(),
                    OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )?
            }
            Err(_) => return Err(BUSY.into()),
        };
        let child = File::from(child_fd);
        let stat = fstat(&child)?;
        if anchor_next {
            if stat.st_uid != uid || stat.st_mode & 0o7777 != 0o700 {
                return Err(BUSY.into());
            }
        } else {
            check_dir(&stat, uid, root_uid)?;
        }
        if index + 1 == names.len() && (stat.st_uid != uid || stat.st_mode & 0o022 != 0) {
            return Err(BUSY.into());
        }
        anchor_next = stat.st_uid == root_uid && stat.st_mode & 0o022 != 0;
        dirs.push(child);
    }
    if anchor_next {
        return Err(BUSY.into());
    }
    validate_chain(&dirs, &names, uid, root_uid, true)?;
    Ok(PinnedPath {
        path: path.to_path_buf(),
        dirs,
        names,
    })
}

impl WriterLease {
    /// Compatibility constructor for one destination. Daemon services should
    /// use `acquire_for_paths` so both schema-approved roots are locked.
    pub fn acquire(config_dir: &Path) -> Result<Self> {
        Self::acquire_for_paths(config_dir, config_dir)
    }

    /// Legacy single-file helpers historically create ordinary XDG ancestors
    /// with 0755 permissions. Keep that filesystem convention while retaining
    /// the same descriptor pinning and writer lock as daemon-owned roots.
    pub(crate) fn acquire_legacy(path: &Path) -> Result<Self> {
        Self::acquire_for_paths_with_mode(path, path, Mode::from_bits_truncate(0o755))
    }

    /// Acquire the known config and state roots in stable path order. Roots
    /// resolving to the same directory inode share one lock; all aliases stay
    /// pinned and validated. A later failure drops every earlier lock.
    pub fn acquire_for_paths(config_dir: &Path, state_dir: &Path) -> Result<Self> {
        Self::acquire_for_paths_with_mode(config_dir, state_dir, Mode::from_bits_truncate(0o700))
    }

    fn acquire_for_paths_with_mode(
        config_dir: &Path,
        state_dir: &Path,
        intermediate_mode: Mode,
    ) -> Result<Self> {
        let uid = Uid::current().as_raw();
        let config_path = config_dir.to_path_buf();
        let state_path = state_dir.to_path_buf();
        let mut requested = vec![config_path.clone(), state_path.clone()];
        requested.sort_by(|left, right| {
            left.as_os_str()
                .as_bytes()
                .cmp(right.as_os_str().as_bytes())
        });

        let mut roots: Vec<WriterRoot> = Vec::new();
        for path in requested {
            if roots
                .iter()
                .any(|root| root.paths.iter().any(|pinned| pinned.path == path))
            {
                continue;
            }
            let pinned = pin_path(&path, uid, intermediate_mode)?;
            let meta = fstat(pinned.dirs.last().ok_or(BUSY)?)?;
            if let Some(existing) = roots.iter_mut().find(|root| {
                let candidate = root.paths.first().and_then(|p| p.dirs.last());
                candidate
                    .and_then(|dir| fstat(dir).ok())
                    .is_some_and(|current| {
                        current.st_dev == meta.st_dev && current.st_ino == meta.st_ino
                    })
            }) {
                existing.paths.push(pinned);
                continue;
            }
            let root = pinned.dirs.last().ok_or(BUSY)?;
            let lock_fd = openat(
                root,
                ".writer.lock",
                OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::from_bits_truncate(0o600),
            )?;
            let lock_file = File::from(lock_fd);
            let lock_meta = lock_file.metadata()?;
            let named = fstatat(root, ".writer.lock", AtFlags::AT_SYMLINK_NOFOLLOW)?;
            if !lock_meta.is_file()
                || lock_meta.uid() != uid
                || lock_meta.nlink() != 1
                || lock_meta.mode() & 0o777 != 0o600
                || named.st_dev as u64 != lock_meta.dev()
                || named.st_ino as u64 != lock_meta.ino()
            {
                return Err(BUSY.into());
            }
            let lock = Flock::lock(lock_file, FlockArg::LockExclusiveNonblock).map_err(|_| BUSY)?;
            roots.push(WriterRoot {
                paths: vec![pinned],
                lock,
            });
        }
        let lease = Self {
            config_path,
            state_path,
            roots,
        };
        lease.ensure_valid()?;
        Ok(lease)
    }

    pub fn ensure_valid(&self) -> Result<()> {
        let uid = Uid::current().as_raw();
        for root in &self.roots {
            for pinned in &root.paths {
                let root_uid = fstat(&pinned.dirs[0])?.st_uid;
                validate_chain(&pinned.dirs, &pinned.names, uid, root_uid, true)?;
                let dir = pinned.dirs.last().ok_or(BUSY)?;
                let named = fstatat(dir, ".writer.lock", AtFlags::AT_SYMLINK_NOFOLLOW)?;
                let opened = fstat(&*root.lock)?;
                if named.st_dev != opened.st_dev
                    || named.st_ino != opened.st_ino
                    || opened.st_uid != uid
                    || opened.st_nlink != 1
                    || opened.st_mode & 0o777 != 0o600
                {
                    return Err(BUSY.into());
                }
            }
        }
        Ok(())
    }

    fn path_root(&self, path: &Path) -> Option<&PinnedPath> {
        let parent = path.parent()?;
        if parent != self.config_path.as_path() && parent != self.state_path.as_path() {
            return None;
        }
        self.roots
            .iter()
            .flat_map(|root| &root.paths)
            .find(|pinned| pinned.path == parent)
    }

    pub fn directory_fd(&self, path: &Path) -> Option<&File> {
        self.path_root(path).and_then(|root| root.dirs.last())
    }

    pub fn contains_path(&self, path: &Path) -> bool {
        self.directory_fd(path).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn private_home() -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        home
    }

    #[test]
    fn one_destination_has_one_writer_and_different_roots_coexist() {
        let home = private_home();
        let first = home.path().join("one/.config/nacre");
        let second = home.path().join("two/.config/nacre");
        let lease = WriterLease::acquire(&first).unwrap();
        assert!(WriterLease::acquire(&first).is_err());
        let other = WriterLease::acquire(&second).unwrap();
        other.ensure_valid().unwrap();
        lease.ensure_valid().unwrap();
    }

    #[test]
    fn legacy_creation_keeps_xdg_ancestor_modes_and_writer_exclusion() {
        let home = private_home();
        let destination = home.path().join(".config/nacre");
        let lease = WriterLease::acquire_legacy(&destination).unwrap();
        assert_eq!(
            std::fs::metadata(home.path().join(".config"))
                .unwrap()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::metadata(&destination).unwrap().mode() & 0o777,
            0o700
        );
        assert!(WriterLease::acquire_legacy(&destination).is_err());
        lease.ensure_valid().unwrap();
    }

    #[test]
    fn legacy_creation_rejects_symlink_ancestry_before_creating_children() {
        let home = private_home();
        let outside = private_home();
        let link = home.path().join("redirect");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        let destination = link.join("must-not-exist").join("nacre");
        assert!(WriterLease::acquire_legacy(&destination).is_err());
        assert!(!outside.path().join("must-not-exist").exists());
    }

    #[test]
    fn config_and_state_roots_are_locked_as_an_ordered_set() {
        let home = private_home();
        let config_a = home.path().join("a/config/nacre");
        let config_b = home.path().join("b/config/nacre");
        let state = home.path().join("shared/state/nacre");
        let lease = WriterLease::acquire_for_paths(&config_a, &state).unwrap();
        assert!(lease.contains_path(&state.join("launcher.json")));
        assert!(!lease.contains_path(&state.join("nested/launcher.json")));
        assert!(WriterLease::acquire_for_paths(&config_b, &state).is_err());
        let disjoint = WriterLease::acquire_for_paths(
            &home.path().join("c/config/nacre"),
            &home.path().join("d/state/nacre"),
        )
        .unwrap();
        lease.ensure_valid().unwrap();
        disjoint.ensure_valid().unwrap();
        assert!(WriterLease::acquire(&config_b).is_ok());
    }

    #[test]
    fn ancestry_replacement_and_mode_changes_fail_closed() {
        let home = private_home();
        let config = home.path().join(".config");
        let root = config.join("nacre");
        let lease = WriterLease::acquire(&root).unwrap();
        std::fs::rename(&config, home.path().join("moved")).unwrap();
        std::fs::create_dir(&config).unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(lease.ensure_valid().is_err());

        let safe = home.path().join("safe/config/nacre");
        let lease = WriterLease::acquire(&safe).unwrap();
        std::fs::set_permissions(
            home.path().join("safe"),
            std::fs::Permissions::from_mode(0o770),
        )
        .unwrap();
        assert!(lease.ensure_valid().is_err());
    }
}
