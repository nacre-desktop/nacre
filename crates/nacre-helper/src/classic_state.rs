//! The existing paired wallpaper/palette publication transaction.
use crate::{i18n::tr, json, paths::Paths, state, Result};
use serde_json::{json, Map, Value};
use sha2::Digest;
use std::{
    fs,
    io::Write,
    os::unix::fs::{symlink, MetadataExt},
    path::{Path, PathBuf},
};

/// Reload only terminals owned by this UID and exact HOME (never another session).
pub fn reload_terminals(home: &Path) {
    let reload = || -> Result<()> {
        // Keep nested/private publication away from another HOME's terminals.
        for entry in fs::read_dir("/proc")? {
            let entry = entry?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<i32>().ok())
                .and_then(rustix::process::Pid::from_raw)
            else {
                continue;
            };
            let root = entry.path();
            let notify = || -> Result<()> {
                if fs::metadata(&root)?.uid() != nix::unistd::getuid().as_raw()
                    || fs::read_to_string(root.join("comm"))?.trim() != "kitty"
                {
                    return Ok(());
                }
                let environment = fs::read(root.join("environ"))?;
                let expected = format!("HOME={}", home.display());
                if !environment
                    .split(|byte| *byte == 0)
                    .any(|value| value == expected.as_bytes())
                {
                    return Ok(());
                }
                let descriptor =
                    rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())?;
                rustix::process::pidfd_send_signal(&descriptor, rustix::process::Signal::USR1)?;
                Ok(())
            };
            if let Err(error) = notify() {
                if !error.downcast_ref::<std::io::Error>().is_some_and(|e| {
                    matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                    )
                }) {
                    eprintln!("Terminal palette reload: {error}");
                }
            }
        }
        Ok(())
    };
    if let Err(error) = reload() {
        eprintln!("Terminal configuration reload: {error}");
    }
}

fn encoded<T: serde::Serialize>(value: &T) -> Result<String> {
    json::dumps(value, false)
}

pub fn validated_colors(data: &Value, assets: &Path) -> Result<Map<String, Value>> {
    if !data.is_object() || !matches!(data["mode"].as_str(), Some("light" | "dark")) {
        return Err(tr(
            "helper.classic-state.invalid-palette-mode",
            "Invalid palette mode",
        )
        .into());
    }
    let reference: Value = serde_json::from_slice(&fs::read(assets.join("reference-style.json"))?)?;
    let Some(values) = data["colours"].as_object().filter(|values| {
        reference["colours"]
            .as_object()
            .is_some_and(|roles| roles.keys().all(|role| values.contains_key(role)))
    }) else {
        return Err(tr(
            "helper.classic-state.palette-is-missing-required-color-roles",
            "Palette is missing required color roles",
        )
        .into());
    };
    let role_pattern = regex::Regex::new(r"^[A-Za-z][A-Za-z0-9_]*$")?;
    let color_pattern = regex::Regex::new(r"^#?[0-9a-fA-F]{6}$")?;
    for (key, value) in values {
        if !role_pattern.is_match(key) {
            return Err(tr(
                "helper.classic-state.invalid-palette-role-name",
                "Invalid palette role name",
            )
            .into());
        }
        if !value.as_str().is_some_and(|v| color_pattern.is_match(v)) {
            return Err(tr(
                "helper.classic-state.invalid-palette-color",
                "Invalid palette color",
            )
            .into());
        }
    }
    for name in ["source", "input"] {
        if data.get(name).is_some_and(|v| !v.is_object()) {
            return Err(tr(
                "helper.classic-state.invalid-palette-metadata",
                "Invalid palette metadata",
            )
            .into());
        }
    }
    Ok(values
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                json!(value.as_str().unwrap().trim_start_matches('#')),
            )
        })
        .collect())
}
pub fn atomic_write(path: &Path, text: &str) -> Result<()> {
    if !state::write_text(path, text)? {
        let parent = path.parent().ok_or("Invalid publication path")?;
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(&format!(
                ".{}",
                path.file_name()
                    .ok_or("Missing filename")?
                    .to_string_lossy()
            ))
            .tempfile_in(parent)?;
        temporary.write_all(text.as_bytes())?;
        temporary.persist(path)?;
    }
    if path
        .file_name()
        .is_some_and(|name| name == "presentation.json")
        && std::env::var("NACRE_PRESENTATION_STREAM").as_deref() == Ok("1")
    {
        println!(
            "{}",
            encoded(&json!({"presentation":serde_json::from_str::<Value>(text)?}))?
        );
        std::io::stdout().flush()?;
    }
    Ok(())
}
fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid publication path")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(&format!(
            ".{}",
            path.file_name()
                .ok_or("Missing filename")?
                .to_string_lossy()
        ))
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.persist(path)?;
    Ok(())
}

fn create_private_dirs(path: &Path, home: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let relative = path.strip_prefix(home)?;
    let mut current = home.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => fs::set_permissions(&current, fs::Permissions::from_mode(0o700))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !current.is_dir() {
                    return Err("App theme directory is not a directory".into());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn gtk_theme_name(paths: &Paths, mode: &str) -> String {
    let dark = mode == "dark";
    let suffix = if dark { "-dark" } else { "" };
    let theme = format!("adw-gtk3{suffix}");
    let local = paths.home.join(".local/share/themes").join(&theme);
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    if local.is_dir()
        || std::env::split_paths(&data_dirs)
            .filter(|root| root.is_absolute())
            .map(|root| root.join("themes").join(&theme))
            .any(|path| path.is_dir())
    {
        format!("adw-gtk3{suffix}")
    } else {
        format!("Adwaita{suffix}")
    }
}

fn executable_on_path(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|root| root.join(name))
        .any(|path| {
            fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

fn app_target_available(paths: &Paths, id: &str) -> bool {
    match id {
        "discord" => paths.home.join(".config/Vencord").is_dir(),
        "chromium" => [
            "google-chrome-stable",
            "google-chrome",
            "brave",
            "brave-browser",
            "brave-origin",
            "chromium",
        ]
        .iter()
        .any(|name| executable_on_path(name)),
        "spotify" => executable_on_path("spicetify"),
        _ => true,
    }
}

fn hex_rgb(value: &str) -> Result<Vec<u8>> {
    let value = value.trim_start_matches('#');
    if value.len() != 6 || !value.is_ascii() {
        return Err("Invalid Chromium theme color".into());
    }
    let channels = (0..3)
        .map(|index| u8::from_str_radix(&value[index * 2..index * 2 + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(channels)
}

fn publish_extended_target(
    paths: &Paths,
    assets: &Path,
    id: &str,
    colors: &Map<String, Value>,
    data: &Value,
) -> Result<bool> {
    if !app_target_available(paths, id) {
        return Ok(false);
    }
    let root = paths.home.join(".local/share/nacre/themes/chromium");
    match id {
        "chromium" => {
            create_private_dirs(&root, &paths.home)?;
            let text = render_template(assets, "", "chromium-theme.json.in", colors)?;
            let mut manifest: Value = serde_json::from_str(&text)?;
            for value in manifest["theme"]["colors"]
                .as_object_mut()
                .ok_or("Missing Chromium theme colors")?
                .values_mut()
            {
                *value = json!(hex_rgb(
                    value.as_str().ok_or("Invalid Chromium theme color")?
                )?);
            }
            let digest = sha2::Sha256::digest(serde_json::to_vec(&(data["mode"].clone(), colors))?);
            let major = u16::from_be_bytes([digest[0], digest[1]]);
            let minor = u16::from_be_bytes([digest[2], digest[3]]);
            manifest["version"] = json!(format!("1.{major}.{minor}"));
            manifest["theme"]["properties"]["ntp_logo_alternate"] =
                json!(if data["mode"] == "dark" { 1 } else { 0 });
            atomic_write(&root.join("manifest.json"), &(encoded(&manifest)? + "\n"))?;
            match fs::remove_file(root.join("Cached Theme.pak")) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        "discord" => {
            let output = paths.home.join(".config/Vencord/themes/nacre.theme.css");
            create_private_dirs(
                output.parent().ok_or("Missing theme directory")?,
                &paths.home,
            )?;
            atomic_write(
                &output,
                &render_template(assets, "", "discord-theme.css.in", colors)?,
            )?;
        }
        "spotify" => {
            let directory = paths.home.join(".config/spicetify/Themes/Nacre");
            create_private_dirs(&directory, &paths.home)?;
            atomic_write(
                &directory.join("color.ini"),
                &render_template(assets, "", "spicetify-colors.ini.in", colors)?,
            )?;
            atomic_write(
                &directory.join("user.css"),
                &render_template(assets, "", "spicetify-user.css.in", colors)?,
            )?;
            let theme = fs::read_to_string(assets.join("spicetify-theme.js"))?;
            atomic_write(&directory.join("theme.js"), &theme)?;
        }
        _ => return Err("Unsupported application theme target".into()),
    }
    Ok(true)
}

fn spicetify_uses_nacre(paths: &Paths) -> bool {
    ["config-xpui.ini", "config.ini"].iter().any(|name| {
        fs::read_to_string(paths.home.join(".config/spicetify").join(name))
            .ok()
            .is_some_and(|text| {
                text.lines().any(|line| {
                    let Some((key, value)) = line.split_once('=') else {
                        return false;
                    };
                    key.trim() == "current_theme" && value.trim() == "Nacre"
                })
            })
    })
}

fn refresh_spicetify(paths: &Paths, live: bool) {
    if !live
        || !theme_target_enabled(paths, "spotify").unwrap_or(false)
        || !paths
            .home
            .join(".config/spicetify/Themes/Nacre/color.ini")
            .is_file()
        || !spicetify_uses_nacre(paths)
        || !executable_on_path("spicetify")
    {
        return;
    }
    let _ = crate::process::spawn_detached_quiet(
        "timeout",
        &[
            "-k".into(),
            "1s".into(),
            "60s".into(),
            "spicetify".into(),
            "refresh".into(),
        ],
    );
}
pub(crate) fn post_publish_external_targets(paths: &Paths, live: bool) {
    refresh_spicetify(paths, live);
}

enum TargetOutputState {
    Missing,
    File(Vec<u8>, u32),
    Link(PathBuf),
}

fn target_output_path(paths: &Paths, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("Invalid application theme output path".into());
    }
    Ok(paths.home.join(path))
}

fn snapshot_target_outputs(
    paths: &Paths,
    target: &Value,
) -> Result<Vec<(PathBuf, TargetOutputState)>> {
    use std::os::unix::fs::PermissionsExt;
    target["outputs"]
        .as_array()
        .ok_or("Invalid target outputs")?
        .iter()
        .map(|output| {
            let relative = output.as_str().ok_or("Invalid target output")?;
            let path = target_output_path(paths, relative)?;
            let state = if path.is_symlink() {
                TargetOutputState::Link(fs::read_link(&path)?)
            } else if path.is_file() {
                TargetOutputState::File(
                    fs::read(&path)?,
                    fs::metadata(&path)?.permissions().mode() & 0o777,
                )
            } else if path.exists() {
                return Err("Application theme output is not a file".into());
            } else {
                TargetOutputState::Missing
            };
            Ok((path, state))
        })
        .collect()
}

fn restore_target_outputs(snapshot: &[(PathBuf, TargetOutputState)]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for (path, state) in snapshot {
        match state {
            TargetOutputState::Missing => match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            },
            TargetOutputState::File(bytes, mode) => {
                atomic_write_bytes(path, bytes)?;
                fs::set_permissions(path, fs::Permissions::from_mode(*mode))?;
            }
            TargetOutputState::Link(target) => atomic_symlink(path, target)?,
        }
    }
    Ok(())
}
pub fn atomic_symlink(path: &Path, target: &Path) -> Result<()> {
    let parent = path.parent().ok_or("Invalid link path")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
    fs::remove_file(&temporary)?;
    symlink(target, &temporary)?;
    fs::rename(&temporary, path)?;
    Ok(())
}
pub fn legacy_rgb(value: &str) -> Result<[f64; 3]> {
    let text = value.trim_start_matches('#').chars().collect::<Vec<_>>();
    let mut rgb = [0.0; 3];
    for (index, channel) in rgb.iter_mut().enumerate() {
        let part = text.iter().skip(index * 2).take(2).collect::<String>();
        let number = u32::from_str_radix(&part, 16).map_err(|_| {
            format!(
                "invalid literal for int() with base 16: {}",
                json::python_repr(&json!(part))
            )
        })?;
        *channel = f64::from(number) / 255.0;
    }
    Ok(rgb)
}
pub fn luminance(value: &str) -> Result<f64> {
    let rgb = legacy_rgb(value)?;
    Ok(rgb
        .into_iter()
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(value, weight)| orient_core::colour::linear(value) * weight)
        .sum())
}
pub fn readable(value: &str, background: &str, minimum: f64) -> Result<String> {
    let value = value.trim_start_matches('#');
    if value.len() != 6 || !value.is_ascii() {
        return Err("Invalid palette color".into());
    }
    let mut rgb = [
        u8::from_str_radix(&value[0..2], 16)?,
        u8::from_str_radix(&value[2..4], 16)?,
        u8::from_str_radix(&value[4..6], 16)?,
    ];
    let bg = luminance(background)?;
    for _ in 0..100 {
        let result = format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        let fg = luminance(&result)?;
        if (bg.max(fg) + 0.05) / (bg.min(fg) + 0.05) >= minimum {
            return Ok(result);
        }
        rgb = rgb.map(|v| {
            if bg > 0.5 {
                (f64::from(v) * 0.92) as u8
            } else {
                (f64::from(v) + (f64::from(255 - v) * 0.08).max(1.0)).min(255.0) as u8
            }
        });
    }
    Ok(if bg > 0.5 { "000000" } else { "ffffff" }.into())
}
pub fn render_template(
    assets: &Path,
    backend: &str,
    name: &str,
    colors: &Map<String, Value>,
) -> Result<String> {
    let source = if name == "compositor-palette" {
        assets.join(if backend == "niri" {
            "compositor_backends/niri/palette.kdl.in"
        } else {
            "compositor_backends/hyprland/palette.lua.in"
        })
    } else {
        assets.join(name)
    };
    crate::palette_engine::render_values(&fs::read_to_string(source)?, colors)
}
/// Follow preferences are opt-out. Shared application outputs use one switch.
pub fn theme_target_key(id: &str) -> &str {
    match id {
        "dolphin" => "kde",
        "thunar" => "gtk3",
        other => other,
    }
}
pub fn theme_target_enabled(paths: &Paths, id: &str) -> Result<bool> {
    let prefs =
        crate::owned_settings::read(&paths.config.join("colors.json"), "colors", json!({}))?;
    Ok(prefs["appThemes"][theme_target_key(id)] != false)
}
pub fn publish_terminal_colors(
    paths: &Paths,
    assets: &Path,
    colors: &Map<String, Value>,
    data: &Value,
) -> Result<()> {
    let color = |role: &str| {
        colors
            .get(role)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Missing palette role: {role}"))
    };
    for (name, role) in [
        ("primary", "primary"),
        ("secondary", "secondary"),
        ("onsurface", "onSurface"),
        ("onprimary", "onPrimary"),
        ("surface", "surface"),
        ("surfacecontainer", "surfaceContainer"),
    ] {
        atomic_write(
            &paths.config.join("colors").join(name),
            &format!("#{}", color(role)?),
        )?;
    }
    let light = data["mode"] == "light";
    let bg = color(if light {
        "inverseSurface"
    } else {
        "surfaceContainerLow"
    })?;
    let fg = color(if light {
        "inverseOnSurface"
    } else {
        "onSurface"
    })?;
    let accent = readable(
        color(if light { "inversePrimary" } else { "primary" })?,
        bg,
        4.5,
    )?;
    let muted = readable(color("onSurfaceVariant")?, bg, 4.5)?;
    let mut roles: Map<String, Value> = [
        ("foreground", fg.to_owned()),
        ("background", bg.to_owned()),
        ("cursor", accent.clone()),
        ("cursor_text_color", bg.to_owned()),
        ("selection_foreground", bg.to_owned()),
        ("selection_background", accent.clone()),
        ("url_color", readable(color("tertiary")?, bg, 4.5)?),
        ("active_border_color", color("primary")?.to_owned()),
        ("inactive_border_color", color("outlineVariant")?.to_owned()),
        ("active_tab_foreground", bg.to_owned()),
        ("active_tab_background", accent.clone()),
        ("inactive_tab_foreground", muted.clone()),
        ("inactive_tab_background", bg.to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), json!(v)))
    .collect();
    for (i, role) in [
        "onSurface",
        "error",
        "green",
        "yellow",
        "blue",
        "mauve",
        "teal",
        "onSurface",
    ]
    .iter()
    .enumerate()
    {
        let value = match i {
            0 => bg.to_owned(),
            7 => fg.to_owned(),
            4 => accent.clone(),
            5 => readable(color("secondary")?, bg, 4.5)?,
            _ => readable(
                color(if colors.contains_key(*role) {
                    role
                } else {
                    "primary"
                })?,
                bg,
                4.5,
            )?,
        };
        let bright = match i {
            0 => muted.clone(),
            6 => readable(color("secondary")?, bg, 4.5)?,
            _ => value.clone(),
        };
        roles.insert(format!("color{i}"), json!(value));
        roles.insert(format!("color{}", i + 8), json!(bright));
    }
    if !theme_target_enabled(paths, "kitty")? {
        return Ok(());
    }
    atomic_write(
        &paths.config.join("kitty-colors.conf"),
        &render_template(assets, "hyprland", "kitty-colors.conf.in", &roles)?,
    )
}
#[allow(clippy::too_many_arguments)] // Explicit inputs preserve the existing publisher contract.
pub fn publish_presentation(
    paths: &Paths,
    data: &Value,
    colors: &Map<String, Value>,
    selected: &str,
    fixed: bool,
    preset: &str,
    options: &Value,
    now: i64,
) -> Result<()> {
    if selected.is_empty() {
        return Ok(());
    }
    let path = paths.state.join("presentation.json");
    let previous = state::read(&path, json!({}))?;
    let mut changed = if previous["poster"] == selected {
        previous["changedAtMs"].as_i64().unwrap_or(0)
    } else {
        0
    };
    if changed == 0 && previous["poster"] == selected {
        let media_path = paths.state.join("wallpaper/media.json");
        let media = state::read(&media_path, json!({}))?;
        if media["poster"] == selected {
            changed = media["appliedAtMs"].as_i64().filter(|v| *v != 0).unwrap_or(
                if media_path.exists() {
                    let meta = fs::metadata(&media_path)?;
                    (meta.mtime() as f64 * 1000.0 + meta.mtime_nsec() as f64 / 1_000_000.0) as i64
                } else {
                    0
                },
            );
        } else if path.exists() {
            let meta = fs::metadata(&path)?;
            changed =
                (meta.mtime() as f64 * 1000.0 + meta.mtime_nsec() as f64 / 1_000_000.0) as i64;
        }
    }
    if changed == 0 {
        changed = now;
    }
    let stored =
        crate::owned_settings::read(&paths.config.join("colors.json"), "colors", json!({}))?;
    let input = |key: &str, preference: &str, fallback: Value| {
        data["input"]
            .get(key)
            .cloned()
            .unwrap_or_else(|| options.get(preference).cloned().unwrap_or(fallback))
    };
    let value = json!({"version":1,"changedAtMs":changed,"publishedAtMs":now.max(previous["publishedAtMs"].as_i64().unwrap_or(0)+1),"mode":data["mode"],"colours":colors,"poster":selected,"paletteOptions":data["source"].get("options").cloned().unwrap_or(json!([])),"selectedAccent":if fixed {Value::Null} else {data["source"].get("selected").filter(|v|!v.is_null()&&v.as_str()!=Some("")).cloned().unwrap_or_else(||data["input"]["accent"].clone())},"palettePreset":preset,"paletteHarmony":input("harmony","paletteHarmony",json!(false)),"palettePersonality":input("personality","palettePersonality",json!("natural")),"paletteBackgroundFromWallpaper":input("background_from_wallpaper","paletteBackgroundFromWallpaper",json!(false)),"workspaceColors":stored["wallpapers"][selected].get("workspaceColors").cloned().unwrap_or(json!(false))});
    atomic_write(&path, &encoded(&value)?)
}

pub fn publish_application_colors(
    paths: &Paths,
    assets: &Path,
    system_icons: &Path,
    colors: &Map<String, Value>,
    data: &Value,
    live: bool,
    connection: Option<&zbus::blocking::Connection>,
) -> Result<String> {
    publish_application_colors_selected(
        paths,
        assets,
        system_icons,
        colors,
        data,
        live,
        connection,
        None,
    )
}
#[allow(clippy::too_many_arguments)]
fn publish_application_colors_selected(
    paths: &Paths,
    assets: &Path,
    system_icons: &Path,
    colors: &Map<String, Value>,
    data: &Value,
    live: bool,
    connection: Option<&zbus::blocking::Connection>,
    only: Option<&str>,
) -> Result<String> {
    let enabled = |id: &str| -> Result<bool> {
        Ok(only.is_none_or(|selected| selected == id) && theme_target_enabled(paths, id)?)
    };
    let root = paths.config.parent().ok_or("Missing config root")?;
    let mode = data["mode"].as_str().ok_or("Missing mode")?;
    let primary = colors["primary"].as_str().ok_or("Missing primary")?;
    let icon_theme = crate::file_icons::theme(paths, system_icons, primary, mode)?;
    let patterns: std::collections::BTreeMap<_, _> = [
        (
            "gtk-application-prefer-dark-theme",
            regex::Regex::new(r"(?m)^gtk-application-prefer-dark-theme\s*=.*$")?,
        ),
        (
            "gtk-theme-name",
            regex::Regex::new(r"(?m)^gtk-theme-name\s*=.*$")?,
        ),
        (
            "gtk-icon-theme-name",
            regex::Regex::new(r"(?m)^gtk-icon-theme-name\s*=.*$")?,
        ),
        (
            "color_scheme_path",
            regex::Regex::new(r"(?m)^color_scheme_path\s*=.*$")?,
        ),
        (
            "custom_palette",
            regex::Regex::new(r"(?m)^custom_palette\s*=.*$")?,
        ),
        ("icon_theme", regex::Regex::new(r"(?m)^icon_theme\s*=.*$")?),
    ]
    .into_iter()
    .collect();
    for version in ["3.0", "4.0"] {
        if !enabled(if version == "3.0" { "gtk3" } else { "gtk4" })? {
            continue;
        }
        let folder = root.join(format!("gtk-{version}"));
        let name = if version == "3.0" {
            "gtk3-colors.css.in"
        } else {
            "gtk4-colors.css.in"
        };
        atomic_write(
            &folder.join("gtk.css"),
            &render_template(assets, "hyprland", name, colors)?,
        )?;
        let settings = folder.join("settings.ini");
        if settings.exists() {
            let mut text = fs::read_to_string(&settings)?;
            let desired_theme = gtk_theme_name(paths, mode);
            for (key, value) in [
                (
                    "gtk-application-prefer-dark-theme",
                    if mode == "dark" { "1" } else { "0" }.to_owned(),
                ),
                ("gtk-theme-name", desired_theme),
            ] {
                text = patterns[key]
                    .replace_all(&text, |_: &regex::Captures| format!("{key}={value}"))
                    .into_owned();
            }
            if !icon_theme.is_empty() {
                text = patterns["gtk-icon-theme-name"]
                    .replace_all(&text, |_: &regex::Captures| {
                        format!("gtk-icon-theme-name={icon_theme}")
                    })
                    .into_owned();
                if !text
                    .lines()
                    .any(|line| line.starts_with("gtk-icon-theme-name="))
                {
                    text.push_str(&format!("\ngtk-icon-theme-name={icon_theme}\n"));
                }
            }
            atomic_write(&settings, &text)?;
        }
    }
    if enabled("kde")? {
        crate::kde_palette::publish(paths, &Value::Object(colors.clone()), &icon_theme)?;
    }
    if enabled("qt")? {
        let qt_roles = [
            "onSurface",
            "surfaceContainer",
            "surfaceBright",
            "surfaceContainerHigh",
            "surfaceDim",
            "outlineVariant",
            "onSurface",
            "onPrimary",
            "onSurface",
            "surface",
            "surface",
            "shadow",
            "primary",
            "onPrimary",
            "primary",
            "tertiary",
            "surfaceContainerLow",
            "onSurface",
            "surfaceContainer",
            "onSurface",
            "onSurfaceVariant",
            "primary",
        ];
        let row = qt_roles
            .into_iter()
            .map(|role| {
                colors
                    .get(role)
                    .and_then(Value::as_str)
                    .map(|value| format!("#ff{value}"))
                    .ok_or_else(|| format!("Missing palette role: {role}").into())
            })
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        let qt_path = paths.config.join("qt.conf");
        let qt = format!(
            "[ColorScheme]\n{}",
            ["active", "inactive", "disabled"]
                .iter()
                .map(|group| format!("{group}_colors={row}\n"))
                .collect::<String>()
        );
        atomic_write(&qt_path, &qt)?;
        for version in [5, 6] {
            let settings = root.join(format!("qt{version}ct/qt{version}ct.conf"));
            if settings.exists() {
                let mut text = fs::read_to_string(&settings)?;
                for (key, value) in [
                    ("color_scheme_path", qt_path.to_string_lossy().into_owned()),
                    ("custom_palette", "true".into()),
                ] {
                    text = patterns[key]
                        .replace_all(&text, |_: &regex::Captures| format!("{key}={value}"))
                        .into_owned();
                }
                if !icon_theme.is_empty() {
                    text = patterns["icon_theme"]
                        .replace_all(&text, |_: &regex::Captures| {
                            format!("icon_theme={icon_theme}")
                        })
                        .into_owned();
                }
                atomic_write(&settings, &text)?;
            }
        }
    }
    if live && enabled("kde")? {
        let connection = connection.ok_or("Missing desktop D-Bus connection")?;
        for change in [0i32, 4] {
            connection.emit_signal(
                None::<&str>,
                "/KGlobalSettings",
                "org.kde.KGlobalSettings",
                "notifyChange",
                &(change, 0i32),
            )?;
        }
    }
    for id in ["chromium", "discord", "spotify"] {
        if enabled(id)? {
            publish_extended_target(paths, assets, id, colors, data)?;
        }
    }
    Ok(icon_theme)
}

/// ca.desrt.dconf Writer.Change carries a serialized GVariant a{smv} changeset.
/// GNOME/dconf common/dconf-changeset.c and service/ca.desrt.dconf.xml define this wire contract.
#[allow(deprecated)] // zvariant 5 retains GVariant support; the workspace is pinned below v6.
pub fn dconf_blob(
    values: &std::collections::BTreeMap<String, zvariant::OwnedValue>,
) -> Result<Vec<u8>> {
    let changes: std::collections::BTreeMap<_, _> = values
        .iter()
        .map(|(key, value)| (key, Some(value)))
        .collect();
    Ok(zvariant::to_bytes(
        zvariant::serialized::Context::new_gvariant(zvariant::LE, 0),
        &changes,
    )?
    .bytes()
    .to_vec())
}
pub fn publish_dconf(
    paths: &Paths,
    connection: &zbus::blocking::Connection,
    mode: &str,
    icon_theme: &str,
) -> Result<()> {
    use zvariant::{OwnedValue, Value as Variant};
    let mut values = std::collections::BTreeMap::new();
    for (key, value) in [
        (
            "cursor-theme",
            std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "breeze_cursors".into()),
        ),
        ("color-scheme", format!("prefer-{mode}")),
        ("gtk-theme", gtk_theme_name(paths, mode)),
    ] {
        values.insert(
            format!("/org/gnome/desktop/interface/{key}"),
            OwnedValue::try_from(Variant::from(value))?,
        );
    }
    let size = std::env::var("XCURSOR_SIZE")
        .unwrap_or_else(|_| "24".into())
        .parse::<i32>()?;
    values.insert(
        "/org/gnome/desktop/interface/cursor-size".into(),
        OwnedValue::from(size),
    );
    if !icon_theme.is_empty() {
        values.insert(
            "/org/gnome/desktop/interface/icon-theme".into(),
            OwnedValue::try_from(Variant::from(icon_theme))?,
        );
    }
    let proxy = zbus::blocking::Proxy::new(
        connection,
        "ca.desrt.dconf",
        "/ca/desrt/dconf/Writer/user",
        "ca.desrt.dconf.Writer",
    )?;
    let _: String = proxy.call("Change", &(dconf_blob(&values)?,))?;
    Ok(())
}

static PUBLICATION_GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub struct PublicationGuard {
    _gate: std::sync::MutexGuard<'static, ()>,
    path: std::path::PathBuf,
    _file: Option<nix::fcntl::Flock<fs::File>>,
}
#[allow(deprecated)] // Safe RawFd API is needed to borrow, never close/unlock, inherited fd9.
pub fn publication_lock(paths: &Paths) -> Result<PublicationGuard> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = crate::paths::weak_canonical(&paths.state.join("palette-commit.lock"))?;
    fs::create_dir_all(path.parent().ok_or("Missing publication lock directory")?)?;
    let gate = PUBLICATION_GATE
        .lock()
        .map_err(|_| "Palette publication lock poisoned")?;
    if std::env::var("NACRE_PALETTE_LOCKED").as_deref() == Ok("1") {
        if let (Ok(inherited), Ok(expected)) =
            (fs::metadata("/proc/self/fd/9"), fs::metadata(&path))
        {
            if (inherited.dev(), inherited.ino()) == (expected.dev(), expected.ino()) {
                nix::fcntl::flock(9, nix::fcntl::FlockArg::LockExclusive)?;
                return Ok(PublicationGuard {
                    _gate: gate,
                    _file: None,
                    path,
                });
            }
        }
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o666)
        .open(&path)?;
    let file = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive)
        .map_err(|(_, error)| error)?;
    Ok(PublicationGuard {
        _gate: gate,
        _file: Some(file),
        path,
    })
}
pub struct Publisher {
    pub paths: Paths,
    pub assets: std::path::PathBuf,
    pub backend: crate::compositor::backend::Backend,
    pub system_icons: std::path::PathBuf,
    pub monitors: Option<Vec<Value>>,
    pub now_ms: Option<i64>,
}
impl Publisher {
    pub fn environment() -> Result<Self> {
        let paths = Paths::environment()?;
        let assets = paths.assets();
        let backend =
            crate::compositor::backend::Backend::new(paths.clone(), std::env::vars().collect())?;
        Ok(Self {
            paths,
            assets,
            backend,
            system_icons: "/usr/share/icons".into(),
            monitors: None,
            now_ms: None,
        })
    }
    /// Settings changes only the selected publisher group, never compositor or wallpaper state.
    pub fn set_theme_target(&mut self, id: &str, enabled: bool) -> Result<()> {
        let manifest: Value =
            serde_json::from_slice(&fs::read(self.assets.join("theme-targets.json"))?)?;
        let target = manifest["targets"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"] == id))
            .ok_or("Unknown application theme target")?;
        let _guard = publication_lock(&self.paths)?;
        let snapshot = if enabled && matches!(id, "chromium" | "discord" | "spotify") {
            Some(snapshot_target_outputs(&self.paths, target)?)
        } else {
            None
        };
        let key = theme_target_key(id);
        let path = self.paths.config.join("colors.json");
        let schema = state::schema(&path).ok_or("Missing colors store schema")?;
        let mut document = crate::owned_settings::Document::open(&path, "colors", json!({}))?;
        let previous_document = document.data.clone();
        let previous = previous_document["appThemes"][key].clone();
        if document.is_hosted() {
            let mut prefs = previous_document.clone();
            if prefs["appThemes"].is_null() {
                prefs["appThemes"] = json!({});
            }
            prefs["appThemes"]
                .as_object_mut()
                .ok_or("Invalid application theme preferences")?
                .insert(key.into(), json!(enabled));
            document.save(&prefs)?;
        } else {
            state::update_as(&path, schema, |mut prefs| {
                if prefs["appThemes"].is_null() {
                    prefs["appThemes"] = json!({});
                }
                if !prefs["appThemes"].is_object() {
                    return Err("Invalid application theme preferences".into());
                }
                prefs["appThemes"][key] = json!(enabled);
                Ok(prefs)
            })?;
        }
        let result = (|| -> Result<()> {
            if enabled {
                let data = state::read(&self.paths.state.join("scheme.json"), Value::Null)?;
                let colors = validated_colors(&data, &self.assets)?;
                match key {
                    "gtk3" | "gtk4" | "qt" | "kde" => {
                        publish_application_colors_selected(
                            &self.paths,
                            &self.assets,
                            &self.system_icons,
                            &colors,
                            &data,
                            false,
                            None,
                            Some(key),
                        )?;
                    }
                    "kitty" => {
                        publish_terminal_colors(&self.paths, &self.assets, &colors, &data)?;
                        reload_terminals(&self.paths.home);
                    }
                    "rofi" => atomic_write(
                        &self.paths.config.join("rofi.rasi"),
                        &render_template(
                            &self.assets,
                            self.backend.name(),
                            "rofi-colors.rasi.in",
                            &colors,
                        )?,
                    )?,
                    "lock" | "login" => {
                        let selected = self.selected(None)?;
                        self.publish_lock_login(&colors, &selected)?;
                    }
                    "chromium" | "discord" | "spotify" => {
                        publish_extended_target(&self.paths, &self.assets, id, &colors, &data)?;
                    }
                    _ => return Err("Unsupported application theme target".into()),
                }
            }
            self.publish_target_manifest()
        })();
        if result.is_err() {
            if let Some(snapshot) = &snapshot {
                restore_target_outputs(snapshot)?;
            }
            if document.is_hosted() {
                document.save(&previous_document)?;
            } else {
                state::update_as(&path, schema, |mut prefs| {
                    if previous.is_null() {
                        prefs["appThemes"]
                            .as_object_mut()
                            .ok_or("Invalid theme preferences")?
                            .remove(key);
                    } else {
                        prefs["appThemes"][key] = previous;
                    }
                    Ok(prefs)
                })?;
            }
        }
        if result.is_ok() && id == "spotify" {
            refresh_spicetify(&self.paths, true);
        }
        result
    }
    pub fn apply_palette(&mut self, wallpaper: Option<&Path>, live: bool) -> Result<()> {
        let _guard = publication_lock(&self.paths)?;
        self.apply_locked(wallpaper, live, true)
    }
    fn selected(&self, wallpaper: Option<&Path>) -> Result<String> {
        if let Some(wallpaper) = wallpaper.filter(|path| !path.as_os_str().is_empty()) {
            return Ok(
                crate::paths::weak_canonical(&crate::paths::expand(wallpaper)?)?
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        match fs::read_to_string(self.paths.state.join("wallpaper/last.txt")) {
            Ok(value) => Ok(value.trim().into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error.into()),
        }
    }
    pub fn publish_compositor_palette(&self, colors: &Map<String, Value>) -> Result<String> {
        let preferences = state::read(&self.paths.config.join("desktop.json"), json!({}))?;
        publish_compositor_palette_with(&self.assets, &self.backend, colors, &preferences)
    }
    fn apply_locked(
        &mut self,
        wallpaper: Option<&Path>,
        live: bool,
        refresh_before_reload: bool,
    ) -> Result<()> {
        let paths = &self.paths;
        let mut data = state::read(&paths.state.join("scheme.json"), Value::Null)?;
        let options = crate::owned_settings::read(
            &paths.config.join("wallpaper-picker.json"),
            "wallpaper-picker",
            json!({}),
        )?;
        let preset = options
            .get("palettePreset")
            .and_then(Value::as_str)
            .unwrap_or("wallpaper");
        let fixed = preset != "wallpaper";
        if fixed {
            let presets: Value =
                serde_json::from_slice(&fs::read(self.assets.join("palette-presets.json"))?)?;
            let mut presets = presets.as_array().ok_or("Invalid palette presets")?.clone();
            presets.extend(
                crate::theme_bundles::catalog(&self.assets)?
                    .as_array()
                    .ok_or("Invalid bundles")?
                    .clone(),
            );
            let stored = crate::owned_settings::read(
                &paths.config.join("colors.json"),
                "colors",
                json!({}),
            )?;
            if let Some(favorites) = stored["favorites"].as_array() {
                for favorite in favorites {
                    let mut favorite = favorite.clone();
                    favorite["id"] = json!(format!(
                        "favorite:{}",
                        favorite["id"].as_str().ok_or("Invalid favorite identity")?
                    ));
                    presets.push(favorite);
                }
            }
            let mode = options
                .get("paletteMode")
                .and_then(Value::as_str)
                .unwrap_or("dark");
            let chosen = presets.iter().find(|value| value["id"] == preset);
            if chosen.is_none() || !["dark", "light"].contains(&mode) {
                return Err(tr(
                    "helper.classic-state.invalid-fixed-palette-preference",
                    "Invalid fixed palette preference",
                )
                .into());
            }
            data["mode"] = json!(mode);
            data["colours"] = chosen.unwrap()["modes"][mode].clone();
        }
        let colors = validated_colors(&data, &self.assets)?;
        if fixed {
            atomic_write(&paths.state.join("scheme.json"), &encoded(&data)?)?;
        }
        let selected = self.selected(wallpaper)?;
        let now = self
            .now_ms
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        publish_presentation(
            paths, &data, &colors, &selected, fixed, preset, &options, now,
        )?;
        crate::branding::publish(&colors, paths, &self.assets)?;
        atomic_write(
            &paths.state.join("scheme/current-mode.txt"),
            data["mode"].as_str().ok_or("Missing palette mode")?,
        )?;
        atomic_write(
            &paths.state.join("scheme/current.txt"),
            &(colors
                .iter()
                .map(|(key, value)| format!("{key} {}", value.as_str().unwrap()))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"),
        )?;
        let config = self.publish_compositor_palette(&colors)?;
        if theme_target_enabled(paths, "rofi")? {
            atomic_write(
                &paths.config.join("rofi.rasi"),
                &render_template(
                    &self.assets,
                    self.backend.name(),
                    "rofi-colors.rasi.in",
                    &colors,
                )?,
            )?;
        }
        let connection = if live {
            match zbus::blocking::Connection::session() {
                Ok(connection) => Some(connection),
                Err(error) => {
                    eprintln!("Desktop palette D-Bus connection: {error}");
                    None
                }
            }
        } else {
            None
        };
        let icon_theme = publish_application_colors(
            paths,
            &self.assets,
            &self.system_icons,
            &colors,
            &data,
            live && connection.is_some(),
            connection.as_ref(),
        )?;
        publish_terminal_colors(paths, &self.assets, &colors, &data)?;
        self.publish_lock_login(&colors, &selected)?;
        self.publish_target_manifest()?;
        if let Some(wallpaper) = wallpaper {
            atomic_write(
                &self.paths.state.join("wallpaper/last.txt"),
                &crate::paths::weak_canonical(&crate::paths::expand(wallpaper)?)?.to_string_lossy(),
            )?;
        }
        if refresh_before_reload {
            refresh_spicetify(&self.paths, live);
        }
        if live {
            self.reload_desktop_consumers(&data, &icon_theme, &config, connection.as_ref())?;
        }
        Ok(())
    }
    pub fn publish_lock_login(
        &mut self,
        colors: &Map<String, Value>,
        selected: &str,
    ) -> Result<()> {
        if theme_target_enabled(&self.paths, "lock")? {
            let preferences = match state::read(&self.paths.config.join("desktop.json"), json!({}))
            {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("Lock palette preferences: {error}");
                    json!({})
                }
            };
            let monitors = if let Some(monitors) = &self.monitors {
                monitors.clone()
            } else {
                crate::lock_config::monitor_layout(&self.paths, &mut self.backend)?
            };
            let config = crate::lock_config::prepare_config(
                &self.paths,
                &self.assets,
                colors,
                Path::new(selected),
                &preferences,
                &self.paths.tools().join("nacre-helper"),
                &monitors,
            )?;
            let config_root = self
                .paths
                .config
                .parent()
                .ok_or("Invalid lock config root")?;
            atomic_write(&config_root.join("hypr/hyprlock.conf"), &config)?;
        }
        if !selected.is_empty() && theme_target_enabled(&self.paths, "login")? {
            let policy = crate::login_appearance::Policy::environment()?;
            if let Err(error) = crate::login_appearance::publish(
                &self.paths,
                &Value::Object(colors.clone()),
                Path::new(selected),
                Some(&self.paths.state.join("shell/login-preview")),
                None,
                &policy,
            ) {
                eprintln!("Login appearance publication failed: {error}");
            }
        }
        Ok(())
    }
    pub fn publish_target_manifest(&self) -> Result<()> {
        let manifest: Value =
            serde_json::from_slice(&fs::read(self.assets.join("theme-targets.json"))?)?;
        let mut targets = Vec::new();
        for target in manifest["targets"]
            .as_array()
            .ok_or("Invalid theme targets")?
        {
            let id = target["id"].as_str().ok_or("Missing target id")?;
            let available = if matches!(id, "chromium" | "discord" | "spotify") {
                app_target_available(&self.paths, id)
            } else {
                let tool = &target["tool"];
                if tool.is_null() || tool.as_str() == Some("") {
                    true
                } else {
                    let tools = tool
                        .as_array()
                        .map(|rows| rows.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                        .unwrap_or_else(|| tool.as_str().into_iter().collect());
                    tools.into_iter().any(executable_on_path)
                }
            };
            let published = target["outputs"]
                .as_array()
                .ok_or("Invalid target outputs")?
                .iter()
                .filter_map(Value::as_str)
                .filter(|path| !path.ends_with("/Cached Theme.pak"))
                .any(|path| self.paths.home.join(path).is_file());
            let mut target = target.clone();
            target["status"] = json!(if !available {
                "tool-missing"
            } else if published {
                "current"
            } else {
                "not-published"
            });
            target["available"] = json!(available);
            if !theme_target_enabled(&self.paths, id)? {
                target["status"] = json!("paused");
            }
            targets.push(target);
        }
        atomic_write(
            &self.paths.state.join("theme-targets.json"),
            &(encoded(&json!({"version":1,"targets":targets}))? + "\n"),
        )
    }
    pub fn reload_desktop_consumers(
        &mut self,
        data: &Value,
        icon_theme: &str,
        config: &str,
        connection: Option<&zbus::blocking::Connection>,
    ) -> Result<()> {
        reload_terminals(&self.paths.home);
        if let Some(connection) = connection.filter(|_| {
            ["gtk3", "gtk4", "kde"]
                .iter()
                .any(|id| theme_target_enabled(&self.paths, id).unwrap_or(false))
        }) {
            if let Err(error) = publish_dconf(
                &self.paths,
                connection,
                data["mode"].as_str().ok_or("Missing mode")?,
                icon_theme,
            ) {
                eprintln!("Desktop palette dconf publication: {error}");
            }
        }
        if self.backend.name() == "hyprland" {
            match self.backend.module_call(
                "apply_config",
                &[json!(config)],
                &json!({"check":false,"timeout":null}),
            ) {
                Ok(reply) => {
                    if reply
                        .get("returncode")
                        .or_else(|| reply.get("code"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        != 0
                    {
                        eprintln!(
                            "Palette saved; live window-border update failed: {}",
                            reply["stderr"].as_str().unwrap_or("")
                        );
                    }
                }
                Err(error) => eprintln!("Palette saved; live window-border update failed: {error}"),
            }
        } else {
            self.backend.module_call("reload", &[], &json!({}))?;
        }
        Ok(())
    }
}

pub const DEMO_PREFERENCES: [&str; 5] = [
    "palettePreset",
    "paletteMode",
    "paletteHarmony",
    "palettePersonality",
    "paletteBackgroundFromWallpaper",
];
impl Publisher {
    fn prepared_inputs(
        &self,
        wallpaper: &Path,
        data: &Value,
        thumbnail: &Path,
    ) -> Result<Option<(std::path::PathBuf, std::path::PathBuf)>> {
        let image = crate::paths::weak_canonical(wallpaper)?;
        let thumbnail = crate::paths::weak_canonical(thumbnail)?;
        if !image.is_file()
            || !thumbnail.is_file()
            || !thumbnail.starts_with(crate::paths::weak_canonical(
                &self.paths.cache.join("wallpapers"),
            )?)
        {
            return Ok(None);
        }
        if validated_colors(data, &self.assets).is_err() || data["name"] != "dynamic" {
            return Ok(None);
        }
        let token = regex::Regex::new(r"^[a-zA-Z0-9_-]{1,40}$")?;
        if ["flavour", "variant"]
            .iter()
            .any(|key| !data[*key].as_str().is_some_and(|s| token.is_match(s)))
        {
            return Ok(None);
        }
        match state::read(&self.paths.config.join("cli.json"), json!({})) {
            Ok(config) if !crate::json::truthy(&config["wallpaper"]["postHook"]) => (),
            _ => return Ok(None),
        }
        Ok(Some((image, thumbnail)))
    }
    pub fn commit_prepared(
        &mut self,
        wallpaper: &Path,
        data: &Value,
        thumbnail: &Path,
        live: bool,
        allow_mode_change: bool,
    ) -> Result<bool> {
        let Some((image, thumbnail)) = self.prepared_inputs(wallpaper, data, thumbnail)? else {
            return Ok(false);
        };
        let guard = publication_lock(&self.paths)?;
        self.commit_checked(
            image,
            data,
            thumbnail,
            live,
            allow_mode_change,
            true,
            &guard,
        )
    }
    pub(crate) fn commit_prepared_held(
        &mut self,
        wallpaper: &Path,
        data: &Value,
        thumbnail: &Path,
        live: bool,
        allow_mode_change: bool,
        guard: &PublicationGuard,
    ) -> Result<bool> {
        let Some((image, thumbnail)) = self.prepared_inputs(wallpaper, data, thumbnail)? else {
            return Ok(false);
        };
        self.commit_checked(
            image,
            data,
            thumbnail,
            live,
            allow_mode_change,
            false,
            guard,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn commit_checked(
        &mut self,
        image: std::path::PathBuf,
        data: &Value,
        thumbnail: std::path::PathBuf,
        live: bool,
        allow_mode_change: bool,
        refresh_before_reload: bool,
        guard: &PublicationGuard,
    ) -> Result<bool> {
        if guard.path
            != crate::paths::weak_canonical(&self.paths.state.join("palette-commit.lock"))?
        {
            return Err("Palette lock belongs to a different home".into());
        }
        let current = match state::read(&self.paths.state.join("scheme.json"), Value::Null) {
            Ok(Value::Null) if allow_mode_change => {
                json!({"name":"dynamic","mode":data["mode"],"flavour":data["flavour"],"variant":data["variant"]})
            }
            Ok(value) => value,
            Err(_) => return Ok(false),
        };
        if !current.is_object()
            || (!allow_mode_change
                && (current["name"] != "dynamic"
                    || current["flavour"] != data["flavour"]
                    || current["mode"] != data["mode"]
                    || current
                        .get("variant")
                        .cloned()
                        .unwrap_or(json!("tonalspot"))
                        != data.get("variant").cloned().unwrap_or(json!("tonalspot"))))
        {
            return Ok(false);
        }
        atomic_write(&self.paths.state.join("scheme.json"), &encoded(data)?)?;
        atomic_write(
            &self.paths.state.join("wallpaper/path.txt"),
            &image.to_string_lossy(),
        )?;
        atomic_symlink(&self.paths.state.join("wallpaper/current"), &image)?;
        atomic_symlink(
            &self.paths.state.join("wallpaper/thumbnail.jpg"),
            &thumbnail,
        )?;
        self.apply_locked(Some(&image), live, refresh_before_reload)?;
        Ok(true)
    }
    pub fn restore_demo(&mut self, snapshot: &Value, live: bool) -> Result<Value> {
        self.restore_demo_with_publish(snapshot, live, |publisher, poster, live| {
            publisher.apply_locked(poster, live, false)
        })
    }
    /// Explicit publisher boundary preserves the historical transaction's fault tests.
    pub fn restore_demo_with_publish<F>(
        &mut self,
        snapshot: &Value,
        live: bool,
        mut publish: F,
    ) -> Result<Value>
    where
        F: FnMut(&mut Self, Option<&Path>, bool) -> Result<()>,
    {
        let data = &snapshot["scheme"];
        validated_colors(data, &self.assets)?;
        let poster = Path::new(snapshot["poster"].as_str().ok_or("Invalid demo poster")?);
        let media = &snapshot["media"];
        if !poster.is_absolute() || !poster.is_file() {
            return Err(tr("helper.classic-state.the-starting-wallpaper-is-unavailable-restore-its-file-and-try-ag", "The starting wallpaper is unavailable. Restore its file and try again.").into());
        }
        if media["poster"].as_str() != Some(poster.to_str().ok_or("Invalid poster path")?)
            || !media["path"]
                .as_str()
                .is_some_and(|path| Path::new(path).is_file())
        {
            return Err(tr("helper.classic-state.the-starting-wallpaper-source-is-unavailable-restore-its-file-and", "The starting wallpaper source is unavailable. Restore its file and try again.").into());
        }
        let preferences = snapshot["preferences"]
            .as_object()
            .ok_or("Invalid demo preferences")?;
        if preferences
            .keys()
            .any(|key| !DEMO_PREFERENCES.contains(&key.as_str()))
        {
            return Err(tr(
                "helper.classic-state.invalid-demo-preference-snapshot",
                "Invalid demo preference snapshot",
            )
            .into());
        }
        let _guard = publication_lock(&self.paths)?;
        let pref_path = self.paths.config.join("wallpaper-picker.json");
        let mut preference_document =
            crate::owned_settings::Document::open(&pref_path, "wallpaper-picker", json!({}))?;
        let original_preferences = preference_document.data.clone();
        let state = &self.paths.state;
        let files = [
            pref_path.clone(),
            state.join("scheme.json"),
            state.join("wallpaper/media.json"),
            state.join("wallpaper/last.txt"),
            state.join("wallpaper/path.txt"),
        ];
        enum SavedLink {
            Link(std::path::PathBuf),
            File(Vec<u8>),
            Absent,
        }
        let mut links = Vec::new();
        for path in [
            state.join("wallpaper/current"),
            state.join("wallpaper/thumbnail.jpg"),
        ] {
            let saved = if path.is_symlink() {
                SavedLink::Link(fs::read_link(&path)?)
            } else if path.is_file() {
                SavedLink::File(fs::read(&path)?)
            } else {
                SavedLink::Absent
            };
            links.push((path, saved));
        }
        let previous = files
            .iter()
            .map(|path| {
                if path.exists() {
                    fs::read_to_string(path).map(Some).map_err(Into::into)
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let mut options: Value = if preference_document.is_hosted() {
            original_preferences.clone()
        } else {
            serde_json::from_str(previous[0].as_deref().unwrap_or("{}"))?
        };
        let object = options
            .as_object_mut()
            .ok_or("Invalid picker preferences")?;
        for key in DEMO_PREFERENCES {
            object.shift_remove(key);
        }
        for (key, value) in preferences {
            object.insert(key.clone(), value.clone());
        }
        let outcome = (|| -> Result<()> {
            if !preference_document.save(&options)? {
                atomic_write(&pref_path, &encoded(&options)?)?;
            }
            atomic_write(&self.paths.state.join("scheme.json"), &encoded(data)?)?;
            atomic_write(
                &self.paths.state.join("wallpaper/media.json"),
                &encoded(media)?,
            )?;
            atomic_write(
                &self.paths.state.join("wallpaper/path.txt"),
                &poster.to_string_lossy(),
            )?;
            atomic_symlink(&self.paths.state.join("wallpaper/current"), poster)?;
            if let Some(thumbnail) = media["thumbnail"]
                .as_str()
                .map(Path::new)
                .filter(|path| path.is_file())
            {
                atomic_symlink(&self.paths.state.join("wallpaper/thumbnail.jpg"), thumbnail)?;
            }
            publish(self, Some(poster), live)
        })();
        if let Err(error) = outcome {
            if preference_document.is_hosted() {
                preference_document.save(&original_preferences)?;
            }
            for (path, text) in files.iter().zip(&previous) {
                if path == &pref_path && preference_document.is_hosted() {
                    continue;
                }
                if let Some(text) = text {
                    atomic_write(path, text)?;
                } else {
                    match fs::remove_file(path) {
                        Ok(()) => (),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            for (path, link) in links {
                match link {
                    SavedLink::Link(target) => atomic_symlink(&path, &target)?,
                    SavedLink::File(raw) => crate::files::atomic(&path, &raw)?,
                    SavedLink::Absent => match fs::remove_file(path) {
                        Ok(()) => (),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                        Err(error) => return Err(error.into()),
                    },
                }
            }
            let old_poster = previous[3]
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(Path::new);
            if let Some(path) = old_poster.filter(|path| path.is_file()) {
                atomic_symlink(&self.paths.state.join("wallpaper/current"), path)?;
            }
            if previous[1].is_some() {
                publish(self, old_poster, live)?;
            }
            return Err(error);
        }
        refresh_spicetify(&self.paths, live);
        Ok(media.clone())
    }
}

impl Publisher {
    pub(crate) fn apply_palette_held(
        &mut self,
        wallpaper: Option<&Path>,
        live: bool,
        guard: &PublicationGuard,
    ) -> Result<()> {
        if guard.path
            != crate::paths::weak_canonical(&self.paths.state.join("palette-commit.lock"))?
        {
            return Err("Palette lock belongs to a different home".into());
        }
        self.apply_locked(wallpaper, live, false)
    }
}

pub(crate) fn publish_compositor_palette_with(
    assets: &Path,
    backend: &crate::compositor::backend::Backend,
    colors: &Map<String, Value>,
    preferences: &Value,
) -> Result<String> {
    let mut values = colors.clone();
    if preferences["interfaceGradients"] == false {
        values.insert("secondary".into(), values["primary"].clone());
    }
    let text = render_template(assets, backend.name(), "compositor-palette", &values)?;
    atomic_write(&backend.config_path("palette")?, &text)?;
    Ok(text)
}
