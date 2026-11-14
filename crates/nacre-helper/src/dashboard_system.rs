//! Visible-only process/disk snapshot.
use crate::Result;
use rustix::process::getuid;
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
}
