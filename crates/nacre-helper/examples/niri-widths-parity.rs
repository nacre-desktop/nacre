//! Developer-only persistent policy boundary for the original width assertions.
use nacre_helper::{
    json,
    niri_widths::{Identity, Policy, SizeHints, X11},
    Result,
};
use serde_json::{json as value, Value};
use std::io::{BufRead, Write};
struct Callbacks;
fn callback(request: Value) -> Result<Value> {
    println!("{}", json::encode(&request)?);
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let response: Value = serde_json::from_str(&line)?;
    if let Some(error) = response["error"].as_str() {
        return Err(error.into());
    }
    Ok(response["value"].clone())
}
impl X11 for Callbacks {
    fn windows(&mut self) -> Result<Vec<u32>> {
        Ok(serde_json::from_value(callback(
            value!({"callback":"windows"}),
        )?)?)
    }
    fn identity(&mut self, window: u32) -> Result<Option<Identity>> {
        Ok(serde_json::from_value(callback(
            value!({"callback":"identity","window":window}),
        )?)?)
    }
    fn ordinary(&mut self, window: u32) -> Result<bool> {
        Ok(serde_json::from_value(callback(
            value!({"callback":"ordinary","window":window}),
        )?)?)
    }
    fn hints(&mut self, window: u32) -> Result<Option<SizeHints>> {
        Ok(serde_json::from_value(callback(
            value!({"callback":"hints","window":window}),
        )?)?)
    }
    fn set_hints(&mut self, window: u32, hints: &SizeHints) -> Result<()> {
        callback(value!({"callback":"set_hints","window":window,"hints":hints}))?;
        Ok(())
    }
}
fn main() -> Result<()> {
    let mut policy = Policy::default();
    let mut x11 = Callbacks;
    loop {
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line)? == 0 {
            return Ok(());
        }
        if line.len() > nacre_helper::niri_widths::MAX_BYTES {
            return Err("Oversized policy fixture".into());
        }
        let input: Value = serde_json::from_str(&line)?;
        let result = match input["operation"].as_str() {
            Some("event") => policy.event(&input["event"]).map(Value::Bool),
            Some("reconcile") => policy
                .reconcile(&mut x11, input["now"].as_f64().unwrap_or(100.0))
                .map(|_| Value::Null),
            Some("apply_openings") => {
                let now = if input["now"] == "infinity" {
                    f64::INFINITY
                } else {
                    input["now"].as_f64().ok_or("Missing clock")?
                };
                policy
                    .apply_openings(now, |id| {
                        callback(value!({"callback":"resize","id":id}))?;
                        Ok(())
                    })
                    .map(|_| Value::Null)
            }
            Some("close") => policy.close(&mut x11).map(|_| Value::Null),
            Some("stop") => return Ok(()),
            _ => Err("Unknown width fixture operation".into()),
        };
        let result = match result {
            Ok(value) => value!({"value":value}),
            Err(error) => value!({"error":error.to_string()}),
        };
        println!(
            "{}",
            json::encode(&value!({"result":result,"state":policy.snapshot()?}))?
        );
        std::io::stdout().flush()?;
    }
}
