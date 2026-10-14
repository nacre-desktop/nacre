//! Developer-only callbacks for the unchanged hide/restore assertions.
use nacre_helper::{
    json,
    window_hide::{self, Runtime},
    Result,
};
use serde_json::{json as value, Value};
use std::{
    io::{BufRead, Write},
    path::Path,
};
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
impl Runtime for Callbacks {
    fn valid_window_id(&mut self, id: &Value) -> Result<bool> {
        Ok(callback(value!({"callback":"valid","id":id}))?
            .as_bool()
            .unwrap_or(false))
    }
    fn query(&mut self, name: &str) -> Result<Value> {
        callback(value!({"callback":"query","name":name}))
    }
    fn dispatch(&mut self, method: &str, window: &str, values: Value) -> Result<()> {
        callback(value!({"callback":"dispatch","method":method,"window":window,"values":values}))?;
        Ok(())
    }
    fn service_action(&mut self, name: &str, window: &str, kwargs: Value) -> Result<Value> {
        callback(value!({"callback":"service","name":name,"window":window,"kwargs":kwargs}))
    }
    fn notify(&mut self, message: &str) -> Result<()> {
        callback(value!({"callback":"notify","message":message}))?;
        Ok(())
    }
}
fn run() -> Result<()> {
    let input: Value = serde_json::from_str(&std::env::args().nth(1).ok_or("Missing request")?)?;
    let mut runtime = Callbacks;
    let folder = Path::new(input["folder"].as_str().unwrap_or(""));
    let result = match input["operation"].as_str() {
        Some("action") => window_hide::action(
            input["name"].as_str().ok_or("Missing action")?,
            folder,
            input["window"].as_str(),
            input["quiet"] == true,
            &mut runtime,
        )
        .map(|_| Value::Null),
        Some("destination") => window_hide::destination(
            input["workspace"].as_i64().ok_or("Missing workspace")?,
            &mut runtime,
        )
        .map(Value::String),
        Some("dispatch") => window_hide::dispatch(
            &mut runtime,
            input["method"].as_str().ok_or("Missing method")?,
            input["window"].as_str().ok_or("Missing window")?,
            input["values"].clone(),
        )
        .map(|_| Value::Null),
        Some("records") => window_hide::records(folder, &input["clients"]).map(|rows| {
            value!(rows
                .into_iter()
                .map(|(_, path, workspace)| value!([path, workspace]))
                .collect::<Vec<_>>())
        }),
        Some("query") => callback(
            value!({"callback":"service-query","name":input["name"],"timeout":window_hide::QUERY_TIMEOUT_SECONDS,"max_bytes":window_hide::MAX_QUERY_BYTES}),
        ),
        Some("valid") => Ok(value!(window_hide::address(&input["id"]))),
        _ => return Err("Unknown window fixture operation".into()),
    };
    let response = match result {
        Ok(value) => value!({"value":value}),
        Err(error) => {
            let message = error.to_string();
            value!({"error":message,"type":if message=="Invalid window address"||message=="The original workspace is unavailable"||message=="Unknown window action"{"ValueError"}else{"RuntimeError"}})
        }
    };
    println!("{}", json::encode(&value!({"result":response}))?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1)
    }
}
