//! Developer-only frozen provider boundary; actual pearl/native encoding, no replay.
use nacre_helper::{
    compositor::{
        hyprland::{Backend, Transport},
        niri_state::Error,
    },
    json,
    pearl_window::{self, Arguments, Runtime},
    Result,
};
use serde_json::{json as value, Value};
use std::time::Instant;
struct FakeTools;
impl Transport for FakeTools {
    fn run(
        &mut self,
        args: Vec<String>,
        timeout: f64,
        _check: bool,
    ) -> std::result::Result<Value, Error> {
        let output = nacre_helper::process::checked(
            "hyprctl",
            &args,
            None,
            std::time::Duration::try_from_secs_f64(timeout).map_err(|error| Error {
                kind: "ValueError",
                message: error.to_string(),
            })?,
        )
        .map_err(|error| Error {
            kind: "RuntimeError",
            message: error.to_string(),
        })?;
        Ok(
            value!({"code":output.status.code().unwrap_or(1),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr)}),
        )
    }
}
struct Provider {
    backend: Backend<FakeTools>,
}
impl Runtime for Provider {
    fn read(&mut self, kind: &str) -> std::result::Result<Value, Error> {
        self.backend.query(
            if kind == "clients" {
                "windows"
            } else {
                "monitors"
            },
            3.0,
            None,
        )
    }
    fn dispatch(
        &mut self,
        method: &str,
        window: &Value,
        values: Value,
    ) -> std::result::Result<(), Error> {
        let (name, args, kwargs) = match method {
            "focus" => ("focus_window", vec![window.clone()], value!({"timeout":3})),
            "float" => (
                "float_window",
                vec![window.clone()],
                value!({"action":values["action"],"timeout":3}),
            ),
            "resize" => (
                "resize_window",
                vec![window.clone(), values["x"].clone(), values["y"].clone()],
                value!({"relative":values["relative"],"timeout":3}),
            ),
            "move" => (
                "move_window",
                vec![window.clone()],
                value!({"x":values["x"],"y":values["y"],"relative":values["relative"],"timeout":3}),
            ),
            "pin" => ("pin_window", vec![window.clone()], value!({"timeout":3})),
            _ => {
                return Err(Error {
                    kind: "ValueError",
                    message: "Unsupported pearl action".into(),
                })
            }
        };
        self.backend.execute(name, &args, &kwargs)?;
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
fn run() -> Result<i32> {
    let args = Arguments::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    let result = pearl_window::execute(
        &args,
        &mut Provider {
            backend: Backend {
                transport: FakeTools,
            },
        },
    );
    let (value, exit) = match result {
        Ok(value) => (value, 0),
        Err(error) => (
            value!({"error":error.message.chars().take(200).collect::<String>()}),
            1,
        ),
    };
    println!("{}", json::encode(&value)?);
    Ok(exit)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
