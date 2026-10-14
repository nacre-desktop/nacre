//! Developer-only reference adapter; never built or installed as a runtime tool.
use nacre_helper::{diagnostics, json, Result};
use serde_json::{json as value, Value};
use std::{
    os::unix::process::ExitStatusExt,
    process::{ExitStatus, Output},
};
struct FixtureProbe {
    request: Value,
    calls: Vec<Value>,
}
impl diagnostics::Probe for FixtureProbe {
    fn call(&mut self, call: diagnostics::Call) -> Result<Output> {
        let args = call.args.iter().map(String::as_str).collect::<Vec<_>>();
        let key = match args.as_slice() {
            ["test"] | ["systemctl", "--user", "show"] => "test-probe",
            ["fixture-compositor", "installedVersion"] => "installedVersion",
            ["fixture-compositor", "runningVersion"] => "runningVersion",
            ["fixture-compositor", "allMonitors"] => "allMonitors",
            ["fixture-compositor", "configErrors"] => "configErrors",
            ["quickshell", "--version"] => "quickshell-version",
            ["/usr/lib/qt6/bin/qtpaths", "--qt-version"] => "qt-version",
            ["pacman", "-Q"] => "packages",
            ["pw-dump"] => "audio",
            ["fixture-python", "--version"] => "python-version",
            ["systemctl", "--user", "show", ..] => {
                if args.contains(&"LoadState") {
                    "services"
                } else {
                    "maintenance-services"
                }
            }
            ["systemctl", "--user", "--failed", "--no-legend", "--plain"] => "failed-services",
            ["journalctl", "--user", "--no-pager", "-o", "cat", "--since", "1 hour ago", "-p", "warning", "-n", "80", ..] => {
                "journal"
            }
            ["quickshell", "log", "--config", "nacre", "--no-color", "--tail", "60"] => "shell-log",
            ["du", "-s", "-B1", "/case/home/.local/state/nacre/releases"] => "release-size",
            ["nmcli", "--terse", "--escape", "no", "--fields", "SSID", "device", "wifi", "list", "--rescan", "no"] => {
                "ssids"
            }
            ["nmcli", "--terse", "--escape", "no", "--fields", "NAME", "connection", "show"] => {
                "connection-names"
            }
            ["lspci", "-s", _] => "gpu-model",
            ["modinfo", "-F", "version", _] => "gpu-driver-version",
            _ => panic!("Unscripted diagnostics reference probe"),
        };
        self.calls.push(value!({"args":call.args,"timeout":call.timeout,"stdin":call.stdin,"capture_output":call.capture_output,"text":call.text,"locale":call.locale,"noColor":call.no_color}));
        let output = &self.request["outputs"][key];
        if output.is_null() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Synthetic unavailable probe",
            )
            .into());
        }
        if let Some(error) = output["error"].as_str() {
            return Err(std::io::Error::new(
                if error == "timeout" {
                    std::io::ErrorKind::TimedOut
                } else {
                    std::io::ErrorKind::NotFound
                },
                "Synthetic unavailable probe",
            )
            .into());
        }
        let stdout = if output["stdout_repeat"].is_object() {
            output["stdout_repeat"]["text"]
                .as_str()
                .ok_or("Missing repeated text")?
                .repeat(
                    output["stdout_repeat"]["count"]
                        .as_u64()
                        .ok_or("Missing repeated count")? as usize,
                )
        } else {
            output["stdout"].as_str().unwrap_or("").to_owned()
        };
        Ok(Output {
            status: ExitStatus::from_raw(output["code"].as_i64().unwrap_or(0) as i32 * 256),
            stdout: stdout.into_bytes(),
            stderr: output["stderr"].as_str().unwrap_or("").as_bytes().to_vec(),
        })
    }
    fn gpu_inventory(&mut self, root: &std::path::Path) -> Result<Value> {
        assert_eq!(root, std::path::Path::new("/case/sys"));
        self.calls.push(value!({"args":["fixture-helper","resource-state","inventory","/case/sys"],"timeout":3,"stdin":-3,"capture_output":true,"text":true,"locale":null,"noColor":null}));
        Ok(self.request["gpus"]
            .as_array()
            .map(|v| Value::Array(v.clone()))
            .unwrap_or(Value::Array(vec![])))
    }
    fn executable(&self, _name: &str) -> Option<std::path::PathBuf> {
        None
    }
    fn configuration_errors(&mut self) -> Result<String> {
        if self.request["config_timeout"] == true {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Synthetic configuration timeout",
            )
            .into())
        } else {
            Ok(self.request["config_errors"].as_str().unwrap_or("").into())
        }
    }
}
fn main() -> Result<()> {
    if std::env::var("NACRE_PARITY_ROOT").as_deref() != Ok("/case")
        || std::env::var("HOME").as_deref() != Ok("/case/home")
    {
        return Err("Diagnostics adapter requires the isolated parity sandbox".into());
    }
    let request: Value =
        serde_json::from_str(&std::env::args().nth(1).ok_or("Missing reference request")?)?;
    let mut queries = FixtureProbe {
        request: request.clone(),
        calls: vec![],
    };
    let checked = "2026-10-01T12:00:00+00:00";
    let context = diagnostics::Context {
        paths: nacre_helper::paths::Paths::new("/case/home"),
        checked: checked.into(),
        epoch: chrono::DateTime::parse_from_rfc3339(checked)?.timestamp() as f64,
        backend: "Fixture".into(),
        version_command: vec!["fixture-compositor".into(), "installedVersion".into()],
        runtime_command: vec!["fixture-python".into(), "--version".into()],
        portal: "xdg-desktop-portal-hyprland.service".into(),
    };
    let platform = diagnostics::Platform {
        sys: "/case/sys".into(),
        etc: "/case/etc".into(),
        proc: "/case/proc".into(),
        kernel: "6.12.4-fixture".into(),
        architecture: "x86_64".into(),
        cpus: 8,
        user: "fixture-user".into(),
        hostname: "fixture-host".into(),
        additional_identities: vec![],
        commands: [
            "runningVersion",
            "installedVersion",
            "allMonitors",
            "configErrors",
        ]
        .into_iter()
        .map(|key| (key.into(), value!(["fixture-compositor", key])))
        .collect(),
        required_commands: [
            "Hyprland",
            "hyprctl",
            "uwsm",
            "python3",
            "quickshell",
            "systemctl",
            "wpctl",
            "nmcli",
            "bluetoothctl",
            "kitty",
            "dolphin",
            "hypridle",
            "hyprlock",
            "grim",
            "slurp",
            "wl-copy",
            "wl-paste",
            "notify-send",
            "brightnessctl",
            "ffmpeg",
            "fc-list",
            "ddcutil",
            "wtype",
        ]
        .iter()
        .map(|s| (*s).into())
        .collect(),
    };
    let result = match request["operation"].as_str().ok_or("Missing operation")? {
        "redact" => diagnostics::redact(
            &request["value"],
            &request["identities"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )?,
        "monitors" => {
            diagnostics::monitors(request["value"].as_str().ok_or("Missing monitor value")?)
        }
        "audio" => {
            diagnostics::audio_devices(request["value"].as_str().ok_or("Missing audio value")?)
        }
        "hardware-fixture" => {
            value!({"hardware":diagnostics::hardware(std::path::Path::new("/case/sys"))?,"monitors":diagnostics::monitors(&serde_json::to_string(&request["monitors"])?) })
        }
        "probe" => diagnostics::probe(
            &request["args"]
                .as_array()
                .map(|args| {
                    args.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_else(|| vec!["test".into()]),
            &mut queries,
        )?,
        "maintenance" => diagnostics::maintenance_facts(&context, &mut queries),
        "maintenance-state" => diagnostics::maintenance_state(&context, &mut queries)?,
        "configuration-errors" => Value::String(diagnostics::configuration_errors(&mut queries)),
        "collect" => diagnostics::collect(&context, &platform, &mut queries)?,
        "markdown" | "support-markdown" => Value::String(diagnostics::markdown(
            &diagnostics::collect(&context, &platform, &mut queries)?,
        )?),
        "support-json" | "doctor-json" => Value::String(
            json::pretty(&diagnostics::collect(&context, &platform, &mut queries)?)? + "\n",
        ),
        "doctor-report" => Value::String(diagnostics::markdown(&diagnostics::collect(
            &context,
            &platform,
            &mut queries,
        )?)?),
        _ => return Err("Unsupported diagnostics reference operation".into()),
    };
    println!(
        "{}",
        json::encode(&value!({"value":result,"calls":queries.calls}))?
    );
    Ok(())
}
