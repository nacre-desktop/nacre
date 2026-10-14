//! Preserve explicit compositor context and independently confirmed acceptance gates.
use crate::{paths::Paths, Result};
use serde_json::Value;
use std::{collections::BTreeMap, fs};
pub fn compositor(environment: &BTreeMap<String, String>) -> &'static str {
    let desktops = environment
        .get("XDG_CURRENT_DESKTOP")
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    let hyprland = desktops.split(':').any(|value| value == "hyprland");
    let niri = desktops.split(':').any(|value| value == "niri");
    if hyprland && !niri {
        return "hyprland";
    }
    if niri && !hyprland {
        return "niri";
    }
    let niri = environment
        .get("NIRI_SOCKET")
        .is_some_and(|value| !value.is_empty());
    let hyprland = environment
        .get("HYPRLAND_INSTANCE_SIGNATURE")
        .is_some_and(|value| !value.is_empty());
    match (niri, hyprland) {
        (true, false) => "niri",
        (false, true) => "hyprland",
        _ => "unknown",
    }
}
pub fn allowed(action: &str, environment: &BTreeMap<String, String>, paths: &Paths) -> bool {
    match compositor(environment) {
        "hyprland" => return true,
        "niri" => {}
        _ => return false,
    }
    let path = paths.config.join("niri-on-pc/acceptance.json");
    let read = || -> Result<Value> { Ok(serde_json::from_slice(&fs::read(&path)?)?) };
    let record = match read() {
        Ok(record) => record,
        Err(error) => {
            if !error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            {
                eprintln!("nacre-helper: session acceptance unavailable: {error}");
            }
            return false;
        }
    };
    if !record.is_object() || record["phase"] != "nacre" {
        return false;
    }
    record["backendReady"] == true
        && match action {
            "idle" => record["unlockVerified"] == true,
            "observer" => record["observerReady"] == true,
            _ => true,
        }
}
pub const HELP:&str="usage: niri-session-gate.py [-h] {shell,observer,idle}\n\nExecCondition for safe, opt-in niri acceptance without changing Hyprland.\n\npositional arguments:\n  {shell,observer,idle}\n\noptions:\n  -h, --help            show this help message and exit\n";
pub fn arguments(args: &[String]) -> Result<Option<&str>> {
    let mut action = None;
    let mut extras = false;
    let mut positional = false;
    for arg in args {
        if !positional && ["--help", "-h"].contains(&arg.as_str()) {
            return Ok(None);
        }
        if !positional && arg == "--" {
            positional = true;
            continue;
        }
        if !positional && arg.starts_with('-') {
            extras = true;
            continue;
        }
        if action.is_none() {
            if !["shell", "observer", "idle"].contains(&arg.as_str()) {
                return Err("Unknown session gate action".into());
            }
            action = Some(arg.as_str());
        } else {
            extras = true;
        }
    }
    if extras {
        return Err("Unexpected session gate arguments".into());
    }
    action
        .map(Some)
        .ok_or_else(|| "Expected shell, observer or idle".into())
}
pub fn run(args: &[String]) -> Result<i32> {
    let action = match arguments(args) {
        Ok(None) => {
            print!("{HELP}");
            return Ok(0);
        }
        Ok(Some(action)) => action,
        Err(error) => {
            eprintln!("niri-session-gate: {error}");
            return Ok(2);
        }
    };
    Ok(i32::from(!allowed(
        action,
        &std::env::vars().collect(),
        &Paths::environment()?,
    )))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_cli_help_and_unsupported_session_exits() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-gate-cli-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing gate CLI cases")?
        {
            let args: Vec<String> = serde_json::from_value(case["request"]["args"].clone())?;
            let (stdout, exit) = match arguments(&args) {
                Ok(None) => (HELP, 0),
                Ok(Some(_)) => ("", 1),
                Err(_) => ("", 2),
            };
            assert_eq!(stdout, case["observed"]["stdout"], "{args:?}");
            assert_eq!(exit, case["observed"]["exit"], "{args:?}");
        }
        Ok(())
    }
    #[test]
    fn frozen_context_acceptance_permissions_and_record_bytes() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-gate-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing gate cases")? {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let request = &case["request"];
            let file = paths.config.join("niri-on-pc/acceptance.json");
            if let Some(record) = request["record"].as_str() {
                fs::create_dir_all(file.parent().ok_or("Invalid gate record")?)?;
                fs::write(&file, record)?;
            }
            let before = fs::read(&file).ok();
            let environment: BTreeMap<String, String> =
                serde_json::from_value(request["environment"].clone())?;
            assert_eq!(
                compositor(&environment),
                case["observed"]["backend"],
                "{}",
                request["name"]
            );
            assert_eq!(
                allowed(
                    request["action"].as_str().ok_or("Missing action")?,
                    &environment,
                    &paths
                ),
                case["observed"]["allowed"],
                "{}",
                request["name"]
            );
            assert_eq!(before, fs::read(&file).ok(), "{}", request["name"]);
        }
        Ok(())
    }
}
