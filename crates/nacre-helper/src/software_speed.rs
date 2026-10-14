//! Opt-in throughput tests, bounded network transfers and disposable disk files.
use crate::{i18n::tr, paths::Paths, software_actions::tool, Result};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

pub fn snapshot() -> Value {
    json!({"terminal":tool("kitty"),"network":tool("librespeed-cli"),"download":tool("curl"),"disk":tool("fio")})
}
pub fn rows(f: &Value) -> Vec<Value> {
    let mut rows = vec![];
    if f["terminal"] != true {
        return rows;
    }
    for (id, label, description, cap) in [
        (
            "network-speed",
            tr("software.speed.network", "Network speed test"),
            tr(
                "software.speed.network-detail",
                "Download, upload and latency; uses Internet traffic",
            ),
            "network",
        ),
        (
            "download-speed",
            tr("software.speed.download", "Quick download speed test"),
            tr(
                "software.speed.download-detail",
                "A bounded 10 MB HTTPS download; not a line-rate guarantee",
            ),
            "download",
        ),
        (
            "disk-speed",
            tr("software.speed.disk", "Disk speed test"),
            tr(
                "software.speed.disk-detail",
                "Use an owned 256 MiB temporary file; no raw device writes",
            ),
            "disk",
        ),
    ] {
        if f[cap] == true {
            rows.push(json!({"id":id,"name":label,"description":description,"section":"health.speed","root":false}))
        }
    }
    rows
}
pub fn plan(id: &str, f: &Value) -> Result<Value> {
    if !rows(f).iter().any(|r| r["id"] == id) {
        return Err("Speed-test tool is unavailable".into());
    }
    let args=match id{
        "network-speed"=>vec!["librespeed-cli","--telemetry-level","disabled","--duration","10"],
        "download-speed"=>vec!["curl","--proto","=https","--tlsv1.2","--fail","--location","--connect-timeout","5","--max-time","30","--output","/dev/null","--write-out","\nDownloaded bytes: %{size_download}\nSeconds including connection/TLS: %{time_total}\nAverage bytes/s: %{speed_download}\n","https://speed.cloudflare.com/__down?bytes=10000000"],
        "disk-speed"=>vec!["fio","--name=nacre-speed","--filename=<owned temporary file>","--size=256m","--rw=readwrite","--bs=1m","--ioengine=sync","--direct=1","--runtime=10","--time_based=1","--group_reporting=1","--unlink=1"],
        _=>return Err("Unknown speed test".into())
    };
    Ok(json!({"id":id,"steps":[{"arguments":args,"root":false,"disk":id=="disk-speed"}]}))
}
pub fn disk_test(paths: &Paths) -> Result<i32> {
    fs::create_dir_all(&paths.cache)?;
    if paths.cache.is_symlink() {
        return Err("Linked benchmark folder is preserved".into());
    }
    let temp = tempfile::tempdir_in(&paths.cache)?;
    run_disk(temp.path())
}
fn run_disk(root: &Path) -> Result<i32> {
    let file = root.join("nacre-speed.bin");
    let args = plan("disk-speed", &json!({"terminal":true,"disk":true}))?["steps"][0]["arguments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let filename = format!(
        "--filename={}",
        file.to_str().ok_or("Invalid benchmark path")?
    );
    let args = args
        .iter()
        .skip(1)
        .map(|a| {
            if a.starts_with("--filename=") {
                filename.as_str()
            } else {
                a.as_str()
            }
        })
        .collect::<Vec<_>>();
    Ok(Command::new("fio").args(args).status()?.code().unwrap_or(1))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tests_are_capability_gated_bounded_and_non_privileged() -> Result<()> {
        assert!(rows(&json!({"terminal":true})).is_empty());
        let f = json!({"terminal":true,"network":true,"download":true,"disk":true});
        assert_eq!(rows(&f).len(), 3);
        let p = plan("download-speed", &f)?;
        let text = p.to_string();
        assert!(text.contains("bytes=10000000"));
        assert!(text.contains("max-time"));
        assert_eq!(p["steps"][0]["root"], false);
        let p = plan("disk-speed", &f)?;
        assert!(p.to_string().contains("owned temporary file"));
        assert!(!p.to_string().contains("/dev/sd"));
        assert!(p.to_string().contains("256m"));
        Ok(())
    }
}
