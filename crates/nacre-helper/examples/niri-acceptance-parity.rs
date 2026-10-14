//! Developer-only argument mapping for the original acceptance assertions.
use nacre_helper::{json, niri_acceptance_monitor as monitor, Result};
use serde_json::{json as value, Value};
use std::path::Path;
fn main() -> Result<()> {
    let request: Value = serde_json::from_str(&std::env::args().nth(1).ok_or("Missing request")?)?;
    let result = match request["operation"].as_str() {
        Some("cycle") => {
            let mut cycle = monitor::Cycle::default();
            value!(request["samples"]
                .as_array()
                .ok_or("Missing samples")?
                .iter()
                .map(|sample| cycle.observe(sample))
                .collect::<Vec<_>>())
        }
        Some("protected_files_match") => {
            let result = monitor::protected_files_match(
                Path::new(request["backup"].as_str().ok_or("Missing backup")?),
                Path::new(request["home"].as_str().ok_or("Missing home")?),
            );
            value!(match result {
                Ok(matches) => matches,
                Err(error) => {
                    eprintln!("Protected-file verification failed: {error}");
                    false
                }
            })
        }
        _ => return Err("Unknown acceptance operation".into()),
    };
    println!("{}", json::encode(&result)?);
    Ok(())
}
