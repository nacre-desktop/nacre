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
    let name = std::env::args_os().next().and_then(|path| {
        std::path::Path::new(&path)
            .file_name()
            .map(|name| name.to_owned())
    });
    let invoked = name.as_deref().and_then(std::ffi::OsStr::to_str);
    let tool = match invoked {
        Some("nacre-brain-ui") => {
            return nacre_helper::brain_web::run("brain-launch", &arguments.collect::<Vec<_>>())
        }
        Some("nacrectl") => "ctl".to_owned(),
        Some("nacre-daemon") => "daemon".to_owned(),
        Some("nacre-desktop-extras") => "desktop-extras".to_owned(),
        Some("nacre-maintenance") => "maintenance".to_owned(),
        Some("nacre-welcome-state") => "welcome".to_owned(),
        Some("nacre-brain" | "nacre-brain-sync" | "nacre-brain-maintain") => {
            let command = name.as_deref().unwrap().to_string_lossy().into_owned();
            return nacre_helper::brain_data::run(
                &std::iter::once(command)
                    .chain(arguments)
                    .collect::<Vec<_>>(),
            );
        }
        Some("nacre-ai-context" | "nacre-ai-memory" | "nacre-ai-usage") => {
            let command = name.as_deref().unwrap().to_string_lossy().into_owned();
            return nacre_helper::ai_core::run(
                &std::iter::once(command)
                    .chain(arguments)
                    .collect::<Vec<_>>(),
            );
        }
        Some("nacre-ai-skills" | "nacre-ai-tools" | "nacre-ai-account" | "nacre-ai-remote") => {
            return nacre_helper::ai_wrappers::dispatch(
                invoked.unwrap(),
                &arguments.collect::<Vec<_>>(),
            );
        }
        Some(
            "brain-control" | "brain-auth" | "brain-discovery" | "brain-launch" | "composer"
            | "sidebar-chat" | "nacre-app" | "nacre-settings" | "nacre-welcome" | "startup-apps"
            | "desktop-extras" | "maintenance" | "welcome",
        ) => invoked.unwrap().to_owned(),
        _ => arguments.next().ok_or("Expected a helper command")?,
    };
    let mut arguments: Vec<String> = arguments.collect();
    if matches!(
        tool.as_str(),
        "brain-control" | "brain-auth" | "brain-discovery" | "brain-launch"
    ) {
        return nacre_helper::brain_web::run(&tool, &arguments);
    }
    if tool == "sidebar-chat" {
        return nacre_helper::sidebar_chat::run(&arguments);
    }
    if tool == "composer" {
        use std::io::Read;
        let action = arguments.first().ok_or("Expected a composer action")?;
        let request = if matches!(action.as_str(), "pick" | "screenshot") {
            serde_json::json!({})
        } else {
            let mut input = String::new();
            std::io::stdin()
                .lock()
                .take(1024 * 1024 + 1)
                .read_line(&mut input)?;
            if input.len() > 1024 * 1024 {
                return Err("Composer request exceeds limit".into());
            }
            serde_json::from_str(&input)?
        };
        println!(
            "{}",
            json::encode(&nacre_helper::composer::run(action, &request))?
        );
        return Ok(0);
    }
    if matches!(
        tool.as_str(),
        "nacre-ai-skills"
            | "nacre-ai-tools"
            | "nacre-ai-account"
            | "nacre-ai-remote"
            | "ai-credentials-status"
    ) {
        return nacre_helper::ai_wrappers::dispatch(&tool, &arguments);
    }
    if tool == "nacre-codex" {
        return nacre_helper::ai_wrappers::codex(&arguments);
    }
    match tool.as_str() {
        "desktop-actions" => return nacre_helper::desktop_actions::run_cli(&arguments),
        "control-tools" => return nacre_helper::control_tools::run(&arguments),
        "display-recover" => return nacre_helper::display_recover::run(&arguments),
        "device-actions" => return nacre_helper::device_actions::run(&arguments),
        "device-location" => return nacre_helper::device_location::run(&arguments),
        "screenshot" => return nacre_helper::screenshot::run(&arguments),
        "desktop-extras" => return nacre_helper::desktop_extras::run(&arguments),
        "maintenance" => return nacre_helper::maintenance::run(&arguments),
        "welcome" => return nacre_helper::launchers::welcome_helper(&arguments),
        _ => {}
    }
    // These commands use files and child executables, with no zbus runtime.
    // In particular, credential launches must inherit the caller's environment.
    if matches!(
        tool.as_str(),
        "nacre-brain" | "nacre-brain-sync" | "nacre-brain-maintain"
    ) {
        let call = std::iter::once(tool).chain(arguments).collect::<Vec<_>>();
        return nacre_helper::brain_data::run(&call);
    }
    if matches!(
        tool.as_str(),
        "nacre-ai-context" | "nacre-ai-memory" | "nacre-ai-usage" | "ai-core"
    ) {
        return nacre_helper::ai_core::run(
            &std::iter::once(tool).chain(arguments).collect::<Vec<_>>(),
        );
    }
    let inherited_tokio_threads = std::env::var_os("TOKIO_WORKER_THREADS").is_some();
    if !matches!(
        tool.as_str(),
        "shell-supervisor" | "shell-environment" | "ctl" | "daemon"
    ) && std::env::var_os("TOKIO_WORKER_THREADS").is_none()
    {
        // zbus's blocking compatibility API owns a Tokio runtime when Tokio is
        // enabled. Bound that fallback pool before any helper threads start;
        // the daemon itself always uses its explicit current-thread runtime.
        // The shell's exported environment and an explicit caller setting stay
        // unchanged.
        std::env::set_var("TOKIO_WORKER_THREADS", "1");
    }
    if tool == "ctl" {
        return nacre_helper::daemon::client::run(&arguments);
    }
    if tool == "daemon" {
        return nacre_helper::daemon::run(&arguments);
    }
    if tool == "daemon-menu-worker" {
        return nacre_helper::daemon::menu_worker::run(&arguments, inherited_tokio_threads);
    }
    if tool == "daemon-notice" {
        return nacre_helper::daemon::workers::notice(&arguments);
    }
    if tool == "session-watch" {
        return nacre_helper::session_watch::run(&arguments);
    }
    if tool == "nacre-app" {
        return nacre_helper::launchers::app(&arguments);
    }
    if tool == "nacre-settings" {
        return match nacre_helper::launchers::settings(&arguments) {
            Ok(code) => Ok(code),
            Err(error) => {
                eprintln!("nacre-settings: {error}");
                Ok(1)
            }
        };
    }
    if tool == "nacre-welcome" {
        return match nacre_helper::launchers::welcome(&arguments) {
            Ok(code) => Ok(code),
            Err(error) => {
                eprintln!("nacre-welcome: {error}");
                Ok(1)
            }
        };
    }
    if tool == "startup-apps" {
        return nacre_helper::launchers::startup_apps(&arguments);
    }
    if tool == "shell-supervisor" {
        return nacre_helper::shell_supervisor::run(None);
    }
    if tool == "shell-environment" {
        let initial: nacre_helper::shell_supervisor::Environment = std::env::vars().collect();
        let environment = nacre_helper::shell_supervisor::environment(
            initial.clone(),
            std::time::Duration::from_secs(20),
            None,
        )?;
        let updates: std::collections::BTreeMap<_, _> = environment
            .into_iter()
            .filter(|(key, value)| initial.get(key) != Some(value))
            .collect();
        println!("{}", serde_json::to_string(&updates)?);
        return Ok(0);
    }
    if tool == "lock-info" {
        return nacre_helper::lock_info::run(&arguments);
    }
    if tool == "weather" {
        return nacre_helper::weather::run(&arguments);
    }
    if tool == "updates" {
        return nacre_helper::updates::run(&arguments);
    }
    if tool == "lock-prepare" {
        return nacre_helper::lock_prepare::run(&arguments);
    }
    if tool == "monitor-workspaces" {
        return nacre_helper::compositor::niri_workspaces::run(&arguments);
    }
    if tool == "wallpaper-media" {
        return nacre_helper::wallpaper_media::run(&arguments);
    }
    if tool == "software-package" {
        let [action, package] = arguments.as_slice() else {
            return Err("Expected software-package install PACKAGE".into());
        };
        if action != "install" {
            return Err("Expected software-package install PACKAGE".into());
        }
        return nacre_helper::aur_review::package_install(package);
    }
    if tool == "aur-review" {
        return match arguments.as_slice() {
            [action] if action == "policy" => {
                println!(
                    "{}",
                    json::encode(&nacre_helper::aur_review::policy(
                        &nacre_helper::paths::Paths::environment()?
                    )?)?
                );
                Ok(0)
            }
            [action, package] if action == "install" => nacre_helper::aur_review::install(package),
            [action, directory] if action == "inspect" => {
                nacre_helper::aur_review::inspect(std::path::Path::new(directory))
            }
            _ => Err("Expected aur-review policy or install PACKAGE".into()),
        };
    }
    if tool == "wallpaper-watch" {
        return nacre_helper::wallpaper_watch::run(&arguments);
    }
    if tool == "nacre-colors" {
        return nacre_helper::colors_launch::run(&arguments);
    }
    if tool == "palette-api" {
        return nacre_helper::palette_api::run(&arguments);
    }
    if tool == "app-theme" {
        let [id, value] = arguments.as_slice() else {
            return Err("Expected theme target and true/false".into());
        };
        let enabled = match value.as_str() {
            "true" => true,
            "false" => false,
            _ => return Err("Expected true or false".into()),
        };
        let mut publisher = nacre_helper::classic_state::Publisher::environment()?;
        publisher.set_theme_target(id, enabled)?;
        println!("{}", serde_json::json!({"ok":true}));
        return Ok(0);
    }
    if tool == "classic-state" {
        let mut publisher = nacre_helper::classic_state::Publisher::environment()?;
        publisher.apply_palette(arguments.first().map(std::path::Path::new), true)?;
        return Ok(0);
    }
    if tool == "lock-config" {
        nacre_helper::palette_api::call(
            "lock-config",
            "publish",
            &nacre_helper::palette_api::Request::default(),
        )?;
        return Ok(0);
    }
    if tool == "file-icons" || tool == "lock-dashboard" {
        return Ok(0);
    }
    if tool == "nacre_shell" {
        return nacre_helper::palette_cli::run(&arguments);
    }
    if tool == "colors" {
        return nacre_helper::colors::run(&arguments);
    }
    if tool == "branding" {
        return nacre_helper::branding::run(&arguments);
    }
    if tool == "demo-wallpapers" {
        return nacre_helper::demo_wallpapers::run(&arguments);
    }
    if tool == "login-appearance" {
        return nacre_helper::login_appearance::run(&arguments);
    }
    if tool == "terminal-font" {
        let [family] = arguments.as_slice() else {
            return Err("Expected a font family (empty resets to the default)".into());
        };
        nacre_helper::terminal_font::publish(&nacre_helper::paths::Paths::environment()?, family)?;
        return Ok(0);
    }
    if tool == "desktop-settings" {
        return nacre_helper::desktop_settings::run(&arguments);
    }
    if tool == "terminal-branding" {
        return nacre_helper::terminal_branding::run(&arguments);
    }
    if tool == "kde-palette" {
        return nacre_helper::kde_palette::run(&arguments);
    }
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
    if tool == "niri-session-runtime" {
        return nacre_helper::niri_session_runtime::run(&arguments);
    }
    if tool == "niri-acceptance-monitor" {
        return nacre_helper::niri_acceptance_monitor::run(&arguments);
    }
    if tool == "niri-preview" {
        return nacre_helper::niri_preview::run(&arguments);
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
    if tool == "input-preferences" {
        let paths = nacre_helper::paths::Paths::environment()?;
        let result = match arguments.as_slice() {
            [operation] if operation == "state" => {
                nacre_helper::software_input::preferences(&paths.config)
            }
            [operation, value] if operation == "layouts" => {
                if value.is_empty() {
                    nacre_helper::software_input::reset_layouts(&paths)
                } else {
                    nacre_helper::software_input::set_layouts(&paths, value)
                }
            }
            [operation, value]
                if operation == "touchpad" && (value == "true" || value == "false") =>
            {
                let backend = nacre_helper::compositor::backend::Backend::new(
                    paths.clone(),
                    std::env::vars().collect(),
                )?;
                let devices = if backend.name() == "hyprland" {
                    nacre_helper::software_actions::touchpads()
                } else {
                    Vec::new()
                };
                if backend.name() == "hyprland" && devices.is_empty() {
                    Err("No touchpad identity is available".into())
                } else {
                    nacre_helper::software_input::set_touchpad(&paths, value == "true", devices)
                }
            }
            _ => Err("Unknown input preference request".into()),
        };
        let (value, code) = match result {
            Ok(data) => (serde_json::json!({"ok":true,"data":data}), 0),
            Err(error) => (serde_json::json!({"ok":false,"error":error.to_string()}), 1),
        };
        println!("{}", json::encode(&value)?);
        return Ok(code);
    }
    if tool == "state-file" {
        return nacre_helper::state::run(&arguments);
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
    if tool == "software-environments" {
        let paths = nacre_helper::paths::Paths::environment()?;
        let operation = arguments.first().map(String::as_str).unwrap_or("load");
        if operation == "run" {
            if arguments.len() != 3 {
                return Err("Expected run install/remove environment ID".into());
            }
            return nacre_helper::software_environments::run(&paths, &arguments[1], &arguments[2]);
        }
        let result = match operation {
            "load" if arguments.len() <= 1 => {
                Ok(nacre_helper::software_environments::snapshot(&paths))
            }
            "plan" if arguments.len() == 3 => {
                nacre_helper::software_environments::plan(&paths, &arguments[1], &arguments[2])
            }
            _ => Err("Unknown development command".into()),
        };
        let (value, code) = match result {
            Ok(value) => (value, 0),
            Err(error) => (serde_json::json!({"error":error.to_string()}), 1),
        };
        println!("{}", json::encode(&value)?);
        return Ok(code);
    }
    if tool == "software-actions" {
        let action = arguments.first().map(String::as_str).unwrap_or("load");
        let id = arguments.get(1).map(String::as_str).unwrap_or("");
        if action == "run" {
            return nacre_helper::software_actions::run_health(id);
        }
        let result = match action {
            "load" => Ok(nacre_helper::software_actions::snapshot()),
            "plan" => {
                let paths = nacre_helper::paths::Paths::environment()?;
                let setup = nacre_helper::software_setup::facts(&paths);
                let hardware = nacre_helper::software_hardware::facts(&paths);
                let speed = nacre_helper::software_speed::snapshot();
                if nacre_helper::software_speed::rows(&speed)
                    .iter()
                    .any(|r| r["id"] == id)
                {
                    let value = nacre_helper::software_speed::plan(id, &speed)?;
                    println!("{}", json::encode(&value)?);
                    return Ok(0);
                }
                if nacre_helper::software_hardware::rows(&hardware)
                    .iter()
                    .any(|r| r["id"] == id)
                {
                    return {
                        let value = nacre_helper::software_hardware::plan(id, &hardware)?;
                        println!("{}", json::encode(&value)?);
                        Ok(0)
                    };
                }
                if nacre_helper::software_setup::rows(&setup)
                    .iter()
                    .any(|r| r["id"] == id)
                {
                    let request =
                        serde_json::from_str(arguments.get(2).map(String::as_str).unwrap_or("{}"))?;
                    nacre_helper::software_setup::plan(id, &setup, &request)
                } else {
                    nacre_helper::software_actions::health_plan(
                        id,
                        &nacre_helper::software_actions::facts(),
                    )
                }
            }
            _ => Err("Unknown system action".into()),
        };
        let (value, code) = match result {
            Ok(value) => (value, 0),
            Err(error) => (serde_json::json!({"error":error.to_string()}), 1),
        };
        println!("{}", json::encode(&value)?);
        return Ok(code);
    }
    if tool == "software-tui" {
        use std::io::Read;
        let paths = nacre_helper::paths::Paths::environment()?;
        let action = arguments.first().map(String::as_str).unwrap_or("load");
        let id = arguments.get(1).map(String::as_str).unwrap_or("");
        let input = if action == "create" {
            let mut text = String::new();
            io::stdin().take(262145).read_to_string(&mut text)?;
            if text.len() > 262144 {
                return Err("Terminal app request is too large".into());
            }
            Some(serde_json::from_str(&text)?)
        } else {
            None
        };
        let result = nacre_helper::software_tui::execute(&paths, action, id, input);
        let (value, code) = match result {
            Ok(value) => (value, 0),
            Err(error) => (serde_json::json!({"error":error.to_string()}), 1),
        };
        println!("{}", json::encode(&value)?);
        return Ok(code);
    }
    if tool == "software-library" {
        use std::io::Read;
        let input = if arguments.first().is_some_and(|a| {
            matches!(
                a.as_str(),
                "import"
                    | "clone"
                    | "theme-import"
                    | "theme-export"
                    | "open"
                    | "tui-create"
                    | "bundle-apply"
            )
        }) {
            let mut line = String::new();
            io::stdin().take(262_145).read_to_string(&mut line)?;
            if line.len() > 262_144 {
                return Err("Software library input is too large".into());
            }
            if line.trim().is_empty() {
                None
            } else {
                Some(serde_json::from_str(&line)?)
            }
        } else {
            None
        };
        let (value, code) = match nacre_helper::software_library::execute(&arguments, input) {
            Ok(value) => (value, 0),
            Err(error) => (serde_json::json!({"error":error.to_string()}), 1),
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
        if let Some(code) = nacre_helper::daemon::network::watch()? {
            return Ok(code);
        }
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
        if let Some(code) = nacre_helper::daemon::serve_bridge::run()? {
            return Ok(code);
        }
        return service::serve(
            io::stdin().lock(),
            io::stdout(),
            service::Limits::default(),
            service::execute,
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
