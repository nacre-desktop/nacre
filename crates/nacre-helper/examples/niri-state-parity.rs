//! Developer-only reducer boundary; calls native code and never loads references.
use nacre_helper::{compositor::niri_state::State, json, Result};
use serde_json::{json as value, Value};

fn run() -> Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 1 {
        return Err("Expected JSON state operation".into());
    }
    let request: Value = serde_json::from_str(&arguments[0])?;
    let mut state: State = serde_json::from_value(request["state"].clone())?;
    let result = match request["operation"].as_str() {
        Some("apply") => state.apply(&request["event"]),
        Some("snapshot") => state.snapshot(),
        _ => return Err("Unknown state operation".into()),
    };
    let result = match result {
        Ok(result) => value!({"value":result}),
        Err(error) => value!({"error":error.message,"type":error.kind}),
    };
    println!(
        "{}",
        json::encode(&value!({"state":state,"result":result}))?
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
