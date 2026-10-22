//! Native maintenance actions; diagnostic state stays in diagnostics.rs.
use crate::{paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    env, fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn command(args: &[String], timeout: Duration) -> Result<String> {
    let Some((program, rest)) = args.split_first() else {
        return Err("Missing command".into());
    };
    let output = crate::process::run(program, rest, None, timeout)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!("{} failed", program).into())
    }
}
fn helper(paths: &Paths) -> PathBuf {
    let installed = paths.tools().join("nacre-helper");
    if installed.is_file() {
        installed
    } else {
        env::current_exe().unwrap_or(installed)
    }
}
fn state() -> Result<Value> {
    crate::diagnostics_native::maintenance_state()
}
fn py_quote(value: &str) -> String {
    format!(
        "'{}'",
        value
            .replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
    )
}
fn sample(paths: &Paths, seconds: u64, label: &str) -> Result<()> {
    if !(1..=30).contains(&seconds) {
        return Err("Sample duration must be 1–30 seconds".into());
    }
    let service = "nacre-shell.service";
    let read = || -> Result<(u64, u64)> {
        let out = command(
            &[
                "systemctl".into(),
                "--user".into(),
                "show".into(),
                service.into(),
                "-p".into(),
                "CPUUsageNSec".into(),
                "-p".into(),
                "MemoryCurrent".into(),
            ],
            Duration::from_secs(5),
        )?;
        let fields: std::collections::HashMap<_, _> =
            out.lines().filter_map(|v| v.split_once('=')).collect();
        Ok((
            fields
                .get("CPUUsageNSec")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            fields
                .get("MemoryCurrent")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        ))
    };
    let before = read()?;
    let start = Instant::now();
    thread::sleep(Duration::from_secs(seconds));
    let after = read()?;
    let elapsed = start.elapsed().as_secs_f64();
    let value = json!({"label":label,"seconds":(elapsed*100.0).round()/100.0,"cpuCorePercent":((after.0.saturating_sub(before.0) as f64 / elapsed / 1e7)*100.0).round()/100.0,"memoryMiB":((after.1 as f64 / 1048576.0)*10.0).round()/10.0,"checked":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros,true)});
    let path = paths.state.join("performance/latest.json");
    fs::create_dir_all(path.parent().ok_or("Missing performance directory")?)?;
    crate::state::write(&path, &value)
}
pub fn run(args: &[String]) -> Result<i32> {
    let Some(action) = args.first().map(String::as_str) else {
        return Err("Expected a maintenance action".into());
    };
    let allowed = [
        "state",
        "doctor",
        "check",
        "profile",
        "restart",
        "repair-portals",
        "sync-now",
        "rollback",
        "session",
    ];
    if !allowed.contains(&action) {
        return Err("Unknown maintenance action".into());
    }
    let paths = Paths::environment()?;
    if action == "doctor" {
        return match crate::diagnostics_native::report(args.iter().any(|v| v == "--report")) {
            Ok(report) => {
                print!("{report}");
                Ok(0)
            }
            Err(_) => {
                println!(
                    "{}",
                    crate::json::encode(&json!({"error":"Support report unavailable."}))?
                );
                Ok(1)
            }
        };
    }
    let outcome: Result<Value> = (|| {
        match action {
            "check" => {
                let home = env::var_os("HOME").ok_or("Missing home directory")?;
                let argv = [
                    "python3".to_owned(),
                    PathBuf::from(home)
                        .join(".local/share/nacre/control/check-overlays.py")
                        .to_string_lossy()
                        .into_owned(),
                    "--keyboard".into(),
                    "/usr/bin/wtype".into(),
                ];
                let output =
                    crate::process::run(&argv[0], &argv[1..], None, Duration::from_secs(30))?;
                if !output.status.success() {
                    let executable = env::split_paths(&env::var_os("PATH").unwrap_or_default())
                        .map(|p| p.join(&argv[0]))
                        .find(|p| p.is_file())
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|| argv[0].clone());
                    let mut full = vec![executable];
                    full.extend(argv[1..].iter().cloned());
                    let rendered = full
                        .iter()
                        .map(|s| py_quote(s))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(format!(
                        "Command '[{rendered}]' returned non-zero exit status {}.",
                        output.status.code().unwrap_or(1)
                    )
                    .into());
                }
            }
            "profile" => {
                let seconds = args
                    .windows(2)
                    .find(|w| w[0] == "--seconds")
                    .and_then(|w| w[1].parse().ok())
                    .unwrap_or(5);
                let label = args
                    .windows(2)
                    .find(|w| w[0] == "--label")
                    .map(|w| w[1].as_str())
                    .unwrap_or("manual");
                sample(&paths, seconds, label)?;
            }
            "restart" => {
                command(
                    &[
                        "systemctl".into(),
                        "--user".into(),
                        "restart".into(),
                        "nacre-shell.service".into(),
                    ],
                    Duration::from_secs(10),
                )?;
            }
            "repair-portals" => {
                let backend = crate::compositor::backend::select(&env::vars().collect())?;
                let portal = if backend == "niri" {
                    "xdg-desktop-portal-gnome.service"
                } else {
                    "xdg-desktop-portal-hyprland.service"
                };
                for unit in [
                    "xdg-document-portal.service",
                    "xdg-desktop-portal.service",
                    portal,
                ] {
                    let failed = command(
                        &[
                            "systemctl".into(),
                            "--user".into(),
                            "is-failed".into(),
                            unit.into(),
                        ],
                        Duration::from_secs(5),
                    )
                    .unwrap_or_default();
                    if failed == "failed" {
                        command(
                            &[
                                "systemctl".into(),
                                "--user".into(),
                                "restart".into(),
                                unit.into(),
                            ],
                            Duration::from_secs(10),
                        )?;
                    }
                }
            }
            "sync-now" => {
                let _ = command(
                    &[
                        paths
                            .home
                            .join(".local/bin/nacre-brain-sync")
                            .to_string_lossy()
                            .into_owned(),
                        "--force".into(),
                    ],
                    Duration::from_secs(75),
                );
            }
            "rollback" => {
                command(
                    &[
                        paths
                            .home
                            .join(".local/bin/nacre")
                            .to_string_lossy()
                            .into_owned(),
                        "rollback".into(),
                    ],
                    Duration::from_secs(75),
                )?;
            }
            "session" => {
                command(
                    &[
                        helper(&paths).to_string_lossy().into_owned(),
                        "session-watch".into(),
                        "check".into(),
                        "--event".into(),
                        "manual".into(),
                    ],
                    Duration::from_secs(10),
                )?;
            }
            _ => {}
        }
        state()
    })();
    match outcome {
        Ok(value) => {
            println!("{}", crate::json::encode(&value)?);
            Ok(0)
        }
        Err(error) => {
            println!(
                "{}",
                crate::json::encode(&json!({"error":error.to_string()}))?
            );
            Ok(0)
        }
    }
}
