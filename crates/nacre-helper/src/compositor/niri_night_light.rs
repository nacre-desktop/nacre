//! Niri gamma-controller policy; direct service control and scoped process records.
use super::niri_state::Error;
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};
type Result<T> = std::result::Result<T, Error>;
pub const SERVICE: &str = "nacre-night-light.service";
fn failure(message: impl Into<String>) -> Error {
    Error {
        kind: "RuntimeError",
        message: message.into(),
    }
}
pub fn command(enabled: bool, display: &str, runtime: &str) -> Vec<String> {
    if !enabled {
        return vec![
            "systemctl".into(),
            "--user".into(),
            "stop".into(),
            SERVICE.into(),
        ];
    }
    [
        "systemd-run",
        "--user",
        "--collect",
        "--unit=nacre-night-light.service",
        "--property=PartOf=graphical-session.target",
        &format!("--setenv=WAYLAND_DISPLAY={display}"),
        &format!("--setenv=XDG_RUNTIME_DIR={runtime}"),
        "wlsunset",
        "-t",
        "4500",
        "-T",
        "4501",
        "-S",
        "00:00",
        "-s",
        "23:59",
        "-d",
        "0",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
pub fn configured_elsewhere(nacre_config_root: &Path) -> bool {
    ["gammastep/config.ini", "wlsunset/config"]
        .iter()
        .any(|name| nacre_config_root.join(name).exists())
}
pub fn external_controller(
    process_root: &Path,
    own: Option<u32>,
    display: &str,
    runtime: &str,
) -> crate::Result<bool> {
    if display.is_empty() {
        return Ok(false);
    }
    for process in fs::read_dir(process_root)? {
        let process = process?;
        let name = process.file_name();
        let Some(pid) = name.to_str().and_then(|name| name.parse::<u32>().ok()) else {
            continue;
        };
        if own == Some(pid) {
            continue;
        }
        let path = process.path();
        let read = || -> std::io::Result<bool> {
            let command = fs::read(path.join("cmdline"))?;
            let command = command.split(|byte| *byte == 0).next().unwrap_or_default();
            use std::os::unix::ffi::OsStrExt;
            let name = Path::new(std::ffi::OsStr::from_bytes(command)).file_name();
            if !name.is_some_and(|name| {
                ["wlsunset", "gammastep", "hyprsunset"]
                    .iter()
                    .any(|candidate| name == *candidate)
            }) {
                return Ok(false);
            }
            let environment = fs::read(path.join("environ"))?;
            let entries = environment.split(|byte| *byte == 0).collect::<Vec<_>>();
            Ok(
                entries.contains(&format!("WAYLAND_DISPLAY={display}").as_bytes())
                    && entries.contains(&format!("XDG_RUNTIME_DIR={runtime}").as_bytes()),
            )
        };
        match read() {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}
pub trait Runtime {
    fn available(&mut self) -> Result<bool>;
    fn stop(&mut self) -> Result<()>;
}
pub fn temperature(enabled: bool, runtime: &mut impl Runtime) -> Result<Value> {
    if enabled {
        if !runtime.available()? {
            return Err(failure(
                "Night light requires gamma control on a physical output",
            ));
        }
    } else {
        runtime.stop()?;
    }
    Ok(json!({"code":0,"stdout":"","stderr":""}))
}
pub fn available_with(installed: bool, probe: impl FnOnce() -> crate::Result<bool>) -> bool {
    if !installed {
        return false;
    }
    match probe() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("nacre-helper: night-light capability probe failed: {error}");
            false
        }
    }
}
pub struct Native;
impl Runtime for Native {
    fn available(&mut self) -> Result<bool> {
        let installed = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .any(|path| path.join("wlsunset").is_file());
        Ok(available_with(installed, || {
            super::wayland_registry::gamma_available(Duration::from_secs(3))
        }))
    }
    fn stop(&mut self) -> Result<()> {
        crate::systemd::stop(SERVICE).map_err(|error| failure(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        available: bool,
        stopped: bool,
    }
    impl Runtime for Fixture {
        fn available(&mut self) -> Result<bool> {
            Ok(self.available)
        }
        fn stop(&mut self) -> Result<()> {
            self.stopped = true;
            Ok(())
        }
    }
    #[test]
    fn frozen_night_light_policy_and_synthetic_controller_records() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-night-light-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let home = tempfile::tempdir()?;
            let result = match input["method"].as_str() {
                Some("night_light_command") => Ok(json!(command(
                    input["args"][0].as_bool().ok_or("Missing toggle")?,
                    "fixture-0",
                    "/case/runtime"
                ))),
                Some("set_night_light_temperature") => {
                    let mut runtime = Fixture {
                        available: input["available"].as_bool().unwrap_or(true),
                        stopped: false,
                    };
                    let result = temperature(
                        input["args"][0].as_bool().ok_or("Missing toggle")?,
                        &mut runtime,
                    );
                    assert_eq!(runtime.stopped, input["args"][0] == false);
                    result
                }
                Some("night_light_available") => Ok(json!(available_with(
                    input["tools"].as_bool().unwrap_or(true),
                    || {
                        if input["probe_raises"] == true {
                            Err("fixture probe failure".into())
                        } else {
                            Ok(input["probe_code"] == 0
                                && input["probe_stdout"]
                                    .as_str()
                                    .unwrap_or("")
                                    .contains("zwlr_gamma_control_manager_v1"))
                        }
                    }
                ))),
                Some("night_light_configured_elsewhere") => {
                    if let Some(name) = input["config"].as_str().filter(|name| !name.is_empty()) {
                        let path = home.path().join(name);
                        fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
                        fs::write(path, "fixture")?;
                    }
                    Ok(json!(configured_elsewhere(home.path())))
                }
                Some("night_light_external_socket") => {
                    for record in input["processes"].as_array().ok_or("Missing processes")? {
                        let folder = home
                            .path()
                            .join(record["pid"].as_str().ok_or("Missing PID")?);
                        fs::create_dir(&folder)?;
                        fs::write(
                            folder.join("cmdline"),
                            format!("{}\0", record["command"].as_str().ok_or("Missing command")?),
                        )?;
                        fs::write(
                            folder.join("environ"),
                            format!(
                                "WAYLAND_DISPLAY={}\0XDG_RUNTIME_DIR=/case/runtime\0",
                                record["display"].as_str().ok_or("Missing display")?
                            ),
                        )?;
                    }
                    Ok(json!(external_controller(
                        home.path(),
                        if input["args"][0] == true {
                            input["own"].as_str().and_then(|pid| pid.parse().ok())
                        } else {
                            None
                        },
                        "fixture-0",
                        "/case/runtime"
                    )?))
                }
                _ => return Err("Unknown night-light reference method".into()),
            };
            let (result, exit) = match result {
                Ok(value) => (value, 0),
                Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
            };
            assert_eq!(
                crate::json::encode(&result)? + "\n",
                case["observed"]["stdout"],
                "{}",
                input["name"]
            );
            assert_eq!(json!(exit), case["observed"]["exit"], "{}", input["name"]);
        }
        Ok(())
    }
}
