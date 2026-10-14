//! Developer-only typed call boundary for existing wrapper mock assertions.
use nacre_helper::{compositor_compat, json, Result};
use serde_json::{json as value, Value};
fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match compositor_compat::call(&args) {
        Ok(call) => value!({"call":call}),
        Err(error) => value!({"error":error.to_string()}),
    };
    println!("{}", json::encode::<Value>(&result)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
