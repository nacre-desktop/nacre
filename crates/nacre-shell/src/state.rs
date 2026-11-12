// SPDX-License-Identifier: GPL-3.0-only
//! Only panel open/page state is durable; transport snapshots and typed text are excluded.
use bead::core::{Stores, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    values: BTreeMap<String, Value>,
}
pub struct File {
    path: PathBuf,
    last: BTreeMap<String, Value>,
}
fn durable(id: &str, value: &Value) -> bool {
    match value {
        Value::Bool(_) => id == "open" || id.ends_with(".open"),
        Value::Text(s) => s.len() <= 256 && (id.ends_with(".page") || id.ends_with(".route")),
        _ => false,
    }
}
/// Failed builds must not force an independently healthy release into safe mode.
pub fn recovery_path(parent: &std::path::Path, release: Option<&str>) -> Result<PathBuf, String> {
    match release {
        None => Ok(parent.join("ui-health.json")),
        Some(id) if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(parent.join(format!("ui-health-{id}.json")))
        }
        Some(_) => Err("invalid UI release identity".into()),
    }
}
/// Shared XDG root for Nacre-owned shell state and presentation reads.
pub fn state_home() -> Result<PathBuf, String> {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .ok_or("state home unavailable")?;
    if !root.is_absolute() {
        return Err("state home must be absolute".into());
    }
    Ok(root)
}
pub fn shell_dir() -> Result<PathBuf, String> {
    Ok(state_home()?.join("nacre/shell"))
}
// shortcut: 2026-12-12, delete migration 30 days after the N0402 release.
pub fn migrate() -> Result<(), Box<dyn std::error::Error>> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let root = state_home()?;
    let old = root.join("bead");
    let mut sources = vec![root.join("nacre/widgets-window.json")];
    match std::fs::read_dir(&old) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name == "panels.json"
                    || name == "ui"
                    || name == "ui-health.json"
                    || name == "ui-health.lock"
                    || name.starts_with("ui-health-")
                    || name.starts_with("panels.tmp-")
                    || (name.starts_with(".ui-") && name.ends_with(".tmp"))
                {
                    sources.push(entry.path());
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    for source in sources {
        match std::fs::symlink_metadata(&source) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        }
        let destination = shell_dir()?.join(source.file_name().ok_or("invalid state path")?);
        let parent = destination.parent().ok_or("invalid state path")?;
        std::fs::create_dir_all(parent)?;
        // Persist the new directory chain before moving data into it.
        std::fs::File::open(&root)?.sync_all()?;
        std::fs::File::open(root.join("nacre"))?.sync_all()?;
        std::fs::File::open(parent)?.sync_all()?;
        let from = CString::new(source.as_os_str().as_bytes())?;
        let to = CString::new(destination.as_os_str().as_bytes())?;
        // NOREPLACE protects existing files, directories and symlinks, including concurrent starts.
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                eprintln!(
                    "STATE_MIGRATION conflict={} kept_both",
                    destination.display()
                );
            } else if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error.into());
            }
        } else {
            std::fs::File::open(parent)?.sync_all()?;
            std::fs::File::open(source.parent().ok_or("invalid state path")?)?.sync_all()?;
        }
    }
    // remove_dir only removes an empty directory; unrelated Bead state stays untouched.
    match std::fs::remove_dir(old) {
        Ok(()) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
impl File {
    pub fn path() -> Result<PathBuf, String> {
        Ok(shell_dir()?.join("panels.json"))
    }
    pub fn open(
        path: PathBuf,
    ) -> Result<(Self, BTreeMap<String, Value>), Box<dyn std::error::Error>> {
        let values = match std::fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(32769).read_to_end(&mut bytes)?;
                if bytes.len() > 32768 {
                    return Err("panel state exceeds32KiB".into());
                }
                let saved: Saved = serde_json::from_slice(&bytes)?;
                if saved.version != 1 || saved.values.iter().any(|(id, v)| !durable(id, v)) {
                    return Err("invalid panel state schema".into());
                }
                saved.values
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok((
            Self {
                path,
                last: values.clone(),
            },
            values,
        ))
    }
    pub fn save(&mut self, stores: &Stores) -> Result<(), Box<dyn std::error::Error>> {
        let mut values = self.last.clone();
        for (id, value) in stores.values().iter().filter(|(id, v)| durable(id, v)) {
            values.insert(id.clone(), value.clone());
        }
        if values == self.last {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&Saved {
            version: 1,
            values: values.clone(),
        })?;
        if bytes.len() > 32768 {
            return Err("panel state exceeds32KiB".into());
        }
        let parent = self.path.parent().ok_or("invalid state path")?;
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &self.path)?;
            std::fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
        self.last = values;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_preserves_existing_state() {
        const CHILD: &str = "NACRE_STATE_MIGRATION_TEST";
        if std::env::var_os(CHILD).is_none() {
            let root = std::env::temp_dir().join(format!("nacre-migration-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "state::tests::migration_preserves_existing_state",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("XDG_STATE_HOME", &root)
                .status()
                .unwrap();
            std::fs::remove_dir_all(root).unwrap();
            assert!(status.success());
            return;
        }
        let root = state_home().unwrap();
        let old = root.join("bead");
        let new = shell_dir().unwrap();
        migrate().unwrap();
        assert!(!old.exists() && !new.exists());
        std::fs::create_dir_all(old.join("ui")).unwrap();
        std::fs::write(old.join("ui/surface.json"), "surface").unwrap();
        for name in [
            "panels.json",
            "ui-health.json",
            "ui-health-release.json",
            "ui-health-release.lock",
            "panels.tmp-123",
            ".ui-123-0.tmp",
        ] {
            std::fs::write(old.join(name), name).unwrap();
        }
        std::fs::create_dir_all(root.join("nacre")).unwrap();
        std::fs::write(root.join("nacre/widgets-window.json"), "window").unwrap();
        std::os::unix::fs::symlink("missing", old.join("ui-health-link")).unwrap();
        migrate().unwrap();
        assert!(!old.exists());
        assert_eq!(
            std::fs::read_link(new.join("ui-health-link")).unwrap(),
            PathBuf::from("missing")
        );
        assert_eq!(File::path().unwrap(), new.join("panels.json"));
        assert_eq!(
            std::fs::read_to_string(new.join("ui/surface.json")).unwrap(),
            "surface"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("widgets-window.json")).unwrap(),
            "window"
        );
        std::fs::create_dir_all(old.join("ui")).unwrap();
        std::fs::write(old.join("panels.json"), "old").unwrap();
        std::fs::write(old.join("unrelated"), "other app").unwrap();
        migrate().unwrap();
        assert_eq!(
            std::fs::read_to_string(old.join("panels.json")).unwrap(),
            "old"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("panels.json")).unwrap(),
            "panels.json"
        );
        assert!(old.join("ui").is_dir() && new.join("ui/surface.json").is_file());
        assert!(old.join("unrelated").is_file());
    }
    #[test]
    fn failed_release_history_does_not_poison_a_known_good_release() {
        let root = std::env::temp_dir().join(format!("bead-recovery-{}", std::process::id()));
        let a = recovery_path(&root, Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")).unwrap();
        let b = recovery_path(&root, Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")).unwrap();
        let mut good = bead::core::recovery::Run::begin(a.clone(), 1).unwrap();
        good.finish().unwrap();
        drop(good);
        for now in 2..5 {
            drop(bead::core::recovery::Run::begin(b.clone(), now).unwrap());
        }
        let bad = bead::core::recovery::Run::begin(b, 5).unwrap();
        assert!(bad.safe_mode);
        let mut recovered = bead::core::recovery::Run::begin(a, 6).unwrap();
        assert!(!recovered.safe_mode);
        recovered.finish().unwrap();
        drop(recovered);
        drop(bad);
        assert!(recovery_path(&root, Some("../outside")).is_err());
        assert!(recovery_path(&root, Some("")).is_err());
        assert_eq!(
            recovery_path(&root, None).unwrap(),
            root.join("ui-health.json")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn atomic_panel_restore_excludes_private_transient_state_and_rejects_bad_schema() {
        let root = std::env::temp_dir().join(format!(
            "bead-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("panels.json");
        let (mut file, _) = File::open(path.clone()).unwrap();
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("panel.open".into(), Value::Bool(false)),
                ("panel.page".into(), Value::Text("sound".into())),
                ("search.text".into(), Value::Text("private".into())),
            ]))
            .unwrap();
        file.save(&stores).unwrap();
        let (_, values) = File::open(path.clone()).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values["panel.open"], Value::Bool(false));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::write(&path, b"{\"version\":2,\"values\":{}}").unwrap();
        assert!(File::open(path).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
