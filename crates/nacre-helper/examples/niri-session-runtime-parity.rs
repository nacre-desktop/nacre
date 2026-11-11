//! Developer-only runtime adapter; all session decisions stay Rust.
use nacre_helper::{json, niri_session_runtime as runtime, paths::Paths, Result};
use serde_json::{json as value, Value};
use std::path::Path;
fn execute(request: &Value) -> Result<Value> {
    let operation = request["operation"].as_str().ok_or("Missing operation")?;
    let paths = Paths::new(request["home"].as_str().unwrap_or("/fixture"));
    let result = match operation {
        "host_preferences" => value!(runtime::host_preferences(&request["desktop"])?),
        "palette_include" => value!(runtime::palette_include(
            &paths,
            Path::new(request["config"].as_str().ok_or("Missing config")?)
        )),
        "command" => {
            let (argv, environment) = runtime::command(
                request["action"].as_str().ok_or("Missing action")?,
                &serde_json::from_value(request["environment"].clone())?,
                &paths,
            )?;
            value!([argv, environment])
        }
        _ => return Err("Unknown runtime parity operation".into()),
    };
    Ok(result)
}
fn main() -> Result<()> {
    let request: Value =
        serde_json::from_str(&std::env::args().nth(1).ok_or("Missing runtime request")?)?;
    let result = match execute(&request) {
        Ok(result) => value!({"result":result}),
        Err(error) => value!({"error":error.to_string()}),
    };
    println!("{}", json::encode(&result)?);
    Ok(())
}
