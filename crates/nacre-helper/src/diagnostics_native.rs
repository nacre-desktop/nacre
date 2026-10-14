//! Production support-report queries: native IPC plus the actual selected tools.
use crate::{
    compositor::Compositor,
    diagnostics::{self, Call, Context, Platform, Probe},
    network_state::{self, IdentityKind},
    paths::Paths,
    process, systemd, Result,
};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    env, fs,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::PathBuf,
    process::{Command, ExitStatus, Output},
    time::{Duration, Instant},
};
pub struct Queries {
    compositor: Option<Compositor>,
    niri: bool,
}
fn output(code: i32, text: String) -> Output {
    Output {
        status: ExitStatus::from_raw(code * 256),
        stdout: text.into_bytes(),
        stderr: vec![],
    }
}
fn find(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join(name))
        .find(|p| p.is_file() && fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0))
}
impl Queries {
    fn socket(&mut self, name: &str, timeout: Duration) -> Result<String> {
        if self.compositor.is_none() {
            self.compositor = Some(Compositor::connect()?);
        }
        let result = self
            .compositor
            .as_mut()
            .ok_or("Missing compositor connection")?
            .diagnostic(name, timeout);
        if result.is_err() {
            self.compositor = None;
        }
        result
    }
}
impl Probe for Queries {
    fn call(&mut self, call: Call) -> Result<Output> {
        let args = call.args.iter().map(String::as_str).collect::<Vec<_>>();
        let timeout = Duration::from_secs(call.timeout);
        match args.as_slice() {
            ["hyprctl", "version"] | ["niri", "msg", "--json", "version"] => {
                return Ok(output(0, self.socket("runningVersion", timeout)?))
            }
            ["hyprctl", "-j", "monitors", "all"] | ["niri", "msg", "--json", "outputs"] => {
                return Ok(output(0, self.socket("allMonitors", timeout)?))
            }
            ["hyprctl", "configerrors"] => {
                return Ok(output(0, self.socket("configErrors", timeout)?))
            }
            ["systemctl", "--user", "show", ..] => {
                let mut units = Vec::new();
                let mut properties = Vec::new();
                let mut at = 3;
                while at < args.len() {
                    if args[at] == "-p" {
                        at += 1;
                        properties.push(args.get(at).ok_or("Missing report property")?.to_string());
                    } else {
                        units.push(args[at].to_owned());
                    }
                    at += 1;
                }
                let (code, text) = systemd::report_units(&units, &properties, timeout)?;
                return Ok(output(code, text));
            }
            ["systemctl", "--user", "--failed", "--no-legend", "--plain"] => {
                return Ok(output(0, systemd::failed_units(timeout)?))
            }
            ["nmcli", "--terse", "--escape", "no", "--fields", "SSID", "device", "wifi", "list", "--rescan", "no"] => {
                return Ok(output(
                    0,
                    network_state::redaction_names(IdentityKind::Ssids)?.join("\n"),
                ))
            }
            ["nmcli", "--terse", "--escape", "no", "--fields", "NAME", "connection", "show"] => {
                return Ok(output(
                    0,
                    network_state::redaction_names(IdentityKind::Profiles)?.join("\n"),
                ))
            }
            ["python3", "--version"] => return Ok(output(1, String::new())), // Retained compatibility field; no interpreter is started.
            ["du", "-s", "-B1", path] => {
                let deadline = Instant::now() + timeout;
                let mut size = 0u64;
                let mut seen = HashSet::new();
                for entry in walkdir::WalkDir::new(path).follow_links(false) {
                    if Instant::now() >= deadline {
                        return Err("Release size query timed out".into());
                    }
                    let entry = entry?;
                    let metadata = fs::symlink_metadata(entry.path())?;
                    if seen.insert((metadata.dev(), metadata.ino())) {
                        size = size
                            .checked_add(metadata.blocks().saturating_mul(512))
                            .ok_or("Release size overflow")?;
                    }
                }
                return Ok(output(0, format!("{size} {path}\n")));
            }
            _ => {}
        }
        let allowed = matches!(
            args.as_slice(),
            ["quickshell", "--version"]
                | [
                    "quickshell",
                    "log",
                    "--config",
                    "nacre",
                    "--no-color",
                    "--tail",
                    "60"
                ]
                | ["pacman", "-Q"]
                | ["pw-dump"]
                | ["lspci", "-s", _]
                | ["modinfo", "-F", "version", _]
                | ["Hyprland", "--version"]
                | ["niri", "--version"]
                | ["niri", "validate"]
        ) || (args.len() == 2 && args[1] == "--qt-version")
            || (args.first() == Some(&"journalctl") && args.get(1) == Some(&"--user"));
        if !allowed {
            return Err("Unsupported support-report query".into());
        }
        let mut command = Command::new(args.first().ok_or("Missing report tool")?);
        command.args(&args[1..]);
        if let Some(locale) = call.locale {
            command.env("LC_ALL", locale);
        }
        if let Some(no_color) = call.no_color {
            command.env("NO_COLOR", no_color);
        }
        process::capture(command, None, timeout)
    }
    fn configuration_errors(&mut self) -> Result<String> {
        if self.niri {
            let call = Call {
                args: vec!["niri".into(), "validate".into()],
                timeout: 5,
                stdin: None,
                capture_output: true,
                text: true,
                locale: None,
                no_color: None,
            };
            let result = self.call(call)?;
            Ok(if result.status.success() {
                String::new()
            } else {
                let bytes = if result.stderr.is_empty() {
                    result.stdout
                } else {
                    result.stderr
                };
                String::from_utf8(bytes)?.trim().to_owned()
            })
        } else {
            self.socket("configErrors", Duration::from_secs(5))
                .map(|s| s.trim().to_owned())
        }
    }
    fn gpu_inventory(&mut self, root: &std::path::Path) -> Result<Value> {
        Ok(crate::resource_state::gpu_inventory(root))
    }
    fn executable(&self, name: &str) -> Option<PathBuf> {
        find(name)
    }
}
pub fn report(as_json: bool) -> Result<String> {
    let explicit = env::var("NACRE_COMPOSITOR")
        .unwrap_or_default()
        .to_lowercase();
    if !explicit.is_empty() && !["niri", "hyprland"].contains(&explicit.as_str()) {
        return Err("Unknown NACRE_COMPOSITOR backend".into());
    }
    let niri = if !explicit.is_empty() {
        explicit == "niri"
    } else if env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase()
        .split(':')
        .any(|v| v == "niri")
    {
        true
    } else if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some_and(|v| !v.is_empty()) {
        false
    } else {
        env::var_os("NIRI_SOCKET").is_some_and(|v| !v.is_empty())
    };
    let checked = chrono::Utc::now();
    let uname = rustix::system::uname();
    let context = Context {
        paths: Paths::environment()?,
        checked: checked.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string(),
        epoch: checked.timestamp() as f64 + checked.timestamp_subsec_nanos() as f64 / 1e9,
        backend: if niri { "niri" } else { "Hyprland" }.into(),
        version_command: if niri {
            vec!["niri".into(), "--version".into()]
        } else {
            vec!["Hyprland".into(), "--version".into()]
        },
        runtime_command: vec!["python3".into(), "--version".into()],
        portal: if niri {
            "xdg-desktop-portal-gnome.service"
        } else {
            "xdg-desktop-portal-hyprland.service"
        }
        .into(),
    };
    let commands = if niri {
        json!({"runningVersion":["niri","msg","--json","version"],"installedVersion":["niri","--version"],"allMonitors":["niri","msg","--json","outputs"],"configErrors":["niri","validate"]})
    } else {
        json!({"runningVersion":["hyprctl","version"],"installedVersion":["Hyprland","--version"],"allMonitors":["hyprctl","-j","monitors","all"],"configErrors":["hyprctl","configerrors"]})
    };
    let platform = Platform {
        sys: "/sys".into(),
        etc: "/etc".into(),
        proc: "/proc".into(),
        kernel: uname.release().to_string_lossy().into_owned(),
        architecture: uname.machine().to_string_lossy().into_owned(),
        cpus: nix::unistd::sysconf(nix::unistd::SysconfVar::_NPROCESSORS_ONLN)?
            .ok_or("Logical CPU count is unavailable")?
            .try_into()?,
        user: ["LOGNAME", "USER", "LNAME", "USERNAME"]
            .iter()
            .find_map(|key| env::var(key).ok().filter(|s| !s.is_empty()))
            .unwrap_or_default(),
        hostname: uname.nodename().to_string_lossy().into_owned(),
        additional_identities: ["USER", "LOGNAME", "HOSTNAME"]
            .iter()
            .filter_map(|key| env::var(key).ok())
            .collect(),
        commands: commands
            .as_object()
            .ok_or("Invalid report commands")?
            .clone(),
        required_commands: [
            "uwsm",
            "quickshell",
            "wpctl",
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
        .map(|s| (*s).to_owned())
        .chain(std::iter::once(
            if niri { "niri" } else { "Hyprland" }.into(),
        ))
        .collect(),
    };
    let report = diagnostics::collect(
        &context,
        &platform,
        &mut Queries {
            compositor: None,
            niri,
        },
    )?;
    if as_json {
        Ok(crate::json::pretty(&report)? + "\n")
    } else {
        diagnostics::markdown(&report)
    }
}
