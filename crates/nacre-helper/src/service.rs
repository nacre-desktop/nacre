//! Bounded JSON-lines protocol with a serial worker lane for each hosted helper.
use crate::Result;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Write},
    sync::{mpsc, Arc, Mutex},
    thread,
};

pub const TOOLS: [&str; 6] = [
    "menu-backend",
    "dashboard-data",
    "dashboard-media",
    "launcher-preferences",
    "notification-history",
    "network-state",
];
pub use nacre_protocol::helpers::{HelperRequest as Request, HelperResult};
#[derive(Clone, Copy)]
pub struct Limits {
    pub request: u64,
    pub response: usize,
    pub pending: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            request: 4 * 1024 * 1024,
            response: 8 * 1024 * 1024,
            pending: 64,
        }
    }
}
pub fn validate(value: &Value) -> Result<Request> {
    let data = value.as_object().ok_or("Invalid helper request")?;
    if data.len() != 4
        || ["id", "tool", "arguments", "input"]
            .iter()
            .any(|k| !data.contains_key(*k))
    {
        return Err("Invalid helper request".into());
    }
    let id = data["id"]
        .as_u64()
        .filter(|id| (1..=9_007_199_254_740_991).contains(id))
        .ok_or("Invalid helper request ID")?;
    let tool = data["tool"]
        .as_str()
        .filter(|tool| TOOLS.contains(tool))
        .ok_or("Unsupported helper")?;
    let arguments = data["arguments"]
        .as_array()
        .ok_or("Invalid helper arguments")?;
    if arguments.len() > 8 {
        return Err("Invalid helper arguments".into());
    }
    let arguments = arguments
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|v| v.chars().count() <= 65536 && !v.contains('\0'))
                .map(str::to_owned)
                .ok_or("Invalid helper arguments")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Request {
        id,
        tool: tool.into(),
        arguments,
        input: (!data["input"].is_null()).then(|| data["input"].clone()),
    })
}
pub(crate) fn error_output(message: &str) -> String {
    crate::json::encode(&json!({"error":message.chars().take(240).collect::<String>()}))
        .expect("finite protocol error")
}
pub(crate) fn send<W: Write>(output: &Mutex<W>, value: Value, limit: usize) -> Result<()> {
    let mut packet = crate::json::encode(&value)?;
    if packet.len() > limit {
        packet = crate::json::encode(
            &json!({"id":value.get("id").unwrap_or(&json!(0)), "code":1, "output":error_output("Helper response is too large")}),
        )?;
    }
    let mut output = output.lock().map_err(|_| "Helper output lock failed")?;
    writeln!(output, "{packet}")?;
    output.flush()?;
    Ok(())
}

pub(crate) fn parse_error(error: serde_json::Error, bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut quoted = false;
    let mut escaped = false;
    for (offset, byte) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if *byte == b'\\' && quoted {
            escaped = true;
            continue;
        }
        if *byte == b'"' {
            quoted = !quoted;
            continue;
        }
        if !quoted
            && (bytes[offset..].starts_with(b"NaN") || bytes[offset..].starts_with(b"Infinity"))
        {
            return "Non-finite JSON value".into();
        }
    }
    let description = error.to_string();
    let message = if description.starts_with("key must be a string") {
        "Expecting property name enclosed in double quotes"
    } else if description.starts_with("expected `:`") {
        "Expecting ':' delimiter"
    } else if description.starts_with("expected `,`") {
        "Expecting ',' delimiter"
    } else if description.starts_with("trailing characters") {
        "Extra data"
    } else {
        "Expecting value"
    };
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index + 1 == error.line() {
            offset += line.chars().take(error.column().saturating_sub(1)).count();
            break;
        }
        offset += line.chars().count();
    }
    format!(
        "{message}: line {} column {} (char {offset})",
        error.line(),
        error.column()
    )
}

pub fn serve<R, W, F>(mut input: R, output: W, limits: Limits, execute: F) -> Result<i32>
where
    R: BufRead,
    W: Write + Send + 'static,
    F: Fn(&Request) -> Result<String> + Send + Sync + 'static,
{
    let output = Arc::new(Mutex::new(output));
    let active = Arc::new(Mutex::new(HashSet::new()));
    let execute = Arc::new(execute);
    let mut lanes: HashMap<String, mpsc::Sender<Request>> = HashMap::new();
    let mut workers = Vec::new();
    let mut status = 0;
    send(&output, json!({"ready":true,"version":1}), limits.response)?;
    loop {
        let mut line = Vec::new();
        if std::io::Read::take(&mut input, limits.request + 1).read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if line.len() as u64 > limits.request {
            send(
                &output,
                json!({"fatal":"Helper request is too large"}),
                limits.response,
            )?;
            status = 1;
            break;
        }
        let value = serde_json::from_slice::<Value>(&line);
        let id = value
            .as_ref()
            .ok()
            .and_then(|v| v.get("id"))
            .filter(|id| id.is_i64() || id.is_u64())
            .cloned()
            .unwrap_or(json!(0));
        let request = value
            .map_err(|error| -> crate::Error { parse_error(error, &line).into() })
            .and_then(|value| validate(&value));
        let request = match request {
            Ok(value) => value,
            Err(error) => {
                send(
                    &output,
                    json!({"id":id,"code":1,"output":error_output(&error.to_string())}),
                    limits.response,
                )?;
                continue;
            }
        };
        {
            let mut active = active.lock().map_err(|_| "Helper active lock failed")?;
            if active.contains(&request.id) {
                send(
                    &output,
                    json!({"fatal":"Duplicate helper request ID"}),
                    limits.response,
                )?;
                status = 1;
                break;
            }
            if active.len() >= limits.pending {
                send(
                    &output,
                    json!({"id":request.id,"code":1,"output":error_output("Helper queue is full")}),
                    limits.response,
                )?;
                continue;
            }
            active.insert(request.id);
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = lanes.entry(request.tool.clone())
        {
            let (sender, receiver) = mpsc::channel::<Request>();
            let output = output.clone();
            let active = active.clone();
            let execute = execute.clone();
            let worker = thread::Builder::new().name(format!("nacre-helper-{}", request.tool)).spawn(move || {
                for request in receiver {
                    let response = match execute(&request) {
                        Ok(result) => json!({"id":request.id,"code":0,"output":result}),
                        Err(error) => json!({"id":request.id,"code":1,"output":error_output(&error.to_string())}),
                    };
                    send(&output, response, limits.response)?;
                    active.lock().map_err(|_| "Helper active lock failed")?.remove(&request.id);
                }
                Ok::<(), crate::Error>(())
            })?;
            workers.push(worker);
            entry.insert(sender);
        }
        lanes[&request.tool].send(request)?;
    }
    drop(lanes);
    for worker in workers {
        worker.join().map_err(|_| "Helper worker failed")??;
    }
    Ok(status)
}

/// Shared implementation for direct service lanes and daemon-owned workers.
pub fn execute(request: &Request) -> Result<String> {
    let value = match request.tool.as_str() {
        "notification-history"
            if request.arguments.len() == 1
                && ((request.arguments[0] == "load" && request.input.is_none())
                    || (request.arguments[0] == "save"
                        && request.input.as_ref().is_some_and(Value::is_array))) =>
        {
            crate::notification_history::execute(&request.arguments, request.input.clone())?
        }
        "launcher-preferences"
            if request.arguments.len() == 1
                && ((request.arguments[0] == "load" && request.input.is_none())
                    || (["favorite", "hide", "record"]
                        .contains(&request.arguments[0].as_str())
                        && request.input.as_ref().is_some_and(Value::is_object))) =>
        {
            crate::launcher_preferences::execute(&request.arguments, request.input.clone())?
        }
        "dashboard-data"
            if (1..=2).contains(&request.arguments.len()) && request.input.is_none() =>
        {
            crate::dashboard_data::execute(&request.arguments, None)?
        }
        "network-state" if request.arguments.is_empty() && request.input.is_none() => {
            crate::network_state::execute(&request.arguments, None)?
        }
        "menu-backend"
            if request.input.is_none()
                && ((request.arguments.len() == 1 && request.arguments[0] == "state")
                    || (request.arguments.len() == 2
                        && ((request.arguments[0] == "state"
                            && request.arguments[1] == "--force")
                            || ["files", "convert", "packages", "ownership"]
                                .contains(&request.arguments[0].as_str())))) =>
        {
            crate::menu::execute(&request.arguments, None)?
        }
        "dashboard-media" if request.arguments.len() == 1 && request.input.is_none() => {
            crate::dashboard_media::execute(&request.arguments, None)?
        }
        _ => return Err("Unsupported helper operation".into()),
    };
    match value {
        Some(value) => Ok(format!("{}\n", crate::json::encode(&value)?)),
        None => Ok(String::new()),
    }
}

/// Encode operation failures exactly as the legacy JSON-lines service does.
pub fn result(request: &Request) -> HelperResult {
    match execute(request) {
        Ok(output) => HelperResult { code: 0, output },
        Err(error) => HelperResult {
            code: 1,
            output: error_output(&error.to_string()),
        },
    }
}
