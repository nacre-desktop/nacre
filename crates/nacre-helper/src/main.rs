//! Multi-call dispatcher; installation switches callers only after contract parity.
use nacre_helper::{
    dashboard_data, dashboard_media, dashboard_system, json, launcher_preferences, light_devices,
    menu, menu_cli, network_state, notification_history, resource_state, service,
    window_chat_title, Result,
};
use serde_json::Value;
use std::io::{self, BufRead};

fn run() -> Result<i32> {
    let mut arguments = std::env::args().skip(1);
    let tool = arguments.next().ok_or("Expected a helper command")?;
    let mut arguments: Vec<String> = arguments.collect();
    if tool == "compositor-compat" {
        return nacre_helper::compositor_compat::run(&arguments);
    }
    if tool == "window-hide" {
        return nacre_helper::window_hide::run(&arguments);
    }
    if tool == "niri_widths" {
        return nacre_helper::niri_widths::cli::run(&arguments);
    }
    if tool == "niri-session-gate" {
        return nacre_helper::niri_session_gate::run(&arguments);
    }
    if tool == "compositor-render" {
        if arguments.len() != 3 {
            return Err("Expected backend, render operation and JSON values".into());
        }
        let value: Value = serde_json::from_str(&arguments[2])?;
        let result = match (arguments[0].as_str(), arguments[1].as_str()) {
            ("niri", "render_keyboard") => nacre_helper::compositor::niri_config::keyboard(&value),
            ("niri", "render_desktop_options") => {
                nacre_helper::compositor::niri_config::desktop(&value)
            }
            ("niri", "render_monitors") => nacre_helper::compositor::niri_config::monitors(&value),
            ("hyprland", "render_keyboard") => {
                nacre_helper::compositor::hyprland::render_keyboard(&value)
            }
            ("hyprland", "render_desktop_options") => {
                nacre_helper::compositor::hyprland::render_desktop(&value)
            }
            ("hyprland", "render_monitors") => {
                nacre_helper::compositor::hyprland::render_monitors(&value)
            }
            _ => return Err("Unknown compositor rendering operation".into()),
        };
        match result {
            Ok(value) => {
                println!("{}", json::encode(&value)?);
                return Ok(0);
            }
            Err(error) => {
                println!(
                    "{}",
                    json::encode(&serde_json::json!({"error":error.message,"type":error.kind}))?
                );
                return Ok(1);
            }
        }
    }
    if tool == "pearl-state" {
        return nacre_helper::pearl_state::run(&arguments);
    }
    if tool == "pearl-window" {
        return nacre_helper::pearl_window::run(&arguments);
    }
    if tool == "compositor" {
        return nacre_helper::compositor::cli::run(&arguments);
    }
    if let Some(module) = match tool.as_str() {
        "compositor_config" => Some("neutral"),
        "hyprland_config" | "compositor-backends-hyprland-config" => Some("hyprland"),
        "niri_config" | "compositor-backends-niri-config" => Some("niri"),
        _ => None,
    } {
        if arguments.is_empty() {
            return Ok(0);
        }
        if arguments.len() != 2 {
            return Err("Expected compiler operation and JSON arguments".into());
        }
        let request: Value = serde_json::from_str(&arguments[1])?;
        let result = nacre_helper::compositor_config::execute(
            module,
            &arguments[0],
            &request,
            &nacre_helper::paths::Paths::environment()?.assets(),
        );
        let (value, code) = match result {
            Ok(value) => (value, 0),
            Err(error) => (
                serde_json::json!({"error":error.message,"type":error.kind}),
                1,
            ),
        };
        println!("{}", json::encode(&value)?);
        return Ok(code);
    }
    if tool == "launcher-workflows" {
        use std::io::Read;
        let input = if arguments
            .first()
            .is_some_and(|a| a == "save" || a == "activate" || a == "record")
        {
            let mut line = String::new();
            io::stdin().take(2_097_153).read_to_string(&mut line)?;
            if line.len() > 2_097_152 {
                return Err("Launcher input is too large".into());
            }
            Some(serde_json::from_str(&line)?)
        } else {
            None
        };
        let (value, code) = match nacre_helper::launcher_workflows::execute(&arguments, input) {
            Ok(value) => (value, 0),
            Err(error) => (serde_json::json!({"error":error.to_string()}), 1),
        };
        println!("{}", json::encode(&value)?);
        return Ok(code);
    }
    if tool == "diagnostics" && (arguments == ["report"] || arguments == ["report", "--json"]) {
        print!(
            "{}",
            nacre_helper::diagnostics_native::report(arguments.len() == 2)?
        );
        return Ok(0);
    }
    if tool == "dashboard-data" || tool == "launcher-preferences" {
        arguments = match nacre_helper::arguments::hosted(&arguments, &tool) {
            Ok(Some(arguments)) => arguments,
            Ok(None) => return Ok(0),
            Err(code) => return Ok(code),
        };
    }
    if tool == "notification-history" || tool == "dashboard-media" {
        if arguments.is_empty() {
            eprintln!("IndexError: list index out of range");
            return Ok(1);
        }
        arguments.truncate(1);
    }
    if tool == "menu-backend" {
        arguments = match nacre_helper::menu_arguments::parse(&arguments) {
            Ok(Some(arguments)) => arguments,
            Ok(None) => return Ok(0),
            Err(code) => return Ok(code),
        };
        if arguments.first().is_some_and(|action| action == "tool")
            && arguments
                .get(1)
                .is_some_and(|name| name == "logs" || name == "updates")
        {
            return match nacre_helper::menu_actions::run_terminal_tool(&arguments[1]) {
                Ok(code) => Ok(code),
                Err(error) => {
                    println!(
                        "{}",
                        json::encode(
                            &serde_json::json!({"error":error.to_string().chars().take(240).collect::<String>()})
                        )?
                    );
                    Ok(1)
                }
            };
        }
    }
    if tool == "network-state" && arguments == ["watch"] {
        return network_state::watch();
    }
    if tool == "dashboard-media" && arguments.first().is_some_and(|arg| arg == "spectrum") {
        return dashboard_media::spectrum();
    }
    if tool == "menu-cli" {
        return menu_cli::run(&arguments);
    }
    if tool == "menu-backend" && arguments.first().is_some_and(|action| action == "config") {
        return match nacre_helper::menu_actions::edit_config(
            arguments.get(1).map(String::as_str).unwrap_or(""),
        ) {
            Ok(code) => Ok(code),
            Err(error) => {
                println!(
                    "{}",
                    json::encode(&serde_json::json!({"error":error.to_string()}))?
                );
                Ok(1)
            }
        };
    }
    if tool == "light-devices" {
        return light_devices::run(&arguments);
    }
    if (tool == "serve" || tool == "helper-service") && arguments.is_empty() {
        return service::serve(
            io::stdin().lock(),
            io::stdout(),
            service::Limits::default(),
            |request| {
                let value = match request.tool.as_str() {
                    "notification-history"
                        if request.arguments.len() == 1
                            && ((request.arguments[0] == "load" && request.input.is_none())
                                || (request.arguments[0] == "save"
                                    && request.input.as_ref().is_some_and(Value::is_array))) =>
                    {
                        notification_history::execute(&request.arguments, request.input.clone())?
                    }
                    "launcher-preferences"
                        if request.arguments.len() == 1
                            && ((request.arguments[0] == "load" && request.input.is_none())
                                || (["favorite", "hide", "record"]
                                    .contains(&request.arguments[0].as_str())
                                    && request.input.as_ref().is_some_and(Value::is_object))) =>
                    {
                        launcher_preferences::execute(&request.arguments, request.input.clone())?
                    }
                    "dashboard-data"
                        if (1..=2).contains(&request.arguments.len())
                            && request.input.is_none() =>
                    {
                        dashboard_data::execute(&request.arguments, None)?
                    }
                    "network-state" if request.arguments.is_empty() && request.input.is_none() => {
                        network_state::execute(&request.arguments, None)?
                    }
                    "menu-backend"
                        if request.input.is_none()
                            && ((request.arguments.len() == 1
                                && request.arguments[0] == "state")
                                || (request.arguments.len() == 2
                                    && ((request.arguments[0] == "state"
                                        && request.arguments[1] == "--force")
                                        || ["files", "convert", "packages", "ownership"]
                                            .contains(&request.arguments[0].as_str())))) =>
                    {
                        menu::execute(&request.arguments, None)?
                    }
                    "dashboard-media"
                        if request.arguments.len() == 1 && request.input.is_none() =>
                    {
                        dashboard_media::execute(&request.arguments, None)?
                    }
                    _ => return Err("Unsupported helper operation".into()),
                };
                match value {
                    Some(value) => Ok(format!("{}\n", json::encode(&value)?)),
                    None => Ok(String::new()),
                }
            },
        );
    }
    if tool == "menu-backend"
        && arguments
            .first()
            .is_some_and(|action| action == "install" || action == "remove")
    {
        let command = nacre_helper::menu_registry::package_command(
            &arguments[0],
            arguments.get(1).map(String::as_str).unwrap_or(""),
        );
        return match command {
            Ok(command) => {
                use std::os::unix::process::ExitStatusExt;
                let status = std::process::Command::new(&command[0])
                    .args(&command[1..])
                    .status()?;
                Ok(status
                    .code()
                    .unwrap_or_else(|| -status.signal().unwrap_or(1)))
            }
            Err(error) => {
                println!(
                    "{}",
                    json::encode(&serde_json::json!({"error":error.to_string()}))?
                );
                Ok(1)
            }
        };
    }
    let input = if (tool == "notification-history"
        && arguments.first().is_some_and(|a| a == "save"))
        || (tool == "launcher-preferences" && arguments.first().is_some_and(|a| a != "load"))
        || (tool == "menu-backend"
            && arguments
                .first()
                .is_some_and(|a| a == "webapp-create" || a == "reminder"))
    {
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        Some(serde_json::from_str::<Value>(&line)?)
    } else {
        None
    };
    let result = match tool.as_str() {
        "notification-history" => match notification_history::execute(&arguments, input) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("ValueError: {error}");
                return Ok(1);
            }
        },
        "launcher-preferences" => match launcher_preferences::execute(&arguments, input) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "{}",
                    json::encode(&serde_json::json!({"error":error.to_string()}))?
                );
                return Ok(1);
            }
        },
        "dashboard-data" => match dashboard_data::execute(&arguments, None) {
            Ok(value) => value,
            Err(error) if error.is::<dashboard_data::DashboardAttribute>() => {
                eprintln!("AttributeError: {error}");
                return Ok(1);
            }
            Err(error) => {
                println!(
                    "{}",
                    json::encode(
                        &serde_json::json!({"error":error.to_string().chars().take(240).collect::<String>()})
                    )?
                );
                return Ok(1);
            }
        },
        "network-state" => match network_state::execute(&arguments, None) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "{}",
                    json::encode(&serde_json::json!({"error":error.to_string()}))?
                );
                return Ok(1);
            }
        },
        "menu-backend" => match menu::execute(&arguments, input) {
            Ok(value) => value,
            Err(error) if error.is::<nacre_helper::menu_registry::ReminderTypeError>() => {
                return Err(error)
            }
            Err(error) => {
                println!(
                    "{}",
                    json::encode(&serde_json::json!({"error":error.to_string()}))?
                );
                return Ok(1);
            }
        },
        "window-chat-title" => window_chat_title::execute(&arguments, None)?,
        "dashboard-system" => dashboard_system::execute(&arguments, None)?,
        "resource-state" => resource_state::execute(&arguments, None)?,
        "dashboard-media" => match dashboard_media::execute(&arguments, None) {
            Ok(value) => value,
            Err(error) if error.is::<dashboard_media::Attribute>() => return Err(error),
            Err(_) => {
                println!(
                    "{}",
                    json::encode(
                        &serde_json::json!({"lines":[],"palette":{},"lyricsMessage":"Media details unavailable"})
                    )?
                );
                return Ok(0);
            }
        },
        _ => return Err("Unsupported helper".into()),
    };
    if let Some(result) = result {
        println!("{}", json::encode(&result)?);
    }
    Ok(0)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if error.is::<nacre_helper::menu_registry::ReminderTypeError>() {
                eprintln!("TypeError: {error}");
            } else {
                eprintln!("{error}");
            }
            std::process::exit(1);
        }
    }
}
