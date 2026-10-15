//! Developer-only provider for retained Python module assertions.
use nacre_helper::{session_watch, Result};
use serde_json::{json, Value};
use std::{
    io::{self, Read},
    path::Path,
};
fn run() -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let input: Value = serde_json::from_str(&input)?;
    let record = session_watch::record(
        input["event"].as_str().unwrap_or("manual"),
        &chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.6f+00:00")
            .to_string(),
        input["desktop"].as_str().unwrap_or(""),
        input["wallet"].as_str().unwrap_or(""),
        &json!([]),
        &json!([]),
        &json!(""),
    )?;
    session_watch::publish(
        Path::new(input["state"].as_str().ok_or("Missing state directory")?),
        &record,
    )?;
    println!("{}", serde_json::to_string(&record)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
