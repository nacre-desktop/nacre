//! Developer-only callback boundary for legacy pearl-window assertions.
use nacre_helper::{
    compositor::niri_state::Error,
    json,
    pearl_window::{self, Arguments, Runtime},
    Result,
};
use serde_json::{json as value, Value};
use std::{
    io::{BufRead, Write},
    time::Instant,
};
struct Callbacks;
fn callback(request: Value) -> std::result::Result<Value, Error> {
    let run = || -> Result<Value> {
        println!("{}", json::encode(&request)?);
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    };
    run().map_err(|error| Error {
        kind: "RuntimeError",
        message: error.to_string(),
    })
}
impl Runtime for Callbacks {
    fn read(&mut self, kind: &str) -> std::result::Result<Value, Error> {
        callback(value!({"callback":"read","kind":kind}))
    }
    fn dispatch(
        &mut self,
        method: &str,
        window: &Value,
        values: Value,
    ) -> std::result::Result<(), Error> {
        callback(value!({"callback":"dispatch","method":method,"window":window,"values":values}))?;
        Ok(())
    }
    fn wait_for_window(
        &mut self,
        _title: &str,
        _pid: i64,
        _deadline: Instant,
    ) -> std::result::Result<Option<Value>, Error> {
        Ok(None)
    }
}
fn run() -> Result<()> {
    let input: Value =
        serde_json::from_str(&std::env::args().nth(1).ok_or("Missing pearl request")?)?;
    let result = match input["operation"].as_str() {
        Some("dispatch-validation") => {
            if nacre_helper::compositor::hyprland::valid_window_id(&input["window"]) {
                Ok(Value::Null)
            } else {
                Err(Error {
                    kind: "ValueError",
                    message: "Invalid window address".into(),
                })
            }
        }
        Some("owned") => pearl_window::owned(
            input["title"].as_str().ok_or("Missing title")?,
            input["pid"].as_i64().ok_or("Missing PID")?,
            &input["clients"],
        )
        .map(|value| value.unwrap_or(Value::Null)),
        Some("execute") => {
            let args = input["argv"]
                .as_array()
                .ok_or("Missing args")?
                .iter()
                .map(|value| value.as_str().map(str::to_owned).ok_or("Invalid arg"))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Arguments::parse(&args).and_then(|args| pearl_window::execute(&args, &mut Callbacks))
        }
        _ => return Err("Unknown pearl test operation".into()),
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
