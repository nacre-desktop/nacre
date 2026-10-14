//! Developer-only validation/reload callbacks around actual Rust file publication.
use nacre_helper::{
    compositor::{
        niri_desktop::{Configuration, Runtime},
        niri_state::Error,
    },
    json,
    paths::Paths,
    Result,
};
use serde_json::{json as value, Value};
use std::{
    io::{BufRead, Write},
    path::Path,
    time::Duration,
};
fn call(request: Value) -> std::result::Result<Value, Error> {
    let exchange = || -> Result<Value> {
        println!("{}", json::encode(&request)?);
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    };
    let result = exchange().map_err(|error| Error {
        kind: "RuntimeError",
        message: error.to_string(),
    })?;
    if let Some(error) = result.get("error") {
        return Err(Error {
            kind: "RuntimeError",
            message: error.as_str().unwrap_or("Fixture error").into(),
        });
    }
    Ok(result)
}
struct Callbacks;
impl Runtime for Callbacks {
    fn monitors(&mut self, timeout: Duration) -> std::result::Result<Value, Error> {
        call(value!({"callback":"query","timeout":timeout.as_secs_f64()}))
    }
    fn validate(&mut self, path: &Path, timeout: Duration) -> std::result::Result<(), Error> {
        call(
            value!({"callback":"validate","path":path.to_string_lossy(),"timeout":timeout.as_secs_f64()}),
        )?;
        Ok(())
    }
    fn reload(&mut self, timeout: Duration) -> std::result::Result<Value, Error> {
        call(value!({"callback":"reload","timeout":timeout.as_secs_f64()}))
    }
}
fn run() -> Result<()> {
    let input: Value =
        serde_json::from_str(&std::env::args().nth(1).ok_or("Missing desktop request")?)?;
    if input["operation"] == "binding_query" || input["operation"] == "binding_capture" {
        let mut records = Vec::new();
        for path in input["paths"].as_array().ok_or("Missing binding paths")? {
            let source = std::fs::read_to_string(path.as_str().ok_or("Invalid binding path")?)?;
            let value = match nacre_helper::compositor::niri_config::bindings(
                &source,
                input["kind"].as_str().unwrap_or("bindings"),
            ) {
                Ok(value) => value,
                Err(error) => {
                    println!(
                        "{}",
                        json::encode(
                            &value!({"result":{"error":error.message,"type":error.kind}})
                        )?
                    );
                    return Ok(());
                }
            };
            records.extend(value.as_array().ok_or("Missing binding records")?.clone());
        }
        let result = if input["operation"] == "binding_query" {
            nacre_helper::compositor::niri_config::binding_query(&Value::Array(records))?
        } else {
            Value::Array(records)
        };
        println!("{}", json::encode(&value!({"result":{"value":result}}))?);
        return Ok(());
    }
    let paths = Paths::new(input["home"].as_str().ok_or("Missing home")?);
    let config = Configuration::new(
        &paths,
        input["loader"].as_str().map(Path::new),
        input["target"].as_str().map(Path::new),
    );
    let result = config.set(
        &input["preferences"],
        Duration::from_secs_f64(input["timeout"].as_f64().unwrap_or(10.0)),
        &mut Callbacks,
    );
    let result = match result {
        Ok(value) => value!({"value":value}),
        Err(error) => value!({"error":error.message,"type":error.kind}),
    };
    println!("{}", json::encode(&value!({"result":result}))?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
