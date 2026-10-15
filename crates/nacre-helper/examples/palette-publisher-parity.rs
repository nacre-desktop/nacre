//! Developer-only fixed-clock/external-input boundary for immutable publication cases.
use nacre_helper::{classic_state::Publisher, compositor::backend::Backend, paths::Paths, Result};
use serde_json::Value;
use std::path::Path;
fn main() {
    let result = (|| -> Result<Option<Value>> {
        let request: Value =
            serde_json::from_str(&std::env::args().nth(1).ok_or("Missing request")?)?;
        let text = |name| request[name].as_str().ok_or("Missing text argument");
        let paths = Paths::new(text("home")?);
        let backend = Backend::new(
            paths.clone(),
            [
                ("NACRE_COMPOSITOR".into(), text("backend")?.into()),
                (
                    "NIRI_SOCKET".into(),
                    format!("{}/absent-niri", paths.home.display()),
                ),
                (
                    "XDG_RUNTIME_DIR".into(),
                    format!("{}/runtime", paths.home.display()),
                ),
                (
                    "HYPRLAND_INSTANCE_SIGNATURE".into(),
                    "absent-fixture".into(),
                ),
            ]
            .into_iter()
            .collect(),
        )?;
        let mut publisher = Publisher {
            paths,
            assets: text("assets")?.into(),
            backend,
            system_icons: text("system_icons")?.into(),
            monitors: Some(
                request["monitors"]
                    .as_array()
                    .ok_or("Missing monitors")?
                    .clone(),
            ),
            now_ms: request["now_ms"].as_i64(),
        };
        match request["operation"].as_str() {
            Some("apply_palette") => {
                publisher.apply_palette(request["wallpaper"].as_str().map(Path::new), false)?;
                Ok(Some(Value::Null))
            }
            Some("commit_prepared") => Ok(Some(serde_json::json!(publisher.commit_prepared(
                Path::new(text("wallpaper")?),
                &request["data"],
                Path::new(text("thumbnail")?),
                false,
                request["allow_mode_change"].as_bool().unwrap_or(false)
            )?))),
            Some("restore_demo_mock") => {
                let mut first = true;
                Ok(Some(publisher.restore_demo_with_publish(
                    &request["snapshot"],
                    false,
                    |_, _, _| {
                        let error = request["publication_error"].as_str().filter(|_| first);
                        first = false;
                        error.map_or(Ok(()), |message| Err(message.into()))
                    },
                )?))
            }
            Some("restore_demo") => Ok(Some(publisher.restore_demo(&request["snapshot"], false)?)),
            Some(_) => Err("Unsupported transaction operation".into()),
            None => {
                publisher.apply_palette(request["wallpaper"].as_str().map(Path::new), false)?;
                Ok(None)
            }
        }
    })();
    match result {
        Ok(Some(value)) => println!(
            "{}",
            nacre_helper::json::encode(&serde_json::json!({"result":value,"error":null})).unwrap()
        ),
        Ok(None) => (),
        Err(error) => {
            println!("{}",nacre_helper::json::encode(&serde_json::json!({"result":null,"error":{"type":"ValueError","message":error.to_string()}})).unwrap());
            std::process::exit(1);
        }
    }
}
