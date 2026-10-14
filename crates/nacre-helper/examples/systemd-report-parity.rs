//! Developer-only comparator for private systemctl wire-format references.
use nacre_helper::{json, systemd, Result};
use serde_json::{json as value, Value};
fn main() -> Result<()> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS")?;
    if !address.starts_with("unix:path=/case/") {
        return Err("Systemd adapter requires a private parity bus".into());
    }
    let request: Value =
        serde_json::from_str(&std::env::args().nth(1).ok_or("Missing report request")?)?;
    let strings = |key: &str| -> Result<Vec<String>> {
        request[key]
            .as_array()
            .ok_or("Missing report list")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Invalid report list".into())
            })
            .collect()
    };
    let (code, output) = systemd::report_units(
        &strings("units")?,
        &strings("properties")?,
        std::time::Duration::from_secs(5),
    )?;
    println!("{}", json::encode(&value!({"stdout":output,"exit":code}))?);
    Ok(())
}
