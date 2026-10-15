//! Developer-only globals/clock mapping for the retained historical assertions.
use nacre_helper::{json, lock_dashboard, paths::Paths, Result};
use serde_json::{json, Value};
use std::{io::Read, path::Path};
fn main() {
    let result = (|| -> Result<Value> {
        let mut raw = String::new();
        std::io::stdin().read_to_string(&mut raw)?;
        let request: Value = serde_json::from_str(&raw)?;
        let op = request["operation"].as_str().ok_or("Missing operation")?;
        let home = request["home"]
            .as_str()
            .map(Paths::new)
            .unwrap_or(Paths::environment()?);
        let mut context = lock_dashboard::Context::default();
        if let Some(info) = request.get("system_info") {
            context.system = Some(info.clone());
        }
        if let Some(time) = request["now"].as_str() {
            context.now = chrono::DateTime::parse_from_rfc3339(time)?;
        }
        if op == "media" {
            let mut media =
                nacre_helper::wallpaper_media::Media::new(home, Paths::environment()?.assets())?;
            media.library = request["library"].as_str().ok_or("Missing library")?.into();
            media.cache_path = request["cache"].as_str().ok_or("Missing cache")?.into();
            media.state_path = request["state"].as_str().ok_or("Missing state")?.into();
            media.prefs_path = request["prefs"].as_str().ok_or("Missing prefs")?.into();
            media.now_ms = request["now_ms"].as_i64();
            let args = request["args"].as_array().ok_or("Missing media args")?;
            let path = |index: usize| -> Result<&Path> {
                Ok(Path::new(args[index].as_str().ok_or("Missing media path")?))
            };
            return match request["method"].as_str().ok_or("Missing media method")? {
                "settings" => media.settings(),
                "media_state" => media.media_state(),
                "palette_presets" => media.palette_presets(),
                "preference" => media.preference(&args[0]),
                "previews" => media.previews(path(0)?),
                "describe" => media.describe(path(0)?),
                "catalog" => media.catalog(),
                "palette_marker" => Ok(json!(media.palette_marker(
                    path(0)?,
                    args[1].as_str().ok_or("Missing flavour")?,
                    args[2].as_bool(),
                    args[3].as_str().map(Path::new),
                    args[4].as_str(),
                    args[5].as_str()
                )?)),
                _ => Err("Unsupported media fixture method".into()),
            };
        }
        match op {
            "lock_render" => {
                use base64::Engine;
                let image = lock_dashboard::render(
                    &request["data"],
                    request["colors"].as_object().ok_or("Missing lock colors")?,
                    Path::new(request["wallpaper"].as_str().unwrap_or("")),
                    request["artwork"].as_str().map(Path::new),
                    &home,
                    request.get("monitor").filter(|v| !v.is_null()),
                    &mut context,
                )?;
                Ok(json!(base64::engine::general_purpose::STANDARD
                    .encode(orient_decoder::encode_rgba_png(&image, 2)?)))
            }
            "lock_publish" => lock_dashboard::publish_with_context(
                &request["data"],
                request["colors"].as_object().ok_or("Missing lock colors")?,
                Path::new(request["wallpaper"].as_str().unwrap_or("")),
                request["artwork"].as_str().map(Path::new),
                &home,
                request["monitors"].as_array().ok_or("Missing monitors")?,
                &mut context,
            ),
            "suitable_images" => {
                let dimensions = request["dimensions"]
                    .as_array()
                    .map(|values| (values[0].as_f64().unwrap(), values[1].as_f64().unwrap()));
                Ok(json!(nacre_helper::palette_cli::suitable_images(
                    Path::new(request["directory"].as_str().ok_or("Missing directory")?),
                    request["threshold"].as_f64().unwrap_or(0.5),
                    request["no_filter"].as_bool().unwrap_or(false),
                    dimensions
                )?))
            }
            "file_icons_theme" => Ok(json!(nacre_helper::file_icons::theme(
                &home,
                Path::new(
                    request["system_icons"]
                        .as_str()
                        .ok_or("Missing system icons")?
                ),
                request["primary"].as_str().ok_or("Missing primary")?,
                request["mode"].as_str().ok_or("Missing mode")?,
            )?)),
            "system_info" => lock_dashboard::system_info(&home.home),
            "notification_icon" => Ok(json!(lock_dashboard::notification_icon(
                request["name"].as_str().unwrap_or(""),
                request["app"].as_str().unwrap_or(""),
                &home
            )?)),
            "layout" => Ok(json!(lock_dashboard::layout(&request["monitor"]))),
            _ => Err("Unsupported legacy image operation".into()),
        }
    })();
    let (code, value) = match result {
        Ok(value) => (0, json!({"result":value})),
        Err(error) => (1, json!({"error":error.to_string(),"type":"ValueError"})),
    };
    println!("{}", json::dumps(&value, false).unwrap());
    std::process::exit(code);
}
