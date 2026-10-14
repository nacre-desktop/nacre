//! Finite compositor operations restricted to the owned panel title and PID.
use crate::compositor::niri_state::Error;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: "ValueError",
        message: message.into(),
    }
}
fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid("Invalid pearl geometry"))
}
fn rounded(value: f64) -> Result<i64> {
    if !value.is_finite() || value < i64::MIN as f64 || value >= -(i64::MIN as f64) {
        return Err(invalid("Invalid pearl geometry"));
    }
    Ok(value.round_ties_even() as i64)
}
pub fn owned(title: &str, pid: i64, clients: &Value) -> Result<Option<Value>> {
    let pattern = regex::Regex::new(r"^Nacre Pearl (Launcher|Dashboard|AI|Controls|Media) [0-9]+$")
        .map_err(|error| invalid(error.to_string()))?;
    if !pattern.is_match(title) || pid <= 0 {
        return Err(invalid("Invalid pearl identity"));
    }
    Ok(clients
        .as_array()
        .ok_or_else(|| invalid("Malformed compositor query response"))?
        .iter()
        .find(|client| client["title"] == title && client["pid"] == pid)
        .cloned())
}
pub trait Runtime {
    fn read(&mut self, kind: &str) -> Result<Value>;
    fn dispatch(&mut self, method: &str, window: &Value, values: Value) -> Result<()>;
    fn wait_for_window(
        &mut self,
        title: &str,
        pid: i64,
        deadline: Instant,
    ) -> Result<Option<Value>>;
}
pub struct Native {
    pub backend: crate::compositor::backend::Backend,
}
impl Runtime for Native {
    fn read(&mut self, kind: &str) -> Result<Value> {
        self.backend.query(
            if kind == "clients" {
                "windows"
            } else {
                "monitors"
            },
            Duration::from_secs(3),
            None,
        )
    }
    fn dispatch(&mut self, method: &str, window: &Value, values: Value) -> Result<()> {
        if !self.backend.valid_window_id(window) {
            return Err(invalid("Invalid window address"));
        }
        let (method, args, kwargs) = match method {
            "focus" => ("focus_window", vec![window.clone()], json!({"timeout":3})),
            "float" => (
                "float_window",
                vec![window.clone()],
                json!({"action":values["action"],"timeout":3}),
            ),
            "resize" => (
                "resize_window",
                vec![window.clone(), values["x"].clone(), values["y"].clone()],
                json!({"relative":values["relative"],"timeout":3}),
            ),
            "move" => (
                "move_window",
                vec![window.clone()],
                json!({"x":values["x"],"y":values["y"],"relative":values["relative"],"timeout":3}),
            ),
            "pin" => ("pin_window", vec![window.clone()], json!({"timeout":3})),
            _ => return Err(invalid("Unsupported pearl action")),
        };
        self.backend.action(method, &args, &kwargs)?;
        Ok(())
    }
    fn wait_for_window(
        &mut self,
        title: &str,
        pid: i64,
        deadline: Instant,
    ) -> Result<Option<Value>> {
        match &mut self.backend {
            crate::compositor::backend::Backend::Hyprland(backend) => {
                let mut events = backend.events()?;
                loop {
                    if let Some(client) =
                        owned(title, pid, &backend.actions.query("windows", 3.0, None)?)?
                    {
                        return Ok(Some(client));
                    }
                    if events.next_before(deadline)?.is_none() {
                        return Ok(None);
                    }
                }
            }
            crate::compositor::backend::Backend::Niri(backend) => {
                let mut socket = crate::compositor::niri_events::EventSocket::connect(
                    std::path::Path::new(backend.session_id()),
                    true,
                )?;
                loop {
                    if let Some(client) = owned(
                        title,
                        pid,
                        &backend.query("windows", Duration::from_secs(3), None)?,
                    )? {
                        return Ok(Some(client));
                    }
                    if socket.read_before(deadline)?.is_none() {
                        return Ok(None);
                    }
                }
            }
        }
    }
}
pub struct Arguments {
    pub action: String,
    pub title: String,
    pub pid: i64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Arguments {
    pub fn parse(args: &[String]) -> Result<Self> {
        if !(3..=7).contains(&args.len())
            || !["geometry", "place", "fit", "pin", "focus"].contains(&args[0].as_str())
        {
            return Err(invalid(
                "Expected pearl action, title, PID and optional geometry",
            ));
        }
        let value = |index: usize| -> Result<f64> {
            args.get(index)
                .map(|value| {
                    value
                        .parse::<f64>()
                        .map_err(|_| invalid("Invalid pearl geometry"))
                })
                .unwrap_or(Ok(0.0))
        };
        Ok(Self {
            action: args[0].clone(),
            title: args[1].clone(),
            pid: args[2].parse().map_err(|_| invalid("Invalid pearl PID"))?,
            x: value(3)?,
            y: value(4)?,
            width: value(5)?,
            height: value(6)?,
        })
    }
}
pub fn execute(args: &Arguments, runtime: &mut impl Runtime) -> Result<Value> {
    let deadline = Instant::now()
        + if args.action == "place" {
            Duration::from_secs(4)
        } else {
            Duration::from_millis(200)
        };
    let mut client = owned(&args.title, args.pid, &runtime.read("clients")?)?;
    if client.is_none() {
        client = runtime.wait_for_window(&args.title, args.pid, deadline)?;
    }
    let mut client = client.ok_or_else(|| invalid("Pearl window not mapped"))?;
    let address = client
        .get("windowId")
        .or_else(|| client.get("address"))
        .cloned()
        .unwrap_or(Value::Null);
    match args.action.as_str() {
        "place" => {
            runtime.dispatch("float", &address, json!({"action":"enable"}))?;
            if args.width > 0.0 && args.height > 0.0 {
                runtime.dispatch(
                    "resize",
                    &address,
                    json!({"x":rounded(args.width)?,"y":rounded(args.height)?,"relative":false}),
                )?;
            }
            runtime.dispatch(
                "move",
                &address,
                json!({"x":rounded(args.x)?,"y":rounded(args.y)?,"relative":false}),
            )?;
        }
        "fit"
            if client
                .get("fullscreen")
                .is_none_or(|value| value == false || value == 0 || value.is_null()) =>
        {
            let monitors = runtime.read("monitors")?;
            let monitor = monitors
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["id"] == client["monitor"]))
                .ok_or_else(|| invalid("Pearl output unavailable"))?;
            let mut mw = number(&monitor["width"])?;
            let mut mh = number(&monitor["height"])?;
            if monitor
                .get("transform")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                % 2
                != 0
            {
                std::mem::swap(&mut mw, &mut mh);
            }
            let scale = monitor
                .get("scale")
                .and_then(Value::as_f64)
                .filter(|scale| *scale != 0.0)
                .unwrap_or(1.0);
            let reserved = monitor
                .get("reserved")
                .cloned()
                .unwrap_or(json!([0, 0, 0, 0]));
            let left = number(&reserved[0])?;
            let top = number(&reserved[1])?;
            let right = number(&reserved[2])?;
            let bottom = number(&reserved[3])?;
            let width =
                (mw / scale - left - right).min(number(&client["size"][0])?.max(args.width));
            let height =
                (mh / scale - top - bottom).min(number(&client["size"][1])?.max(args.height));
            let x = (number(&monitor["x"])? + left).max(
                number(&client["at"][0])?.min(number(&monitor["x"])? + mw / scale - right - width),
            );
            let y = (number(&monitor["y"])? + top).max(
                number(&client["at"][1])?
                    .min(number(&monitor["y"])? + mh / scale - bottom - height),
            );
            if json!([rounded(width)?, rounded(height)?]) != client["size"] {
                runtime.dispatch(
                    "resize",
                    &address,
                    json!({"x":rounded(width)?,"y":rounded(height)?,"relative":false}),
                )?;
            }
            if json!([rounded(x)?, rounded(y)?]) != client["at"] {
                runtime.dispatch(
                    "move",
                    &address,
                    json!({"x":rounded(x)?,"y":rounded(y)?,"relative":false}),
                )?;
            }
        }
        "focus" => runtime.dispatch("focus", &address, json!({}))?,
        "pin" => {
            if client
                .get("floating")
                .is_none_or(|value| value == false || value == 0 || value.is_null())
            {
                return Err(invalid("Float the window before pinning it"));
            }
            runtime.dispatch("pin", &address, json!({}))?;
        }
        _ => {}
    }
    client = owned(&args.title, args.pid, &runtime.read("clients")?)?.unwrap_or(client);
    Ok(
        json!({"address":address,"x":client["at"][0],"y":client["at"][1],"width":client["size"][0],"height":client["size"][1]}),
    )
}
pub fn run(argv: &[String]) -> crate::Result<i32> {
    let args = match Arguments::parse(argv) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("pearl-window: {}", error.message);
            return Ok(2);
        }
    };
    let result = (|| -> crate::Result<Value> {
        let backend = crate::compositor::backend::Backend::new(
            crate::paths::Paths::environment()?,
            std::env::vars().collect(),
        )?;
        Ok(execute(&args, &mut Native { backend })?)
    })();
    match result {
        Ok(value) => {
            println!("{}", crate::json::encode(&value)?);
            Ok(0)
        }
        Err(error) => {
            println!(
                "{}",
                crate::json::encode(
                    &json!({"error":error.to_string().chars().take(200).collect::<String>()})
                )?
            );
            Ok(1)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        input: Value,
        reads: Vec<Value>,
        calls: Vec<Value>,
    }
    impl Runtime for Fixture {
        fn read(&mut self, kind: &str) -> Result<Value> {
            self.reads.push(json!(kind));
            Ok(self.input[kind].clone())
        }
        fn dispatch(&mut self, method: &str, window: &Value, values: Value) -> Result<()> {
            self.calls
                .push(json!({"method":method,"window":window,"kwargs":values}));
            Ok(())
        }
        fn wait_for_window(
            &mut self,
            _title: &str,
            _pid: i64,
            _deadline: Instant,
        ) -> Result<Option<Value>> {
            Ok(None)
        }
    }
    #[test]
    fn frozen_current_python_geometry_ownership_reads_and_actions() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/pearl-window-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let argv = input["argv"]
                .as_array()
                .ok_or("Missing argv")?
                .iter()
                .map(|value| value.as_str().map(str::to_owned).ok_or("Malformed argv"))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let args = Arguments::parse(&argv)?;
            let mut runtime = Fixture {
                input: input.clone(),
                reads: vec![],
                calls: vec![],
            };
            let (value, exit) = match execute(&args, &mut runtime) {
                Ok(value) => (value, 0),
                Err(error) => (
                    json!({"error":error.message.chars().take(200).collect::<String>()}),
                    1,
                ),
            };
            assert_eq!(
                crate::json::encode(&value)? + "\n",
                case["observed"]["stdout"],
                "{}",
                input["name"]
            );
            assert_eq!(json!(exit), case["observed"]["exit"], "{}", input["name"]);
            assert_eq!(
                json!(runtime.calls),
                case["observed"]["calls"],
                "{}",
                input["name"]
            );
            assert_eq!(
                json!(runtime.reads),
                case["observed"]["reads"],
                "{}",
                input["name"]
            );
        }
        Ok(())
    }
}
