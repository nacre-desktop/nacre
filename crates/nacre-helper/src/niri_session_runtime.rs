//! Prepared-session routing and configuration, independent of a running shell.
use crate::{json, paths::Paths, Result};
use serde_json::{json as value, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
pub fn command(
    action: &str,
    environment: &BTreeMap<String, String>,
    paths: &Paths,
    mut companion: impl FnMut(&Paths) -> Option<PathBuf>,
) -> Result<(Vec<String>, BTreeMap<String, String>)> {
    let mut environment = environment.clone();
    let original = if action == "brain" {
        vec![
            "/usr/bin/python3".into(),
            paths
                .share
                .join("brain/control.py")
                .to_string_lossy()
                .into_owned(),
            "watch-brain".into(),
        ]
    } else {
        vec![
            paths
                .home
                .join(".local/bin/nacre-shell")
                .to_string_lossy()
                .into_owned(),
            "start".into(),
        ]
    };
    let context = crate::niri_session_gate::compositor(&environment);
    if ["shell", "brain"].contains(&action) && context == "hyprland" {
        return Ok((original, environment));
    }
    if context != "niri" {
        return Err("Niri runtime requires an explicit niri session".into());
    }
    let root = paths.share.join("niri-on-pc/session");
    let shell = root.join("home-view/.local/share/nacre/shell");
    if [
        "source/shell.qml",
        "tools/nacre_native_compositor.py",
        "bin/qs",
    ]
    .iter()
    .any(|name| !shell.join(name).is_file())
    {
        return Err("Prepare the separate niri source before logging in".into());
    }
    let mut args = vec![
        "bwrap".into(),
        "--bind".into(),
        "/".into(),
        "/".into(),
        "--dev-bind".into(),
        "/dev".into(),
        "/dev".into(),
        "--proc".into(),
        "/proc".into(),
        "--ro-bind".into(),
        shell.to_string_lossy().into_owned(),
        paths.share.join("shell").to_string_lossy().into_owned(),
        "--die-with-parent".into(),
    ];
    for (name, value) in [
        ("NACRE_COMPOSITOR", "niri".into()),
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
            paths
                .config
                .parent()
                .ok_or("Invalid config root")?
                .join("quickshell/nacre/shell.qml")
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "NACRE_NIRI_CONFIG",
            root.join("config/config.kdl")
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "NACRE_NIRI_DESKTOP_CONFIG",
            root.join("config/desktop.kdl")
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "NACRE_NIRI_TOOLS_SOURCE",
            shell.join("tools").to_string_lossy().into_owned(),
        ),
    ] {
        environment.insert(name.into(), value);
    }
    let hover = if ["widths", "widths-check"].contains(&action) || root.join("stock-niri").is_file()
    {
        None
    } else {
        companion(paths)
    };
    environment.insert(
        "NACRE_NIRI_HOVER_STREAM".into(),
        if hover.is_some() { "1" } else { "0" }.into(),
    );
    match action {
        "compositor" => args.extend([
            hover
                .unwrap_or_else(|| PathBuf::from("/usr/bin/niri"))
                .to_string_lossy()
                .into_owned(),
            "--session".into(),
        ]),
        "brain" => args.extend(original),
        "widths" | "widths-check" => {
            args.extend([
                paths
                    .tools()
                    .join("nacre-helper")
                    .to_string_lossy()
                    .into_owned(),
                "niri_widths".into(),
            ]);
            if action == "widths-check" {
                args.push("--check-session".into());
            }
        }
        _ => args.extend([
            "/usr/bin/python3".into(),
            paths
                .tools()
                .join("shell-supervisor.py")
                .to_string_lossy()
                .into_owned(),
        ]),
    }
    Ok((args, environment))
}
pub const HOVER_SOURCE: &str = "8ed0da44d974c32c6877d2f4630c314da0717ecb";
pub trait Versions {
    fn version(&mut self, binary: &Path) -> Result<String>;
}
pub struct NativeVersions;
impl Versions for NativeVersions {
    fn version(&mut self, binary: &Path) -> Result<String> {
        let output = crate::process::checked(
            binary.to_str().ok_or("Invalid compositor path")?,
            &["--version".into()],
            None,
            Duration::from_secs(2),
        )?;
        Ok(String::from_utf8(output.stdout)?)
    }
}
pub fn verified_hover_binary(
    root: &Path,
    owner: u32,
    packaged: bool,
    versions: &mut impl Versions,
) -> Result<Option<PathBuf>> {
    let receipt = root.join("manifest.json");
    let binary = root.join("niri");
    let meta = match fs::symlink_metadata(&receipt) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let binary_meta = match fs::symlink_metadata(&binary) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if meta.file_type().is_symlink()
        || binary_meta.file_type().is_symlink()
        || !binary_meta.is_file()
        || meta.uid() != owner
        || binary_meta.uid() != owner
        || binary_meta.permissions().mode() & 0o111 == 0
        || packaged && (meta.permissions().mode() | binary_meta.permissions().mode()) & 0o022 != 0
    {
        return Ok(None);
    }
    let manifest: Value = serde_json::from_slice(&fs::read(&receipt)?)?;
    if !manifest.is_object()
        || !(manifest["schema"] == true || manifest["schema"].as_f64() == Some(1.0))
        || manifest["enabled"] != true
        || !packaged && manifest["nestedVerified"] != true
        || manifest["sourceRevision"] != HOVER_SOURCE
        || manifest["protocol"] != "NacreOverviewHoverStream"
    {
        return Ok(None);
    }
    if !packaged {
        let digest = format!("{:x}", Sha256::digest(fs::read(&binary)?));
        if manifest["binarySha256"] != digest {
            return Ok(None);
        }
        if !versions
            .version(Path::new("/usr/bin/niri"))?
            .starts_with("niri 26.04 ")
        {
            return Ok(None);
        }
    }
    Ok(versions
        .version(&binary)?
        .starts_with("niri 26.04 ")
        .then_some(binary))
}
pub fn overview_hover_binary(paths: &Paths, versions: &mut impl Versions) -> Option<PathBuf> {
    select_companion(paths, |root, owner, packaged| {
        match verified_hover_binary(root, owner, packaged, versions) {
            Ok(binary) => binary,
            Err(error) => {
                eprintln!("nacre-helper: compositor trust probe failed: {error}");
                None
            }
        }
    })
}
pub fn select_companion(
    paths: &Paths,
    mut verify: impl FnMut(&Path, u32, bool) -> Option<PathBuf>,
) -> Option<PathBuf> {
    for (root, owner, packaged) in [
        (PathBuf::from("/usr/lib/nacre/niri"), 0, true),
        (
            paths.share.join("niri/overview-hover"),
            nix::unistd::getuid().as_raw(),
            false,
        ),
        (
            paths.share.join("niri-on-pc/overview-hover"),
            nix::unistd::getuid().as_raw(),
            false,
        ),
    ] {
        if let Some(binary) = verify(&root, owner, packaged) {
            return Some(binary);
        }
    }
    None
}

fn numeric(value: &Value, message: &str, min: f64, max: f64, strict_min: bool) -> Result<()> {
    let valid = value.as_f64().is_some_and(|number| {
        number.is_finite()
            && if strict_min {
                number > min && number <= max
            } else {
                number >= min && number <= max
            }
    });
    if valid {
        Ok(())
    } else {
        Err(message.into())
    }
}
pub fn host_preferences(desktop: &Value) -> Result<String> {
    let natural = desktop
        .get("naturalScroll")
        .cloned()
        .unwrap_or(value!(true));
    let natural = natural
        .as_bool()
        .ok_or("Invalid natural scrolling preference")?;
    let gap = desktop.get("gapsOut").cloned().unwrap_or(value!(12));
    let border = desktop.get("borderSize").cloned().unwrap_or(value!(1));
    for value in [&gap, &border] {
        numeric(value, "Invalid saved window margins", 0.0, 100.0, false)?;
    }
    let mut text=format!("prefer-no-csd\nlayout {{\n    gaps {}\n    default-column-width {{ proportion 0.5; }}\n    preset-column-widths {{\n        proportion 0.33333\n        proportion 0.5\n        proportion 0.66667\n        proportion 1.0\n    }}\n    focus-ring {{ width {}; }}\n}}\ninput {{\n    touchpad {{\n",json::encode(&gap)?,json::encode(&border)?);
    if natural {
        text.push_str("        natural-scroll\n");
    }
    text.push_str("        scroll-factor 0.25\n    }\n}\n");
    if let Some(displays) = desktop.get("displays") {
        for display in displays.as_array().ok_or("Invalid saved display scale")? {
            if display.get("disabled").is_some_and(truthy) {
                continue;
            }
            let name = display["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or("Invalid saved display scale")?;
            numeric(
                &display["scale"],
                "Invalid saved display scale",
                0.0,
                10.0,
                true,
            )?;
            text.push_str(&format!(
                "output {} {{\n    scale {}\n}}\n",
                json::encode(&name)?,
                json::encode(&display["scale"])?
            ));
        }
    }
    text.push_str("window-rule {\n    match app-id=\"^nacre-ai-(dashboard|task)$\"\n    default-column-width { proportion 0.5; }\n}\nbinds {\n    Super+Ctrl+Tab { toggle-overview; }\n    Super+M { maximize-column; }\n    Super+R { switch-preset-column-width; }\n    Super+Shift+WheelScrollUp cooldown-ms=150 { focus-column-left; }\n    Super+Shift+WheelScrollDown cooldown-ms=150 { focus-column-right; }\n}\n");
    Ok(text)
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
pub fn palette_include(paths: &Paths, config: &Path) -> PathBuf {
    let published = paths.config.join("niri/palette.kdl");
    if published.is_file() {
        published
    } else {
        config.join("colors.kdl")
    }
}

/// Write in the original order, retaining its partial-output failure semantics.
pub fn prepare_configuration(
    backend: &Path,
    destination: &Path,
    paths: &Paths,
    profile: &Value,
    keyboard: &Value,
    stock: bool,
    validate: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let directory = backend.join("nacre/shell-tools");
    let contract: Value =
        serde_json::from_slice(&fs::read(directory.join("compositor-contract.json"))?)?;
    let count = profile["workspaceCount"]
        .as_u64()
        .ok_or("Invalid workspace count")?;
    let names: Vec<String> = (1..=count)
        .map(|index| {
            profile["workspaceNames"]
                .get(index as usize - 1)
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("nacre-{index}"))
        })
        .collect();
    let config = destination.join("config");
    fs::create_dir(&config)?;
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700))?;
    let generated = crate::compositor_config::render(&contract, &directory, "niri")?;
    for (name, text) in generated
        .as_object()
        .ok_or("Invalid generated configuration")?
    {
        let basename = Path::new(name)
            .file_name()
            .ok_or("Invalid generated file")?;
        fs::write(
            config.join(basename),
            text.as_str().ok_or("Invalid generated text")?,
        )?;
    }
    fs::write(
        config.join("wallpaper.kdl"),
        fs::read(backend.join("niri/wallpaper.kdl"))?,
    )?;
    fs::write(
        config.join("keyboard.kdl"),
        crate::compositor_config::render_keyboard(keyboard, "niri")?,
    )?;
    let workspace_names = names
        .iter()
        .enumerate()
        .map(|(index, name)| (index as i64 + 1, name.clone()))
        .collect();
    fs::write(
        config.join("profile.kdl"),
        crate::compositor_config::render_profile(
            profile,
            "niri",
            &workspace_names,
            &contract,
            &directory,
        )?,
    )?;
    let scheme: Value = serde_json::from_slice(&fs::read(paths.state.join("scheme.json"))?)?;
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
    fs::write(config.join("colors.kdl"), palette)?;
    let desktop_path = paths.config.join("desktop.json");
    let desktop: Value = match fs::read(desktop_path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => value!({}),
        Err(error) => return Err(error.into()),
    };
    let mut host = desktop.clone();
    host.as_object_mut()
        .ok_or("Invalid desktop preferences")?
        .insert("displays".into(), value!([]));
    fs::write(config.join("host.kdl"), host_preferences(&host)?)?;
    let mut preferences = crate::compositor::niri_config::desktop(&desktop)?;
    if let Some(saved) = desktop
        .get("compositorDisplays")
        .and_then(|outputs| outputs.get("niri"))
    {
        if !saved.is_null() {
            preferences.push_str(&crate::compositor::niri_config::monitors(saved)?);
        } else {
            append_scales(&mut preferences, &desktop)?;
        }
    } else {
        append_scales(&mut preferences, &desktop)?;
    }
    fs::write(config.join("desktop.kdl"), preferences)?;
    let mut text = String::new();
    for name in [
        "wallpaper.kdl",
        "bindings.kdl",
        "window-rules.kdl",
        "keyboard.kdl",
        "profile.kdl",
        "colors.kdl",
        "host.kdl",
    ] {
        let path = if name == "colors.kdl" {
            palette_include(paths, &config)
        } else {
            config.join(name)
        };
        text.push_str(&format!(
            "include {}\n",
            json::encode(&path.to_string_lossy())?
        ));
    }
    text.push_str("include \"desktop.kdl\"\n");
    for name in names {
        text.push_str(&format!("workspace {}\n", json::encode(&name)?));
    }
    text.push_str("hotkey-overlay {\n    skip-at-startup\n}\n");
    fs::write(config.join("config.kdl"), text)?;
    for file in fs::read_dir(&config)? {
        fs::set_permissions(file?.path(), fs::Permissions::from_mode(0o600))?;
    }
    validate(&config.join("config.kdl"))?;
    if stock {
        let marker = destination.join("stock-niri");
        fs::write(&marker, "Prepared with --stock-niri\n")?;
        fs::set_permissions(marker, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn append_scales(preferences: &mut String, desktop: &Value) -> Result<()> {
    if let Some(displays) = desktop.get("displays") {
        for display in displays.as_array().ok_or("Invalid saved display scale")? {
            if display.get("disabled").is_some_and(truthy) {
                continue;
            }
            let name = display["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or("Invalid saved display scale")?;
            numeric(
                &display["scale"],
                "Invalid saved display scale",
                0.0,
                10.0,
                true,
            )?;
            preferences.push_str(&format!(
                "output {} {{ scale {}; }}\n",
                json::encode(&name)?,
                json::encode(&display["scale"])?
            ));
        }
    }
    Ok(())
}

/// locale1 is the same system source used by localectl, without starting it.
pub fn system_keyboard() -> Result<Value> {
    let connection = zbus::blocking::connection::Builder::system()?
        .method_timeout(Duration::from_secs(3))
        .build()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.locale1",
        "/org/freedesktop/locale1",
        "org.freedesktop.locale1",
    )?;
    let mut keyboard = value!({"layout":"us", "model":"", "variant":"", "options":""});
    for (key, property) in [
        ("layout", "X11Layout"),
        ("model", "X11Model"),
        ("variant", "X11Variant"),
        ("options", "X11Options"),
    ] {
        let text: String = proxy.get_property(property)?;
        if !text.is_empty()
            && text.len() <= 200
            && text != "n/a"
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_+:,.-".contains(&byte))
        {
            keyboard[key] = value!(text);
        }
    }
    Ok(keyboard)
}

pub fn prepare(backend: &Path, destination: &Path, paths: &Paths, stock: bool) -> Result<()> {
    let backend = backend.canonicalize()?;
    let profile: Value = serde_json::from_slice(&fs::read(paths.config.join("profile.json"))?)?;
    let compiler = backend.join("nacre/shell-tools");
    let mut helper = compiler.join("nacre-helper");
    if !helper.is_file() {
        let output = crate::process::checked(
            "bash",
            &[backend
                .join("tools/build-helper.sh")
                .to_string_lossy()
                .into_owned()],
            None,
            Duration::from_secs(3600),
        )?;
        use std::io::Write;
        std::io::stderr().write_all(&output.stderr)?;
        helper = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    }
    let files = crate::niri_session_stage::file_map(&backend, &profile)?;
    print!(
        "{}",
        crate::niri_session_stage::stage(
            &backend,
            &destination.join("home-view"),
            &profile,
            &files,
            &compiler,
            &helper
        )?
    );
    let bins = paths.share.join("shell/bin");
    if bins.is_dir() {
        crate::niri_session_stage::copy_tree(
            &bins,
            &destination.join("home-view/.local/share/nacre/shell/bin"),
            &[],
        )?;
    }
    let keyboard = match system_keyboard() {
        Ok(keyboard) => keyboard,
        Err(error) => {
            eprintln!("System keyboard discovery failed; using us defaults: {error}");
            value!({"layout":"us", "model":"", "variant":"", "options":""})
        }
    };
    prepare_configuration(
        &backend,
        destination,
        paths,
        &profile,
        &keyboard,
        stock,
        |config| {
            let status = std::process::Command::new("/usr/bin/niri")
                .args(["validate", "-c"])
                .arg(config)
                .status()?;
            if !status.success() {
                return Err(format!("Niri configuration validation failed: {status}").into());
            }
            Ok(())
        },
    )?;
    let revision = crate::process::checked(
        "git",
        &[
            "-C".into(),
            backend.to_string_lossy().into_owned(),
            "rev-parse".into(),
            "HEAD".into(),
        ],
        None,
        Duration::from_secs(10),
    )?;
    fs::write(destination.join("source-revision"), revision.stdout)?;
    Ok(())
}

const ACTIONS: &[&str] = &[
    "prepare",
    "compositor",
    "shell",
    "brain",
    "widths",
    "widths-check",
];
const USAGE: &str = "usage: niri-session-runtime.py [-h] [--backend BACKEND] [--stock-niri]\n                               {prepare,compositor,shell,brain,widths,widths-check}\n";
#[derive(Debug)]
pub struct Arguments {
    pub action: String,
    pub backend: Option<PathBuf>,
    pub stock: bool,
}
pub fn arguments(args: &[String]) -> std::result::Result<Option<Arguments>, String> {
    let error = |message: &str| format!("{USAGE}niri-session-runtime.py: error: {message}\n");
    let mut action = None;
    let mut backend = None;
    let mut stock = false;
    let mut unknown = Vec::new();
    let mut index = 0;
    let mut positional = false;
    while index < args.len() {
        let argument = &args[index];
        let (option, explicit) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(option, value)| {
                (option, Some(value))
            });
        let option = if !positional && option.starts_with("--") && option.len() > 2 {
            ["--help", "--backend", "--stock-niri"]
                .into_iter()
                .find(|name| name.starts_with(option))
                .unwrap_or(option)
        } else {
            option
        };
        if !positional && argument == "--" {
            positional = true;
        } else if !positional && ["--help", "-h"].contains(&option) {
            if let Some(value) = explicit {
                return Err(error(&format!(
                    "argument -h/--help: ignored explicit argument '{value}'"
                )));
            }
            return Ok(None);
        } else if !positional && option == "--stock-niri" {
            if let Some(value) = explicit {
                return Err(error(&format!(
                    "argument --stock-niri: ignored explicit argument '{value}'"
                )));
            }
            stock = true;
        } else if !positional && option == "--backend" {
            if let Some(path) = explicit {
                backend = Some(PathBuf::from(path));
                index += 1;
                continue;
            }
            index += 1;
            let path = args
                .get(index)
                .filter(|arg| !arg.starts_with('-'))
                .ok_or_else(|| error("argument --backend: expected one argument"))?;
            backend = Some(PathBuf::from(path));
        } else if !positional && argument.starts_with('-') {
            unknown.push(argument.clone());
        } else if action.is_none() {
            if !ACTIONS.contains(&argument.as_str()) {
                return Err(error(&format!("argument action: invalid choice: '{}' (choose from 'prepare', 'compositor', 'shell', 'brain', 'widths', 'widths-check')", argument)));
            }
            action = Some(argument.clone());
        } else {
            unknown.push(argument.clone());
        }
        index += 1;
    }
    let action = action.ok_or_else(|| error("the following arguments are required: action"))?;
    if !unknown.is_empty() {
        return Err(error(&format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    if action == "prepare" && backend.is_none() {
        return Err(error("prepare requires --backend"));
    }
    Ok(Some(Arguments {
        action,
        backend,
        stock,
    }))
}
pub fn run(args: &[String]) -> Result<i32> {
    let parsed = match arguments(args) {
        Ok(None) => {
            print!(
                "{}",
                include_str!("../assets/niri-session-runtime-help.txt")
            );
            return Ok(0);
        }
        Ok(Some(parsed)) => parsed,
        Err(error) => {
            eprint!("{error}");
            return Ok(2);
        }
    };
    let paths = Paths::environment()?;
    let environment: BTreeMap<_, _> = std::env::vars().collect();
    if parsed.action == "prepare" {
        let backend = parsed
            .backend
            .ok_or("prepare requires --backend")?
            .canonicalize()?;
        if !parsed.stock {
            crate::niri_hover_install::ensure(&backend, &paths, &environment)?;
        }
        prepare(
            &backend,
            &paths.share.join("niri-on-pc/session"),
            &paths,
            parsed.stock,
        )?;
        return Ok(0);
    }
    let (args, environment) = command(&parsed.action, &environment, &paths, |paths| {
        overview_hover_binary(paths, &mut NativeVersions)
    })?;
    use std::os::unix::process::CommandExt;
    Err(std::process::Command::new(&args[0])
        .args(&args[1..])
        .env_clear()
        .envs(environment)
        .exec()
        .into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_original_argument_parser_help_errors_and_exit_codes() -> Result<()> {
        for source in [
            include_str!("../tests/fixtures/niri-session-runtime-cli-reference.json"),
            include_str!("../tests/fixtures/niri-session-runtime-cli-extra-reference.json"),
        ] {
            let reference: Value = serde_json::from_str(source)?;
            for case in reference["cases"].as_array().ok_or("Missing CLI cases")? {
                let args: Vec<String> = serde_json::from_value(case["arguments"].clone())?;
                match arguments(&args) {
                    Ok(None) => {
                        assert_eq!(
                            case["stdout"],
                            include_str!("../assets/niri-session-runtime-help.txt")
                        );
                        assert_eq!(case["exit"], 0);
                    }
                    Err(error) => {
                        assert_eq!(error, case["stderr"]);
                        assert_eq!(case["exit"], 2);
                    }
                    Ok(Some(_)) => panic!("Expected parser terminal outcome"),
                }
            }
        }
        Ok(())
    }
    #[test]
    fn frozen_preparation_bytes_modes_and_partial_failure_writes() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-preparation-reference.json"
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
            fs::create_dir_all(&paths.config)?;
            fs::create_dir_all(&paths.state)?;
            fs::write(
                paths.config.join("desktop.json"),
                serde_json::to_vec(&case["desktop"])?,
            )?;
            fs::write(
                paths.state.join("scheme.json"),
                serde_json::to_vec(&case["scheme"])?,
            )?;
            if case["live"] == true {
                fs::create_dir_all(paths.config.join("niri"))?;
                fs::write(paths.config.join("niri/palette.kdl"), "live palette\n")?;
            }
            let destination = home.path().join("stage");
            fs::create_dir_all(destination.join("home-view"))?;
            let mut validated = false;
            let result = prepare_configuration(
                &backend,
                &destination,
                &paths,
                &case["profile"],
                &case["keyboard"],
                case["stock"] == true,
                |_| {
                    validated = true;
                    Ok(())
                },
            );
            let expected = &case["observed"];
            if expected["error"].is_null() {
                result?;
                assert!(validated, "{}", case["name"]);
                fs::write(destination.join("source-revision"), "fixture-revision\n")?;
            } else {
                assert_eq!(
                    result.expect_err("Invalid fixture accepted").to_string(),
                    expected["error"]["message"].as_str().unwrap(),
                    "{}",
                    case["name"]
                );
                assert!(!validated);
            }
            let mut files = serde_json::Map::new();
            if destination.join("source-revision").is_file() {
                files.insert("source-revision".into(), value!({"text":"fixture-revision\n", "mode": fs::metadata(destination.join("source-revision"))?.permissions().mode() & 0o777}));
            }
            for entry in fs::read_dir(destination.join("config"))? {
                let file = entry?.path();
                let text = fs::read_to_string(&file)?
                    .replace(home.path().to_str().unwrap(), "$HOME")
                    .replace(backend.to_str().unwrap(), "$REPO");
                files.insert(format!("config/{}", file.file_name().unwrap().to_string_lossy()), value!({"text":text, "mode": fs::metadata(&file)?.permissions().mode() & 0o777}));
            }
            if destination.join("stock-niri").is_file() {
                files.insert(
                    "stock-niri".into(),
                    value!({"text":"Prepared with --stock-niri\n", "mode":0o600}),
                );
            }
            assert_eq!(Value::Object(files), expected["files"], "{}", case["name"]);
            assert_eq!(
                fs::metadata(destination.join("config"))?
                    .permissions()
                    .mode()
                    & 0o777,
                expected["directories"]["config"].as_u64().unwrap() as u32
            );
        }
        Ok(())
    }
    #[test]
    fn frozen_routing_overlay_environment_and_companion_selection() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-runtime-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing runtime cases")?
            .iter()
            .filter(|case| case["request"]["operation"] == "command")
        {
            let request = &case["request"];
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let root = paths.share.join("niri-on-pc/session");
            let shell = root.join("home-view/.local/share/nacre/shell");
            if request["ready"] == true {
                for name in [
                    "source/shell.qml",
                    "tools/nacre_native_compositor.py",
                    "bin/qs",
                ] {
                    let file = shell.join(name);
                    fs::create_dir_all(file.parent().ok_or("Invalid fixture path")?)?;
                    fs::write(file, "")?;
                }
            }
            if request["stock"] == true {
                fs::write(root.join("stock-niri"), "stock\n")?;
            }
            let environment: BTreeMap<String, String> =
                serde_json::from_value(request["environment"].clone())?;
            let mut calls = vec![];
            let result = command(
                request["action"].as_str().ok_or("Missing action")?,
                &environment,
                &paths,
                |_| {
                    calls.push(value!({"method":"overview_hover_binary"}));
                    (request["companion"] == true).then(|| PathBuf::from("/companion/niri"))
                },
            );
            let observed = match result {
                Ok((argv, environment)) => {
                    value!({"value":{"argv":argv,"environment":environment},"error":null})
                }
                Err(error) => {
                    value!({"value":null,"error":{"type":"ValueError","message":error.to_string()}})
                }
            };
            fn normalize(value: Value, home: &str) -> Value {
                match value {
                    Value::String(text) => value!(text.replace(home, "{home}")),
                    Value::Array(values) => Value::Array(
                        values
                            .into_iter()
                            .map(|value| normalize(value, home))
                            .collect(),
                    ),
                    Value::Object(values) => Value::Object(
                        values
                            .into_iter()
                            .map(|(key, value)| (key, normalize(value, home)))
                            .collect(),
                    ),
                    other => other,
                }
            }
            let observed = normalize(observed, home.path().to_str().ok_or("Invalid home")?);
            assert_eq!(
                observed["value"], case["observed"]["value"],
                "{}",
                request["name"]
            );
            assert_eq!(
                observed["error"], case["observed"]["error"],
                "{}",
                request["name"]
            );
            assert_eq!(
                value!(calls),
                case["observed"]["calls"],
                "{}",
                request["name"]
            );
        }
        Ok(())
    }
    #[test]
    fn frozen_companion_trust_checks_and_offline_palette_paths() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-runtime-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing runtime cases")?
        {
            let request = &case["request"];
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            if request["operation"] == "palette_include" {
                if request["live"] == true {
                    fs::create_dir_all(paths.config.join("niri"))?;
                    fs::write(paths.config.join("niri/palette.kdl"), "palette\n")?;
                }
                let rendered = palette_include(&paths, &home.path().join("stage/config"))
                    .to_string_lossy()
                    .replace(home.path().to_str().ok_or("Invalid home")?, "{home}");
                assert_eq!(rendered, case["observed"]["value"]);
                continue;
            }
            if request["operation"] != "verified_hover_binary" {
                continue;
            }
            let variant = request["variant"].as_str().ok_or("Missing trust variant")?;
            let root = home.path().join("companion");
            fs::create_dir(&root)?;
            let binary = root.join("niri");
            fs::write(&binary, "verified companion\n")?;
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
            let mut manifest = value!({"schema":1,"enabled":true,"nestedVerified":true,"sourceRevision":HOVER_SOURCE,"protocol":"NacreOverviewHoverStream","binarySha256":format!("{:x}",Sha256::digest(fs::read(&binary)?))});
            match variant {
                "changed-binary" => fs::write(&binary, "changed\n")?,
                "no-nested-proof" => manifest["nestedVerified"] = value!(false),
                "wrong-source" => manifest["sourceRevision"] = value!("untrusted"),
                "wrong-protocol" => manifest["protocol"] = value!("Other"),
                "disabled" => manifest["enabled"] = value!(false),
                "not-executable" => {
                    fs::set_permissions(&binary, fs::Permissions::from_mode(0o600))?
                }
                _ => {}
            }
            let receipt = root.join("manifest.json");
            fs::write(&receipt, json::encode(&manifest)?)?;
            fs::set_permissions(&receipt, fs::Permissions::from_mode(0o600))?;
            if variant == "binary-symlink" {
                fs::rename(&binary, root.join("target"))?;
                std::os::unix::fs::symlink(root.join("target"), &binary)?;
            }
            if variant == "receipt-symlink" {
                fs::rename(&receipt, root.join("receipt"))?;
                std::os::unix::fs::symlink(root.join("receipt"), &receipt)?;
            }
            if variant == "packaged-writable" {
                fs::set_permissions(&receipt, fs::Permissions::from_mode(0o622))?;
            }
            struct Fixture<'a> {
                variant: &'a str,
                binary: PathBuf,
                home: &'a str,
                calls: Vec<Value>,
            }
            impl Versions for Fixture<'_> {
                fn version(&mut self, binary: &Path) -> Result<String> {
                    self.calls.push(value!({"method":"check_output","args":[binary.to_string_lossy().replace(self.home,"{home}"),"--version"],"kwargs":{"text":true,"timeout":2}}));
                    Ok(if self.variant == "stock-new-version"
                        && binary == Path::new("/usr/bin/niri")
                        || self.variant == "candidate-new-version" && binary == self.binary
                    {
                        "niri 26.12 (new)"
                    } else {
                        "niri 26.04 (fixture)"
                    }
                    .into())
                }
            }
            let mut fixture = Fixture {
                variant,
                binary: binary.clone(),
                home: home.path().to_str().ok_or("Invalid home")?,
                calls: vec![],
            };
            let result = verified_hover_binary(
                &root,
                nix::unistd::getuid().as_raw(),
                variant.starts_with("packaged"),
                &mut fixture,
            )?;
            let result = result.map(|path| path.to_string_lossy().replace(fixture.home, "{home}"));
            assert_eq!(
                serde_json::to_value(result)?,
                case["observed"]["value"],
                "{variant}"
            );
            assert_eq!(
                value!(fixture.calls),
                case["observed"]["calls"],
                "{variant}"
            );
        }
        Ok(())
    }
    #[test]
    fn frozen_host_preference_text_and_validation() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-runtime-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing runtime cases")?
            .iter()
            .filter(|case| case["request"]["operation"] == "host_preferences")
        {
            let observed = match host_preferences(&case["request"]["desktop"]) {
                Ok(text) => value!({"value":text,"error":null}),
                Err(error) => {
                    value!({"value":null,"error":{"type":"ValueError","message":error.to_string()}})
                }
            };
            assert_eq!(
                observed["value"], case["observed"]["value"],
                "{}",
                case["request"]["name"]
            );
            assert_eq!(
                observed["error"], case["observed"]["error"],
                "{}",
                case["request"]["name"]
            );
        }
        Ok(())
    }
}
