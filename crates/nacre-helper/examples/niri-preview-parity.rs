//! Developer-only argument mapping; no preview behavior or expected outputs here.
use nacre_helper::{json, niri_preview as preview, paths::Paths, Result};
use serde_json::{json as value, Value};
use std::path::Path;
fn main() -> Result<()> {
    let request: Value = serde_json::from_str(&std::env::args().nth(1).ok_or("Missing request")?)?;
    let path = |key: &str| -> Result<&Path> {
        Ok(Path::new(request[key].as_str().ok_or("Missing path")?))
    };
    let result: Result<Value> = (|| {
        Ok(match request["operation"].as_str() {
            Some("allowlists") => {
                value!({"config":preview::SAFE_CONFIG,"state":preview::SAFE_STATE})
            }
            Some("command") => {
                let (args, environment) = preview::command(
                    path("destination")?,
                    &Paths::new(path("home")?),
                    &serde_json::from_value(request["environment"].clone())?,
                    path("runtime")?,
                    request["quickshell"].as_str().map(Path::new),
                )?;
                value!([args, environment])
            }
            Some("private_copy") => {
                preview::private_copy(path("source")?, path("target")?)?;
                Value::Null
            }
            Some("copy_cached_presentation") => {
                preview::copy_cached_presentation(
                    &Paths::new(path("home")?),
                    path("state")?,
                    path("cache")?,
                )?;
                Value::Null
            }
            Some("preview_bindings") => value!(preview::preview_bindings(
                &Paths::new(path("home")?),
                &serde_json::from_value(request["names"].clone())?
            )?),
            _ => return Err("Unknown preview operation".into()),
        })
    })();
    let response = match result {
        Ok(result) => value!({"result":result}),
        Err(error) => value!({"error":error.to_string()}),
    };
    println!("{}", json::encode(&response)?);
    Ok(())
}
