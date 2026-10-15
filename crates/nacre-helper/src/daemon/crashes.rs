//! Runtime-only crash counts. No settings, window text, paths or panic payloads.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
#[derive(Default, Serialize, Deserialize)]
struct Record {
    kernel_boot: String,
    running: bool,
    failures: Vec<f64>,
    notified: bool,
}
pub struct Counter {
    path: PathBuf,
    record: Record,
    pub safe: bool,
    pub notify: bool,
}
impl Counter {
    pub fn begin(folder: &Path) -> Result<Self> {
        let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")?
            .trim()
            .to_owned();
        let now = fs::read_to_string("/proc/uptime")?
            .split_whitespace()
            .next()
            .ok_or("Missing monotonic uptime")?
            .parse::<f64>()?;
        Self::at(folder, boot, now)
    }
    fn at(folder: &Path, boot: String, now: f64) -> Result<Self> {
        let path = folder.join("daemon-crashes.json");
        let previous = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&path)
        {
            Ok(file) => {
                let meta = file.metadata()?;
                if !meta.is_file()
                    || meta.uid() != nix::unistd::getuid().as_raw()
                    || meta.nlink() != 1
                    || meta.mode() & 0o777 != 0o600
                {
                    return Err("Refusing unsafe daemon crash record".into());
                }
                if meta.len() > 4096 {
                    return Err("Oversized daemon crash record".into());
                }
                let mut bytes = Vec::new();
                file.take(4097).read_to_end(&mut bytes)?;
                match serde_json::from_slice::<Record>(&bytes) {
                    Ok(record) => record,
                    Err(_) => {
                        tracing::warn!(
                            "Invalid daemon crash record; preserving it before recovery"
                        );
                        let backup = folder.join("daemon-crashes.corrupt.bak");
                        let mut backup = fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(0o600)
                            .custom_flags(nix::libc::O_NOFOLLOW)
                            .open(&backup)?;
                        backup.write_all(&bytes)?;
                        backup.sync_all()?;
                        Record {
                            kernel_boot: boot.clone(),
                            running: false,
                            failures: vec![now; 3],
                            notified: false,
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Record::default(),
            Err(error) => return Err(error.into()),
        };
        let mut record = if previous.kernel_boot == boot {
            previous
        } else {
            Record {
                kernel_boot: boot,
                ..Record::default()
            }
        };
        record
            .failures
            .retain(|at| at.is_finite() && *at <= now && now - *at < 120.0);
        if record.running {
            record.failures.push(now);
        }
        let safe = record.failures.len() >= 3;
        let notify = safe && !record.notified;
        record.running = true;
        record.notified = safe;
        let counter = Self {
            path,
            record,
            safe,
            notify,
        };
        counter.save()?;
        Ok(counter)
    }
    fn save(&self) -> Result<()> {
        crate::files::atomic(&self.path, &serde_json::to_vec(&self.record)?)
    }
    pub fn clean(&mut self) -> Result<()> {
        self.record.running = false;
        self.save()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_unclean_exits_enter_safe_mode_once_and_clean_exit_does_not_count() -> Result<()> {
        let home = tempfile::tempdir()?;
        let first = Counter::at(home.path(), "test-boot".into(), 1.0)?;
        assert!(!first.safe);
        drop(first);
        drop(Counter::at(home.path(), "test-boot".into(), 2.0)?);
        drop(Counter::at(home.path(), "test-boot".into(), 3.0)?);
        let mut fourth = Counter::at(home.path(), "test-boot".into(), 4.0)?;
        assert!(fourth.safe && fourth.notify);
        fourth.clean()?;
        let fifth = Counter::at(home.path(), "test-boot".into(), 5.0)?;
        assert!(fifth.safe && !fifth.notify);
        drop(fifth);
        let mut later = Counter::at(home.path(), "test-boot".into(), 130.0)?;
        assert!(!later.safe);
        later.clean()?;
        let reboot = Counter::at(home.path(), "new-boot".into(), 1.0)?;
        assert!(!reboot.safe);
        Ok(())
    }
    #[test]
    fn linked_and_hard_linked_crash_records_are_refused() -> Result<()> {
        let home = tempfile::tempdir()?;
        let original = home.path().join("preserved");
        fs::write(&original, "private")?;
        let record = home.path().join("daemon-crashes.json");
        std::os::unix::fs::symlink(&original, &record)?;
        assert!(Counter::at(home.path(), "b".into(), 1.0).is_err());
        fs::remove_file(&record)?;
        fs::hard_link(&original, &record)?;
        assert!(Counter::at(home.path(), "b".into(), 1.0).is_err());
        assert_eq!(fs::read_to_string(original)?, "private");
        Ok(())
    }
}
