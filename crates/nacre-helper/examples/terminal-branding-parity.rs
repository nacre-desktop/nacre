//! Developer-only function boundary, never installed with the runtime.
use nacre_helper::{paths::Paths, terminal_branding as terminal, Result};
use serde_json::{json, Value};
use std::path::Path;
fn main() -> Result<()> {
    let request: Value = serde_json::from_str(&std::env::args().nth(1).ok_or("Missing request")?)?;
    let text = |name| request[name].as_str().ok_or("Missing text argument");
    let result = match text("operation")? {
        "identity" => {
            let info = terminal::identity(
                request["pid"].as_u64().ok_or("Missing PID")?.try_into()?,
                Path::new(text("proc")?),
            )?;
            json!([info.parent, info.device, info.started])
        }
        "packet" => json!(terminal::packet(
            &request["image"],
            Path::new(text("path")?),
            request["edit"].as_bool().unwrap_or(false)
        )),
        "register" => {
            let input = &request["input"];
            let meta = terminal::Terminal {
                uid: nix::unistd::getuid().as_raw()
                    + u32::from(!input["owned"].as_bool().unwrap_or(true)),
                character: input["character"].as_bool().unwrap_or(true),
                inode: input["inode"].as_u64().ok_or("Missing inode")?,
                device: input["device"].as_u64().ok_or("Missing device")?,
            };
            terminal::register_at(
                &Paths::new(text("home")?),
                Path::new(text("proc")?),
                input["tty"].as_str().ok_or("Missing TTY")?,
                &meta,
                input["parent"]
                    .as_u64()
                    .ok_or("Missing parent")?
                    .try_into()?,
                input["image"].as_u64().ok_or("Missing image")?.try_into()?,
            )?
            .unwrap_or(Value::Null)
        }
        "refresh" => {
            terminal::refresh(&Paths::new(text("home")?), Path::new(text("proc")?))?;
            Value::Null
        }
        "render" => json!(terminal::render_with(
            &Paths::new(text("home")?),
            &serde_json::from_value(request["environment"].clone())?,
            request["prepare"].as_bool().unwrap_or(false),
            || Ok(Some(request["record"].clone()))
        )?),
        _ => return Err("Unknown terminal branding operation".into()),
    };
    println!("{}", nacre_helper::json::encode(&result)?);
    Ok(())
}
