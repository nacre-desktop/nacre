//! Developer-only legacy callback boundary; all encoding/query logic remains Rust.
use nacre_helper::{
    compositor::{
        hyprland::{Backend, Transport},
        niri_state::Error,
    },
    json, Result,
};
use serde_json::{json as value, Value};
use std::io::{BufRead, Write};
struct Callbacks;
impl Transport for Callbacks {
    fn run(
        &mut self,
        args: Vec<String>,
        timeout: f64,
        check: bool,
    ) -> std::result::Result<Value, Error> {
        let callback = || -> Result<Value> {
            println!(
                "{}",
                json::encode(
                    &value!({"callback":"run","args":args,"timeout":timeout,"check":check})
                )?
            );
            std::io::stdout().flush()?;
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            Ok(serde_json::from_str(&line)?)
        };
        callback().map_err(|error| Error {
            kind: "RuntimeError",
            message: error.to_string(),
        })
    }
}
fn run() -> Result<()> {
    let input: Value = serde_json::from_str(
        &std::env::args()
            .nth(1)
            .ok_or("Missing Hyprland test operation")?,
    )?;
    let mut backend = Backend {
        transport: Callbacks,
    };
    let args = input["args"].as_array().ok_or("Missing args")?;
    let kwargs = &input["kwargs"];
    let operation = input["operation"].as_str().ok_or("Missing operation")?;
    if input["observe_nested"] == true
        && (operation == "set_animations" || operation == "set_performance")
    {
        let enabled = args.first().is_some_and(|value| value == true);
        let (method, args) = if operation == "set_performance" && !enabled {
            ("reload", vec![])
        } else {
            (
                "set_options",
                vec![if operation == "set_animations" {
                    value!({"animations":{"enabled":enabled}})
                } else {
                    nacre_helper::compositor::hyprland::performance_options()
                }],
            )
        };
        println!(
            "{}",
            json::encode(
                &value!({"callback":"nested","method":method,"args":args,"timeout":kwargs["timeout"].as_f64().unwrap_or(10.0)})
            )?
        );
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        println!("{}", json::encode(&value!({"result":{"value":null}}))?);
        return Ok(());
    }
    let result = match operation {
        "session_id" => Ok(input["environment"]["HYPRLAND_INSTANCE_SIGNATURE"]
            .as_str()
            .map(|value| value!(value))
            .unwrap_or(value!(""))),
        "environment_reply" => {
            struct Reply(Value);
            impl Transport for Reply {
                fn run(
                    &mut self,
                    _args: Vec<String>,
                    _timeout: f64,
                    _check: bool,
                ) -> std::result::Result<Value, Error> {
                    Ok(self.0.clone())
                }
            }
            let mut reply = Backend {
                transport: Reply(input["reply"].clone()),
            };
            reply.environment_updates(&args[0])
        }
        "query" => backend.query(
            args[0].as_str().ok_or("Missing query")?,
            kwargs["timeout"].as_f64().unwrap_or(10.0),
            kwargs["max_bytes"].as_u64().map(|value| value as usize),
        ),
        "normalize_reply" => {
            struct Reply(Value);
            impl Transport for Reply {
                fn run(
                    &mut self,
                    _args: Vec<String>,
                    _timeout: f64,
                    _check: bool,
                ) -> std::result::Result<Value, Error> {
                    Ok(self.0.clone())
                }
            }
            let mut reply = Backend {
                transport: Reply(value!({"stdout":input["stdout"]})),
            };
            reply.query(
                args[0].as_str().ok_or("Missing query")?,
                10.0,
                kwargs["max_bytes"].as_u64().map(|value| value as usize),
            )
        }
        "environment_updates" => backend.environment_updates(&args[0]),
        operation => backend.execute(operation, args, kwargs),
    };
    let result = match result {
        Ok(result) => value!({"value":result}),
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
