//! Developer-only module-operation boundary. Not installed with nacre-helper.
//! `native_demo_wallpapers.py` drives the one operation still asserted from Python.
use nacre_helper::{json, paths::Paths, Result};
use serde_json::{json as value, Value};

fn call(helper: &str, operation: &str) -> Result<Value> {
    match (helper, operation) {
        ("demo-wallpapers", "collection") => nacre_helper::demo_wallpapers::collection(
            &Paths::environment()?.assets().join("demo-wallpapers"),
        ),
        _ => Err("Unsupported native module operation".into()),
    }
}
fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let result = if arguments.len() == 3 {
        serde_json::from_str::<Value>(&arguments[2])
            .map_err(Into::into)
            .and_then(|_| call(&arguments[0], &arguments[1]))
    } else {
        Err("Expected helper, operation and JSON request".into())
    };
    let (reply, code) = match result {
        Ok(v) => (v, 0),
        Err(error) => (value!({"error":error.to_string(),"type":"ValueError"}), 1),
    };
    match json::encode(&reply) {
        Ok(encoded) => println!("{encoded}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
    std::process::exit(code);
}
