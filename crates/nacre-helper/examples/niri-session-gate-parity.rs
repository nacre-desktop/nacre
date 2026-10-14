//! Developer-only pure function boundary retaining the original gate assertions.
use nacre_helper::{json, niri_session_gate, paths::Paths, Result};
use serde_json::{json as value, Value};
fn main() -> Result<()> {
    let request: Value =
        serde_json::from_str(&std::env::args().nth(1).ok_or("Missing gate request")?)?;
    let environment = serde_json::from_value(request["environment"].clone())?;
    let result = if request["operation"] == "compositor" {
        value!(niri_session_gate::compositor(&environment))
    } else {
        value!(niri_session_gate::allowed(
            request["action"].as_str().ok_or("Missing gate action")?,
            &environment,
            &Paths::new(request["home"].as_str().ok_or("Missing home")?)
        ))
    };
    println!("{}", json::encode(&result)?);
    Ok(())
}
