//! Developer-only fixed clock and module boundary; never installed.
use nacre_helper::{shell_supervisor as supervisor, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let root = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share/nacre/shell");
    let result: Result<Value> = if mode.starts_with("env-") {
        supervisor::environment(std::env::vars().collect(), Duration::ZERO, None).map(|env| {
            let mut output = String::from("{");
            for (index, name) in [
                "WAYLAND_DISPLAY",
                "XDG_SESSION_TYPE",
                "KEEP_ME",
                "NACRE_COMPOSITOR",
            ]
            .iter()
            .enumerate()
            {
                if index > 0 {
                    output.push_str(", ");
                }
                output.push_str(&format!(
                    "\"{name}\": {}",
                    nacre_helper::json::dumps(&json!(env.get(*name)), false).unwrap()
                ));
            }
            output.push('}');
            println!("{output}");
            Value::Null
        })
    } else if mode == "restore" {
        let root = PathBuf::from(std::env::args().nth(2).unwrap());
        supervisor::restore(&root, "20261001T120000").map(|v| {
            println!("{v}");
            Value::Null
        })
    } else if mode == "health-module" {
        let root = PathBuf::from(std::env::args().nth(2).unwrap());
        let env = serde_json::from_str(&std::env::args().nth(3).unwrap()).unwrap();
        supervisor::health(&root, &env).map(|v| {
            println!("{v}");
            Value::Null
        })
    } else if mode.starts_with("health-") {
        let env: BTreeMap<_, _> = std::env::vars().collect();
        supervisor::health(&root, &env).map(|v| {
            println!("{v}");
            Value::Null
        })
    } else {
        match supervisor::run(Some("20261001T120000")) {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    };
    if let Err(error) = result {
        println!(
            "{{\"error\": {}, \"type\": \"RuntimeError\"}}",
            nacre_helper::json::dumps(&json!(error.to_string()), false).unwrap()
        );
        std::process::exit(1);
    }
}
