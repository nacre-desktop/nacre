//! Shared, versioned input preferences projected into managed compositor config.
use crate::{compositor::backend::Backend, files, paths::Paths, state, Result};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

/// Comma-separated XKB layout identifiers, as the Settings picker offers them.
pub fn layouts_valid(layouts: &str) -> bool {
    layouts.len() <= 160
        && layouts.split(',').all(|part| {
            !part.is_empty()
                && part.len() <= 32
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
        })
}
pub fn preferences(config: &Path) -> Result<Value> {
    let value = state::read_strict(&config.join("software-input.json"), json!({}))?;
    if let Some(layout) = value.get("keyboardLayouts") {
        let layout = layout.as_str().ok_or("Invalid keyboard layouts")?;
        if !layouts_valid(layout) {
            return Err("Use comma-separated XKB layout identifiers".into());
        }
    }
    if value
        .get("touchpadEnabled")
        .is_some_and(|v| !v.is_boolean())
    {
        return Err("Invalid touchpad preference".into());
    }
    if let Some(devices) = value.get("touchpadDevices") {
        if devices.as_array().is_none_or(|a| {
            a.len() > 32
                || a.iter().any(|v| {
                    v.as_str().is_none_or(|s| {
                        s.len() > 160 || !s.is_ascii() || s.chars().any(|c| c.is_control())
                    })
                })
        }) {
            return Err("Invalid touchpad identities".into());
        }
    }
    Ok(value)
}
pub fn overlay(config: &Path, base: &Value) -> Result<Value> {
    let mut value = base.clone();
    let map = value.as_object_mut().ok_or("Invalid input configuration")?;
    let input = preferences(config)?;
    for key in ["keyboardLayouts", "touchpadEnabled", "touchpadDevices"] {
        if let Some(value) = input.get(key) {
            map.entry(key).or_insert_with(|| value.clone());
        }
    }
    Ok(value)
}
pub fn set_touchpad(paths: &Paths, enabled: bool, devices: Vec<String>) -> Result<Value> {
    apply(
        paths,
        json!({"touchpadEnabled":enabled,"touchpadDevices":devices}),
    )
}
pub fn reset_layouts(paths: &Paths) -> Result<Value> {
    apply(paths, json!({"keyboardLayouts":null}))
}

pub fn set_layouts(paths: &Paths, layouts: &str) -> Result<Value> {
    if !layouts_valid(layouts) {
        return Err("Use comma-separated XKB layout identifiers".into());
    }
    apply(paths, json!({"keyboardLayouts":layouts}))
}
/// The Settings page's layout write: empty means "System default".
pub fn switch_layouts(paths: &Paths, layouts: &str) -> Result<Value> {
    if layouts.is_empty() {
        reset_layouts(paths)
    } else {
        set_layouts(paths, layouts)
    }
}
/// The Settings page's touchpad switch. Hyprland names the devices it switches, so one must be
/// known; niri switches the class.
pub fn switch_touchpad(paths: &Paths, enabled: bool) -> Result<Value> {
    let backend = Backend::new(paths.clone(), std::env::vars().collect())?;
    let hyprland = backend.name() == "hyprland";
    let devices = if hyprland {
        crate::software_actions::touchpads()
    } else {
        Vec::new()
    };
    if hyprland && devices.is_empty() {
        return Err("No touchpad identity is available".into());
    }
    set_touchpad(paths, enabled, devices)
}
fn apply(paths: &Paths, patch: Value) -> Result<Value> {
    let mut backend = Backend::new(paths.clone(), std::env::vars().collect())?;
    let path = paths.config.join("software-input.json");
    let _guard = files::lock(&path.with_extension("action.lock"))?;
    let target = backend.config_path("desktop")?;
    if target.is_symlink() {
        return Err("Linked compositor configuration is preserved".into());
    }
    let old_config = if target.exists() {
        Some(fs::read(&target)?)
    } else {
        None
    };
    let previous = if path.exists() {
        Some(fs::read(&path)?)
    } else {
        None
    };
    let mut input = preferences(&paths.config)?;
    let map = input.as_object_mut().ok_or("Invalid input preferences")?;
    for (key, value) in patch.as_object().unwrap() {
        if value.is_null() {
            map.remove(key);
        } else {
            map.insert(key.clone(), value.clone());
        }
    }
    state::write(&path, &input)?;
    let published = fs::read(&path)?;
    let mut generated = None;
    let result = (|| -> Result<Value> {
        if backend.name() == "hyprland" {
            let existing = old_config
                .as_ref()
                .ok_or("Managed desktop configuration is missing")?;
            let text = std::str::from_utf8(existing)?;
            const BEGIN: &str = "-- Nacre software input begin\n";
            const END: &str = "-- Nacre software input end\n";
            let mut body = text.to_string();
            if let Some(start) = body.find(BEGIN) {
                let end = body[start..]
                    .find(END)
                    .ok_or("Incomplete managed input block; original preserved")?
                    + start
                    + END.len();
                body.replace_range(start..end, "");
            }
            body.push_str(BEGIN);
            let projection = backend.render_desktop(&json!({}))?;
            let fragment = projection
                .split_once(BEGIN)
                .map(|(_, rest)| rest.split_once(END).map(|(value, _)| value).unwrap_or(rest))
                .unwrap_or("");
            body.push_str(fragment);
            body.push_str(END);
            files::atomic(&target, body.as_bytes())?;
            generated = Some(body.into_bytes());
        }
        backend.set_desktop(&json!({}), Duration::from_secs(8))?;
        Ok(json!({"message":"Input preferences saved and applied"}))
    })();
    if result.is_err() && fs::read(&path)? == published {
        match previous {
            Some(bytes) => files::atomic(&path, &bytes)?,
            None => fs::remove_file(&path)?,
        }
        if backend.name() == "hyprland"
            && generated
                .as_ref()
                .is_some_and(|expected| fs::read(&target).is_ok_and(|bytes| &bytes == expected))
        {
            if let Some(bytes) = old_config {
                files::atomic(&target, &bytes)?;
            }
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn touchpad_projection_keeps_scroll_and_layout_preferences() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.config)?;
        state::write(
            &paths.config.join("software-input.json"),
            &json!({"keyboardLayouts":"us,no","touchpadEnabled":false,"touchpadDevices":["fixture-touchpad"]}),
        )?;
        for compositor in ["niri", "hyprland"] {
            let backend = Backend::new(
                paths.clone(),
                std::collections::BTreeMap::from([("NACRE_COMPOSITOR".into(), compositor.into())]),
            )?;
            let text = backend.render_desktop(&json!({"naturalScroll":true}))?;
            assert!(text.contains("us,no"));
            if compositor == "niri" {
                assert!(text.contains("off; natural-scroll"));
            } else {
                assert!(text.contains("enabled=false"));
                assert!(text.contains("fixture-touchpad"));
            }
        }
        Ok(())
    }
    #[test]
    fn renderer_projects_only_registered_keyboard_preference() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.config)?;
        state::write(
            &paths.config.join("software-input.json"),
            &json!({"keyboardLayouts":"us,no"}),
        )?;
        for compositor in ["hyprland", "niri"] {
            let backend = Backend::new(
                paths.clone(),
                std::collections::BTreeMap::from([("NACRE_COMPOSITOR".into(), compositor.into())]),
            )?;
            let generated = backend.render_desktop(&json!({"naturalScroll":true}))?;
            assert!(generated.contains("us,no"));
            assert!(generated.contains(if compositor == "niri" {
                "natural-scroll"
            } else {
                "natural_scroll"
            }));
        }
        Ok(())
    }
    #[test]
    fn absent_input_has_no_effect_and_future_versions_are_protected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = Paths::new(dir.path());
        let base = json!({"naturalScroll":true});
        assert_eq!(overlay(&paths.config, &base)?, base);
        fs::create_dir_all(&paths.config)?;
        state::write(
            &paths.config.join("software-input.json"),
            &json!({"keyboardLayouts":"us,no"}),
        )?;
        assert_eq!(overlay(&paths.config, &base)?["keyboardLayouts"], "us,no");
        fs::write(
            paths.config.join("software-input.json"),
            b"{\"version\":99}",
        )?;
        assert!(preferences(&paths.config).is_err());
        Ok(())
    }
}
