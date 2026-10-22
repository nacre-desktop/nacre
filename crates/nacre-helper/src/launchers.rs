//! Native, standalone keystroke and login launchers.
//!
//! These commands deliberately use the existing compositor client and the
//! existing desktop-entry routes. They do not require the daemon.
use crate::{compositor, paths::Paths, process, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::fs::PermissionsExt,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

const APP_ROUTES: &[(&str, &[&str])] = &[
    ("terminal", &["kitty"]),
    ("browser", &["google-chrome-stable"]),
    ("files", &["nacre-shell", "dolphin"]),
    (
        "mail",
        &["google-chrome-stable", "--app=https://mail.google.com/"],
    ),
    ("emoji", &["nacre-menu", "actions.emoji"]),
];

fn which(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program)
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0);
    }
    env::split_paths(&env::var_os("PATH").unwrap_or_default()).any(|directory| {
        directory
            .join(program)
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
}

fn app_route(paths: &Paths, name: &str) -> Result<Vec<String>> {
    let config = paths.config.join("apps.json");
    let settings: Option<Value> = if config.is_file() {
        Some(serde_json::from_slice(&fs::read(&config)?)?)
    } else {
        None
    };
    if settings.as_ref().is_some_and(|value| !value.is_object()) {
        return Err("'list' object has no attribute 'get'".into());
    }
    let configured = settings
        .as_ref()
        .is_some_and(|value| value.get(name).is_some());
    let mut argv = if let Some(value) = &settings {
        match value.get(name) {
            Some(Value::Array(values)) => values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "Unknown or invalid app route".into())
                })
                .collect::<Result<Vec<_>>>()?,
            Some(_) => return Err("Unknown or invalid app route".into()),
            None => APP_ROUTES
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, args)| args.iter().map(|arg| (*arg).to_owned()).collect())
                .ok_or("Unknown or invalid app route")?,
        }
    } else {
        APP_ROUTES
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, args)| args.iter().map(|arg| (*arg).to_owned()).collect())
            .ok_or("Unknown or invalid app route")?
    };
    if name == "terminal" && !configured {
        let kitty = paths.config.join("terminal/kitty.conf");
        if kitty.is_file() {
            argv = vec![
                "kitty".into(),
                "--config".into(),
                kitty.to_string_lossy().into(),
            ];
        }
    }
    if name == "mail" && !configured && which("evolution") {
        argv = vec!["evolution".into()];
    }
    if name == "files" && !configured {
        argv = vec![
            paths
                .home
                .join(".local/bin/nacre-shell")
                .to_string_lossy()
                .into(),
            "dolphin".into(),
        ];
    }
    if argv.is_empty() || argv.iter().any(|arg| arg.contains('\0')) {
        return Err("Unknown or invalid app route".into());
    }
    Ok(argv)
}

fn restricted_niri_session() -> bool {
    if env::var("NACRE_COMPOSITOR").ok().as_deref() != Some("niri") {
        return false;
    }
    let uid_map = fs::read_to_string("/proc/self/uid_map");
    let status = fs::read_to_string("/proc/self/status");
    let (Ok(uid_map), Ok(status)) = (uid_map, status) else {
        return false;
    };
    restricted_niri_session_for("niri", &uid_map, &status)
}

fn restricted_niri_session_for(compositor: &str, uid_map: &str, status: &str) -> bool {
    if compositor != "niri" {
        return false;
    }
    let unmapped_host_root = uid_map.lines().any(|row| {
        row.split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value == 0)
    });
    (!uid_map.trim().is_empty() && !unmapped_host_root)
        || status
            .lines()
            .any(|line| line.split_whitespace().collect::<Vec<_>>() == ["NoNewPrivs:", "1"])
}

fn host_terminal(argv: &[String]) -> Result<()> {
    let runner = find_program("systemd-run")
        .ok_or("Cannot open a normal terminal: systemd-run is unavailable")?;
    let cwd = env::current_dir()?;
    let environment = env::vars().collect::<BTreeMap<_, _>>();
    let command = host_terminal_argv(argv, &cwd, &environment);
    let output = Command::new(&runner)
        .args(&command)
        .status()
        .map_err(|error| format!("Cannot open a normal terminal: {error}"))?;
    if !output.success() {
        return Err(format!(
            "Cannot open a normal terminal through the user manager (exit {})",
            output.code().unwrap_or(1)
        )
        .into());
    }
    Ok(())
}

fn find_program(program: &str) -> Option<String> {
    if program.contains('/') {
        return Path::new(program)
            .metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .then(|| program.into());
    }
    env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(program))
        .find(|path| {
            path.metadata()
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
        .map(|path| path.to_string_lossy().into())
}

fn host_terminal_argv(argv: &[String], cwd: &Path, envs: &BTreeMap<String, String>) -> Vec<String> {
    let mut command = vec![
        "--user".into(),
        "--quiet".into(),
        "--wait".into(),
        "--collect".into(),
        "--expand-environment=no".into(),
        "--property=Type=exec".into(),
        format!("--working-directory={}", cwd.display()),
    ];
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XAUTHORITY",
        "NIRI_SOCKET",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        "NACRE_COMPOSITOR",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "SSH_AUTH_SOCK",
    ] {
        if let Some(value) = envs.get(name) {
            command.push(format!("--setenv={name}={value}"));
        }
    }
    command.extend(["--".into(), "/usr/bin/env".into(), "--".into()]);
    command.extend(argv.iter().cloned());
    command
}

pub fn app(arguments: &[String]) -> Result<i32> {
    let paths = Paths::environment()?;
    let name = arguments.first().map(String::as_str).unwrap_or("browser");
    let mut argv = app_route(&paths, name)?;
    argv.extend(arguments.iter().skip(1).cloned());
    if !which(&argv[0]) {
        return Err("Install the selected application or edit ~/.config/nacre/apps.json".into());
    }
    if name == "terminal" && restricted_niri_session() {
        host_terminal(&argv)?;
        return Ok(0);
    }
    let error = Command::new(&argv[0]).args(&argv[1..]).exec();
    Err(format!("{}: {error}", argv[0]).into())
}

fn settings_registry(paths: &Paths) -> Result<Value> {
    let relative = Path::new("nacre/shell/modules/settings/registry/navigation.json");
    let mut root = env::current_dir().unwrap_or_default();
    loop {
        let candidate = root.join(relative);
        if candidate.is_file() {
            return Ok(serde_json::from_slice(&fs::read(candidate)?)?);
        }
        if !root.pop() {
            break;
        }
    }
    let installed = paths
        .share
        .join("shell/source/modules/settings/registry/navigation.json");
    if installed.is_file() {
        return Ok(serde_json::from_slice(&fs::read(installed)?)?);
    }
    Err("The Settings registry is not installed".into())
}

fn valid_settings_route(paths: &Paths, route: &str) -> Result<bool> {
    if route.is_empty() {
        return Ok(true);
    }
    if route.len() > 256
        || !regex::Regex::new(r"\A[a-z0-9/-]+(?:#[a-zA-Z0-9._/-]+)?\z")?.is_match(route)
    {
        return Ok(false);
    }
    let page = route.split_once('#').map_or(route, |(page, _)| page);
    let registry = settings_registry(paths)?;
    let alias = registry
        .get("aliases")
        .and_then(Value::as_object)
        .is_some_and(|aliases| aliases.contains_key(page));
    let page_id = registry
        .get("pages")
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter()
                .any(|row| row.get("id").and_then(Value::as_str) == Some(page))
        });
    Ok(alias || page_id)
}

fn run_bounded(program: &str, args: &[&str], seconds: u64) -> Result<bool> {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let result = process::run(program, &args, None, Duration::from_secs(seconds))?;
    Ok(result.status.success())
}

fn open_shell_page(app: &str, page: &str, attempts: usize) -> Result<()> {
    open_shell_page_with(app, page, attempts, run_bounded, || {
        thread::sleep(Duration::from_millis(250))
    })
}

fn open_shell_page_with(
    app: &str,
    page: &str,
    attempts: usize,
    mut run: impl FnMut(&str, &[&str], u64) -> Result<bool>,
    mut pause: impl FnMut(),
) -> Result<()> {
    if run(
        "quickshell",
        &["ipc", "-c", "nacre", "call", app, "open", page],
        3,
    )
    .unwrap_or(false)
    {
        return Ok(());
    }
    if !run(
        "systemctl",
        &["--user", "start", "--no-block", "nacre-shell.service"],
        3,
    )? {
        return Err(format!(
            "Could not start the desktop service for {app_name}",
            app_name = if app == "settingsApp" {
                "Settings."
            } else {
                "Welcome"
            }
        )
        .into());
    }
    for _ in 0..attempts {
        pause();
        if run(
            "quickshell",
            &["ipc", "-c", "nacre", "call", app, "open", page],
            2,
        )
        .unwrap_or(false)
        {
            return Ok(());
        }
    }
    Err(if app == "settingsApp" {
        "Settings could not connect to the desktop. Check nacre-shell.service."
    } else {
        "Welcome could not connect. Check nacre-shell.service"
    }
    .into())
}

fn notify(message: &str) {
    if let Ok(mut backend) = compositor::backend::Backend::new(
        Paths::environment().unwrap_or_else(|_| Paths::new("/")),
        env::vars().collect::<BTreeMap<_, _>>(),
    ) {
        let _ = backend.action(
            "notify",
            &[
                json!("3"),
                json!(6000),
                json!("0"),
                json!(format!("Nacre Settings: {message}")),
            ],
            &json!({}),
        );
    }
}

fn welcome_preferences(path: &Path) -> Result<Value> {
    let value: Value = if path.is_file() {
        crate::state::read_strict(path, json!({}))?
    } else {
        json!({})
    };
    if !value.is_object()
        || value.get("version").is_some_and(|v| v != 1)
        || value.get("showAtLogin").is_some_and(|v| !v.is_boolean())
    {
        return Err(
            "Unsupported Welcome preferences; restore welcome.json or use version 1".into(),
        );
    }
    let mut value = value;
    value["version"] = json!(1);
    if value.get("showAtLogin").is_none() {
        value["showAtLogin"] = json!(true);
    }
    Ok(value)
}

fn welcome_session(paths: &Paths) -> Result<PathBuf> {
    Ok(paths.state.join("welcome/session.json"))
}

fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    if crate::state::schema(path).is_some() {
        return crate::state::write(path, value);
    }
    let mut content = serde_json::to_vec_pretty(value)?;
    content.push(b'\n');
    crate::files::atomic(path, &content)
}

fn claim_login(paths: &Paths, instance: &str) -> Result<Option<String>> {
    if instance.is_empty() {
        return Ok(None);
    }
    let _guard = crate::files::lock(&paths.state.join("welcome/owner.lock"))?;
    let pref_path = paths.config.join("welcome.json");
    let prefs = welcome_preferences(&pref_path)?;
    if prefs["showAtLogin"] == false
        || prefs.get("firstRunShown").is_some_and(|v| v == true)
        || prefs.get("tourCompleted").is_some_and(|v| v == true)
    {
        return Ok(None);
    }
    let session = welcome_session(paths)?;
    let previous = if session.is_file() {
        crate::state::read_strict(&session, json!({})).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };
    if previous.get("instance").and_then(Value::as_str) == Some(instance) {
        return Ok(None);
    }
    let claim = uuid::Uuid::new_v4().simple().to_string();
    atomic_json(&session, &json!({"instance":instance,"claim":claim}))?;
    Ok(Some(claim))
}

fn acknowledge_login(paths: &Paths, instance: &str, claim: &str) -> Result<()> {
    let _guard = crate::files::lock(&paths.state.join("welcome/owner.lock"))?;
    let session = welcome_session(paths)?;
    let previous: Value = if session.is_file() {
        crate::state::read_strict(&session, json!({}))?
    } else {
        json!({})
    };
    if previous != json!({"instance":instance,"claim":claim}) {
        return Err("Welcome activation claim is no longer current".into());
    }
    let mut prefs = welcome_preferences(&paths.config.join("welcome.json"))?;
    prefs["firstRunShown"] = json!(true);
    atomic_json(&paths.config.join("welcome.json"), &prefs)?;
    Ok(())
}

fn release_login(paths: &Paths, instance: &str, claim: &str) -> Result<()> {
    let _guard = crate::files::lock(&paths.state.join("welcome/owner.lock"))?;
    let session = welcome_session(paths)?;
    if !session.is_file() {
        return Ok(());
    }
    let previous: Value = match crate::state::read_strict(&session, json!({})) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    if previous == json!({"instance":instance,"claim":claim}) {
        fs::remove_file(session)?;
    }
    Ok(())
}

pub fn settings(arguments: &[String]) -> Result<i32> {
    if arguments == ["-h"] || arguments == ["--help"] {
        print!("usage: nacre-settings [-h] [page[/subpage][#setting]]\n\nOpen or focus Nacre Settings, optionally at a specific page.\n\npositional arguments:\n  page[/subpage][#setting]\n\noptions:\n  -h, --help            show this help message and exit\n");
        return Ok(0);
    }
    if arguments.len() > 1 {
        eprintln!("nacre-settings: unexpected arguments");
        return Ok(2);
    }
    let route = arguments.first().map(String::as_str).unwrap_or("");
    let paths = Paths::environment()?;
    if !valid_settings_route(&paths, route)? {
        eprintln!("nacre-settings: Unknown Settings route: {route}");
        return Ok(2);
    }
    match open_shell_page("settingsApp", route, 32) {
        Ok(()) => Ok(0),
        Err(error) => {
            notify(&error.to_string());
            Err(error)
        }
    }
}

pub fn welcome(arguments: &[String]) -> Result<i32> {
    if arguments == ["-h"] || arguments == ["--help"] {
        print!("usage: nacre-welcome [-h] [--login | --enable | --disable]\n                     [{{home,colors,shortcuts,apps,ready,help}}]\n\nOpen the single Nacre Welcome window; automatically shown once on first login.\n\npositional arguments:\n  {{home,colors,shortcuts,apps,ready,help}}\n\noptions:\n  -h, --help            show this help message and exit\n  --login\n  --enable\n  --disable\n");
        return Ok(0);
    }
    let login = arguments.iter().any(|arg| arg == "--login");
    let enabled = arguments.iter().any(|arg| arg == "--enable");
    let disabled = arguments.iter().any(|arg| arg == "--disable");
    if enabled && disabled || (enabled || disabled) && login {
        eprintln!("nacre-welcome: argument --login: not allowed with argument --enable/--disable");
        return Ok(2);
    }
    if arguments.iter().any(|arg| {
        arg.starts_with('-') && !["--login", "--enable", "--disable"].contains(&arg.as_str())
    }) {
        eprintln!("nacre-welcome: unrecognized argument");
        return Ok(2);
    }
    if enabled || disabled {
        // Welcome remains the owner of its existing preference format. This
        // native implementation performs the same bounded private-file update.
        let paths = Paths::environment()?;
        let _guard = crate::files::lock(&paths.state.join("welcome/owner.lock"))?;
        let path = paths.config.join("welcome.json");
        let mut value = welcome_preferences(&path)?;
        value["showAtLogin"] = json!(enabled);
        atomic_json(&path, &value)?;
        let _ = run_bounded(
            "quickshell",
            &["ipc", "-c", "nacre", "call", "welcomeApp", "refresh"],
            2,
        );
        return Ok(0);
    }
    let page = arguments
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("home");
    let pages = arguments.iter().filter(|arg| !arg.starts_with('-')).count();
    if pages > 1 || !["home", "colors", "shortcuts", "apps", "ready", "help"].contains(&page) {
        return Ok(2);
    }
    if login {
        let paths = Paths::environment()?;
        let envs = env::vars().collect::<BTreeMap<_, _>>();
        let backend = compositor::backend::Backend::new(paths.clone(), envs)?;
        let session = backend.session_id().to_owned();
        let Some(claim) = claim_login(&paths, &session)? else {
            return Ok(0);
        };
        return match open_shell_page("welcomeApp", page, 40) {
            Ok(()) => match acknowledge_login(&paths, &session, &claim) {
                Ok(()) => Ok(0),
                Err(error) => {
                    let _ = release_login(&paths, &session, &claim);
                    Err(error)
                }
            },
            Err(error) => {
                let _ = release_login(&paths, &session, &claim);
                Err(error)
            }
        };
    }
    open_shell_page("welcomeApp", page, 40)?;
    Ok(0)
}

/// State and login-claim API used by the Welcome window's data worker.
pub fn welcome_helper(arguments: &[String]) -> Result<i32> {
    let Some(action) = arguments.first().map(String::as_str) else {
        return Err("Expected a Welcome operation".into());
    };
    let paths = Paths::environment()?;
    let value_arg = arguments.get(1).map(String::as_str).unwrap_or("");
    let result: Result<Value> = (|| match action {
        "state" | "set-startup" | "set-demo-wallpapers" | "finish-tour" => {
            if action == "set-startup" || action == "set-demo-wallpapers" || action == "finish-tour"
            {
                let _guard = crate::files::lock(&paths.state.join("welcome/owner.lock"))?;
                let path = paths.config.join("welcome.json");
                let mut prefs = welcome_preferences(&path)?;
                match action {
                    "set-startup" => {
                        let value: Value = serde_json::from_str(value_arg)?;
                        if !value.is_boolean() {
                            return Err("Welcome startup choice must be a boolean".into());
                        }
                        prefs["showAtLogin"] = value;
                    }
                    "set-demo-wallpapers" => {
                        let values: Value = serde_json::from_str(value_arg)?;
                        let rows = values
                            .as_array()
                            .ok_or("Choose four different local wallpapers")?;
                        if rows.len() != 4
                            || rows.iter().any(|v| !v.is_string())
                            || rows
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<std::collections::HashSet<_>>()
                                .len()
                                != 4
                        {
                            return Err("Choose four different local wallpapers".into());
                        }
                        for row in rows {
                            let wallpaper = Path::new(row.as_str().unwrap());
                            if !wallpaper.is_absolute() || !wallpaper.is_file() {
                                return Err(
                                    "Demo wallpapers must be existing absolute local paths".into(),
                                );
                            }
                        }
                        prefs["demoWallpapers"] = values;
                        atomic_json(&paths.config.join("welcome.json"), &prefs)?;
                        return Ok(json!({"configured":true}));
                    }
                    "finish-tour" => prefs["tourCompleted"] = json!(true),
                    _ => {}
                }
                atomic_json(&path, &prefs)?;
            }
            let preferences = welcome_preferences(&paths.config.join("welcome.json"))?;
            let available_ai = which("nacre-ai");
            let brain = paths
                .home
                .join(".local/share/nacre/brain/control.py")
                .is_file()
                && which("nacre-brain");
            let shortcut_error = match crate::process::run("hyprctl", &["binds".into(),"-j".into()],None,std::time::Duration::from_secs(2)) {
                    Ok(output) if output.status.success() => "No described Nacre shortcuts are registered. Reload your managed Hyprland configuration.",
                    _ => "Shortcuts are available when running inside your Hyprland session.",
                };
            Ok(
                json!({"shortcuts":[],"shortcutError":shortcut_error,"preferences":preferences,"available":{"ai":available_ai,"brain":brain}}),
            )
        }
        "claim-login" => Ok(claim_login(&paths, &welcome_instance(&paths)?)?.map_or(
            json!({"show":false,"claim":""}),
            |claim| json!({"show":true,"claim":claim}),
        )),
        "acknowledge-login" => {
            acknowledge_login(&paths, &welcome_instance(&paths)?, value_arg)?;
            Ok(json!({"acknowledged":true}))
        }
        "release-login" => {
            release_login(&paths, &welcome_instance(&paths)?, value_arg)?;
            Ok(json!({"released":true}))
        }
        _ => Err("Unknown Welcome operation".into()),
    })();
    match result {
        Ok(value) => {
            println!("{}", crate::json::encode(&value)?);
            Ok(0)
        }
        Err(error) => {
            println!(
                "{}",
                crate::json::encode(&json!({"error":error.to_string()}))?
            );
            Ok(1)
        }
    }
}

fn welcome_instance(paths: &Paths) -> Result<String> {
    let backend = crate::compositor::backend::Backend::new(paths.clone(), env::vars().collect())?;
    Ok(backend.session_id().to_owned())
}

pub fn startup_apps(_arguments: &[String]) -> Result<i32> {
    if _arguments.first().is_some_and(|arg| arg == "plan") {
        let input = _arguments.get(1).ok_or("Missing startup plan JSON")?;
        let request: Value = serde_json::from_str(input)?;
        let clients = request
            .get("args")
            .and_then(Value::as_array)
            .and_then(|v| v.first())
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let profile = request
            .get("kwargs")
            .and_then(Value::as_object)
            .and_then(|v| v.get("profile"))
            .cloned();
        let paths = Paths::environment()?;
        let profile = profile.unwrap_or(serde_json::from_slice(
            &fs::read(paths.config.join("profile.json"))
                .unwrap_or_else(|_| br#"{"version":1,"workspaceNames":[],"startup":[]}"#.to_vec()),
        )?);
        let items = profile
            .get("startup")
            .and_then(Value::as_array)
            .ok_or("Invalid profile startup list")?;
        let entries = items
            .iter()
            .map(|row| validate_startup(row, workspace_count(&profile)))
            .collect::<Result<Vec<_>>>()?;
        let output: Vec<_> = entries
            .iter()
            .map(|entry| {
                let exists = startup_exists(&Value::Array(clients.clone()), entry);
                json!([entry.label, entry.workspace, entry.argv, exists])
            })
            .collect();
        println!("{}", crate::json::encode(&Value::Array(output))?);
        return Ok(0);
    }
    let paths = Paths::environment()?;
    let profile_path = paths.config.join("profile.json");
    let profile: Value = if profile_path.is_file() {
        serde_json::from_slice(&fs::read(profile_path)?)?
    } else {
        json!({"version":1,"workspaceNames":[],"startup":[]})
    };
    let startup = profile
        .get("startup")
        .and_then(Value::as_array)
        .ok_or("Invalid profile startup list")?;
    if startup.is_empty() {
        return Ok(0);
    }
    let runtime = env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid().as_raw())));
    let lock_path = runtime.join("nacre-startup-apps.lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    let _guard = match nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockExclusiveNonblock) {
        Ok(guard) => guard,
        Err((_, nix::errno::Errno::EAGAIN)) => return Ok(0),
        Err((_, error)) => return Err(error.into()),
    };
    let mut driver = StartupRuntime {
        paths,
        environment: env::vars().collect(),
        backend: None,
    };
    run_startup(&profile, &mut driver)
}

trait StartupDriver {
    fn ready(&mut self) -> Result<()>;
    fn windows(&mut self) -> Result<Value>;
    fn per_output_workspaces(&self) -> bool;
    fn switch_workspace(&mut self, target: Value) -> Result<()>;
    fn executable(&self, program: &str) -> bool;
    fn launch(&mut self, argv: &[String]) -> Result<()>;
    fn pause(&mut self, duration: Duration);
}

struct StartupRuntime {
    paths: Paths,
    environment: BTreeMap<String, String>,
    backend: Option<compositor::backend::Backend>,
}

impl StartupRuntime {
    fn backend(&mut self) -> Result<&mut compositor::backend::Backend> {
        if self.backend.is_none() {
            self.backend = Some(compositor::backend::Backend::new(
                self.paths.clone(),
                self.environment.clone(),
            )?);
        }
        Ok(self.backend.as_mut().expect("backend initialized"))
    }
}

impl StartupDriver for StartupRuntime {
    fn ready(&mut self) -> Result<()> {
        let helper = self.paths.tools().join("nacre-helper");
        if !helper.is_file() {
            return Err("Native helper installation is unavailable".into());
        }
        let ready = process::run(
            helper.to_str().ok_or("Invalid helper path")?,
            &["session-watch".into(), "ready".into()],
            None,
            Duration::from_secs(20),
        )?;
        if ready.status.success() {
            Ok(())
        } else {
            Err("Desktop session is not ready".into())
        }
    }
    fn windows(&mut self) -> Result<Value> {
        Ok(self
            .backend()?
            .query("windows", Duration::from_secs(10), None)?)
    }
    fn per_output_workspaces(&self) -> bool {
        self.backend
            .as_ref()
            .is_some_and(|backend| backend.capabilities()["perOutputWorkspaces"] == true)
    }
    fn switch_workspace(&mut self, target: Value) -> Result<()> {
        self.backend()?.action(
            "switch_workspace",
            &[target],
            &json!({"current_monitor":true}),
        )?;
        Ok(())
    }
    fn executable(&self, program: &str) -> bool {
        which(program)
    }
    fn launch(&mut self, argv: &[String]) -> Result<()> {
        let runner = find_program("systemd-run").ok_or("systemd-run is unavailable")?;
        let mut command = vec![
            "--user".into(),
            "--scope".into(),
            "--collect".into(),
            "--quiet".into(),
        ];
        command.extend(argv.iter().cloned());
        drop(process::spawn_detached_quiet(&runner, &command)?);
        Ok(())
    }
    fn pause(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}

fn run_startup(profile: &Value, driver: &mut impl StartupDriver) -> Result<i32> {
    let startup = profile
        .get("startup")
        .and_then(Value::as_array)
        .ok_or("Invalid profile startup list")?;
    if startup.is_empty() {
        return Ok(0);
    }
    let entries = startup
        .iter()
        .map(|row| validate_startup(row, workspace_count(profile)))
        .collect::<Result<Vec<_>>>()?;
    driver.ready()?;
    let initial = driver.windows()?;
    for entry in &entries {
        if startup_exists(&initial, entry) || !driver.executable(&entry.argv[0]) {
            continue;
        }
        let target = workspace_target(profile, entry.workspace, driver.per_output_workspaces());
        driver.switch_workspace(target)?;
        driver.launch(&entry.argv)?;
        if entry.label == "Brain" {
            continue;
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            driver.pause(Duration::from_millis(200));
            if startup_exists(&driver.windows()?, entry) {
                break;
            }
        }
    }
    let target = workspace_target(profile, 1, driver.per_output_workspaces());
    driver.switch_workspace(target)?;
    Ok(0)
}

#[derive(Clone)]
struct StartupEntry {
    label: String,
    workspace: i64,
    argv: Vec<String>,
    classes: Vec<String>,
}
fn workspace_count(profile: &Value) -> i64 {
    profile
        .get("workspaceCount")
        .and_then(Value::as_i64)
        .filter(|count| (1..=10).contains(count))
        .unwrap_or(6)
}

fn validate_startup(row: &Value, count: i64) -> Result<StartupEntry> {
    if !row.as_object().is_some_and(|value| {
        value.len() == 4
            && ["label", "workspace", "argv", "classes"]
                .iter()
                .all(|key| value.contains_key(*key))
    }) {
        return Err("Invalid profile entry".into());
    }
    let label = row
        .get("label")
        .and_then(Value::as_str)
        .filter(|v| {
            !v.is_empty()
                && v.len() <= 200
                && !v.contains('\0')
                && !v.contains('\n')
                && !v.contains('\r')
        })
        .ok_or("Invalid profile startup label")?;
    let workspace = row
        .get("workspace")
        .and_then(Value::as_i64)
        .filter(|v| (1..=count).contains(v))
        .ok_or("Invalid profile startup workspace")?;
    let argv = row
        .get("argv")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or("Invalid profile startup arguments")?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.contains('\0'))
                .map(str::to_owned)
                .ok_or_else(|| crate::Error::from("Invalid profile startup argument"))
        })
        .collect::<Result<Vec<_>>>()?;
    let classes = row
        .get("classes")
        .and_then(Value::as_array)
        .ok_or("Invalid profile startup classes")?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|value| value.len() <= 200)
                .map(str::to_owned)
                .ok_or_else(|| crate::Error::from("Invalid profile startup class"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(StartupEntry {
        label: label.into(),
        workspace,
        argv,
        classes,
    })
}
fn startup_exists(windows: &Value, entry: &StartupEntry) -> bool {
    let rows = windows
        .as_array()
        .or_else(|| windows.get("windows").and_then(Value::as_array));
    rows.is_some_and(|rows| {
        rows.iter().any(|window| {
            let class = window
                .get("class")
                .or_else(|| window.get("className"))
                .and_then(Value::as_str)
                .unwrap_or("");
            entry
                .classes
                .iter()
                .any(|name| name.eq_ignore_ascii_case(class))
                || (entry.label == "Brain"
                    && window.get("title").and_then(Value::as_str) == Some("Nacre Brain"))
        })
    })
}
fn workspace_target(profile: &Value, workspace: i64, per_output: bool) -> Value {
    if !per_output {
        return json!(workspace);
    }
    let name = profile
        .get("workspaceNames")
        .and_then(Value::as_array)
        .and_then(|names| names.get((workspace - 1) as usize))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("nacre-{workspace}"));
    json!({"name":name})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeStartup {
        events: Vec<String>,
        windows: VecDeque<Value>,
        switches: Vec<Value>,
        launches: Vec<Vec<String>>,
        per_output: bool,
        ready_fails: bool,
    }

    impl StartupDriver for FakeStartup {
        fn ready(&mut self) -> Result<()> {
            self.events.push("ready".into());
            if self.ready_fails {
                Err("fixture readiness failure".into())
            } else {
                Ok(())
            }
        }
        fn windows(&mut self) -> Result<Value> {
            self.events.push("windows".into());
            Ok(self.windows.pop_front().unwrap_or_else(|| json!([])))
        }
        fn per_output_workspaces(&self) -> bool {
            self.per_output
        }
        fn switch_workspace(&mut self, target: Value) -> Result<()> {
            self.events.push("switch".into());
            self.switches.push(target);
            Ok(())
        }
        fn executable(&self, _program: &str) -> bool {
            true
        }
        fn launch(&mut self, argv: &[String]) -> Result<()> {
            self.events.push("launch".into());
            self.launches.push(argv.to_vec());
            Ok(())
        }
        fn pause(&mut self, _duration: Duration) {
            self.events.push("pause".into());
        }
    }
    #[test]
    fn app_routes_preserve_user_argv_and_file_path_spaces() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(&paths.config)?;
        fs::write(
            paths.config.join("apps.json"),
            r#"{"files":["thunar"],"terminal":["alacritty","--title","My terminal"]}"#,
        )?;
        assert_eq!(app_route(&paths, "files")?, ["thunar"]);
        assert_eq!(
            app_route(&paths, "terminal")?,
            ["alacritty", "--title", "My terminal"]
        );
        assert!(app_route(&paths, "unknown").is_err());
        Ok(())
    }
    #[test]
    fn default_app_route_keeps_the_styled_files_path_literal() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        assert_eq!(
            app_route(&paths, "files")?,
            [
                home.path()
                    .join(".local/bin/nacre-shell")
                    .to_string_lossy()
                    .to_string(),
                "dolphin".into()
            ]
        );
        Ok(())
    }
    #[test]
    fn default_terminal_uses_the_private_kitty_configuration() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let config = paths.config.join("terminal/kitty.conf");
        fs::create_dir_all(config.parent().ok_or("Missing configuration parent")?)?;
        fs::write(&config, "font_size 11\n")?;
        assert_eq!(
            app_route(&paths, "terminal")?,
            [
                "kitty",
                "--config",
                config.to_str().ok_or("Invalid test path")?
            ]
        );
        Ok(())
    }
    #[test]
    fn restricted_niri_detection_requires_niri_and_matches_proc_evidence() {
        assert!(restricted_niri_session_for(
            "niri",
            "1000 1000 1\n",
            "NoNewPrivs:\t0\n"
        ));
        assert!(restricted_niri_session_for(
            "niri",
            "0 0 4294967295\n",
            "NoNewPrivs:\t1\n"
        ));
        assert!(!restricted_niri_session_for(
            "niri",
            "0 0 4294967295\n",
            "NoNewPrivs:\t0\n"
        ));
        assert!(!restricted_niri_session_for(
            "hyprland",
            "1000 1000 1\n",
            "NoNewPrivs:\t1\n"
        ));
        assert!(!restricted_niri_session_for(
            "",
            "1000 1000 1\n",
            "NoNewPrivs:\t1\n"
        ));
    }
    #[test]
    fn host_terminal_argv_preserves_environment_and_literal_arguments() {
        let argv = vec![
            "custom-terminal".into(),
            "--title".into(),
            "With spaces".into(),
            "$HOME".into(),
        ];
        let command = host_terminal_argv(
            &argv,
            Path::new("/home/test/a directory"),
            &BTreeMap::from([
                ("NIRI_SOCKET".into(), "/run/niri.sock".into()),
                ("PATH".into(), "/overlay/bin".into()),
            ]),
        );
        assert!(command.contains(&"--working-directory=/home/test/a directory".into()));
        assert!(command.contains(&"--setenv=NIRI_SOCKET=/run/niri.sock".into()));
        assert!(!command
            .iter()
            .any(|part| part.starts_with("--setenv=PATH=")));
        assert_eq!(
            &command[command.len() - 4..],
            ["custom-terminal", "--title", "With spaces", "$HOME"].as_slice()
        );
        assert_eq!(
            &command[command.len() - 7..command.len() - 4],
            ["--", "/usr/bin/env", "--"].as_slice()
        );
        assert!(!command.contains(&"--scope".into()));
    }
    #[test]
    fn settings_route_checks_canonical_ids_and_aliases() -> Result<()> {
        let paths =
            Paths::new(env!("CARGO_MANIFEST_DIR").to_owned() + "/../../tests/fixtures/empty-home");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../nacre/shell/modules/settings/registry/navigation.json");
        assert!(root.is_file());
        assert!(valid_settings_route(&paths, "sound")?);
        assert!(valid_settings_route(&paths, "panels/dashboard#pages")?);
        assert!(!valid_settings_route(&paths, "colors;bad")?);
        Ok(())
    }
    #[test]
    fn settings_keeps_argparse_help_flags() -> Result<()> {
        assert_eq!(settings(&["-h".into()])?, 0);
        assert_eq!(settings(&["--help".into()])?, 0);
        Ok(())
    }
    #[test]
    fn settings_activation_uses_one_warm_ipc_and_bounded_cold_retry() -> Result<()> {
        let mut calls: Vec<(String, String, u64)> = Vec::new();
        open_shell_page_with(
            "settingsApp",
            "sound",
            32,
            |program, args, timeout| {
                calls.push((program.to_owned(), args.join(" "), timeout));
                Ok(true)
            },
            || panic!("warm activation must not sleep"),
        )?;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "quickshell");
        assert_eq!(calls[0].1, "ipc -c nacre call settingsApp open sound");
        assert_eq!(calls[0].2, 3);

        let mut calls: Vec<(String, String, u64)> = Vec::new();
        let mut pauses = 0;
        let mut results = [false, true, false, true].into_iter();
        open_shell_page_with(
            "settingsApp",
            "displays",
            32,
            |program, args, timeout| {
                calls.push((program.to_owned(), args.join(" "), timeout));
                Ok(results.next().unwrap_or(false))
            },
            || pauses += 1,
        )?;
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[1].0, "systemctl");
        assert_eq!(calls[1].1, "--user start --no-block nacre-shell.service");
        assert_eq!(pauses, 2);
        assert_eq!(calls[3].1.split_whitespace().last(), Some("displays"));
        Ok(())
    }
    #[test]
    fn settings_activation_failure_stops_when_service_start_fails() {
        let mut calls: Vec<(String, String)> = Vec::new();
        let mut pauses = 0;
        let result = open_shell_page_with(
            "settingsApp",
            "displays",
            32,
            |program, args, _| {
                calls.push((program.to_owned(), args.join(" ")));
                Ok(false)
            },
            || pauses += 1,
        );
        assert!(result.is_err());
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].0, "systemctl");
        assert_eq!(pauses, 0);
    }
    #[test]
    fn welcome_page_set_is_bounded_and_login_flags_are_exclusive() -> Result<()> {
        assert_eq!(welcome(&["unknown".into()])?, 2);
        assert!(!restricted_niri_session_for(
            "hyprland",
            "1000 1000 1\n",
            "NoNewPrivs:\t1\n"
        ));
        Ok(())
    }
    #[test]
    fn welcome_login_claim_is_consumed_only_after_acknowledgement() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let claim = claim_login(&paths, "session-1")?.ok_or("expected first claim")?;
        assert!(claim_login(&paths, "session-1")?.is_none());
        assert!(acknowledge_login(&paths, "session-1", "stale").is_err());
        acknowledge_login(&paths, "session-1", &claim)?;
        assert!(
            welcome_preferences(&paths.config.join("welcome.json"))?["firstRunShown"]
                .as_bool()
                .unwrap_or(false)
        );
        assert!(claim_login(&paths, "session-2")?.is_none());
        Ok(())
    }
    #[test]
    fn welcome_failed_activation_releases_claim_for_retry() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let claim = claim_login(&paths, "session-1")?.ok_or("expected first claim")?;
        let failed = open_shell_page_with("welcomeApp", "home", 40, |_, _, _| Ok(false), || {});
        assert!(failed.is_err());
        release_login(&paths, "session-1", &claim)?;
        assert!(claim_login(&paths, "session-1")?.is_some());
        Ok(())
    }
    #[test]
    fn welcome_concurrent_claims_activate_a_session_only_once() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = std::sync::Arc::new(Paths::new(home.path()));
        let claims = std::thread::scope(|scope| {
            (0..4)
                .map(|_| {
                    let paths = paths.clone();
                    scope.spawn(move || claim_login(&paths, "shared-session"))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|thread| thread.join().expect("claim worker did not panic"))
                .collect::<Result<Vec<_>>>()
        })?;
        assert_eq!(claims.iter().filter(|claim| claim.is_some()).count(), 1);
        Ok(())
    }
    #[test]
    fn startup_plan_retains_workspace_targets_and_client_matching() -> Result<()> {
        let profile = json!({"workspaceNames":["", "Work"]});
        assert_eq!(workspace_target(&profile, 2, true), json!({"name":"Work"}));
        assert_eq!(
            workspace_target(&profile, 1, true),
            json!({"name":"nacre-1"})
        );
        assert_eq!(workspace_target(&profile, 2, false), json!(2));
        let entry = validate_startup(
            &json!({"label":"Browser","workspace":3,"argv":["example-browser"],"classes":["Example-browser"]}),
            6,
        )?;
        assert!(startup_exists(
            &json!([{"class":"example-browser","title":""}]),
            &entry
        ));
        let brain = validate_startup(
            &json!({"label":"Brain","workspace":2,"argv":["nacre-brain-ui"],"classes":["nacre-brain"]}),
            6,
        )?;
        assert!(startup_exists(
            &json!([{"class":"chrome-127.0.0.1__-Default","title":"Nacre Brain"}]),
            &brain
        ));
        Ok(())
    }
    #[test]
    fn startup_empty_profile_has_no_readiness_or_compositor_side_effects() -> Result<()> {
        let mut driver = FakeStartup::default();
        assert_eq!(run_startup(&json!({"startup":[]}), &mut driver)?, 0);
        assert!(driver.events.is_empty());
        Ok(())
    }
    #[test]
    fn startup_waits_for_readiness_routes_only_missing_apps_and_restores_workspace() -> Result<()> {
        let profile = json!({
            "workspaceNames":[],
            "startup":[
                {"label":"Browser","workspace":3,"argv":["example-browser","literal $HOME"],"classes":["Example-browser"]},
                {"label":"Brain","workspace":2,"argv":["nacre-brain-ui"],"classes":["nacre-brain"]}
            ]
        });
        let mut driver = FakeStartup {
            windows: [
                json!([]),
                json!([{"class":"example-browser","title":"Browser"}]),
            ]
            .into(),
            ..FakeStartup::default()
        };
        run_startup(&profile, &mut driver)?;
        assert_eq!(driver.events[0], "ready");
        assert_eq!(
            driver
                .events
                .iter()
                .filter(|event| *event == "windows")
                .count(),
            2
        );
        assert_eq!(driver.switches, [json!(3), json!(2), json!(1)]);
        assert_eq!(driver.launches[0], ["example-browser", "literal $HOME"]);
        assert_eq!(driver.launches[1], ["nacre-brain-ui"]);
        Ok(())
    }
    #[test]
    fn startup_readiness_failure_prevents_queries_and_launches() {
        let profile = json!({"startup":[{"label":"Browser","workspace":3,"argv":["browser"],"classes":["browser"]}]});
        let mut driver = FakeStartup {
            ready_fails: true,
            ..FakeStartup::default()
        };
        assert!(run_startup(&profile, &mut driver).is_err());
        assert_eq!(driver.events, ["ready"]);
        assert!(driver.launches.is_empty());
        assert!(driver.switches.is_empty());
    }
}
