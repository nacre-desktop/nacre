//! Exclusive ownership of the actual daemon config directory.
use crate::Result;
use nix::{
    fcntl::{openat, AtFlags, Flock, FlockArg, OFlag},
    sys::stat::{fstat, fstatat, mkdirat, Mode, SFlag},
    unistd::Uid,
};
use std::{
    fs::File,
    os::unix::fs::MetadataExt,
    path::{Component, Path},
};

const BUSY: &str = "Nacre destination busy or unsafe";

pub struct WriterLease {
    config_path: std::path::PathBuf,
    dirs: Vec<File>,
    names: Vec<String>,
    lock: Flock<File>,
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
            anchor_next = false;
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

impl WriterLease {
    pub fn acquire(config_dir: &Path) -> Result<Self> {
        if !config_dir.is_absolute()
            || config_dir
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            return Err(BUSY.into());
        }
        let uid = Uid::current().as_raw();
        let mut dirs = vec![File::open("/")?];
        let root_uid = fstat(&dirs[0])?.st_uid;
        let names = config_dir
            .components()
            .filter_map(|part| match part {
                Component::RootDir => None,
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut sticky_anchor = false;
        for (index, name) in names.iter().enumerate() {
            validate_chain(&dirs, &names[..index], uid, root_uid, false)?;
            let parent = dirs.last().ok_or(BUSY)?;
            let parent_stat = fstat(parent)?;
            sticky_anchor = parent_stat.st_uid == root_uid && parent_stat.st_mode & 0o022 != 0;
            let child_fd = match openat(
                parent,
                name.as_str(),
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(nix::errno::Errno::ENOENT) if !sticky_anchor => {
                    mkdirat(parent, name.as_str(), Mode::from_bits_truncate(0o700))?;
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
            if sticky_anchor {
                if stat.st_uid != uid || stat.st_mode & 0o7777 != 0o700 {
                    return Err(BUSY.into());
                }
                sticky_anchor = false;
            } else {
                check_dir(&stat, uid, root_uid)?;
            }
            if index + 1 == names.len() && (stat.st_uid != uid || stat.st_mode & 0o022 != 0) {
                return Err(BUSY.into());
            }
            sticky_anchor = stat.st_uid == root_uid && stat.st_mode & 0o022 != 0;
            dirs.push(child);
        }
        if sticky_anchor {
            return Err(BUSY.into());
        }
        let root = dirs.last().ok_or(BUSY)?;
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
        let lease = Self {
            config_path: config_dir.to_path_buf(),
            dirs,
            names,
            lock,
        };
        lease.ensure_valid()?;
        Ok(lease)
    }

    pub fn ensure_valid(&self) -> Result<()> {
        let uid = Uid::current().as_raw();
        let root_uid = fstat(&self.dirs[0])?.st_uid;
        validate_chain(&self.dirs, &self.names, uid, root_uid, true)?;
        let root = self.dirs.last().ok_or(BUSY)?;
        let named = fstatat(root, ".writer.lock", AtFlags::AT_SYMLINK_NOFOLLOW)?;
        let opened = fstat(&*self.lock)?;
        if named.st_dev != opened.st_dev
            || named.st_ino != opened.st_ino
            || opened.st_uid != uid
            || opened.st_nlink != 1
            || opened.st_mode & 0o777 != 0o600
        {
            return Err(BUSY.into());
        }
        Ok(())
    }

    pub fn config_dir_fd(&self) -> &File {
        self.dirs.last().expect("lease always pins config root")
    }

    pub fn contains_path(&self, path: &Path) -> bool {
        path.parent() == Some(self.config_path.as_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn one_destination_has_one_writer_and_different_roots_coexist() {
        let home = tempfile::tempdir().unwrap();
        let first = home.path().join("one/.config/nacre");
        let second = home.path().join("two/.config/nacre");
        let lease = WriterLease::acquire(&first).unwrap();
        assert!(WriterLease::acquire(&first).is_err());
        let other = WriterLease::acquire(&second).unwrap();
        other.ensure_valid().unwrap();
        lease.ensure_valid().unwrap();
    }

    #[test]
    fn ancestry_replacement_and_mode_changes_fail_closed() {
        let home = tempfile::tempdir().unwrap();
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
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
