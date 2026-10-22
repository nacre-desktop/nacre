//! Status and explicit capture/color-picker controls for the device panel.
use crate::compositor::backend::Backend;
use crate::{i18n::tr, Result};
use serde_json::{json, Value};
use std::process::Output;
use std::time::Duration;
fn available(program: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .any(|folder| folder.join(program).is_file())
}
fn bounded(program: &str, args: &[&str], timeout: u64) -> Result<Output> {
    let arguments = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    crate::process::run(program, &arguments, None, Duration::from_secs(timeout))
}
fn color_picker_result(output: Result<Output>) -> std::result::Result<i32, String> {
    let output = output.map_err(|error| error.to_string())?;
    let Some(code) = output.status.code() else {
        return Err("hyprpicker terminated without an exit code".into());
    };
    if code == 0 {
        Ok(code)
    } else {
        Err(format!(
            "Command '['hyprpicker', '-a']' returned non-zero exit status {code}."
        ))
    }
}
fn backend(paths: &crate::paths::Paths) -> Result<Backend> {
    Ok(Backend::new(paths.clone(), std::env::vars().collect())?)
}
fn state_value(
    screenshot: bool,
    color_picker: bool,
    power_profiles: bool,
    night_light: bool,
    reason: String,
    external: bool,
    enabled: bool,
) -> Value {
    json!({"screenshot":screenshot,"colorPicker":color_picker,"powerProfilesSupported":power_profiles,"nightLightSupported":night_light,"nightLightReason":reason,"nightLightExternal":external,"nightLightEnabled":enabled})
}
fn owns_service(active: bool, owner: &str, data: &Value) -> bool {
    active && !owner.is_empty() && data["owner"] == owner
}
fn externally_managed(
    active: bool,
    owner: &str,
    stored_owner: &str,
    configured: bool,
    socket: bool,
) -> bool {
    configured || socket || (active && (owner.is_empty() || stored_owner != owner))
}
fn supported_probe<T>(supported: bool, probe: impl FnOnce() -> T) -> Option<T> {
    supported.then(probe)
}
fn owned_record(owner: &str, enabled: bool) -> Value {
    json!({"owner":owner,"enabled":enabled})
}
fn save_record(path: &std::path::Path, owner: &str, enabled: bool) -> Result<()> {
    crate::state::write(path, &owned_record(owner, enabled))
}
fn screenshot_error(paths: &crate::paths::Paths, result: Result<i32>) -> String {
    match result {
        Ok(code) => format!(
            "Command '['python3', '{}', 'region']' returned non-zero exit status {code}.",
            paths.tools().join("screenshot.py").display()
        ),
        Err(error) => error.to_string(),
    }
}
fn apply_with_retry<T>(
    started: bool,
    owner: &str,
    mut current_owner: impl FnMut() -> String,
    mut apply: impl FnMut() -> Result<T>,
    mut stop: impl FnMut(),
) -> Result<T> {
    for attempt in 0..=11 {
        match apply() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let still_owner = started && current_owner() == owner;
                if started && attempt < 11 && still_owner {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                if still_owner {
                    stop()
                }
                return Err(error);
            }
        }
    }
    Err("Night-light controller did not become ready".into())
}
fn state(paths: &crate::paths::Paths, backend: &mut Backend) -> Result<Value> {
    let supported = backend.capabilities()["nightLightController"]
        .as_bool()
        .unwrap_or(true);
    let night_available = supported_probe(supported, || {
        backend.module_call("night_light_available", &[], &json!({}))
    })
    .transpose()?
    .and_then(|value| value.as_bool())
    .unwrap_or(false);
    let unit = if backend.name() == "niri" {
        "nacre-night-light.service"
    } else {
        "hyprsunset.service"
    };
    let service = if supported && (night_available || backend.name() == "niri") {
        crate::systemd::status(unit).unwrap_or_default()
    } else {
        crate::systemd::Status::default()
    };
    let owner = service.owner();
    let path = paths.state.join(if backend.name() == "niri" {
        "control-tools-niri.json"
    } else {
        "control-tools.json"
    });
    let data = crate::state::read(&path, json!({})).unwrap_or_else(|_| json!({}));
    let owned = owns_service(service.active, &owner, &data);
    let configured = supported_probe(supported, || {
        backend.module_call(
            "night_light_configured_elsewhere",
            &[json!(paths.home)],
            &json!({}),
        )
    })
    .and_then(|result| result.ok())
    .and_then(|value| value.as_bool())
    .unwrap_or(false);
    let external_socket = supported_probe(supported, || {
        backend.module_call(
            "night_light_external_socket",
            &[json!(service.active)],
            &json!({}),
        )
    })
    .and_then(|result| result.ok())
    .and_then(|value| value.as_bool())
    .unwrap_or(false);
    let external = (service.active && !owned) || configured || external_socket;
    Ok(state_value(
        available("grim") && available("slurp"),
        available("hyprpicker"),
        available("powerprofilesctl"),
        night_available || (owned && backend.name() == "niri"),
        if night_available {
            "".to_owned()
        } else {
            tr(
                "ui.compositor.requires-gamma-controller",
                "Requires a controller and output gamma control.",
            )
        },
        external,
        owned && data["enabled"] == true,
    ))
}
fn night_light(paths: &crate::paths::Paths, backend: &mut Backend, enabled: bool) -> Result<Value> {
    let supported = backend.capabilities()["nightLightController"]
        .as_bool()
        .unwrap_or(true);
    let available = supported
        && backend
            .module_call("night_light_available", &[], &json!({}))?
            .as_bool()
            .unwrap_or(false);
    if !supported || (enabled && !available) {
        return Err(tr(
            "helper.control-tools.install-the-system-hyprsunset-package-for-night-light",
            "Night light requires a compatible controller and output gamma control",
        )
        .into());
    }
    let unit = if backend.name() == "niri" {
        "nacre-night-light.service"
    } else {
        "hyprsunset.service"
    };
    let record = paths.state.join(if backend.name() == "niri" {
        "control-tools-niri.json"
    } else {
        "control-tools.json"
    });
    let data = crate::state::read(&record, json!({})).unwrap_or_else(|_| json!({}));
    let service = crate::systemd::status(unit).unwrap_or_default();
    let mut owner = service.owner();
    let configured = backend
        .module_call(
            "night_light_configured_elsewhere",
            &[json!(paths.home)],
            &json!({}),
        )
        .unwrap_or(json!(false))
        == true;
    let external = backend
        .module_call(
            "night_light_external_socket",
            &[json!(service.active)],
            &json!({}),
        )
        .unwrap_or(json!(false))
        == true;
    if externally_managed(
        service.active,
        &owner,
        data["owner"].as_str().unwrap_or(""),
        configured,
        external,
    ) {
        return Err(tr(
            "helper.control-tools.night-light-is-managed-externally-use-its-existing-controller",
            "Night light is managed externally; use its existing controller",
        )
        .into());
    }
    let started = enabled && !service.active;
    if started {
        if backend.name() == "niri" {
            let args: Vec<String> = serde_json::from_value(backend.module_call(
                "night_light_command",
                &[json!(true)],
                &json!({}),
            )?)?;
            let Some((program, argv)) = args.split_first() else {
                return Err("Night-light controller command is empty".into());
            };
            if !bounded(
                program,
                argv.iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                10,
            )?
            .status
            .success()
            {
                return Err("Night-light service did not start".into());
            }
        } else if !bounded("systemctl", &["--user", "start", unit], 10)?
            .status
            .success()
        {
            return Err("Night-light service did not start".into());
        }
        let now = crate::systemd::status(unit)?;
        owner = now.owner();
        if !now.active || owner.is_empty() {
            return Err(tr(
                "helper.control-tools.night-light-service-did-not-become-ready",
                "Night-light service did not become ready",
            )
            .into());
        }
        save_record(&record, &owner, false)?;
    }
    if service.active || started {
        apply_with_retry(
            started,
            &owner,
            || {
                crate::systemd::status(unit)
                    .map(|status| status.owner())
                    .unwrap_or_default()
            },
            || {
                Ok(backend.module_call(
                    "set_night_light_temperature",
                    &[json!(enabled)],
                    &json!({"timeout":1}),
                )?)
            },
            || {
                let _ = bounded("systemctl", &["--user", "stop", unit], 10);
            },
        )?;
    }
    let active = if !enabled && backend.name() == "niri" {
        false
    } else {
        crate::systemd::status(unit).is_ok_and(|status| status.active)
    };
    if !active {
        owner.clear()
    }
    save_record(&record, &owner, enabled && active)?;
    state(paths, backend)
}
pub fn run(args: &[String]) -> Result<i32> {
    let action = args.first().map(String::as_str).unwrap_or("");
    match action {
        "state" if args.len() == 1 => {
            let paths = crate::paths::Paths::environment()?;
            let mut backend = backend(&paths)?;
            let value = state(&paths, &mut backend)?;
            println!("{}", crate::json::encode(&value)?);
            Ok(0)
        }
        "night-light" => {
            let enabled = match args.get(1).map(String::as_str) {
                Some("on") => true,
                Some("off") => false,
                _ => {
                    let err = tr("helper.control-tools.choose-on-or-off", "Choose on or off");
                    println!("{}", crate::json::encode(&json!({"error":err}))?);
                    return Ok(1);
                }
            };
            let paths = crate::paths::Paths::environment()?;
            let mut backend = backend(&paths)?;
            match night_light(&paths, &mut backend, enabled) {
                Ok(value) => {
                    println!("{}", crate::json::encode(&value)?);
                    Ok(0)
                }
                Err(error) => {
                    println!(
                        "{}",
                        crate::json::encode(&json!({"error":error.to_string()}))?
                    );
                    Ok(1)
                }
            }
        }
        "screenshot" if args.len() == 1 => {
            let result = crate::screenshot::run(&["region".into()]);
            if result.as_ref().is_ok_and(|code| *code == 0) {
                Ok(0)
            } else {
                let error = screenshot_error(&crate::paths::Paths::environment()?, result);
                println!("{}", crate::json::encode(&json!({"error":error}))?);
                Ok(1)
            }
        }
        "color-picker" if args.len() == 1 => {
            match color_picker_result(bounded("hyprpicker", &["-a"], 180)) {
                Ok(code) => Ok(code),
                Err(error) => {
                    println!("{}", crate::json::encode(&json!({"error":error}))?);
                    Ok(1)
                }
            }
        }
        _ => {
            let error = tr(
                "helper.control-tools.choose-on-or-off",
                "Unsupported control action",
            );
            println!("{}", crate::json::encode(&json!({"error":error}))?);
            Ok(1)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_shape_keeps_optional_helpers_boolean() {
        let value = state_value(
            false,
            true,
            false,
            false,
            "Requires a controller and output gamma control.".into(),
            false,
            false,
        );
        for key in [
            "screenshot",
            "colorPicker",
            "powerProfilesSupported",
            "nightLightSupported",
            "nightLightExternal",
            "nightLightEnabled",
        ] {
            assert!(value[key].is_boolean());
        }
        assert_eq!(
            value["nightLightReason"],
            "Requires a controller and output gamma control."
        );
    }
    #[test]
    fn state_discovery_is_read_only_and_shows_external_owner() {
        let home = tempfile::tempdir().unwrap();
        let record = home.path().join("control-tools.json");
        let data = json!({"owner":"owner","enabled":true});
        let owner = "other";
        let owned = owns_service(true, owner, &data);
        let external =
            externally_managed(true, owner, data["owner"].as_str().unwrap(), false, false);
        let snapshot = state_value(
            true,
            true,
            true,
            true,
            "".into(),
            external,
            owned && data["enabled"] == true,
        );
        assert!(snapshot["nightLightExternal"].as_bool().unwrap());
        assert!(!snapshot["nightLightEnabled"].as_bool().unwrap());
        assert!(!record.exists());
    }
    #[test]
    fn unsupported_controller_keeps_other_controls_available() {
        let value = state_value(
            true,
            true,
            true,
            false,
            "Requires a controller and output gamma control.".into(),
            false,
            false,
        );
        assert!(value["screenshot"].as_bool().unwrap());
        assert!(value["colorPicker"].as_bool().unwrap());
        assert!(!value["nightLightSupported"].as_bool().unwrap());
        assert!(!value["nightLightExternal"].as_bool().unwrap());
        let called = std::cell::Cell::new(false);
        assert_eq!(
            supported_probe(false, || {
                called.set(true);
                true
            }),
            None
        );
        assert!(!called.get());
    }
    #[test]
    fn dead_socket_does_not_block_control_but_live_listener_is_external() {
        use std::{collections::BTreeMap, os::unix::net::UnixListener};
        let home = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let folder = runtime.path().join("hypr/test");
        std::fs::create_dir_all(&folder).unwrap();
        let socket = folder.join(".hyprsunset.sock");
        let mut backend = Backend::new(
            crate::paths::Paths::new(home.path()),
            BTreeMap::from([
                ("NACRE_COMPOSITOR".into(), "hyprland".into()),
                ("HYPRLAND_INSTANCE_SIGNATURE".into(), "test".into()),
                (
                    "XDG_RUNTIME_DIR".into(),
                    runtime.path().to_string_lossy().into_owned(),
                ),
            ]),
        )
        .unwrap();
        {
            let listener = UnixListener::bind(&socket).unwrap();
            drop(listener);
        }
        assert_eq!(
            backend
                .module_call("night_light_external_socket", &[json!(false)], &json!({}))
                .unwrap(),
            json!(false)
        );
        std::fs::remove_file(&socket).unwrap();
        let _listener = UnixListener::bind(&socket).unwrap();
        assert_eq!(
            backend
                .module_call("night_light_external_socket", &[json!(false)], &json!({}))
                .unwrap(),
            json!(true)
        );
    }
    #[test]
    fn external_schedules_and_sockets_are_refused() {
        assert!(externally_managed(false, "", "", true, false));
        assert!(externally_managed(false, "", "", false, true));
        assert!(externally_managed(true, "other", "owned", false, false));
        assert!(!externally_managed(true, "owned", "owned", false, false));
    }
    #[test]
    fn newly_started_owner_retries_and_failed_temperature_rolls_back() {
        let mut calls = 0;
        let mut stops = 0;
        let result = apply_with_retry(
            true,
            "owned",
            || "owned".into(),
            || {
                calls += 1;
                if calls == 1 {
                    Err("not ready".into())
                } else {
                    Ok(())
                }
            },
            || stops += 1,
        );
        assert!(result.is_ok());
        assert_eq!(calls, 2);
        assert_eq!(stops, 0);
        let mut stops = 0;
        let failed = apply_with_retry(
            true,
            "owned",
            || "owned".into(),
            || Err::<(), crate::Error>("native socket failure".into()),
            || stops += 1,
        );
        assert!(failed.is_err());
        assert_eq!(stops, 1);
    }
    #[test]
    fn retries_and_rollback_stop_only_the_started_invocation() {
        let mut owners = vec!["replacement".to_owned()].into_iter();
        let mut stops = 0;
        let failed = apply_with_retry(
            true,
            "owned",
            || owners.next().unwrap_or_default(),
            || Err::<(), crate::Error>("owner changed".into()),
            || stops += 1,
        );
        assert!(failed.is_err());
        assert_eq!(stops, 0);
        let mut stops = 0;
        let failed = apply_with_retry(
            false,
            "external",
            || "external".into(),
            || Err::<(), crate::Error>("external socket".into()),
            || stops += 1,
        );
        assert!(failed.is_err());
        assert_eq!(stops, 0);
    }
    #[test]
    fn failed_temperature_never_publishes_enabled() {
        assert_eq!(
            owned_record("owned", false),
            json!({"owner":"owned","enabled":false})
        );
        assert_eq!(owned_record("", false)["enabled"], false);
    }
    #[test]
    fn ownership_record_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("control-tools.json");
        save_record(&path, "owned", true).unwrap();
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn screenshot_failure_keeps_the_control_tools_error_envelope() {
        let paths = crate::paths::Paths::new("/fixture/home");
        assert_eq!(
            screenshot_error(&paths, Ok(1)),
            "Command '['python3', '/fixture/home/.local/share/nacre/shell/tools/screenshot.py', 'region']' returned non-zero exit status 1."
        );
        assert_eq!(
            screenshot_error(&paths, Err("grim failed".into())),
            "grim failed"
        );
    }
    #[test]
    fn color_picker_failure_uses_the_control_tools_json_error_envelope() {
        use std::os::unix::process::ExitStatusExt;
        let output = Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let error = color_picker_result(Ok(output)).unwrap_err();
        assert_eq!(
            error,
            "Command '['hyprpicker', '-a']' returned non-zero exit status 1."
        );
        assert_eq!(
            json!({"error":error}),
            json!({"error":"Command '['hyprpicker', '-a']' returned non-zero exit status 1."})
        );
        let success = Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert_eq!(color_picker_result(Ok(success)), Ok(0));
    }
    #[test]
    fn owned_service_can_be_disabled_after_claim() {
        let data = owned_record("321:invocation", true);
        assert!(owns_service(true, "321:invocation", &data));
        let disabled = owned_record("", false);
        assert!(!disabled["enabled"].as_bool().unwrap());
        assert!(!owns_service(false, "", &disabled));
    }
}
