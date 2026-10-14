//! Developer-only native function boundary; executes compiler code, never references.
use nacre_helper::{compositor_config, json, paths::Paths, Result};
use serde_json::{json as value, Value};

fn run() -> Result<i32> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 3 {
        return Err("Expected module, operation and JSON arguments".into());
    }
    let request: Value = serde_json::from_str(&arguments[2])?;
    let result = compositor_config::execute(
        &arguments[0],
        &arguments[1],
        &request,
        &Paths::environment()?.assets(),
    );
    let (result, code) = match result {
        Ok(result) => (result, 0),
        Err(error) => (value!({"error":error.message,"type":error.kind}), 1),
    };
    println!("{}", json::encode(&result)?);
    Ok(code)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
