//! Developer-only probe/service provider around native gamma policy.
use nacre_helper::{
    compositor::{
        niri_night_light::{self, Runtime},
        niri_state::Error,
    },
    json, Result,
};
use serde_json::{json as value, Value};
use std::io::{BufRead, Write};
fn callback(request: Value) -> Result<Value> {
    println!("{}", json::encode(&request)?);
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(serde_json::from_str(&line)?)
}
struct Provider;
impl Runtime for Provider {
    fn available(&mut self) -> std::result::Result<bool, Error> {
        Ok(true)
    }
    fn stop(&mut self) -> std::result::Result<(), Error> {
        callback(value!({"callback":"stop"})).map_err(|error| Error {
            kind: "RuntimeError",
            message: error.to_string(),
        })?;
        Ok(())
    }
}
fn run() -> Result<()> {
    let input: Value = serde_json::from_str(
        &std::env::args()
            .nth(1)
            .ok_or("Missing night-light request")?,
    )?;
    let result = if input["operation"] == "available" {
        Ok(value!(niri_night_light::available_with(
            input["installed"] == true,
            || Ok(input["code"] == 0
                && input["stdout"]
                    .as_str()
                    .unwrap_or("")
                    .contains("zwlr_gamma_control_manager_v1"))
        )))
    } else {
        niri_night_light::temperature(input["enabled"] == true, &mut Provider)
    };
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
