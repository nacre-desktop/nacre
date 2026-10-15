//! Developer-only fixed-clock/private-consumer replay; no runtime testing overrides.
use nacre_helper::{colors::Colors, image_text::FontBook, json, Result};
use serde_json::{json, Value};
fn main() {
    let result = (|| -> Result<Value> {
        let request: Value =
            serde_json::from_str(&std::env::args().nth(1).ok_or("Missing replay request")?)?;
        let mut app = Colors::environment()?;
        let clock = &request["clock"];
        app.now_ms = clock["now_ms"].as_i64();
        app.hour = clock["hour"].as_u64().map(|v| v as u32);
        app.export_date = clock["export_date"].as_str().map(str::to_owned);
        app.media.now_ms = app.now_ms;
        app.media.hour = app.hour;
        app.publisher.now_ms = app.now_ms;
        app.publisher.monitors = Some(
            request["monitors"]
                .as_array()
                .ok_or("Missing replay monitors")?
                .clone(),
        );
        app.publisher.system_icons = app.paths.home.join("empty-system-icons");
        app.fonts = FontBook::with_overrides(
            [(
                "DejaVu Sans".into(),
                std::path::PathBuf::from(request["card_font"].as_str().ok_or("Missing card font")?),
            )]
            .into_iter()
            .collect(),
        );
        // The original multi-HOME oracle retained its first SDK worker HOME.
        // Reproduce that external cache context only in this developer replay.
        if let Some(cache) = request["engine_cache"].as_str() {
            std::env::set_var("XDG_CACHE_HOME", cache);
        }
        let mut results: Vec<Value> = Vec::new();
        for step in request["steps"].as_array().ok_or("Missing replay steps")? {
            let identity = || -> Result<Value> {
                let index = step["id_from"]
                    .as_u64()
                    .ok_or("Missing prior preview index")? as usize;
                Ok(results.get(index).ok_or("Missing prior preview")?["id"].clone())
            };
            let value = match step["action"].as_str().ok_or("Missing replay action")? {
                "preview" => app.stage(&step["request"])?,
                "favorite" => app.favorite(&identity()?, step.get("name").unwrap_or(&json!("")))?,
                "apply" => app.apply(&identity()?, false)?,
                "catalog" => app.catalog()?,
                "delete" => app.delete(&step["id"])?,
                "export" => app.export(&identity()?)?,
                "card" => app.card(&identity()?)?,
                _ => return Err("Unsupported replay action".into()),
            };
            results.push(value);
        }
        Ok(json!(results))
    })();
    match result {
        Ok(value) => println!("{}", json::dumps(&value, false).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
