//! Developer-only callbacks preserve original action assertions against Rust.
use nacre_helper::{
    compositor::{
        niri_actions::{Backend, Transport},
        niri_state::Error,
    },
    json, Result,
};
use serde_json::{json as value, Value};
use std::io::{self, BufRead, Write};

struct Callbacks {
    query_override: bool,
}
impl Callbacks {
    fn call(&mut self, request: Value) -> std::result::Result<Value, Error> {
        let exchange = || -> Result<Value> {
            println!("{}", json::encode(&request)?);
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            Ok(serde_json::from_str(&line)?)
        };
        exchange().map_err(|error| Error {
            kind: "RuntimeError",
            message: error.to_string(),
        })
    }
}
impl Transport for Callbacks {
    fn query_override(
        &mut self,
        name: &str,
        timeout: f64,
        max_bytes: usize,
    ) -> Option<std::result::Result<Value, Error>> {
        self.query_override.then(|| {
            self.call(
                value!({"callback":"query","name":name,"timeout":timeout,"max_bytes":max_bytes}),
            )
        })
    }
    fn window(&mut self, window: &str, timeout: f64) -> std::result::Result<Value, Error> {
        self.call(value!({"callback":"window","window":window,"timeout":timeout}))
    }
    fn snapshot(&mut self, timeout: f64, max_bytes: usize) -> std::result::Result<Value, Error> {
        self.call(value!({"callback":"snapshot","timeout":timeout,"max_bytes":max_bytes}))
    }
    fn request(
        &mut self,
        request: Value,
        timeout: f64,
        max_bytes: usize,
    ) -> std::result::Result<Value, Error> {
        self.call(value!({"callback":"request","request":request,"timeout":timeout,"max_bytes":max_bytes}))
    }
}
fn run() -> Result<()> {
    let request: Value =
        serde_json::from_str(&std::env::args().nth(1).ok_or("Expected action request")?)?;
    let mut backend = Backend {
        transport: Callbacks {
            query_override: request["query_override"] == true,
        },
    };
    let result = backend.execute(
        request["method"].as_str().ok_or("Missing method")?,
        request["args"].as_array().ok_or("Missing arguments")?,
        &request["kwargs"],
    );
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
