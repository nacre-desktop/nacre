//! Developer-only original connection lifecycle provider, never installed.
//! Production actions/state/rendering are used directly; no expected output loads.
use nacre_helper::{
    compositor::{
        backend, cli, hyprland, hyprland_events, hyprland_transport, niri,
        niri_actions::{self, Transport},
        niri_config,
        niri_events::{self, Outputs},
        niri_state::{Error, State},
        niri_transport::Connection,
    },
    json, Result,
};
use serde_json::{json as value, Value};
use std::{io::Write, path::Path, time::Duration};
type NativeResult<T> = std::result::Result<T, Error>;
struct OriginalConnections;
impl OriginalConnections {
    fn response(&mut self, name: &str, timeout: Duration, max_bytes: usize) -> NativeResult<Value> {
        Connection::new(std::env::var("NIRI_SOCKET").unwrap_or_default())
            .response(name, timeout, max_bytes)
    }
}
impl Transport for OriginalConnections {
    fn request(&mut self, request: Value, timeout: f64, max_bytes: usize) -> NativeResult<Value> {
        Connection::new(std::env::var("NIRI_SOCKET").unwrap_or_default()).request_value(
            &request,
            Duration::from_secs_f64(timeout),
            max_bytes,
        )
    }
    fn snapshot(&mut self, timeout: f64, max_bytes: usize) -> NativeResult<Value> {
        let timeout = Duration::from_secs_f64(timeout);
        let outputs = self
            .response("Outputs", timeout, max_bytes)?
            .as_object()
            .cloned()
            .ok_or_else(|| Error {
                kind: "ValueError",
                message: "Malformed niri output records".into(),
            })?;
        let mut state = State::new(outputs);
        state.apply(&value!({"WorkspacesChanged":{"workspaces":self.response("Workspaces",timeout,max_bytes)?}}))?;
        state.apply(
            &value!({"WindowsChanged":{"windows":self.response("Windows",timeout,max_bytes)?}}),
        )?;
        state.snapshot()
    }
}
impl Outputs for OriginalConnections {
    fn outputs(&mut self) -> NativeResult<Value> {
        self.response("Outputs", Duration::from_secs(10), 4 * 1024 * 1024)
    }
}
fn print(value: &Value) -> Result<()> {
    println!("{}", json::encode(value)?);
    std::io::stdout().flush()?;
    Ok(())
}
fn module_call(name: &str, operation: &str, input: &Value) -> NativeResult<Value> {
    let args = input["args"].as_array().cloned().unwrap_or_default();
    let kwargs = input.get("kwargs").cloned().unwrap_or(value!({}));
    if name == "hyprland" {
        let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_default();
        let socket = hyprland_transport::Socket {
            folder: Path::new(&runtime).join("hypr/fixture"),
        };
        return hyprland::Backend { transport: socket }.execute(operation, &args, &kwargs);
    }
    match operation {
        "render_keyboard" => niri_config::keyboard(&args[0]).map(Value::from),
        "parse_event" => {
            let event =
                json::parse_value(args[0].as_str().unwrap_or("")).map_err(|error| Error {
                    kind: "JSONDecodeError",
                    message: error.to_string(),
                })?;
            State::default().apply(&event)
        }
        "valid_window_id" => Ok(value!(niri_config::valid_window_id(&args[0]))),
        operation => niri_actions::Backend {
            transport: OriginalConnections,
        }
        .execute(operation, &args, &kwargs),
    }
}
fn run() -> Result<i32> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "module") {
        let input: Value = serde_json::from_str(args.get(3).ok_or("Missing module input")?)?;
        let operation = args[2].strip_prefix("Backend.").unwrap_or(&args[2]);
        let result = module_call(&args[1], operation, &input);
        return match result {
            Ok(value) => {
                print(&value)?;
                Ok(0)
            }
            Err(error) => {
                print(&value!({"error":error.message,"type":error.kind}))?;
                Ok(1)
            }
        };
    }
    let name = backend::select(&std::env::vars().collect())?;
    let command = cli::parse(&args)?;
    if name == "hyprland" {
        let folder = Path::new(&std::env::var("XDG_RUNTIME_DIR")?).join("hypr/fixture");
        let mut backend = hyprland::Backend {
            transport: hyprland_transport::Socket {
                folder: folder.clone(),
            },
        };
        match command {
            cli::Command::Capabilities => {
                print(&value!({"backend":name,"capabilities":hyprland::capabilities()}))?
            }
            cli::Command::Query(query) => print(&backend.query(&query, 10.0, None)?)?,
            cli::Command::Action {
                method,
                args,
                kwargs,
            } => {
                backend.execute(&method, &args, &kwargs)?;
            }
            cli::Command::Events => {
                let mut events =
                    hyprland_events::Events::connect(&folder.join(".socket2.sock"), &mut backend)?;
                while let Some(event) = events.next_value()? {
                    print(&event)?;
                }
            }
        }
    } else {
        let mut backend = niri_actions::Backend {
            transport: OriginalConnections,
        };
        match command {
            cli::Command::Capabilities => {
                print(&value!({"backend":name,"capabilities":niri::capabilities()}))?
            }
            cli::Command::Query(query) => {
                print(&backend.query_bounded(&query, 10.0, 4 * 1024 * 1024)?)?
            }
            cli::Command::Action {
                method,
                args,
                kwargs,
            } => {
                backend.execute(&method, &args, &kwargs)?;
            }
            cli::Command::Events => {
                let socket = niri_events::EventSocket::connect(
                    Path::new(&std::env::var("NIRI_SOCKET")?),
                    true,
                )?;
                let mut events = niri_events::Events::new(socket, OriginalConnections)?;
                loop {
                    print(&events.next_value()?)?;
                }
            }
        }
    }
    Ok(0)
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
