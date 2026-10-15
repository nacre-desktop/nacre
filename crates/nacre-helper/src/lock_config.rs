//! Existing Hyprlock geometry and commands; authentication stays in Hyprlock.
use crate::{i18n::tr, json, paths::Paths, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
pub fn panel_key(monitor: &Value) -> Result<String> {
    let mut values = Map::new();
    for key in ["height", "name", "transform", "width"] {
        values.insert(key.into(), monitor[key].clone());
    }
    Ok(format!("{:x}", Sha256::digest(json::encode(&values)?.as_bytes()))[..12].to_owned())
}
pub fn panel_scale(monitor: &Value) -> f64 {
    let mut width = monitor["width"].as_f64().unwrap_or(1920.0);
    let mut height = monitor["height"].as_f64().unwrap_or(1080.0);
    if monitor["transform"].as_i64().unwrap_or(0) % 2 != 0 {
        std::mem::swap(&mut width, &mut height);
    }
    (width / 2048.0).min(height / 1152.0)
}
fn py_text(value: &Value) -> String {
    match value {
        Value::String(v) => v.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        _ => value.to_string(),
    }
}
pub fn block(widget: &str, monitor: &str, options: &[(&str, Value)]) -> String {
    tr(
        "helper.lock-config.value0-monitor-value1-value2",
        "{value0} {\n    monitor = {value1}\n{value2}}\n",
    )
    .replace("{value0}", widget)
    .replace("{value1}", monitor)
    .replace(
        "{value2}",
        &options
            .iter()
            .map(|(key, value)| format!("    {key} = {}\n", py_text(value)))
            .collect::<String>(),
    )
}
fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&c))
    {
        text.into()
    } else {
        format!("'{}'", text.replace('\'', "'\"'\"'"))
    }
}
#[allow(clippy::too_many_arguments)] // Original render arguments plus explicit asset/HOME boundaries.
pub fn render(
    assets: &Path,
    paths: &Paths,
    colors: &Map<String, Value>,
    wallpaper: &str,
    preferences: &Value,
    helper: &Path,
    monitors: &[Value],
    ready: Option<&Path>,
    artwork: Option<&Path>,
) -> Result<String> {
    if wallpaper.contains(['\n', '\r']) {
        return Err(tr(
            "helper.lock-config.unsupported-wallpaper-path",
            "Unsupported wallpaper path",
        )
        .into());
    }
    let ready = ready
        .map(Path::to_owned)
        .unwrap_or_else(|| paths.cache.join("lock-ready"));
    let mut result =
        fs::read_to_string(assets.join("hyprlock.conf.in"))?.replace("{{wallpaper}}", wallpaper);
    let colors: Map<_, _> = colors
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                json!(value
                    .as_str()
                    .ok_or("Invalid lock color")?
                    .trim_start_matches('#')),
            ))
        })
        .collect::<Result<_>>()?;
    for (key, value) in &colors {
        result = result.replace(
            &format!("{{{{{key}}}}}"),
            value.as_str().ok_or("Invalid lock color")?,
        );
    }
    let fallback = vec![json!({"name":"","width":1920,"height":1080})];
    for monitor in if monitors.is_empty() {
        &fallback
    } else {
        monitors
    } {
        let name = monitor["name"].as_str().unwrap_or("");
        let scale = panel_scale(monitor);
        let key = panel_key(monitor)?;
        let pointer = ready.join(format!("dashboard-{key}.txt"));
        let initial = ready.join(format!("dashboard-{key}-initial.png"));
        let path = if initial.is_file() {
            initial
        } else {
            artwork
                .map(Path::to_owned)
                .unwrap_or_else(|| paths.share.join("branding/nacre-lock.png"))
        };
        let round = |v: f64| v.round_ties_even() as i64;
        let position = |x: f64, y: f64| {
            format!(
                "{}, {}",
                round((x - 720.0) * scale),
                round((405.0 - y) * scale)
            )
        };
        let rgba = |role: &str, alpha: &str| -> Result<String> {
            Ok(tr(
                "helper.lock-config.rgba-value0-value1",
                "rgba({value0}{value1})",
            )
            .replace(
                "{value0}",
                colors[role].as_str().ok_or("Missing lock color")?,
            )
            .replace("{value1}", alpha))
        };
        let label = |text: &str, x: f64, y: f64, size: f64, role: &str| -> Result<String> {
            Ok(block(
                "label",
                name,
                &[
                    ("text", json!(text)),
                    ("position", json!(position(x, y))),
                    ("halign", json!("center")),
                    ("valign", json!("center")),
                    ("color", json!(rgba(role, "ff")?)),
                    ("font_size", json!(round(size * scale * 0.75))),
                    ("font_family", json!("IBM Plex Sans")),
                    ("zindex", json!(2)),
                ],
            ))
        };
        result.push_str(&block(
            "image",
            name,
            &[
                ("path", json!(path)),
                ("size", json!(round(810.0 * scale))),
                ("rounding", json!(0)),
                ("border_size", json!(0)),
                ("position", json!("0, 0")),
                ("halign", json!("center")),
                ("valign", json!("center")),
                ("reload_time", json!(2)),
                (
                    "reload_cmd",
                    json!(format!(
                        "/usr/bin/cat {}",
                        shell_quote(&pointer.to_string_lossy())
                    )),
                ),
                ("zindex", json!(0)),
            ],
        ));
        result.push_str(&label(
            "cmd[update:1000] date '+%I'",
            671.0,
            120.0,
            104.0,
            "primary",
        )?);
        result.push_str(&label(
            "cmd[update:1000] date '+%M'",
            766.0,
            105.0,
            54.0,
            "primary",
        )?);
        result.push_str(&label(
            "cmd[update:1000] date '+%p'",
            766.0,
            151.0,
            27.0,
            "onSurface",
        )?);
        result.push_str(&label(
            "cmd[update:60000] date '+%A · %d %b' | tr '[:lower:]' '[:upper:]'",
            720.0,
            193.0,
            20.0,
            "onSurface",
        )?);
        result.push_str(&block(
            "input-field",
            name,
            &[
                (
                    "size",
                    json!(format!("{}, {}", round(260.0 * scale), round(46.0 * scale))),
                ),
                ("position", json!(position(720.0, 625.0))),
                ("halign", json!("center")),
                ("valign", json!("center")),
                ("inner_color", json!(rgba("surfaceContainer", "b0")?)),
                ("outer_color", json!(rgba("primary", "50")?)),
                ("font_color", json!(rgba("onSurface", "ff")?)),
                ("font_family", json!("IBM Plex Sans")),
                ("outline_thickness", json!(1)),
                ("rounding", json!(round(23.0 * scale))),
                ("dots_size", json!(0.2)),
                ("dots_spacing", json!(0.3)),
                ("fade_on_empty", json!("false")),
                ("placeholder_text", json!("Enter your password  →")),
                ("fail_text", json!("$FAIL")),
                ("check_color", json!(rgba("primary", "ff")?)),
                ("fail_color", json!(rgba("error", "ff")?)),
                ("zindex", json!(3)),
            ],
        ));
        for (x, icon, onclick) in [
            (672.0, "bedtime", Some("systemctl suspend")),
            (768.0, "lock", None),
        ] {
            let mut options = vec![
                ("text", json!(icon)),
                ("position", json!(position(x, 690.0))),
                ("halign", json!("center")),
                ("valign", json!("center")),
                ("color", json!(rgba("primary", "ff")?)),
                ("font_family", json!("Material Symbols Rounded")),
                ("font_size", json!(round(24.0 * scale * 0.75))),
            ];
            if let Some(command) = onclick {
                options.push(("onclick", json!(command)));
            }
            options.push(("zindex", json!(3)));
            result.push_str(&block("label", name, &options));
        }
        if preferences["lockMedia"].as_bool().unwrap_or(true) {
            for (x, icon, action) in [
                (165.0, "skip_previous".into(), "previous"),
                (
                    238.0,
                    format!(
                        "cmd[update:2000] /usr/bin/cat {}",
                        shell_quote(&ready.join("play-icon.txt").to_string_lossy())
                    ),
                    "toggle",
                ),
                (311.0, "skip_next".into(), "next"),
            ] {
                let command = format!(
                    "python3 {} {action}",
                    shell_quote(&helper.to_string_lossy())
                );
                result.push_str(&block(
                    "shape",
                    name,
                    &[
                        (
                            "size",
                            json!(format!(
                                "{}, {}",
                                round(if action == "toggle" { 72.0 } else { 38.0 } * scale),
                                round(42.0 * scale)
                            )),
                        ),
                        ("position", json!(position(x, 687.0))),
                        ("halign", json!("center")),
                        ("valign", json!("center")),
                        ("rounding", json!(round(21.0 * scale))),
                        ("color", json!("rgba(00000000)")),
                        ("onclick", json!(command)),
                        ("zindex", json!(2)),
                    ],
                ));
                result.push_str(&block(
                    "label",
                    name,
                    &[
                        ("text", json!(icon)),
                        ("position", json!(position(x, 687.0))),
                        ("halign", json!("center")),
                        ("valign", json!("center")),
                        (
                            "color",
                            json!(rgba(
                                if action == "toggle" {
                                    "onPrimary"
                                } else {
                                    "onSurface"
                                },
                                "ff"
                            )?),
                        ),
                        (
                            "font_size",
                            json!(round(
                                if action == "toggle" { 28.0 } else { 22.0 } * scale * 0.75
                            )),
                        ),
                        ("font_family", json!("Material Symbols Rounded")),
                        ("onclick", json!(command)),
                        ("zindex", json!(3)),
                    ],
                ));
            }
        }
    }
    Ok(result)
}

pub fn monitor_layout(
    paths: &Paths,
    backend: &mut crate::compositor::backend::Backend,
) -> Result<Vec<Value>> {
    let cache = paths.cache.join("lock-outputs.json");
    match backend.query("monitors", std::time::Duration::from_secs(2), None) {
        Ok(value) if value.as_array().is_some_and(|items| !items.is_empty()) => {
            let parent = cache.parent().ok_or("Missing lock output directory")?;
            fs::create_dir_all(parent)?;
            crate::files::atomic(&cache, json::encode(&value)?.as_bytes())?;
            Ok(value.as_array().unwrap().clone())
        }
        result => {
            match result {
                Err(error) => eprintln!("Lock output discovery: {error}"),
                Ok(_) => eprintln!(
                    "{}",
                    tr(
                        "helper.lock-config.no-current-lock-outputs",
                        "No current lock outputs"
                    )
                ),
            }
            match fs::read(&cache) {
                Ok(bytes) => Ok(serde_json::from_slice::<Value>(&bytes)?
                    .as_array()
                    .ok_or("Invalid lock output cache")?
                    .clone()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(error) => Err(error.into()),
            }
        }
    }
}
pub fn prepare_config(
    paths: &Paths,
    assets: &Path,
    colors: &Map<String, Value>,
    wallpaper: &Path,
    preferences: &Value,
    helper: &Path,
    monitors: &[Value],
) -> Result<String> {
    let ready = paths.cache.join("lock-ready");
    let initial = ready.join("initial-art.png");
    let artwork = if initial.is_file() {
        initial
    } else {
        paths.share.join("branding/nacre-lock.png")
    };
    let read = |path: &Path| -> Result<Value> {
        match fs::read(path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(value) => Ok(value),
                Err(error) => {
                    eprintln!("Lock preparation metadata: {error}");
                    Ok(json!({}))
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
            Err(error) => Err(error.into()),
        }
    };
    let mut data = read(&ready.join("snapshot.json"))?;
    data["preferences"] = preferences.clone();
    let prior = read(&ready.join("dashboard.json"))?;
    let fallback = vec![json!({"name":"","width":1920,"height":1080})];
    let missing = (if monitors.is_empty() {
        &fallback
    } else {
        monitors
    })
    .iter()
    .map(|monitor| {
        panel_key(monitor).map(|key| !ready.join(format!("dashboard-{key}-initial.png")).is_file())
    })
    .collect::<Result<Vec<_>>>()?
    .into_iter()
    .any(|missing| missing);
    let changed = [
        ("lockNotificationContents", false),
        ("lockMedia", true),
        ("lockWeather", true),
        ("lockNotifications", true),
    ]
    .into_iter()
    .any(|(key, default)| {
        prior["preferences"]
            .get(key)
            .cloned()
            .unwrap_or(json!(default))
            != preferences.get(key).cloned().unwrap_or(json!(default))
    });
    if missing || changed {
        crate::lock_dashboard::publish(&data, colors, wallpaper, Some(&artwork), paths, monitors)?;
    }
    render(
        assets,
        paths,
        colors,
        &wallpaper.to_string_lossy(),
        preferences,
        helper,
        monitors,
        Some(&ready),
        Some(&artwork),
    )
}
