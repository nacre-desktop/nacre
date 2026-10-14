//! Visible-only process/disk snapshot and identity-checked process termination.
use crate::{i18n, Result};
use rustix::process::{
    getpid, getppid, getuid, pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::HashSet, fs, io, os::unix::fs::MetadataExt, path::Path};

#[derive(Debug, Serialize)]
pub struct ProcessInfo {
    pub pid: i64,
    pub name: String,
    pub start: String,
    pub uid: u32,
    pub memory: i64,
    #[serde(rename = "cpuSeconds")]
    pub cpu_seconds: f64,
    pub parent: i64,
}

pub fn process_info(pid: i64, proc: &Path) -> Option<ProcessInfo> {
    let process = proc.join(pid.to_string());
    let result = (|| -> Result<ProcessInfo> {
        let raw = fs::read_to_string(process.join("stat"))?;
        let begin = raw.find('(').ok_or("Malformed process name")?;
        let end = raw.rfind(')').ok_or("Malformed process name")?;
        let fields: Vec<_> = raw
            .get(end + 2..)
            .ok_or("Malformed process stat")?
            .split_whitespace()
            .collect();
        let field = |index| fields.get(index).copied().ok_or("Incomplete process stat");
        Ok(ProcessInfo {
            pid,
            name: raw
                .get(begin + 1..end)
                .ok_or("Malformed process name")?
                .chars()
                .take(80)
                .collect(),
            start: field(19)?.to_owned(),
            uid: fs::metadata(&process)?.uid(),
            memory: field(21)?.parse::<i64>()? * rustix::param::page_size() as i64,
            cpu_seconds: (field(11)?.parse::<i64>()? + field(12)?.parse::<i64>()?) as f64
                / rustix::param::clock_ticks_per_second() as f64,
            parent: field(1)?.parse()?,
        })
    })();
    match result {
        Ok(info) => Some(info),
        Err(error) => {
            if !error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
            {
                eprintln!("Dashboard process metadata: {error}");
            }
            None
        }
    }
}

pub fn snapshot(proc: &Path) -> Result<Value> {
    let mut processes = Vec::new();
    for entry in fs::read_dir(proc)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name
            .to_str()
            .filter(|name| !name.is_empty() && name.bytes().all(|ch| ch.is_ascii_digit()))
        else {
            continue;
        };
        if let Ok(pid) = name.parse() {
            if let Some(info) = process_info(pid, proc).filter(|info| info.uid == getuid().as_raw())
            {
                processes.push(info);
            }
        }
    }
    processes.sort_by_key(|info| std::cmp::Reverse(info.memory));
    processes.truncate(12);
    let mut disks = Vec::new();
    let mut seen = HashSet::new();
    for line in fs::read_to_string(proc.join("mounts"))?.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 3 || !fields[0].starts_with("/dev/") {
            continue;
        }
        let mount = fields[1].replace("\\040", " ");
        let result = (|| -> Result<Option<Value>> {
            if !seen.insert(fs::metadata(&mount)?.dev()) {
                return Ok(None);
            }
            let stats = rustix::fs::statvfs(mount.as_str())?;
            Ok(Some(
                json!({"path":mount,"total":stats.f_blocks * stats.f_frsize,"used":(stats.f_blocks - stats.f_bfree) * stats.f_frsize}),
            ))
        })();
        match result {
            Ok(Some(disk)) => disks.push(disk),
            Ok(None) => {}
            Err(error) => eprintln!("Dashboard filesystem metadata: {error}"),
        }
    }
    disks.truncate(12);
    Ok(json!({"processes":processes,"disks":disks}))
}

fn require_identity(info: Option<&ProcessInfo>, start: &str, uid: u32) -> Result<()> {
    if !info.is_some_and(|info| info.start == start && info.uid == uid) {
        return Err(i18n::tr(
            "helper.dashboard-system.process-changed-refresh-before-ending-it",
            "Process changed; refresh before ending it",
        )
        .into());
    }
    Ok(())
}

pub fn terminate(pid: i64, start: &str, proc: &Path) -> Result<()> {
    require_identity(process_info(pid, proc).as_ref(), start, getuid().as_raw())?;
    let mut ancestors = HashSet::from([i64::from(getpid().as_raw_nonzero().get())]);
    let mut parent = getppid().map_or(0, |pid| i64::from(pid.as_raw_nonzero().get()));
    for _ in 0..20 {
        ancestors.insert(parent);
        let Some(info) = process_info(parent, proc).filter(|info| info.parent > 1) else {
            break;
        };
        parent = info.parent;
    }
    if pid <= 1 || ancestors.contains(&pid) {
        return Err(i18n::tr(
            "helper.dashboard-system.cannot-end-the-dashboard-or-its-session",
            "Cannot end the dashboard or its session",
        )
        .into());
    }
    let pid = Pid::from_raw(i32::try_from(pid)?).ok_or("Invalid process ID")?;
    let descriptor = pidfd_open(pid, PidfdFlags::empty())?;
    let current = process_info(i64::from(pid.as_raw_nonzero().get()), proc);
    if !current.is_some_and(|info| info.start == start) {
        return Err(i18n::tr("helper.dashboard-system.process-changed", "Process changed").into());
    }
    pidfd_send_signal(&descriptor, Signal::TERM)?;
    Ok(())
}

pub fn execute(arguments: &[String], _input: Option<Value>) -> Result<Option<Value>> {
    let result = (|| -> Result<Value> {
        if let Some(action) = arguments.first() {
            if action != "end" {
                return Err(
                    i18n::tr("helper.dashboard-system.unknown-action", "Unknown action").into(),
                );
            }
            let pid = arguments
                .get(1)
                .ok_or("list index out of range")?
                .trim()
                .parse()?;
            let start = arguments.get(2).ok_or("list index out of range")?;
            terminate(pid, start, Path::new("/proc"))?;
        }
        snapshot(Path::new("/proc"))
    })();
    // This helper historically reports operation errors as JSON with exit zero.
    Ok(Some(match result {
        Ok(value) => value,
        Err(error) => json!({"error":error.to_string().chars().take(180).collect::<String>()}),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    const STAT: &str = "123 (demo worker) S 1 0 0 0 0 0 0 0 0 0 100 50 0 0 0 0 0 0 1000 0 10 0 0 0";

    #[test]
    fn process_fields_and_diskless_snapshot_match_frozen_reference() -> Result<()> {
        let proc = tempfile::tempdir()?;
        fs::create_dir(proc.path().join("123"))?;
        fs::write(proc.path().join("123/stat"), STAT)?;
        fs::write(proc.path().join("mounts"), "")?;
        let snapshot = snapshot(proc.path())?;
        assert_eq!(
            snapshot["processes"][0],
            json!({"pid":123,"name":"demo worker","start":"1000","uid":getuid().as_raw(),"memory":10 * rustix::param::page_size(),"cpuSeconds":150.0 / rustix::param::clock_ticks_per_second() as f64,"parent":1})
        );
        assert_eq!(snapshot["disks"], json!([]));
        Ok(())
    }

    #[test]
    fn changed_identity_and_foreign_owner_are_rejected() {
        let info = ProcessInfo {
            pid: 5000,
            name: String::new(),
            start: "new".into(),
            uid: 1000,
            memory: 0,
            cpu_seconds: 0.0,
            parent: 1,
        };
        assert_eq!(
            require_identity(Some(&info), "old", 1000)
                .unwrap_err()
                .to_string(),
            "Process changed; refresh before ending it"
        );
        assert_eq!(
            require_identity(Some(&info), "new", 1001)
                .unwrap_err()
                .to_string(),
            "Process changed; refresh before ending it"
        );
    }

    #[test]
    fn current_helper_cannot_receive_a_termination_signal() -> Result<()> {
        let proc = tempfile::tempdir()?;
        let pid = i64::from(getpid().as_raw_nonzero().get());
        fs::create_dir(proc.path().join(pid.to_string()))?;
        fs::write(proc.path().join(pid.to_string()).join("stat"), STAT)?;
        assert_eq!(
            terminate(pid, "1000", proc.path()).unwrap_err().to_string(),
            "Cannot end the dashboard or its session"
        );
        Ok(())
    }
}
