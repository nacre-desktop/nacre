//! Developer-only interactive provider mapping; all runtime decisions stay Rust.
use nacre_helper::{
    json, niri_hover_install, niri_session_runtime as runtime, paths::Paths, Result,
};
use serde_json::{json as value, Value};
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};
fn callback(request: Value) -> Result<Value> {
    println!("{}", json::encode(&value!({"callback":request}))?);
    std::io::stdout().flush()?;
    let mut reply = String::new();
    std::io::stdin().lock().read_line(&mut reply)?;
    let result: Value = serde_json::from_str(&reply)?;
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        return Err(error.to_owned().into());
    }
    Ok(result["value"].clone())
}
struct Versions;
impl runtime::Versions for Versions {
    fn version(&mut self, binary: &Path) -> Result<String> {
        callback(value!({"operation":"version","binary":binary}))?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "Invalid version provider".into())
    }
}
fn path_callback(request: Value, error: &mut Option<String>) -> Option<PathBuf> {
    match callback(request) {
        Ok(Value::Null) => None,
        Ok(Value::String(path)) => Some(path.into()),
        Ok(_) => {
            *error = Some("Invalid path provider".into());
            None
        }
        Err(failure) => {
            *error = Some(failure.to_string());
            None
        }
    }
}
fn execute(request: &Value) -> Result<Value> {
    let operation = request["operation"].as_str().ok_or("Missing operation")?;
    let paths = Paths::new(request["home"].as_str().unwrap_or("/fixture"));
    let mut error = None;
    let result = match operation {
        "host_preferences" => value!(runtime::host_preferences(&request["desktop"])?),
        "palette_include" => value!(runtime::palette_include(
            &paths,
            Path::new(request["config"].as_str().ok_or("Missing config")?)
        )),
        "verified_hover_binary" => value!(runtime::verified_hover_binary(
            Path::new(request["root"].as_str().ok_or("Missing root")?),
            request["owner"].as_u64().ok_or("Missing owner")? as u32,
            request["packaged"] == true,
            &mut Versions
        )?),
        "overview_hover_binary" => value!(runtime::select_companion(
            &paths,
            |root, owner, packaged| path_callback(
                value!({"operation":"verify","root":root,"owner":owner,"packaged":packaged}),
                &mut error
            )
        )),
        "command" => {
            let (argv, environment) = runtime::command(
                request["action"].as_str().ok_or("Missing action")?,
                &serde_json::from_value(request["environment"].clone())?,
                &paths,
                |paths| {
                    path_callback(
                        value!({"operation":"companion","home":paths.home}),
                        &mut error,
                    )
                },
            )?;
            value!([argv, environment])
        }
        "ensure_overview_hover" => {
            niri_hover_install::ensure_with(
                || {
                    path_callback(
                        value!({"operation":"companion","home":paths.home}),
                        &mut error,
                    )
                },
                || {
                    callback(value!({"operation":"build","backend":request["backend"]}))?;
                    Ok(())
                },
            )?;
            Value::Null
        }
        _ => return Err("Unknown runtime parity operation".into()),
    };
    if let Some(error) = error {
        return Err(error.into());
    }
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
