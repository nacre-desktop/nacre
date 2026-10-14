//! Existing hyprctl-lua wrapper vocabulary mapped to native compositor operations.
use crate::{compositor::backend::Backend, json, paths::Paths, Result};
use serde_json::{json as value, Value};
fn argument(args: &[String], index: usize) -> Result<&str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| "list index out of range".into())
}
fn boolean(value: &str) -> Result<bool> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("Expected true or false".into()),
    }
}
pub fn call(argv: &[String]) -> Result<Value> {
    let command = argv.first().ok_or("A desktop command is required")?;
    let args = &argv[1..];
    let call = |method: &str, args: Vec<Value>, kwargs: Value| value!({"method":method,"args":args,"kwargs":kwargs});
    Ok(match command.as_str() {
        "valid-window-id" => call(
            "valid_window_id",
            vec![value!(argument(args, 0)?)],
            value!({}),
        ),
        "window-job-token" => call(
            "window_job_token",
            vec![value!(argument(args, 0)?)],
            value!({}),
        ),
        "query" => call(
            "query",
            vec![value!(argument(args, 0)?)],
            value!({"timeout":5}),
        ),
        "focus-workspace" => call(
            "switch_workspace",
            vec![value!(argument(args, 0)?)],
            value!({"current_monitor":args.get(1).map(|arg|boolean(arg)).transpose()?.unwrap_or(false)}),
        ),
        "focus-window" => call("focus_window", vec![value!(argument(args, 0)?)], value!({})),
        "move-window" => call(
            "move_window",
            vec![value!(argument(args, 1)?)],
            value!({"workspace":argument(args,0)?,"follow":args.get(2).map(|arg|boolean(arg)).transpose()?.unwrap_or(false)}),
        ),
        "move-workspace" => call(
            "move_workspace",
            vec![value!(argument(args, 0)?), value!(argument(args, 1)?)],
            value!({}),
        ),
        "close-window" | "kill-window" => call(
            if command == "close-window" {
                "close_window"
            } else {
                "kill_window"
            },
            vec![value!(argument(args, 0)?)],
            value!({}),
        ),
        "fullscreen" => call(
            "fullscreen_window",
            vec![value!(args
                .get(1)
                .map(String::as_str)
                .unwrap_or("activewindow"))],
            value!({"mode":args.first().map(String::as_str).unwrap_or("fullscreen")}),
        ),
        "resize-window" => call(
            "resize_window",
            vec![
                value!(args.get(2).map(String::as_str).unwrap_or("activewindow")),
                value!(argument(args, 0)?.parse::<i64>()?),
                value!(argument(args, 1)?.parse::<i64>()?),
            ],
            value!({"relative":false}),
        ),
        "dpms" => call(
            "dpms",
            vec![value!(argument(args, 0)?)],
            value!({"monitor":args.get(1)}),
        ),
        "monitor" => {
            let disabled = argument(args, 1)? == "disable";
            let mut record = value!({"output":argument(args,0)?,"disabled":disabled});
            if !disabled {
                record["mode"] = value!(argument(args, 1)?);
                record["position"] = value!(args.get(2).map(String::as_str).unwrap_or("auto"));
                record["scale"] = args
                    .get(3)
                    .map(|value| value.parse::<f64>().map(|number| value!(number)))
                    .transpose()?
                    .unwrap_or(value!(1));
                if let Some(mirror) = args.get(4).filter(|value| !value.is_empty()) {
                    record["mirror"] = value!(mirror);
                }
            }
            call("configure_monitors", vec![value!([record])], value!({}))
        }
        "animations" | "performance" => call(
            if command == "animations" {
                "set_animations"
            } else {
                "set_performance"
            },
            vec![value!(boolean(argument(args, 0)?)?)],
            value!({}),
        ),
        "toggle-all-float" => call("toggle_workspace_floating", vec![], value!({})),
        "exit" => call("logout", vec![], value!({"graceful":true})),
        _ => return Err(format!("Unknown desktop command: {command}").into()),
    })
}
pub fn run(argv: &[String]) -> Result<i32> {
    let result = (|| -> Result<i32> {
        let request = call(argv)?;
        let mut backend = Backend::new(Paths::environment()?, std::env::vars().collect())?;
        let result = backend.module_call(
            request["method"].as_str().ok_or("Missing method")?,
            request["args"].as_array().ok_or("Missing args")?,
            &request["kwargs"],
        )?;
        match argv[0].as_str() {
            "valid-window-id" => return Ok(if result == true { 0 } else { 1 }),
            "window-job-token" => println!("{}", result.as_str().ok_or("Invalid job token")?),
            "query" => println!("{}", json::encode(&result)?),
            _ => {}
        }
        Ok(0)
    })();
    match result {
        Ok(code) => Ok(code),
        Err(error) => {
            eprintln!("{error}");
            Ok(2)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_python_wrapper_dispatch_outputs_and_error_exits() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/compositor-compat-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let args = case["request"]["argv"]
                .as_array()
                .ok_or("Missing argv")?
                .iter()
                .map(|value| value.as_str().map(str::to_owned).ok_or("Invalid argv"))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            match call(&args) {
                Ok(call) => assert_eq!(
                    value!([call]),
                    case["observed"]["calls"],
                    "{}",
                    case["request"]["name"]
                ),
                Err(error) => {
                    assert_eq!(error.to_string() + "\n", case["observed"]["stderr"]);
                    assert_eq!(case["observed"]["exit"], 2);
                }
            }
        }
        Ok(())
    }
}
