//! Existing compositor CLI vocabulary mapped to typed native operations.
use super::niri_state::Error;
use serde_json::{json, Map, Value};
use std::io::{BufRead, Read, Write};
pub enum Command {
    Events,
    Capabilities,
    Query(String),
    Action {
        method: String,
        args: Vec<Value>,
        kwargs: Value,
    },
}
const QUERIES: &[&str] = &[
    "monitors",
    "all_monitors",
    "workspaces",
    "windows",
    "active_window",
    "active_workspace",
    "cursor",
    "instances",
    "bindings",
];
const ACTIONS: &[&str] = &[
    "reload",
    "logout",
    "switch_workspace",
    "focus_window",
    "focus_direction",
    "cycle_window",
    "move_direction",
    "close_window",
    "kill_window",
    "move_window",
    "place_window",
    "move_workspace",
    "fullscreen_window",
    "resize_window",
    "float_window",
    "pin_window",
    "dpms",
    "set_cursor",
    "set_options",
    "configure_monitors",
    "toggle_workspace_floating",
    "notify",
];
fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: "ArgumentError",
        message: message.into(),
    }
}
pub fn parse(argv: &[String]) -> std::result::Result<Command, Error> {
    let command = argv
        .first()
        .ok_or_else(|| invalid("A compositor command is required"))?;
    match command.as_str() {
        "events" if argv.len() == 1 => return Ok(Command::Events),
        "capabilities" if argv.len() == 1 => return Ok(Command::Capabilities),
        "query" if argv.len() == 2 => {
            return if QUERIES.contains(&argv[1].as_str()) {
                Ok(Command::Query(argv[1].clone()))
            } else {
                Err(invalid("Unknown compositor query"))
            }
        }
        "action" => {}
        _ => return Err(invalid("Expected events, capabilities, query or action")),
    }
    let method = argv
        .get(1)
        .filter(|method| ACTIONS.contains(&method.as_str()))
        .ok_or_else(|| invalid("Unknown compositor action"))?;
    let mut options = Map::new();
    let mut index = 2;
    while index < argv.len() {
        let option = argv[index]
            .strip_prefix("--")
            .ok_or_else(|| invalid("Unexpected compositor argument"))?;
        let value = argv
            .get(index + 1)
            .ok_or_else(|| invalid(format!("Missing value for --{option}")))?;
        let value = if ["width", "height", "x", "y", "size", "icon", "timeout-ms"].contains(&option)
        {
            json!(value
                .parse::<i64>()
                .map_err(|_| invalid(format!("Invalid integer for --{option}")))?)
        } else if ["relative", "follow", "current-monitor"].contains(&option) {
            if !["true", "false"].contains(&value.as_str()) {
                return Err(invalid(format!("Expected true or false for --{option}")));
            }
            json!(value)
        } else if [
            "direction",
            "window",
            "workspace",
            "monitor",
            "mode",
            "action",
            "theme",
            "message",
            "color",
            "records",
            "options",
        ]
        .contains(&option)
        {
            json!(value)
        } else {
            return Err(invalid(format!("Unknown compositor option --{option}")));
        };
        options.insert(option.into(), value);
        index += 2;
    }
    let get = |name: &str| options.get(name).cloned().unwrap_or(Value::Null);
    let default = |name: &str, value: &str| {
        options
            .get(name)
            .filter(|value| value.as_str().is_some_and(|value| !value.is_empty()))
            .cloned()
            .unwrap_or(json!(value))
    };
    let mut workspace = get("workspace");
    if workspace
        .as_str()
        .is_some_and(|value| value.starts_with('{'))
    {
        workspace = crate::json::parse_value(workspace.as_str().unwrap_or_default())
            .map_err(|error| invalid(error.to_string()))?;
    }
    let (args, kwargs) = match method.as_str() {
        "reload" | "logout" | "toggle_workspace_floating" => (vec![], json!({})),
        "switch_workspace" => (
            vec![workspace],
            json!({"current_monitor":get("current-monitor").as_str().unwrap_or("true")=="true"}),
        ),
        "move_workspace" => (vec![workspace, get("monitor")], json!({})),
        "move_window" => (
            vec![default("window", "")],
            json!({"workspace":workspace,"follow":get("follow")=="true","x":get("x"),"y":get("y")}),
        ),
        "fullscreen_window" => (
            vec![default("window", "")],
            json!({"mode":default("mode","fullscreen")}),
        ),
        "resize_window" => (
            vec![default("window", ""), get("width"), get("height")],
            json!({"relative":get("relative")=="true"}),
        ),
        "focus_direction" | "move_direction" | "cycle_window" => {
            (vec![get("direction")], json!({}))
        }
        "place_window" => (
            vec![default("window", "")],
            json!({"x":get("x"),"y":get("y"),"width":get("width"),"height":get("height")}),
        ),
        "focus_window" | "close_window" | "kill_window" => (vec![default("window", "")], json!({})),
        "float_window" | "pin_window" => (
            vec![default("window", "")],
            json!({"action":default("action","toggle")}),
        ),
        "dpms" => (vec![get("action"), get("monitor")], json!({})),
        "set_cursor" => (vec![get("theme"), get("size")], json!({})),
        "set_options" | "configure_monitors" => {
            let key = if method == "set_options" {
                "options"
            } else {
                "records"
            };
            let value = get(key);
            let value = crate::json::parse_value(
                value
                    .as_str()
                    .ok_or_else(|| invalid(format!("Missing --{key}")))?,
            )
            .map_err(|error| invalid(error.to_string()))?;
            (vec![value], json!({}))
        }
        "notify" => (
            vec![
                get("icon"),
                get("timeout-ms"),
                default("color", "0"),
                get("message"),
            ],
            json!({}),
        ),
        _ => return Err(invalid("Unknown compositor action")),
    };
    Ok(Command::Action {
        method: method.clone(),
        args,
        kwargs,
    })
}
pub fn run(argv: &[String]) -> crate::Result<i32> {
    if argv == ["serve-api"] {
        return serve_api();
    }
    if argv == ["serve-actions"] {
        return serve_actions();
    }
    let command = match parse(argv) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("compositor: {}", error.message);
            return Ok(2);
        }
    };
    let mut backend = super::backend::Backend::new(
        crate::paths::Paths::environment()?,
        std::env::vars().collect(),
    )?;
    match command {
        Command::Capabilities => println!(
            "{}",
            crate::json::encode(
                &json!({"backend":backend.name(),"capabilities":backend.capabilities()})
            )?
        ),
        Command::Query(name) => println!(
            "{}",
            crate::json::encode(&backend.query(
                &name,
                std::time::Duration::from_secs(10),
                None
            )?)?
        ),
        Command::Action {
            method,
            args,
            kwargs,
        } => {
            backend.action(&method, &args, &kwargs)?;
        }
        Command::Events => match backend {
            super::backend::Backend::Hyprland(mut backend) => {
                let mut events = backend.events()?;
                while let Some(value) = events.next_value()? {
                    println!("{}", crate::json::encode(&value)?);
                    std::io::stdout().flush()?;
                }
            }
            super::backend::Backend::Niri(backend) => {
                let mut events = backend.events()?;
                loop {
                    println!("{}", crate::json::encode(&events.next_value()?)?);
                    std::io::stdout().flush()?;
                }
            }
        },
    }
    Ok(0)
}
pub fn module_response(backend: &mut super::backend::Backend, request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let result = (|| -> std::result::Result<Value, Error> {
        let operation = request["operation"]
            .as_str()
            .ok_or_else(|| invalid("Expected compositor operation"))?;
        let args = request["args"]
            .as_array()
            .ok_or_else(|| invalid("Expected compositor arguments"))?;
        backend.module_call(operation, args, request.get("kwargs").unwrap_or(&json!({})))
    })();
    match result {
        Ok(value) => json!({"id":id,"value":value}),
        Err(error) => json!({"id":id,"error":error.message,"type":error.kind}),
    }
}
fn serve_api() -> crate::Result<i32> {
    let mut backend = super::backend::Backend::new(
        crate::paths::Paths::environment()?,
        std::env::vars().collect(),
    )?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut line = Vec::new();
        let count = (&mut input)
            .take(4 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(0);
        }
        if line.len() > 4 * 1024 * 1024 || line.last() != Some(&b'\n') {
            return Err("Truncated or oversized compositor API request".into());
        }
        let response = match std::str::from_utf8(&line)
            .ok()
            .and_then(|text| crate::json::parse_value(text).ok())
        {
            Some(request) => module_response(&mut backend, &request),
            None => json!({"id":null,"error":"Invalid compositor API JSON","type":"ValueError"}),
        };
        println!("{}", crate::json::encode(&response)?);
        std::io::stdout().flush()?;
    }
}
pub fn action_response(backend: &mut super::backend::Backend, request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let result = (|| -> crate::Result<()> {
        let argv = request["argv"]
            .as_array()
            .ok_or("Expected action arguments")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("Action arguments must be strings")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let Command::Action {
            method,
            args,
            kwargs,
        } = parse(&argv)?
        else {
            return Err("Expected a compositor action".into());
        };
        backend.action(&method, &args, &kwargs)?;
        Ok(())
    })();
    match result {
        Ok(()) => json!({"id":id,"ok":true}),
        Err(error) => json!({"id":id,"ok":false,"error":error.to_string()}),
    }
}
fn serve_actions() -> crate::Result<i32> {
    let mut backend = super::backend::Backend::new(
        crate::paths::Paths::environment()?,
        std::env::vars().collect(),
    )?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut line = Vec::new();
        let count = (&mut input)
            .take(4 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(0);
        }
        if line.len() > 4 * 1024 * 1024 || line.last() != Some(&b'\n') {
            return Err("Truncated or oversized compositor action request".into());
        }
        let response = match std::str::from_utf8(&line)
            .ok()
            .and_then(|text| crate::json::parse_value(text).ok())
        {
            Some(request) => action_response(&mut backend, &request),
            None => json!({"id":null,"ok":false,"error":"Invalid compositor action JSON"}),
        };
        println!("{}", crate::json::encode(&response)?);
        std::io::stdout().flush()?;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn module_api_retains_frozen_metadata_and_rendering_without_ipc() -> crate::Result<()> {
        for (name, source) in [
            (
                "niri",
                include_str!("../../tests/fixtures/niri-config-query-reference.json"),
            ),
            (
                "hyprland",
                include_str!("../../tests/fixtures/hyprland-backend-reference.json"),
            ),
        ] {
            let reference: Value = serde_json::from_str(source)?;
            let mut backend = super::super::backend::Backend::new(
                crate::paths::Paths::new("/case/home"),
                std::collections::BTreeMap::from([("NACRE_COMPOSITOR".into(), name.into())]),
            )?;
            for case in reference["cases"]
                .as_array()
                .ok_or("Missing API references")?
            {
                let input = &case["request"];
                let operation = input["method"].as_str().ok_or("Missing method")?;
                if ![
                    "render_keyboard",
                    "render_desktop_options",
                    "render_monitors",
                    "config_path",
                    "valid_window_id",
                    "window_job_token",
                    "legacy_power_binding",
                    "capabilities",
                ]
                .contains(&operation)
                {
                    continue;
                }
                let reply = module_response(
                    &mut backend,
                    &json!({"id":"fixture","operation":operation,"args":input["args"],"kwargs":input["kwargs"]}),
                );
                assert_eq!(reply["id"], "fixture");
                let value = if reply.get("error").is_some() {
                    json!({"error":reply["error"],"type":reply["type"]})
                } else {
                    reply["value"].clone()
                };
                assert_eq!(
                    crate::json::encode(&value)? + "\n",
                    case["observed"]["stdout"],
                    "{name}: {}",
                    input["name"]
                );
            }
        }
        Ok(())
    }
    #[test]
    fn action_server_reuses_one_niri_connection_and_preserves_ids_and_failures() -> crate::Result<()>
    {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::net::UnixListener,
        };
        let home = tempfile::tempdir()?;
        let path = home.path().join("niri.sock");
        let listener = UnixListener::bind(&path)?;
        let peer = std::thread::spawn(move || -> crate::Result<()> {
            let (socket, _) = listener.accept()?;
            let mut socket = BufReader::new(socket);
            for (id, reply) in [
                (1, "{\"Ok\":\"Handled\"}"),
                (2, "{\"Ok\":\"Handled\"}"),
                (3, "{\"Err\":\"fixture rejection\"}"),
            ] {
                let mut line = String::new();
                socket.read_line(&mut line)?;
                let request: Value = serde_json::from_str(&line)?;
                assert_eq!(request["Action"]["CloseWindow"]["id"], id);
                writeln!(socket.get_mut(), "{reply}")?;
            }
            Ok(())
        });
        let mut backend = super::super::backend::Backend::new(
            crate::paths::Paths::new(home.path()),
            std::collections::BTreeMap::from([
                ("NACRE_COMPOSITOR".into(), "niri".into()),
                ("NIRI_SOCKET".into(), path.to_string_lossy().into()),
            ]),
        )?;
        for id in 1..=3 {
            let response = action_response(
                &mut backend,
                &json!({"id":format!("request-{id}"),"argv":["action","close_window","--window",format!("niri-window:{id}")]}),
            );
            assert_eq!(response["id"], format!("request-{id}"));
            assert_eq!(response["ok"], id != 3);
            if id == 3 {
                assert!(response["error"]
                    .as_str()
                    .ok_or("Missing error")?
                    .contains("fixture rejection"));
            }
        }
        let invalid = action_response(&mut backend, &json!({"id":4,"argv":["query","windows"]}));
        assert_eq!(invalid["ok"], false);
        peer.join().map_err(|_| "Action server peer panicked")??;
        Ok(())
    }
    #[test]
    fn frozen_python_cli_dispatch_defaults_and_rejections() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/compositor-cli-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing CLI cases")? {
            let argv = case["request"]["argv"]
                .as_array()
                .ok_or("Missing argv")?
                .iter()
                .map(|value| value.as_str().map(str::to_owned).ok_or("Malformed argv"))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let command = parse(&argv);
            if case["observed"]["exit"] != 0 {
                assert!(command.is_err(), "{}", case["request"]["name"]);
                continue;
            }
            let calls = match command? {
                Command::Action {
                    method,
                    args,
                    kwargs,
                } => json!([{"method":method,"args":args,"kwargs":kwargs}]),
                Command::Query(name) => json!([{"method":"query","args":[name],"kwargs":{}}]),
                Command::Capabilities => json!([]),
                Command::Events => return Err("Unexpected CLI fixture".into()),
            };
            assert_eq!(
                calls, case["observed"]["calls"],
                "{}",
                case["request"]["name"]
            );
        }
        Ok(())
    }
}
