//! Developer-only fixed media clock, with the actual publication/generation code.
use nacre_helper::{json, palette_cli::Cli, wallpaper_media::Media, Result};
use serde_json::{json, Value};
use std::{io::Read, path::Path};
fn main() {
    let result = (|| -> Result<Value> {
        let mut raw = String::new();
        std::io::stdin().read_to_string(&mut raw)?;
        let request: Value = serde_json::from_str(&raw)?;
        let mut cli = Cli::environment(true)?;
        let mut media = Media::new(cli.paths.clone(), cli.publisher.assets.clone())?;
        media.now_ms = request["now_ms"].as_i64();
        let mut results = Vec::new();
        for step in request["steps"].as_array().ok_or("Missing media steps")? {
            let value = match step["action"].as_str().ok_or("Missing action")? {
                "select" => media.select(
                    &mut cli,
                    Path::new(step["value"].as_str().ok_or("Missing path")?),
                )?,
                "theme" => media.theme(&mut cli, &step["value"])?,
                "preferences" => media.preference(&step["value"])?,
                "catalog" => media.catalog()?,
                "describe" => {
                    media.describe(Path::new(step["value"].as_str().ok_or("Missing path")?))?
                }
                "warm" => media.warm(&mut cli, step["include_demo"].as_bool().unwrap_or(false))?,
                "media-state" => {
                    let value =
                        media.describe(Path::new(step["value"].as_str().ok_or("Missing path")?))?;
                    let mut saved = value.clone();
                    saved["appliedAtMs"] = json!(1700000000125_i64);
                    nacre_helper::wallpaper_media::atomic(
                        &media.wallpaper_state().join("media.json"),
                        &saved,
                    )?;
                    value
                }
                "clear-branding-cache" => {
                    for path in std::fs::read_dir(media.paths.cache.join("branding"))? {
                        let path = path?.path();
                        if path.extension().is_some_and(|v| v == "png") {
                            std::fs::remove_file(path)?;
                        }
                    }
                    Value::Null
                }
                "demo-start" => media
                    .demo_start_with_token(step["token"].as_str().ok_or("Missing fixed token")?)?,
                _ => return Err("Unknown replay action".into()),
            };
            results.push(value);
        }
        Ok(json!(results))
    })();
    match result {
        Ok(value) => println!("{}", json::dumps(&value, false).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
