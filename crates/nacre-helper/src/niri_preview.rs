//! Private preview copies and code namespace; never imports session activation.
use crate::{json, paths::Paths, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
pub const SAFE_CONFIG: &[&str] = &[
    "profile.json",
    "profile.lua",
    "desktop.json",
    "wallpaper-picker.json",
    "apps.json",
    "workflows.json",
    "lock.json",
    "launcher.json",
    "shortcuts.lua",
    "dashboard.json",
    "colors.json",
    "kitty-colors.conf",
    "welcome.json",
    "cli.json",
];
pub const SAFE_STATE: &[&str] = &["scheme.json", "presentation.json", "wallpaper"];
pub fn private_copy(source: &Path, target: &Path) -> Result<()> {
    if !source.try_exists()? {
        return Ok(());
    }
    fs::create_dir_all(target.parent().ok_or("Invalid private copy target")?)?;
    if source.is_dir() {
        crate::niri_session_stage::copy_tree(source, target, &[])?;
    } else {
        fs::write(target, fs::read(source)?)?;
        fs::set_permissions(target, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
pub fn copy_cached_presentation(paths: &Paths, state: &Path, cache: &Path) -> Result<()> {
    let root = match paths.cache.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    fn visit(value: &Value, root: &Path, cache: &Path) -> Result<()> {
        match value {
            Value::Object(values) => {
                for value in values.values() {
                    visit(value, root, cache)?;
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, root, cache)?;
                }
            }
            Value::String(value)
                if value.starts_with(&(root.to_string_lossy().into_owned() + "/")) =>
            {
                let source = match Path::new(value).canonicalize() {
                    Ok(path) => path,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                if source.starts_with(root) && source.is_file() {
                    private_copy(&source, &cache.join(source.strip_prefix(root)?))?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for name in ["presentation.json", "wallpaper/media.json"] {
        let path = state.join(name);
        if path.is_file() {
            visit(&serde_json::from_slice(&fs::read(path)?)?, &root, cache)?;
        }
    }
    Ok(())
}
pub fn preview_bindings(paths: &Paths, names: &BTreeMap<u64, String>) -> Result<String> {
    let controller = json::encode(&paths.home.join(".local/bin/nacre-shell").to_string_lossy())?;
    let mut rows = vec![
        format!("    Alt+Space {{ spawn {controller} \"launcher\"; }}"),
        format!("    Alt+O {{ spawn {controller} \"toggle\"; }}"),
        "    Alt+Return { spawn \"kitty\"; }".into(),
        "    Alt+Shift+Escape { quit skip-confirmation=true; }".into(),
    ];
    for (number, name) in names {
        if *number <= 9 {
            rows.push(format!(
                "    Alt+{number} {{ focus-workspace {}; }}",
                json::encode(name)?
            ));
        }
    }
    Ok(format!("binds {{\n{}\n}}\n", rows.join("\n")))
}
pub fn command(
    destination: &Path,
    paths: &Paths,
    environment: &BTreeMap<String, String>,
    runtime: &Path,
    quickshell: Option<&Path>,
) -> Result<(Vec<String>, BTreeMap<String, String>)> {
    let display = environment
        .get("WAYLAND_DISPLAY")
        .filter(|display| !display.is_empty())
        .ok_or("Windowed preview needs the running desktop's Wayland display")?;
    let outer = environment
        .get("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid())));
    let parent = if Path::new(display).is_absolute() {
        PathBuf::from(display)
    } else {
        outer.join(display)
    };
    let tree = destination.join("home-view");
    let mut args: Vec<String> = [
        "bwrap",
        "--ro-bind",
        "/",
        "/",
        "--proc",
        "/proc",
        "--dev-bind",
        "/dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/run/dbus",
        "--unshare-pid",
        "--die-with-parent",
        "--bind",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.extend([
        runtime.to_string_lossy().into_owned(),
        runtime.to_string_lossy().into_owned(),
    ]);
    let config_root = paths.config.parent().ok_or("Invalid config root")?;
    let cache_root = paths.cache.parent().ok_or("Invalid cache root")?;
    for (source, target) in [
        (tree.join(".config/nacre"), paths.config.clone()),
        (tree.join(".config/hypr"), config_root.join("hypr")),
        (
            tree.join(".config/quickshell"),
            config_root.join("quickshell"),
        ),
        (tree.join(".config/kitty"), config_root.join("kitty")),
        (destination.join("state"), paths.state.clone()),
        (destination.join("cache"), paths.cache.clone()),
        (destination.join("kitty-cache"), cache_root.join("kitty")),
    ] {
        args.extend([
            "--bind".into(),
            source.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
        ]);
    }
    args.extend([
        "--ro-bind".into(),
        destination.join("config").to_string_lossy().into_owned(),
        paths
            .config
            .join("niri-on-pc/preview")
            .to_string_lossy()
            .into_owned(),
        "--ro-bind".into(),
        destination.join("bin").to_string_lossy().into_owned(),
        destination.join("bin").to_string_lossy().into_owned(),
    ]);
    let mut environment = environment.clone();
    for name in [
        "NIRI_SOCKET",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "NOTIFY_SOCKET",
        "GDK_BACKEND",
    ] {
        environment.remove(name);
    }
    for (name, value) in [
        ("NACRE_COMPOSITOR", "niri".into()),
        ("XDG_CURRENT_DESKTOP", "niri".into()),
        ("XDG_RUNTIME_DIR", runtime.to_string_lossy().into_owned()),
        ("WAYLAND_DISPLAY", parent.to_string_lossy().into_owned()),
        (
            "NACRE_COMPOSITOR_TOOLS",
            paths.tools().to_string_lossy().into_owned(),
        ),
        (
            "NACRE_PEARL_TOOLS",
            paths.tools().to_string_lossy().into_owned(),
        ),
        (
            "NACRE_QS_CONFIG",
            config_root
                .join("quickshell/nacre/shell.qml")
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "NACRE_QS_BIN",
            quickshell
                .unwrap_or(Path::new("/usr/bin/quickshell"))
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "PATH",
            format!(
                "{}:{}",
                destination.join("bin").display(),
                environment
                    .get("PATH")
                    .map_or("/bin:/usr/bin", String::as_str)
            ),
        ),
    ] {
        environment.insert(name.into(), value);
    }
    // Native hardware helpers use D-Bus directly, so an inherited custom system
    // address must not escape the /run/dbus namespace barrier.
    environment.insert(
        "DBUS_SYSTEM_BUS_ADDRESS".into(),
        format!("unix:path={}/absent-system-bus", runtime.display()),
    );
    args.extend([
        "--ro-bind".into(),
        tree.join(".local/share/nacre")
            .to_string_lossy()
            .into_owned(),
        paths.share.to_string_lossy().into_owned(),
    ]);
    for name in ["palette-runtime", "branding", "ai", "control"] {
        let original = paths.share.join(name);
        if original.is_dir() {
            args.extend([
                "--ro-bind".into(),
                original.canonicalize()?.to_string_lossy().into_owned(),
                original.to_string_lossy().into_owned(),
            ]);
        }
    }
    args.extend([
        "--ro-bind".into(),
        tree.join(".local/bin").to_string_lossy().into_owned(),
        paths.home.join(".local/bin").to_string_lossy().into_owned(),
        "dbus-run-session".into(),
        "--".into(),
        "niri".into(),
        "-c".into(),
        paths
            .config
            .join("niri-on-pc/preview/config.kdl")
            .to_string_lossy()
            .into_owned(),
    ]);
    Ok((args, environment))
}

pub fn prepare_files(
    backend: &Path,
    destination: &Path,
    paths: &Paths,
    keyboard: &Value,
    stage: impl FnOnce(&Path, &Value) -> Result<()>,
    revision: impl FnOnce() -> Result<String>,
) -> Result<()> {
    if destination.is_dir() && fs::read_dir(destination)?.next().is_some() {
        return Err("Preview destination must be empty".into());
    }
    if !destination.exists() {
        fs::create_dir_all(destination)?;
        fs::set_permissions(destination, fs::Permissions::from_mode(0o700))?;
    }
    let profile_path = paths.config.join("profile.json");
    let profile_bytes = fs::read(&profile_path)?;
    let raw_profile: Value = serde_json::from_slice(&profile_bytes)?;
    let tree = destination.join("home-view");
    stage(&tree, &raw_profile)?;
    for name in SAFE_CONFIG {
        private_copy(
            &paths.config.join(name),
            &tree.join(".config/nacre").join(name),
        )?;
    }
    let state = destination.join("state");
    let cache = destination.join("cache");
    for path in [&state, &cache] {
        fs::create_dir(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    for name in SAFE_STATE {
        private_copy(&paths.state.join(name), &state.join(name))?;
    }
    copy_cached_presentation(paths, &state, &cache)?;
    private_copy(
        &paths.cache.join("weather.json"),
        &cache.join("weather.json"),
    )?;
    let config_root = paths.config.parent().ok_or("Invalid configuration root")?;
    private_copy(&config_root.join("kitty"), &tree.join(".config/kitty"))?;
    let kitty_cache = destination.join("kitty-cache");
    fs::create_dir(&kitty_cache)?;
    fs::set_permissions(kitty_cache, fs::Permissions::from_mode(0o700))?;
    let shell = tree.join(".local/share/nacre/shell");
    private_copy(&paths.share.join("shell/bin"), &shell.join("bin"))?;
    let controllers = tree.join(".local/bin");
    fs::create_dir_all(&controllers)?;
    for name in [
        "nacre-shell",
        "nacre-settings",
        "nacre-menu",
        "nacre-app",
        "nacre-brain",
    ] {
        let original = paths.home.join(".local/bin").join(name);
        if original.is_file() {
            let target = controllers.join(name);
            fs::write(&target, fs::read(original)?)?;
            fs::set_permissions(target, fs::Permissions::from_mode(0o700))?;
        }
    }
    let directory = backend.join("nacre/shell-tools");
    let profile = crate::niri_session_stage::validate_profile(&raw_profile)?;
    let contract: Value =
        serde_json::from_slice(&fs::read(directory.join("compositor-contract.json"))?)?;
    let generated = crate::compositor_config::render(&contract, &directory, "niri")?;
    let generated_dir = destination.join("config");
    fs::create_dir(&generated_dir)?;
    fs::set_permissions(&generated_dir, fs::Permissions::from_mode(0o700))?;
    for (name, text) in generated
        .as_object()
        .ok_or("Invalid generated preview configuration")?
    {
        let target = generated_dir.join(
            Path::new(name)
                .file_name()
                .ok_or("Invalid generated file")?,
        );
        fs::write(&target, text.as_str().ok_or("Invalid generated text")?)?;
        fs::set_permissions(target, fs::Permissions::from_mode(0o600))?;
    }
    private_copy(
        &backend.join("niri/wallpaper.kdl"),
        &generated_dir.join("wallpaper.kdl"),
    )?;
    fs::write(
        generated_dir.join("keyboard.kdl"),
        crate::compositor_config::render_keyboard(keyboard, "niri")?,
    )?;
    let count = profile["workspaceCount"]
        .as_u64()
        .ok_or("Invalid workspace count")?;
    let names: BTreeMap<u64, String> = (1..=count)
        .map(|number| {
            (
                number,
                profile["workspaceNames"]
                    .get(number as usize - 1)
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("nacre-{number}")),
            )
        })
        .collect();
    let workspace_names = names
        .iter()
        .map(|(number, name)| (*number as i64, name.clone()))
        .collect();
    fs::write(
        generated_dir.join("profile.kdl"),
        crate::compositor_config::render_profile(
            &profile,
            "niri",
            &workspace_names,
            &contract,
            &directory,
        )?,
    )?;
    let scheme: Value = serde_json::from_slice(&fs::read(state.join("scheme.json"))?)?;
    let mut palette =
        fs::read_to_string(directory.join("compositor_backends/niri/palette.kdl.in"))?;
    for role in ["primary", "secondary", "outlineVariant", "error"] {
        let color = scheme["colours"][role]
            .as_str()
            .ok_or("Invalid palette role")?
            .trim_start_matches('#');
        if ![6, 8].contains(&color.len()) || !color.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Invalid palette role".into());
        }
        palette = palette.replace(&format!("{{{{{role}}}}}"), color);
    }
    fs::write(generated_dir.join("colors.kdl"), palette)?;
    let virtual_config = paths.config.join("niri-on-pc/preview");
    let mut text = String::new();
    for name in [
        "wallpaper.kdl",
        "bindings.kdl",
        "window-rules.kdl",
        "keyboard.kdl",
        "profile.kdl",
        "colors.kdl",
    ] {
        text.push_str(&format!(
            "include {}\n",
            json::encode(&virtual_config.join(name).to_string_lossy())?
        ));
    }
    for name in names.values() {
        text.push_str(&format!("workspace {}\n", json::encode(name)?));
    }
    text.push_str(&format!(
        "spawn-at-startup \"quickshell\" \"-n\" \"-p\" {}\n",
        json::encode(
            &config_root
                .join("quickshell/nacre/shell.qml")
                .to_string_lossy()
        )?
    ));
    text.push_str("hotkey-overlay {\n    skip-at-startup\n}\n");
    text.push_str(&preview_bindings(paths, &names)?);
    fs::write(generated_dir.join("config.kdl"), text)?;
    fs::create_dir_all(tree.join(".config/nacre/niri-on-pc/preview"))?;
    for name in ["palette-runtime", "branding", "ai", "control"] {
        if paths.share.join(name).is_dir() {
            fs::create_dir_all(tree.join(".local/share/nacre").join(name))?;
        }
    }
    let quickshell_config = tree.join(".config/quickshell");
    fs::create_dir_all(&quickshell_config)?;
    std::os::unix::fs::symlink(
        paths.share.join("shell/source"),
        quickshell_config.join("nacre"),
    )?;
    let binary = destination.join("bin");
    fs::create_dir(&binary)?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
    fs::write(binary.join("uwsm"),"#!/bin/sh\nif [ \"$1\" = app ]; then shift; [ \"${1:-}\" != -- ] || shift; exec \"$@\"; fi\necho \"Session management is unavailable in the windowed preview\" >&2\nexit 1\n")?;
    for name in [
        "loginctl",
        "systemctl",
        "powerprofilesctl",
        "brightnessctl",
        "ddcutil",
        "pkexec",
        "sudo",
        "hyprlock",
        "hypridle",
    ] {
        fs::write(binary.join(name),"#!/bin/sh\necho \"System/session actions are disabled in the isolated preview\" >&2\nexit 1\n")?;
    }
    for entry in fs::read_dir(&binary)? {
        fs::set_permissions(entry?.path(), fs::Permissions::from_mode(0o700))?;
    }
    use sha2::{Digest, Sha256};
    let manifest = serde_json::json!({"backendRevision":revision()?.trim(),"profileDigest":format!("{:x}",Sha256::digest(&profile_bytes)),"workspaceCount":profile["workspaceCount"],"customShortcutsCopied":true,"customLuaShortcutsTranslated":false,"mode":"windowed-preview","activated":false});
    fs::write(destination.join("manifest.json"), json::pretty(&manifest)?)?;
    println!("Prepared private Nacre/niri preview; original profile and runtime untouched.");
    Ok(())
}

pub fn prepare(backend: &Path, destination: &Path, paths: &Paths) -> Result<()> {
    let backend = backend.canonicalize()?;
    let directory = backend.join("nacre/shell-tools");
    let mut helper = directory.join("nacre-helper");
    if !helper.is_file() {
        let result = crate::process::checked(
            "bash",
            &[backend
                .join("tools/build-helper.sh")
                .to_string_lossy()
                .into_owned()],
            None,
            std::time::Duration::from_secs(3600),
        )?;
        use std::io::Write;
        std::io::stderr().write_all(&result.stderr)?;
        helper = PathBuf::from(String::from_utf8(result.stdout)?.trim());
    }
    let keyboard = match crate::niri_session_runtime::system_keyboard() {
        Ok(keyboard) => keyboard,
        Err(error) => {
            eprintln!("System keyboard discovery failed; using us defaults: {error}");
            serde_json::json!({"layout":"us","model":"","variant":"","options":""})
        }
    };
    prepare_files(
        &backend,
        destination,
        paths,
        &keyboard,
        |target, profile| {
            let validated = crate::niri_session_stage::validate_profile(profile)?;
            let files = crate::niri_session_stage::file_map(&backend, &validated)?;
            print!(
                "{}",
                crate::niri_session_stage::stage(
                    &backend, target, profile, &files, &directory, &helper
                )?
            );
            Ok(())
        },
        || {
            let result = crate::process::checked(
                "git",
                &[
                    "-C".into(),
                    backend.to_string_lossy().into_owned(),
                    "rev-parse".into(),
                    "HEAD".into(),
                ],
                None,
                std::time::Duration::from_secs(10),
            )?;
            Ok(String::from_utf8(result.stdout)?)
        },
    )
}

#[derive(Debug)]
pub struct Arguments {
    pub action: String,
    pub backend: Option<PathBuf>,
    pub destination: PathBuf,
}
pub fn arguments(args: &[String]) -> std::result::Result<Option<Arguments>, String> {
    let usage = "usage: niri-preview.py [-h] {prepare,run} ...\n";
    let error = |action: Option<&str>, message: &str| {
        let (usage, program) = match action {
            Some("prepare") => (
                "usage: niri-preview.py prepare [-h] --backend BACKEND\n                               --destination DESTINATION\n",
                "niri-preview.py prepare",
            ),
            Some("run") => (
                "usage: niri-preview.py run [-h] --destination DESTINATION\n",
                "niri-preview.py run",
            ),
            _ => (usage, "niri-preview.py"),
        };
        format!("{usage}{program}: error: {message}\n")
    };
    if args
        .first()
        .is_some_and(|arg| ["-h", "--help"].contains(&arg.as_str()))
    {
        return Ok(None);
    }
    let action = args
        .first()
        .ok_or_else(|| error(None, "the following arguments are required: action"))?;
    if !["prepare", "run"].contains(&action.as_str()) {
        return Err(error(
            None,
            &format!("argument action: invalid choice: '{action}' (choose from 'prepare', 'run')"),
        ));
    }
    let mut backend = None;
    let mut destination = None;
    let mut unknown = Vec::new();
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if ["-h", "--help"].contains(&arg.as_str()) {
            return Ok(None);
        }
        let (name, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(name, value)| (name, Some(value)));
        if name == "--destination" || name == "--backend" && action == "prepare" {
            let path = if let Some(path) = inline {
                path
            } else {
                index += 1;
                args.get(index)
                    .filter(|arg| !arg.starts_with('-'))
                    .ok_or_else(|| {
                        error(
                            Some(action),
                            &format!("argument {name}: expected one argument"),
                        )
                    })?
            };
            if name == "--backend" {
                backend = Some(PathBuf::from(path));
            } else {
                destination = Some(PathBuf::from(path));
            }
        } else {
            unknown.push(arg.clone());
        }
        index += 1;
    }
    let mut required = Vec::new();
    if action == "prepare" && backend.is_none() {
        required.push("--backend");
    }
    if destination.is_none() {
        required.push("--destination");
    }
    if !required.is_empty() {
        return Err(error(
            Some(action),
            &format!(
                "the following arguments are required: {}",
                required.join(", ")
            ),
        ));
    }
    if !unknown.is_empty() {
        return Err(error(
            Some(action),
            &format!("unrecognized arguments: {}", unknown.join(" ")),
        ));
    }
    Ok(Some(Arguments {
        action: action.clone(),
        backend,
        destination: destination.ok_or_else(|| error(Some(action), "Missing destination"))?,
    }))
}
pub fn run(args: &[String]) -> Result<i32> {
    let arguments = match arguments(args) {
        Ok(None) => {
            let help = match args.first().map(String::as_str) {
                Some("prepare") => include_str!("../assets/niri-preview-prepare-help.txt"),
                Some("run") => include_str!("../assets/niri-preview-run-help.txt"),
                _ => include_str!("../assets/niri-preview-help.txt"),
            };
            print!("{help}");
            return Ok(0);
        }
        Ok(Some(arguments)) => arguments,
        Err(error) => {
            eprint!("{error}");
            return Ok(2);
        }
    };
    let paths = Paths::environment()?;
    let destination = if arguments.destination.is_absolute() {
        arguments.destination
    } else {
        std::env::current_dir()?.join(arguments.destination)
    };
    if arguments.action == "prepare" {
        prepare(
            &arguments.backend.ok_or("Missing backend")?,
            &destination,
            &paths,
        )?;
        return Ok(0);
    }
    let destination = destination.canonicalize()?;
    if !destination.join("manifest.json").is_file() {
        return Err("Prepare the preview first".into());
    }
    let outer = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid())));
    let runtime = tempfile::Builder::new()
        .prefix("nacre-niri-preview-")
        .tempdir_in(outer)?;
    fs::set_permissions(runtime.path(), fs::Permissions::from_mode(0o700))?;
    let quickshell = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join("quickshell"))
            .find(|path| {
                path.is_file()
                    && fs::metadata(path)
                        .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
            })
    });
    let (args, environment) = command(
        &destination,
        &paths,
        &std::env::vars().collect(),
        runtime.path(),
        quickshell.as_deref(),
    )?;
    let status = std::process::Command::new(&args[0])
        .args(&args[1..])
        .env_clear()
        .envs(environment)
        .status()?;
    Ok(status.code().unwrap_or_else(|| {
        use std::os::unix::process::ExitStatusExt;
        -status.signal().unwrap_or(1)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json as value;
    fn reference() -> Result<Value> {
        Ok(serde_json::from_str(include_str!(
            "../tests/fixtures/niri-preview-reference.json"
        ))?)
    }
    #[test]
    fn frozen_preview_cli_help_required_fields_and_exit_codes() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-preview-cli-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing CLI cases")? {
            let args: Vec<String> = serde_json::from_value(case["args"].clone())?;
            match arguments(&args) {
                Ok(None) => assert_eq!(case["exit"], 0),
                Err(error) => {
                    assert_eq!(case["exit"], 2);
                    assert_eq!(error, case["stderr"]);
                }
                Ok(Some(_)) => panic!("Expected terminal parser outcome"),
            }
        }
        Ok(())
    }
    #[test]
    fn frozen_all_preparation_files_modes_links_and_partial_failure_writes() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-preview-preparation-reference.json"
        ))?;
        let backend = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing preparation cases")?
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let destination = home.path().join("preview");
            for path in [&paths.config, &paths.state, &paths.cache] {
                fs::create_dir_all(path)?;
            }
            let files = [
                ("profile.json", json::encode(&case["profile"])?),
                ("desktop.json", "{}\n".into()),
                ("profile.lua", "custom fixture\n".into()),
                ("shortcuts.lua", "custom shortcuts\n".into()),
                ("auth.json", "excluded credentials fixture\n".into()),
            ];
            for (name, text) in files {
                fs::write(paths.config.join(name), text)?;
            }
            fs::write(
                paths.state.join("scheme.json"),
                json::encode(&case["scheme"])?,
            )?;
            fs::write(
                paths.state.join("presentation.json"),
                json::encode(&value!({"poster":paths.cache.join("poster.png")}))?,
            )?;
            for (name, text) in [
                ("poster.png", "image fixture\n"),
                ("weather.json", "weather fixture\n"),
                ("unrelated.json", "unrelated fixture\n"),
            ] {
                fs::write(paths.cache.join(name), text)?;
            }
            fs::create_dir_all(paths.share.join("shell/bin"))?;
            fs::write(paths.share.join("shell/bin/qs"), "qs fixture\n")?;
            for name in ["palette-runtime", "branding", "ai", "control"] {
                fs::create_dir(paths.share.join(name))?;
            }
            if case["variant"] == "nonempty" {
                fs::create_dir(&destination)?;
                fs::write(destination.join("existing"), "keep existing\n")?;
            }
            let mut calls = Vec::new();
            let result = prepare_files(
                &backend,
                &destination,
                &paths,
                &case["keyboard"],
                |target, profile| {
                    calls.push(value!({"operation":"stage","profile":profile}));
                    fs::create_dir_all(target.join(".config/nacre"))?;
                    fs::create_dir_all(target.join(".local/share/nacre/shell/tools"))?;
                    Ok(())
                },
                || Ok("fixture-revision\n".into()),
            );
            if case["observed"]["error"].is_null() {
                result?;
            } else {
                assert_eq!(
                    result.expect_err("Invalid preview accepted").to_string(),
                    case["observed"]["error"]["message"].as_str().unwrap()
                );
            }
            assert_eq!(value!(calls), case["observed"]["calls"]);
            fn collect(
                root: &Path,
                folder: &Path,
                files: &mut serde_json::Map<String, Value>,
                directories: &mut serde_json::Map<String, Value>,
            ) -> Result<()> {
                for entry in fs::read_dir(folder)? {
                    let path = entry?.path();
                    let relative = path
                        .strip_prefix(root)?
                        .to_str()
                        .ok_or("Invalid fixture path")?
                        .to_owned();
                    if path.is_symlink() {
                        files.insert(relative, value!({"symlink":fs::read_link(path)?}));
                    } else if path.is_file() {
                        files.insert(relative,value!({"text":fs::read_to_string(&path)?,"mode":fs::metadata(path)?.permissions().mode()&0o777}));
                    } else if path.is_dir() {
                        directories.insert(
                            relative,
                            value!(fs::metadata(&path)?.permissions().mode() & 0o777),
                        );
                        collect(root, &path, files, directories)?;
                    }
                }
                Ok(())
            }
            let mut files = serde_json::Map::new();
            let mut directories = serde_json::Map::new();
            collect(&destination, &destination, &mut files, &mut directories)?;
            let actual = serde_json::to_string(&value!({"files":files,"directories":directories}))?
                .replace(home.path().to_str().unwrap(), "$HOME")
                .replace(backend.to_str().unwrap(), "$REPO");
            let actual: Value = serde_json::from_str(&actual)?;
            assert_eq!(
                actual["files"], case["observed"]["files"],
                "{}",
                case["variant"]
            );
            assert_eq!(
                actual["directories"], case["observed"]["directories"],
                "{}",
                case["variant"]
            );
            assert!(!destination
                .join("home-view/.config/nacre/auth.json")
                .exists());
        }
        Ok(())
    }
    #[test]
    fn frozen_private_copies_and_referenced_cache_scope() -> Result<()> {
        for case in reference()?["cases"]
            .as_array()
            .ok_or("Missing copy cases")?
            .iter()
            .filter(|case| case["operation"] == "copy")
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let target = home.path().join("snapshot");
            let variant = case["variant"].as_str().ok_or("Missing copy variant")?;
            if variant == "cache-scope" {
                fs::create_dir_all(&paths.cache)?;
                fs::write(paths.cache.join("poster.png"), "image fixture\n")?;
                fs::write(paths.cache.join("unrelated.json"), "unrelated\n")?;
                fs::write(home.path().join("private.txt"), "outside\n")?;
                std::os::unix::fs::symlink(
                    home.path().join("private.txt"),
                    paths.cache.join("escape.png"),
                )?;
                let state = home.path().join("state");
                fs::create_dir(&state)?;
                fs::write(
                    state.join("presentation.json"),
                    json::encode(
                        &value!({"poster":paths.cache.join("poster.png"),"escape":paths.cache.join("escape.png"),"nested":[paths.cache.join("poster.png")]}),
                    )?,
                )?;
                copy_cached_presentation(&paths, &state, &target)?;
            } else {
                let source = home.path().join("source");
                if variant == "file" {
                    fs::write(&source, "{\"aiFullAccess\":true}\n")?;
                    fs::set_permissions(&source, fs::Permissions::from_mode(0o640))?;
                } else if ["directory", "wallpaper-link"].contains(&variant) {
                    fs::create_dir(&source)?;
                    fs::write(source.join("data.json"), "fixture\n")?;
                    fs::set_permissions(
                        source.join("data.json"),
                        fs::Permissions::from_mode(0o640),
                    )?;
                    if variant == "wallpaper-link" {
                        let asset = home.path().join("poster.png");
                        fs::write(&asset, "image fixture\n")?;
                        std::os::unix::fs::symlink(asset, source.join("current"))?;
                    }
                }
                private_copy(&source, &target)?;
            }
            let mut files = serde_json::Map::new();
            if target.is_file() {
                files.insert(".".into(),value!({"text":fs::read_to_string(&target)?,"mode":fs::metadata(&target)?.permissions().mode()&0o777,"symlink":target.is_symlink()}));
            } else if target.is_dir() {
                for entry in fs::read_dir(&target)? {
                    let path = entry?.path();
                    files.insert(path.file_name().unwrap().to_string_lossy().into_owned(),value!({"text":fs::read_to_string(&path)?,"mode":fs::metadata(&path)?.permissions().mode()&0o777,"symlink":path.is_symlink()}));
                }
            }
            assert_eq!(Value::Object(files), case["observed"], "{variant}");
        }
        Ok(())
    }
    #[test]
    fn frozen_safe_allowlists_and_named_preview_aliases() -> Result<()> {
        for case in reference()?["cases"].as_array().ok_or("Missing cases")? {
            if case["operation"] == "allowlists" {
                assert_eq!(
                    value!({"config":SAFE_CONFIG,"state":SAFE_STATE}),
                    case["observed"]
                );
            }
            if case["operation"] == "preview_bindings" {
                let names = case["names"]
                    .as_object()
                    .ok_or("Missing names")?
                    .iter()
                    .map(|(key, value)| {
                        Ok((
                            key.parse::<u64>()?,
                            value.as_str().ok_or("Invalid workspace")?.to_owned(),
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()?;
                assert_eq!(
                    preview_bindings(
                        &Paths::new(case["home"].as_str().ok_or("Missing home")?),
                        &names
                    )?,
                    case["observed"]
                );
            }
        }
        Ok(())
    }
    #[test]
    fn frozen_namespace_arguments_and_environment_with_system_bus_isolation() -> Result<()> {
        for case in reference()?["cases"]
            .as_array()
            .ok_or("Missing cases")?
            .iter()
            .filter(|case| case["operation"] == "command")
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            if case["managed"] == true {
                fs::create_dir_all(&paths.share)?;
                for name in ["palette-runtime", "branding", "ai", "control"] {
                    let release = home.path().join(format!("release-{name}"));
                    fs::create_dir(&release)?;
                    std::os::unix::fs::symlink(release, paths.share.join(name))?;
                }
            }
            let input = serde_json::to_string(&case["environment"])?
                .replace("$HOME", home.path().to_str().ok_or("Invalid home")?);
            let environment: BTreeMap<String, String> = serde_json::from_str(&input)?;
            let result = command(
                &home.path().join("preview"),
                &paths,
                &environment,
                &home.path().join("runtime"),
                case["which"].as_str().map(Path::new),
            );
            if !case["error"].is_null() {
                assert_eq!(
                    result.expect_err("Missing display accepted").to_string(),
                    case["error"].as_str().unwrap()
                );
                continue;
            }
            let (argv, mut child) = result?;
            assert_eq!(
                child.remove("DBUS_SYSTEM_BUS_ADDRESS"),
                Some(format!(
                    "unix:path={}/runtime/absent-system-bus",
                    home.path().display()
                ))
            );
            let mut expected = case["observed"].clone();
            expected["environment"]
                .as_object_mut()
                .ok_or("Missing expected environment")?
                .remove("DBUS_SYSTEM_BUS_ADDRESS");
            let actual = serde_json::to_string(&value!({"argv":argv,"environment":child}))?
                .replace(home.path().to_str().unwrap(), "$HOME");
            assert_eq!(serde_json::from_str::<Value>(&actual)?, expected);
        }
        Ok(())
    }
}
