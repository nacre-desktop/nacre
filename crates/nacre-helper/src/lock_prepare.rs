//! Prepare the existing private lock artifacts outside the lock startup path.
use crate::{json, lock_dashboard, lock_info::Info, Result};
use serde_json::{json, Value};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::Path};

pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Missing lock preparation directory")?;
    if !parent.exists() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o600))?;
    temporary.persist(path)?;
    Ok(())
}

pub fn prepare(
    data: &Value,
    info: &mut Info,
    assets: &Path,
    context: &mut lock_dashboard::Context,
) -> Result<()> {
    let mut data = data
        .as_object()
        .ok_or("Lock preparation requires an object")?
        .clone();
    let ready = info.paths.cache.join("lock-ready");
    if !crate::classic_state::theme_target_enabled(&info.paths, "lock")? {
        let retained = match fs::read_to_string(ready.join("snapshot.json")) {
            Ok(raw) => match serde_json::from_str::<Value>(&raw) {
                Ok(previous) => previous
                    .get("colors")
                    .filter(|value| json::truthy(value))
                    .cloned(),
                Err(error) => {
                    eprintln!("Retained lock palette: {error}");
                    None
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                eprintln!("Retained lock palette: {error}");
                None
            }
        };
        let colors = match retained {
            Some(colors) => colors,
            None => {
                serde_json::from_slice::<Value>(&fs::read(assets.join("reference-style.json"))?)?
                    ["colours"]
                    .clone()
            }
        };
        data.insert("colors".into(), colors);
    }
    let settings = data.get("preferences").cloned().unwrap_or(json!({}));
    data.insert(
        "batteryLabel".into(),
        json!(info.label("status", &Value::Object(data.clone()), &settings)?),
    );
    let data = Value::Object(data);
    for kind in ["weather", "media", "play-icon", "status"] {
        atomic(
            &ready.join(format!("{kind}.txt")),
            info.label(kind, &data, &settings)?.as_bytes(),
        )?;
    }
    for private in [false, true] {
        let mut preferences = settings
            .as_object()
            .ok_or("Invalid lock preferences")?
            .clone();
        preferences.insert("lockNotificationContents".into(), json!(private));
        let name = if private {
            "notifications-private"
        } else {
            "notifications-safe"
        };
        atomic(
            &ready.join(format!("{name}.txt")),
            info.label("notifications", &data, &Value::Object(preferences))?
                .as_bytes(),
        )?;
    }
    let fallback = info.paths.share.join("branding/nacre-lock.png");
    let pointer = ready.join("art-path.txt");
    if !pointer.exists() {
        atomic(&pointer, fallback.to_string_lossy().as_bytes())?;
    }
    let artwork = if settings.get("lockMedia").is_none_or(json::truthy) {
        info.artwork(&data)?
    } else {
        fallback
    };
    if artwork.is_file() {
        atomic(&ready.join("initial-art.png"), &fs::read(&artwork)?)?;
    }
    atomic(&pointer, artwork.to_string_lossy().as_bytes())?;
    atomic(
        &ready.join("snapshot.json"),
        json::dumps(&data, false)?.as_bytes(),
    )?;
    let colors = if json::truthy(&data["colors"]) {
        data["colors"].clone()
    } else {
        match fs::read_to_string(info.paths.state.join("scheme.json"))
            .map_err(Into::into)
            .and_then(|s| -> Result<Value> {
                Ok(serde_json::from_str::<Value>(&s)?
                    .get("colours")
                    .ok_or("Missing cached lock colors")?
                    .clone())
            }) {
            Ok(colors) => colors,
            Err(error) => {
                if !error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    eprintln!("Lock preparation palette fallback: {error}");
                }
                serde_json::from_slice::<Value>(&fs::read(assets.join("reference-style.json"))?)?
                    ["colours"]
                    .clone()
            }
        }
    };
    let wallpaper = if json::truthy(&data["wallpaper"]) {
        data["wallpaper"]
            .as_str()
            .ok_or("Invalid lock wallpaper")?
            .to_owned()
    } else {
        match fs::read_to_string(info.paths.state.join("wallpaper/last.txt")) {
            Ok(value) => value.trim().to_owned(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                eprintln!("Lock preparation wallpaper: {error}");
                String::new()
            }
        }
    };
    let outputs = match fs::read_to_string(info.paths.cache.join("lock-outputs.json")) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(outputs) => outputs,
            Err(error) => {
                eprintln!("Lock preparation output cache: {error}");
                json!([])
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!([]),
        Err(error) => {
            eprintln!("Lock preparation output cache: {error}");
            json!([])
        }
    };
    lock_dashboard::publish_with_context(
        &data,
        colors.as_object().ok_or("Invalid lock colors")?,
        Path::new(&wallpaper),
        Some(&artwork),
        &info.paths,
        outputs.as_array().ok_or("Invalid lock outputs")?,
        context,
    )?;
    let mut cached = Vec::new();
    for entry in fs::read_dir(info.cache.parent().ok_or("Missing lock cache parent")?)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with("lock-art-"))
            && path.extension().is_some_and(|s| s == "png")
        {
            cached.push((entry.metadata()?.modified()?, path));
        }
    }
    cached.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, path) in cached.into_iter().skip(12) {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn run(_args: &[String]) -> Result<i32> {
    use std::io::BufRead;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let data = json::parse_legacy_value(if line.is_empty() { "{}" } else { &line })?;
    let paths = crate::paths::Paths::environment()?;
    let assets = paths.assets();
    prepare(
        &data,
        &mut Info::new(paths),
        &assets,
        &mut lock_dashboard::Context::default(),
    )?;
    Ok(0)
}
