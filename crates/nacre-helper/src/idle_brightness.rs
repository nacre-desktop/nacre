//! Hypridle's reversible dim action, using the same guarded devices as the controls.
use crate::{files, light_devices, paths::Paths, process, Result};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

fn connected(root: &Path, connector: &str) -> bool {
    fs::read_to_string(root.join("class/drm").join(connector).join("status"))
        .is_ok_and(|s| s.trim() == "connected")
}

fn screens(root: &Path) -> Vec<Value> {
    fs::read_dir(root.join("class/drm"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| connected(root, &entry.file_name().to_string_lossy()))
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .split_once('-')
                .map(|(_, name)| json!(name))
        })
        .collect()
}

fn apply(
    paths: &Paths,
    root: &Path,
    dim: bool,
    run: &mut impl FnMut(&str, &[String], Duration) -> Result<String>,
) -> Result<()> {
    let path = paths.state.join("idle-brightness.json");
    let _guard = files::lock(&path.with_extension("lock"))?;
    let mut saved: Vec<Value> = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    // A saved level for an unplugged display would override brightness set after it returns.
    saved.retain(|d| {
        d["connector"]
            .as_str()
            .is_none_or(|connector| connected(root, connector))
    });
    let devices = if dim {
        let found = light_devices::discover(&screens(root), root, run)?;
        let devices: Vec<Value> = std::iter::once(&found["backlight"])
            .chain(found["ddc"].as_array().into_iter().flatten())
            .filter(|d| {
                // An unplugged display awaiting restore must not block other devices.
                !saved.iter().any(|old| {
                    ["kind", "device", "connector", "identity", "bus", "maximum"]
                        .iter()
                        .all(|key| old[*key] == d[*key])
                }) && d["current"].as_i64().is_some_and(|current| {
                    current > (d["maximum"].as_i64().unwrap_or(0) / 10).max(1)
                })
            })
            .cloned()
            .collect();
        // Duplicate timeouts must never replace the pre-dim level with 10%.
        saved.extend(devices.iter().cloned());
        files::atomic(&path, &serde_json::to_vec(&saved)?)?;
        devices
    } else {
        saved
    };
    let mut remaining = Vec::new();
    let mut error = None;
    for device in &devices {
        let value = if dim {
            (device["maximum"]
                .as_i64()
                .ok_or("Missing brightness range")?
                / 10)
                .max(1)
        } else {
            device["current"]
                .as_i64()
                .ok_or("Missing saved brightness")?
        };
        if let Err(failure) = light_devices::write(device, value, root, run) {
            remaining.push(device.clone());
            error = Some(failure);
        }
    }
    if !dim {
        files::atomic(&path, &serde_json::to_vec(&remaining)?)?;
    }
    if let Some(error) = error {
        return Err(error);
    }
    Ok(())
}

pub fn restore(paths: &Paths) -> Result<()> {
    apply(paths, Path::new("/sys"), false, &mut native_run)
}

fn native_run(program: &str, args: &[String], timeout: Duration) -> Result<String> {
    Ok(String::from_utf8(
        process::checked(program, args, None, timeout)?.stdout,
    )?)
}

pub fn run(arguments: &[String]) -> Result<i32> {
    let dim = match arguments {
        [action] if action == "dim" => true,
        [action] if action == "restore" => false,
        _ => return Err("Expected idle-brightness dim|restore".into()),
    };
    apply(
        &Paths::environment()?,
        Path::new("/sys"),
        dim,
        &mut native_run,
    )?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dim_and_resume_restore_brightness_without_replacing_the_saved_level() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let device = home.path().join("class/backlight/panel");
        fs::create_dir_all(&device)?;
        fs::write(device.join("max_brightness"), "100")?;
        fs::write(device.join("brightness"), "70")?;
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[String], _: Duration| {
            assert_eq!(program, "brightnessctl");
            let level = args.last().unwrap().clone();
            calls.push(level.clone());
            fs::write(device.join("brightness"), level)?;
            Ok(String::new())
        };
        apply(&paths, home.path(), true, &mut run)?;
        apply(&paths, home.path(), true, &mut run)?;
        apply(&paths, home.path(), false, &mut run)?;
        assert_eq!(calls, ["10", "70"]);
        assert_eq!(fs::read_to_string(device.join("brightness"))?, "70");
        Ok(())
    }
    #[test]
    fn external_displays_dim_and_restore_through_guarded_ddc() -> Result<()> {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let connector = home.path().join("class/drm/card0-DP-1");
        let bus = home.path().join("bus/i2c-4");
        fs::create_dir_all(&connector)?;
        fs::create_dir_all(&bus)?;
        fs::write(connector.join("status"), "connected")?;
        fs::write(connector.join("edid"), "fixture-edid")?;
        symlink(&bus, connector.join("ddc"))?;
        let mut current = 80;
        let mut writes = Vec::new();
        let mut run = |program: &str, args: &[String], _: Duration| {
            assert_eq!(program, "ddcutil");
            if args.iter().any(|arg| arg == "setvcp") {
                current = args.last().unwrap().parse::<i64>()?;
                writes.push(current);
            }
            Ok(format!("VCP 10 C {current} 100\n"))
        };
        apply(&paths, home.path(), true, &mut run)?;
        apply(&paths, home.path(), false, &mut run)?;
        assert_eq!(writes, [10, 80]);
        Ok(())
    }
    #[test]
    fn failed_dim_keeps_the_original_level_for_resume() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let device = home.path().join("class/backlight/panel");
        fs::create_dir_all(&device)?;
        fs::write(device.join("max_brightness"), "100")?;
        fs::write(device.join("brightness"), "70")?;
        assert!(apply(&paths, home.path(), true, &mut |_, _, _| Err(
            "fixture failure".into()
        ))
        .is_err());
        let mut restored = None;
        apply(&paths, home.path(), false, &mut |_, args, _| {
            restored = args.last().cloned();
            Ok(String::new())
        })?;
        assert_eq!(restored.as_deref(), Some("70"));
        Ok(())
    }
    #[test]
    fn disconnected_display_is_forgotten_instead_of_blocking_other_screens() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let device = home.path().join("class/backlight/panel");
        fs::create_dir_all(&device)?;
        fs::write(device.join("max_brightness"), "100")?;
        fs::write(device.join("brightness"), "70")?;
        files::atomic(
            &paths.state.join("idle-brightness.json"),
            &serde_json::to_vec(&json!([
                {"kind":"ddc","screen":"DP-1","bus":4,"connector":"card0-DP-1","identity":"disconnected","current":80,"maximum":100}
            ]))?,
        )?;
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[String], _: Duration| {
            assert_eq!(program, "brightnessctl");
            let value = args.last().unwrap().clone();
            calls.push(value.clone());
            fs::write(device.join("brightness"), value)?;
            Ok(String::new())
        };
        apply(&paths, home.path(), true, &mut run)?;
        apply(&paths, home.path(), false, &mut run)?;
        assert_eq!(calls, ["10", "70"]);
        let saved: Vec<Value> =
            serde_json::from_slice(&fs::read(paths.state.join("idle-brightness.json"))?)?;
        assert!(saved.is_empty());
        Ok(())
    }
    #[test]
    fn dim_never_brightens_an_already_dark_screen() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let device = home.path().join("class/backlight/panel");
        fs::create_dir_all(&device)?;
        fs::write(device.join("max_brightness"), "100")?;
        fs::write(device.join("brightness"), "5")?;
        apply(&paths, home.path(), true, &mut |_, _, _| {
            panic!("No brightness write expected")
        })?;
        Ok(())
    }
}
