//! Developer-only clock/system/font inputs for retained lock contracts.
use nacre_helper::{json, lock_dashboard, lock_info::Info, lock_prepare, paths::Paths, Result};
use serde_json::{json, Value};
use std::{io::Read, path::Path};
fn main() {
    let result = (|| -> Result<Value> {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        let request: Value = serde_json::from_str(&input)?;
        let paths = Paths::new(request["home"].as_str().ok_or("Missing fixture home")?);
        let assets = Paths::environment()?.assets();
        let mut info = Info::new(paths);
        info.now = request["now"].as_f64();
        if let Some(path) = request["cache"].as_str() {
            info.cache = path.into();
        }
        if let Some(path) = request["sys"].as_str() {
            info.sys = path.into();
        }
        if let Some(path) = request["font"].as_str() {
            info.font_path = Some(path.into());
        }
        match request["operation"]
            .as_str()
            .ok_or("Missing lock operation")?
        {
            "prepare" => {
                let mut context = lock_dashboard::Context::default();
                if let Some(system) = request.get("system_info") {
                    context.system = Some(system.clone());
                }
                if let Some(now) = request["native_clock"].as_str() {
                    context.now = chrono::DateTime::parse_from_rfc3339(now)?;
                }
                lock_prepare::prepare(&request["data"], &mut info, &assets, &mut context)?;
                if request["observe_prepared"].as_bool() == Some(true) {
                    Ok(serde_json::from_slice(&std::fs::read(
                        info.paths.cache.join("lock-ready/snapshot.json"),
                    )?)?)
                } else {
                    Ok(Value::Null)
                }
            }
            "snapshot" => {
                if let Some(value) = request["ipc_output"].as_str() {
                    info.snapshot_with(|| {
                        if let Some(log) = request["ipc_log"].as_str() {
                            std::fs::write(log, b"lockWidgets state\n")?;
                        }
                        Ok(value.into())
                    })
                } else {
                    info.snapshot()
                }
            }
            "plain" => Ok(json!(info.plain(
                &request["value"],
                request["width"].as_f64().unwrap_or(288.0),
                request["lines"].as_u64().unwrap_or(2) as usize,
                request["font_size"].as_u64().unwrap_or(14) as u32
            )?)),
            "artwork" => Ok(json!(info.artwork(&request["data"])?)),
            "label" => Ok(json!(info.label(
                request["kind"].as_str().ok_or("Missing label kind")?,
                &request["data"],
                &request["settings"]
            )?)),
            "atomic" => {
                lock_prepare::atomic(
                    Path::new(request["path"].as_str().ok_or("Missing atomic path")?),
                    request["text"]
                        .as_str()
                        .ok_or("Missing atomic text")?
                        .as_bytes(),
                )?;
                Ok(Value::Null)
            }
            _ => Err("Unknown lock parity operation".into()),
        }
    })();
    match result {
        Ok(value) => println!("{}", json::dumps(&json!({"result":value}), false).unwrap()),
        Err(error) => {
            println!(
                "{}",
                json::dumps(&json!({"error":error.to_string()}), false).unwrap()
            );
            std::process::exit(1)
        }
    }
}
