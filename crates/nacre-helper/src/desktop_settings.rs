//! Desktop preferences, reversible connected-output changes and existing presets.
use crate::Result;
use serde_json::{json, Value};
use std::sync::OnceLock;

pub const INPUT_KEYS: &[&str] = &[
    "keyRepeatDelay",
    "keyRepeatRate",
    "mouseSpeed",
    "mouseAcceleration",
    "mouseLeftHanded",
    "mouseNaturalScroll",
    "touchpadSpeed",
    "touchpadTap",
    "touchpadTypingGuard",
];
pub fn defaults() -> &'static Value {
    static DEFAULTS: OnceLock<Value> = OnceLock::new();
    DEFAULTS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../nacre/shell-tools/desktop-defaults.json"
        ))
        .expect("checked desktop defaults")
    })
}
pub fn preset_keys() -> Vec<String> {
    defaults()
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| {
            !INPUT_KEYS.contains(&key.as_str())
                && ![
                    "language",
                    "monoFont",
                    "niriWorkspaceMode",
                    "workspacePrimaryOutput",
                    "monitorWorkspaceActions",
                ]
                .contains(&key.as_str())
                && !["lock", "weather", "interface"]
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
        })
        .cloned()
        .collect()
}
pub fn validate(key: &str, value: &Value, developer: bool) -> Result<Value> {
    validate_with(defaults(), key, value, developer)
}
fn validate_with(defaults: &Value, key: &str, value: &Value, developer: bool) -> Result<Value> {
    if defaults.get(key).is_none() {
        return Err("Unknown setting".into());
    }
    let range = match key {
        "gapsIn" => (0, 30),
        "gapsOut" => (0, 80),
        "borderSize" => (0, 8),
        "rounding" => (0, 40),
        "frameWidth" => (0, 30),
        "frameRounding" => (0, 40),
        "lipHoverDelay" => (0, 800),
        "keyRepeatDelay" => (100, 1000),
        "keyRepeatRate" => (1, 80),
        "textSize" => (9, 18),
        _ => (0, 0),
    };
    match key {
        "mouseSpeed" | "touchpadSpeed" => {
            if !value
                .as_f64()
                .is_some_and(|number| (-1.0..=1.0).contains(&number))
            {
                return Err("Pointer speed must be from -1 to 1".into());
            }
        }
        "mouseAcceleration" => {
            if !matches!(value.as_str(), Some("adaptive" | "flat")) {
                return Err("Choose Adaptive or Flat acceleration".into());
            }
        }
        _ if range != (0, 0) => {
            if !value
                .as_i64()
                .is_some_and(|number| (range.0..=range.1).contains(&number))
            {
                return Err(
                    format!("{key} must be an integer from {} to {}", range.0, range.1).into(),
                );
            }
        }
        "niriWorkspaceMode" => {
            if !matches!(value.as_str(), Some("primary-dynamic" | "fixed-all")) {
                return Err(
                    "Choose primary with dynamic secondary monitors, or fixed sets on all monitors"
                        .into(),
                );
            }
        }
        "language" => {
            if !matches!(value.as_str(), Some("system" | "en")) && !(value == "pseudo" && developer)
            {
                return Err("Choose System default or English".into());
            }
        }
        "weatherLocation" | "interfaceFont" | "monoFont" | "workspacePrimaryOutput" => {
            if !value.as_str().is_some_and(|text| {
                text.chars().count() <= 80 && !text.chars().any(|c| (c as u32) < 32)
            }) {
                return Err("Use text up to80characters without control characters".into());
            }
        }
        _ => {
            if !value.is_boolean() {
                return Err(format!("{key} must be true or false").into());
            }
        }
    }
    Ok(value.clone())
}
pub fn display_plan(monitors: &[Value], primary: &str, mode: &str) -> Result<Value> {
    if !["extend-right", "extend-left", "mirror"].contains(&mode) {
        return Err("Unsupported display arrangement".into());
    }
    if monitors.len() < 2 {
        return Err("Connect a second display first".into());
    }
    let mut main = monitors
        .iter()
        .find(|monitor| monitor["name"] == primary)
        .cloned()
        .ok_or("Selected display is disconnected")?;
    main["x"] = json!(0);
    main["y"] = json!(0);
    main["mirrorOf"] = json!("none");
    let width = |monitor: &Value| -> Result<i64> {
        let key = if monitor["transform"].as_i64().unwrap_or(0) % 2 != 0 {
            "height"
        } else {
            "width"
        };
        let size = monitor[key].as_f64().ok_or("Missing display dimensions")?;
        let scale = monitor["scale"].as_f64().ok_or("Missing display scale")?;
        if scale == 0.0 {
            return Err("float division by zero".into());
        }
        Ok((size / scale).round_ties_even() as i64)
    };
    let mut x = if mode == "extend-right" {
        width(&main)?
    } else {
        0
    };
    let mut result = vec![main];
    for original in monitors {
        if original["name"] == primary {
            continue;
        }
        let mut monitor = original.clone();
        monitor["y"] = json!(0);
        monitor["mirrorOf"] = json!(if mode == "mirror" { primary } else { "none" });
        if mode == "extend-left" {
            x -= width(&monitor)?;
        }
        monitor["x"] = json!(if mode == "mirror" { 0 } else { x });
        if mode == "extend-right" {
            x += width(&monitor)?;
        }
        result.push(monitor);
    }
    Ok(json!(result))
}

use crate::{classic_state, compositor::backend::Backend, json as wire, paths::Paths, state};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub enum Effect<'a> {
    Font(&'a str),
    Lock,
    Gradients,
    Workflow {
        action: &'a str,
        name: &'a str,
        roles: &'a Value,
    },
    Rollback {
        token: &'a str,
        environment: &'a [String],
    },
}
/// Replace only hardware and timer effects in parity fixtures; storage/renderers
/// remain the same native implementations as the installed command.
pub trait Runtime {
    fn compositor(&mut self, operation: &str, args: &[Value], kwargs: &Value) -> Result<Value>;
    fn monitors(&mut self) -> Result<Value> {
        let all = self.compositor("query", &[json!("all_monitors")], &json!({"timeout":8}))?;
        monitor_records(&all)
    }
    fn effect(&mut self, effect: Effect<'_>) -> Result<()>;
    fn write(&mut self, path: &Path, text: &str) -> Result<()> {
        classic_state::atomic_write(path, text)
    }
    fn read(&mut self, path: &Path, fallback: Value) -> Result<Value> {
        state::read(path, fallback)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn token(&mut self) -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }
    fn sleep(&mut self, seconds: u64) {
        std::thread::sleep(Duration::from_secs(seconds));
    }
}
pub struct Settings<R> {
    pub paths: Paths,
    pub state: PathBuf,
    pub config: PathBuf,
    pub pending: PathBuf,
    pub defaults: Value,
    pub backend: String,
    pub capabilities: Value,
    pub options: Vec<String>,
    pub runtime: R,
    pub environment: std::collections::BTreeMap<String, String>,
}
impl<R: Runtime> Settings<R> {
    pub fn new(paths: Paths, mut runtime: R) -> Result<Self> {
        let backend = runtime
            .compositor("backend_name", &[], &json!({}))?
            .as_str()
            .ok_or("Invalid compositor name")?
            .to_owned();
        let capabilities = runtime.compositor("capabilities", &[], &json!({}))?;
        let options =
            serde_json::from_value(runtime.compositor("desktop_option_keys", &[], &json!({}))?)?;
        let config = runtime
            .compositor(
                "config_path",
                &[json!(paths.home), json!("desktop")],
                &json!({}),
            )?
            .as_str()
            .ok_or("Invalid configuration path")?
            .into();
        let defaults = match fs::read(paths.assets().join("desktop-defaults.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => defaults().clone(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            state: paths.config.join("desktop.json"),
            pending: paths.config.join(if backend == "niri" {
                "display-pending-niri.json"
            } else {
                "display-pending.json"
            }),
            environment: [
                "NACRE_DEVELOPER_MODE",
                "NACRE_COMPOSITOR",
                "NIRI_SOCKET",
                "NIRI_CONFIG",
                "HOME",
                "XDG_CONFIG_HOME",
                "XDG_STATE_HOME",
                "XDG_CACHE_HOME",
                "XDG_DATA_HOME",
                "NACRE_HELPER_BINARY",
                "NACRE_NIRI_CONFIG",
                "NACRE_NIRI_DESKTOP_CONFIG",
            ]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok().map(|value| (key.to_owned(), value)))
            .collect(),
            paths,
            config,
            defaults,
            backend,
            capabilities,
            options,
            runtime,
        })
    }
    pub fn monitors(&mut self) -> Result<Value> {
        self.runtime.monitors()
    }
    pub fn load(&mut self) -> Result<Value> {
        let mut data = self
            .defaults
            .as_object()
            .ok_or("Invalid desktop defaults")?
            .clone();
        for key in INPUT_KEYS {
            data.shift_remove(*key);
        }
        if self.runtime.exists(&self.state) {
            let saved = self.runtime.read(&self.state, json!({}))?;
            for (key, value) in saved.as_object().ok_or("Invalid desktop preferences")? {
                data.insert(key.clone(), value.clone());
            }
        } else {
            // Python used a set; retain its frozen ordering for the independent
            // startup reads, then include any newly supported keys.
            let order = [
                "shadow",
                "followMouse",
                "borderSize",
                "rounding",
                "gapsOut",
                "gapsIn",
                "naturalScroll",
                "blur",
                "animations",
            ];
            let mut options = self.options.clone();
            options.sort_by_key(|key| {
                order
                    .iter()
                    .position(|name| *name == key)
                    .unwrap_or(order.len())
            });
            for key in options {
                match self.runtime.compositor(
                    "get_desktop_option",
                    &[json!(key)],
                    &json!({"timeout":8}),
                ) {
                    Ok(value) => {
                        data.insert(key, value);
                    }
                    Err(error) => eprintln!("Desktop setting {key} unavailable: {error}"),
                }
            }
        }
        data.shift_remove("wallpaperTransition");
        if self.capabilities["perOutputWorkspaces"] == true {
            data.insert(
                "displays".into(),
                data.get("compositorDisplays")
                    .and_then(|v| v.get(&self.backend))
                    .cloned()
                    .unwrap_or(json!([])),
            );
        }
        if let Some(snapshot) = data
            .get_mut("normalSnapshot")
            .and_then(Value::as_object_mut)
        {
            snapshot.shift_remove("wallpaperTransition");
        }
        Ok(Value::Object(data))
    }
    pub fn persist(&mut self, data: &Value) -> Result<()> {
        let mut data = data.clone();
        if self.capabilities["perOutputWorkspaces"] == true
            && self.capabilities["monitorConfiguration"] == true
            && data
                .get("displays")
                .is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty))
        {
            data["displays"] = self.monitors()?;
        }
        let monitors = if self.capabilities["monitorConfiguration"] == false {
            json!([])
        } else {
            data.get("displays").cloned().unwrap_or(json!([]))
        };
        let options =
            self.runtime
                .compositor("render_desktop_options", &[data.clone()], &json!({}))?;
        let monitor_text = self
            .runtime
            .compositor("render_monitors", &[monitors], &json!({}))?;
        let text = format!(
            "{}{}\n",
            options.as_str().ok_or("Invalid desktop configuration")?,
            monitor_text
                .as_str()
                .ok_or("Invalid monitor configuration")?
        );
        let mut stored = data.clone();
        if self.capabilities["perOutputWorkspaces"] == true {
            let previous = if self.runtime.exists(&self.state) {
                self.runtime.read(&self.state, json!({}))?
            } else {
                json!({})
            };
            let mut displays = previous
                .get("compositorDisplays")
                .cloned()
                .unwrap_or(json!({}));
            displays[&self.backend] = data.get("displays").cloned().unwrap_or(json!([]));
            stored["compositorDisplays"] = displays;
            stored["displays"] = previous.get("displays").cloned().unwrap_or(json!([]));
        }
        self.runtime
            .write(&self.state, &format!("{}\n", wire::pretty(&stored)?))?;
        self.runtime.write(&self.config, &text)
    }
    pub fn status(&mut self) -> Result<Value> {
        Ok(
            json!({"data":self.load()?,"monitors":self.monitors()?,"pending":self.pending.exists(),"message":"Changes save automatically"}),
        )
    }
    pub fn revert(&mut self, token: Option<&str>) -> Result<()> {
        if !self.pending.exists() {
            return Ok(());
        }
        let saved = state::read(&self.pending, json!({}))?;
        if token.is_some_and(|token| !token.is_empty() && saved["token"] != token) {
            return Ok(());
        }
        self.runtime.compositor(
            "configure_monitors",
            &[saved["monitors"].clone()],
            &json!({"timeout":8}),
        )?;
        let mut data = self.load()?;
        data["displays"] = saved["previous"].clone();
        self.persist(&data)?;
        remove(&self.pending)
    }
    fn preset_keys(&self) -> Vec<String> {
        self.defaults
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| {
                !INPUT_KEYS.contains(&key.as_str())
                    && ![
                        "language",
                        "monoFont",
                        "niriWorkspaceMode",
                        "workspacePrimaryOutput",
                        "monitorWorkspaceActions",
                    ]
                    .contains(&key.as_str())
                    && !["lock", "weather", "interface"]
                        .iter()
                        .any(|prefix| key.starts_with(prefix))
            })
            .cloned()
            .collect()
    }
    pub fn action(&mut self, action: &str, args: &[String]) -> Result<()> {
        if let Some(expected) = match action {
            "set" | "display" => Some(2),
            "display-edit" => Some(3),
            _ => None,
        } {
            if args.len() != expected {
                return Err(if args.len() < expected {
                    format!(
                        "not enough values to unpack (expected {expected}, got {})",
                        args.len()
                    )
                } else {
                    format!("too many values to unpack (expected {expected})")
                }
                .into());
            }
        }
        let argument = |index| {
            args.get(index)
                .map(String::as_str)
                .ok_or("Missing desktop setting argument")
        };
        match action {
            "state" => (),
            "init" => {
                if self.pending.exists() {
                    self.revert(None)?;
                }
                let data = self.load()?;
                self.runtime
                    .effect(Effect::Font(data["monoFont"].as_str().unwrap_or("")))?;
                self.persist(&data)?;
                self.runtime.compositor(
                    "ensure_desktop_loader",
                    &[json!(self.paths.home)],
                    &json!({}),
                )?;
            }
            "set" => {
                let key = argument(0)?;
                if [
                    "animations",
                    "blur",
                    "shadow",
                    "followMouse",
                    "naturalScroll",
                    "gapsIn",
                    "gapsOut",
                    "borderSize",
                    "rounding",
                ]
                .contains(&key)
                    && !self.options.iter().any(|option| option == key)
                {
                    return Err("This compositor setting is unavailable; configure it in the compositor instead".into());
                }
                let value = crate::json::parse_value(argument(1)?)?;
                let value = validate_with(
                    &self.defaults,
                    key,
                    &value,
                    self.environment
                        .get("NACRE_DEVELOPER_MODE")
                        .is_some_and(|value| value == "1"),
                )?;
                let mut data = self.load()?;
                data[key] = value.clone();
                if self.options.iter().any(|option| option == key) || INPUT_KEYS.contains(&key) {
                    self.runtime.compositor(
                        "set_desktop_options",
                        &[json!({key:value})],
                        &json!({"timeout":8}),
                    )?;
                }
                if key == "monoFont" {
                    let previous = self.load()?["monoFont"].as_str().unwrap_or("").to_owned();
                    self.runtime.effect(Effect::Font(value.as_str().unwrap()))?;
                    if let Err(error) = self
                        .runtime
                        .write(&self.state, &format!("{}\n", wire::pretty(&data)?))
                    {
                        self.runtime.effect(Effect::Font(&previous))?;
                        return Err(error);
                    }
                } else {
                    self.persist(&data)?;
                }
                if key == "interfaceGradients" && self.paths.state.join("scheme.json").exists() {
                    self.runtime.effect(Effect::Gradients)?;
                }
                if key.starts_with("lock") {
                    self.runtime.effect(Effect::Lock)?;
                }
            }
            "preset" => {
                let name = argument(0)?;
                if ![
                    "normal",
                    "focused",
                    "presentation",
                    "minimal",
                    "meeting",
                    "music",
                    "docked",
                ]
                .contains(&name)
                {
                    return Err("Unknown desktop preset".into());
                }
                let mut data = self.load()?;
                let keys = self.preset_keys();
                let baseline = if data["preset"].as_str().unwrap_or("normal") != "normal" {
                    data.get("normalSnapshot").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Object(
                        keys.iter()
                            .map(|key| (key.clone(), data[key].clone()))
                            .collect(),
                    )
                };
                let baseline = if baseline.as_object().is_none_or(|v| v.is_empty()) {
                    self.defaults.clone()
                } else {
                    baseline
                };
                let baseline = Value::Object(
                    baseline
                        .as_object()
                        .ok_or("Invalid desktop snapshot")?
                        .iter()
                        .filter(|(key, _)| keys.contains(key))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );
                for (key, value) in baseline.as_object().unwrap() {
                    data[key] = value.clone();
                }
                let overrides = match name {
                    "focused" => json!({"blur":false,"shadow":false,"animations":false,"dnd":true}),
                    "presentation" => {
                        json!({"aiSidebarEnabled":false,"topBarEnabled":false,"frameTopEnabled":false,"frameLeftEnabled":false,"frameRightEnabled":false,"controlCenterEnabled":false,"frameBottomEnabled":false,"dnd":true})
                    }
                    "meeting" => json!({"dnd":true,"blur":false}),
                    "music" => json!({"dnd":true}),
                    "docked" => json!({"dnd":false}),
                    "minimal" => {
                        json!({"topBarEnabled":true,"frameTopEnabled":true,"clickEdgeMenus":false,"frameLeftEnabled":false,"frameRightEnabled":false,"controlCenterEnabled":false,"frameBottomEnabled":false,"frameWidth":0,"gapsIn":3,"gapsOut":6,"borderSize":0,"shadow":false})
                    }
                    _ => json!({}),
                };
                for (key, value) in overrides.as_object().unwrap() {
                    data[key] = value.clone();
                }
                data["preset"] = json!(name);
                data["normalSnapshot"] = baseline;
                self.runtime.compositor(
                    "set_desktop_options",
                    &[data.clone()],
                    &json!({"timeout":8}),
                )?;
                self.persist(&data)?;
                self.runtime.effect(Effect::Workflow {
                    action: "apply",
                    name,
                    roles: &json!([]),
                })?;
            }
            "save-workflow" => {
                let roles = if let Some(raw) = args.get(1) {
                    crate::json::parse_value(raw)?
                } else {
                    json!([])
                };
                self.runtime.effect(Effect::Workflow {
                    action: "save",
                    name: argument(0)?,
                    roles: &roles,
                })?;
            }
            "display" | "display-edit" => {
                if self.pending.exists() {
                    return Err("Keep or revert the current display change first".into());
                }
                let mut data = self.load()?;
                let monitors = self.monitors()?;
                let plan = if action == "display" {
                    let mode = argument(0)?;
                    let primary = argument(1)?;
                    if mode == "mirror" && self.capabilities["monitorMirroring"] == false {
                        return Err("Output mirroring is unavailable on this compositor".into());
                    }
                    display_plan(monitors.as_array().unwrap(), primary, mode)?
                } else {
                    let name = argument(0)?;
                    let scale = argument(1)?.parse::<f64>()?;
                    let mode = argument(2)?;
                    if ![1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0].contains(&scale) {
                        return Err("Unsupported display scale".into());
                    }
                    let mut plan = monitors.clone();
                    let selected = plan
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|m| m["name"] == name)
                        .ok_or("Selected display is disconnected")?;
                    if !mode.is_empty() {
                        if !selected["availableModes"]
                            .as_array()
                            .is_some_and(|modes| modes.contains(&json!(mode)))
                        {
                            return Err("Unavailable display mode".into());
                        }
                        let pattern = regex::Regex::new(r"\A(\d+)x(\d+)@(\d+(?:\.\d+)?)Hz\z")
                            .expect("mode pattern");
                        let parts = pattern.captures(mode).ok_or("Unsupported mode format")?;
                        selected["width"] = json!(parts[1].parse::<u64>()?);
                        selected["height"] = json!(parts[2].parse::<u64>()?);
                        selected["refreshRate"] = json!(parts[3].parse::<f64>()?);
                    }
                    selected["scale"] = json!(scale);
                    plan
                };
                let token = self.runtime.token();
                self.runtime.write(&self.pending,&wire::dumps(&json!({"token":token,"previous":data.get("displays").cloned().unwrap_or(json!([])),"monitors":monitors}),false)?)?;
                data["displays"] = plan.clone();
                let mut environment = Vec::new();
                if self.capabilities["perOutputWorkspaces"] == true {
                    environment.push(format!("NACRE_COMPOSITOR={}", self.backend));
                    for key in [
                        "NIRI_SOCKET",
                        "NIRI_CONFIG",
                        "HOME",
                        "XDG_CONFIG_HOME",
                        "XDG_STATE_HOME",
                        "XDG_CACHE_HOME",
                        "XDG_DATA_HOME",
                        "NACRE_HELPER_BINARY",
                        "NACRE_NIRI_CONFIG",
                        "NACRE_NIRI_DESKTOP_CONFIG",
                    ] {
                        if let Some(value) = self.environment.get(key) {
                            if !value.is_empty() {
                                environment.push(format!("{key}={value}"));
                            }
                        }
                    }
                }
                if let Err(error) = self.runtime.effect(Effect::Rollback {
                    token: &token,
                    environment: &environment,
                }) {
                    remove(&self.pending)?;
                    return Err(error);
                }
                let result = self
                    .runtime
                    .compositor("configure_monitors", &[plan], &json!({"timeout":8}))
                    .and_then(|_| self.persist(&data));
                if let Err(error) = result {
                    self.revert(Some(&token))?;
                    return Err(error);
                }
            }
            "confirm" => remove(&self.pending)?,
            "revert" | "rollback-after" => self.revert(args.first().map(String::as_str))?,
            _ => return Err("Unknown desktop action".into()),
        }
        Ok(())
    }
    pub fn execute(&mut self, action: &str, args: &[String]) -> Result<Value> {
        if action == "rollback-after" {
            self.runtime.sleep(20);
        }
        let parent = self.state.parent().ok_or("Invalid desktop state path")?;
        fs::create_dir_all(parent)?;
        use nix::fcntl::{Flock, FlockArg};
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o666)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(self.state.with_extension("lock"))?;
        let meta = lock.metadata()?;
        if !meta.is_file() || meta.nlink() != 1 || meta.uid() != nix::unistd::getuid().as_raw() {
            return Err("Refusing linked or foreign desktop lock".into());
        }
        let guard = Flock::lock(lock, FlockArg::LockExclusive).map_err(|(_, error)| error)?;
        guard.set_len(0)?;
        match self.action(action, args).and_then(|_| self.status()) {
            Ok(value) => Ok(value),
            Err(error) => Ok(
                json!({"data":self.load()?,"error":error.to_string(),"pending":self.pending.exists()}),
            ),
        }
    }
}
fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub struct Native<'a> {
    paths: Paths,
    backend: &'a mut Backend,
}
impl<'a> Native<'a> {
    pub fn new(paths: Paths, backend: &'a mut Backend) -> Self {
        Self { backend, paths }
    }
    fn arm(&self, token: &str, environment: &[String]) -> Result<()> {
        use zbus::zvariant::OwnedValue;
        let executable = std::env::current_exe()?;
        let executable = executable.to_str().ok_or("Invalid helper executable")?;
        let mut environment = environment.to_vec();
        environment.retain(|value| !value.starts_with("NACRE_COMPOSITOR="));
        environment.push(format!("NACRE_COMPOSITOR={}", self.backend.name()));
        // Both compositor backends must restore the same explicitly selected
        // HOME/XDG roots, even when the user manager's environment differs.
        for key in [
            "XDG_RUNTIME_DIR",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NIRI_SOCKET",
            "NIRI_CONFIG",
            "NACRE_HELPER_BINARY",
            "NACRE_HELPER_TOOLS",
            "NACRE_NIRI_CONFIG",
            "NACRE_NIRI_DESKTOP_CONFIG",
            "NACRE_DAEMON_SOCKET",
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
            "XDG_DATA_HOME",
        ] {
            if !environment
                .iter()
                .any(|value| value.starts_with(&format!("{key}=")))
            {
                if let Ok(value) = std::env::var(key) {
                    environment.push(format!("{key}={value}"));
                }
            }
        }
        let argv = vec![executable, "desktop-settings", "rollback-after", token];
        let properties: Vec<(&str, OwnedValue)> = vec![
            (
                "Description",
                OwnedValue::try_from(zbus::zvariant::Value::new("Nacre display rollback"))?,
            ),
            (
                "CollectMode",
                OwnedValue::try_from(zbus::zvariant::Value::new("inactive-or-failed"))?,
            ),
            ("AddRef", OwnedValue::from(true)),
            (
                "Environment",
                OwnedValue::try_from(zbus::zvariant::Value::new(environment))?,
            ),
            (
                "ExecStart",
                OwnedValue::try_from(zbus::zvariant::Value::new(vec![(executable, argv, false)]))?,
            ),
        ];
        crate::menu_registry::systemd_job(
            "StartTransientUnit",
            &(
                format!("nacre-display-revert-{token}.service"),
                "fail",
                properties,
                Vec::<(String, Vec<(String, OwnedValue)>)>::new(),
            ),
        )
        .map_err(|error| {
            eprintln!("Could not arm display rollback: {error}");
            "Automatic display rollback could not be armed".into()
        })
    }
}
impl Runtime for Native<'_> {
    fn compositor(&mut self, operation: &str, args: &[Value], kwargs: &Value) -> Result<Value> {
        Ok(self.backend.module_call(operation, args, kwargs)?)
    }
    fn effect(&mut self, effect: Effect<'_>) -> Result<()> {
        self.effect_with_desktop(effect, None)
    }
}
impl Native<'_> {
    pub(crate) fn effect_with_desktop(
        &mut self,
        effect: Effect<'_>,
        preferences: Option<&Value>,
    ) -> Result<()> {
        match effect {
            Effect::Font(family) => crate::terminal_font::publish(&self.paths, family),
            Effect::Lock => crate::lock_config::publish(&self.paths, self.backend, preferences),
            Effect::Gradients => {
                let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
                let colors = classic_state::validated_colors(&scheme, &self.paths.assets())?;
                let saved;
                let preferences = if let Some(preferences) = preferences {
                    preferences
                } else {
                    saved = state::read(&self.paths.config.join("desktop.json"), json!({}))?;
                    &saved
                };
                let text = Value::String(classic_state::publish_compositor_palette_with(
                    &self.paths.assets(),
                    self.backend,
                    &colors,
                    preferences,
                )?);
                if self.backend.name() == "hyprland" {
                    self.backend
                        .module_call("apply_config", &[text], &json!({"timeout":8}))?;
                } else {
                    self.backend.module_call("reload", &[], &json!({}))?;
                }
                Ok(())
            }
            Effect::Rollback { token, environment } => self.arm(token, environment),
            Effect::Workflow {
                action,
                name,
                roles,
            } => {
                // This canonical Python library is ported later in the same
                // wave. The narrow bridge is deleted with that port.
                let payload =
                    serde_json::to_string(&json!({"action":action,"name":name,"roles":roles}))?;
                let output = crate::process::run(
                    "python3",
                    &[
                        self.paths
                            .assets()
                            .join("workflow-profiles.py")
                            .to_string_lossy()
                            .into_owned(),
                        "--native-controller-bridge".into(),
                    ],
                    Some(&payload),
                    Duration::from_secs(120),
                )?;
                let response: Value = serde_json::from_slice(&output.stdout)?;
                if !output.status.success() {
                    return Err(response["error"]
                        .as_str()
                        .unwrap_or("Workflow profile action failed")
                        .to_owned()
                        .into());
                }
                Ok(())
            }
        }
    }
}

const CHOICES: &str =
    "state,init,set,display,display-edit,confirm,revert,rollback-after,preset,save-workflow";
fn usage() -> String {
    format!("usage: desktop-settings.py [-h]\n                           {{{CHOICES}}}\n                           [args ...]\n")
}
pub fn run(arguments: &[String]) -> Result<i32> {
    let mut action = None;
    let mut args = Vec::new();
    let mut unknown = Vec::new();
    let mut literal = false;
    let error = |message: String| {
        eprintln!("{}desktop-settings.py: error: {message}", usage());
        2
    };
    for argument in arguments {
        if !literal && argument == "--" {
            literal = true;
            continue;
        }
        if !literal
            && (argument == "-h" || (argument.starts_with("--") && "--help".starts_with(argument)))
        {
            print!("{}\npositional arguments:\n  {{{CHOICES}}}\n  args\n\noptions:\n  -h, --help            show this help message and exit\n",usage());
            return Ok(0);
        }
        if !literal {
            if let Some(value) = argument.strip_prefix("--help=") {
                return Ok(error(format!(
                    "argument -h/--help: ignored explicit argument {}",
                    wire::python_repr(&json!(value))
                )));
            }
            let negative = argument.strip_prefix('-').is_some_and(|tail| {
                let digits =
                    |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
                tail.split_once('.').map_or_else(
                    || digits(tail),
                    |(whole, fraction)| (whole.is_empty() || digits(whole)) && digits(fraction),
                )
            });
            if argument.starts_with('-') && !negative {
                unknown.push(argument.clone());
                continue;
            }
        }
        if action.is_none() {
            if !CHOICES.split(',').any(|choice| choice == argument) {
                let choices = CHOICES
                    .split(',')
                    .map(|choice| format!("'{choice}'"))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(error(format!(
                    "argument action: invalid choice: {} (choose from {choices})",
                    wire::python_repr(&json!(argument))
                )));
            }
            action = Some(argument.as_str());
        } else {
            args.push(argument.clone());
        }
    }
    let Some(action) = action else {
        return Ok(error("the following arguments are required: action".into()));
    };
    if !unknown.is_empty() {
        return Ok(error(format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    let recovery = action == "rollback-after";
    let action = if recovery {
        // Recovery is independently armed before a physical output change. Wait
        // outside the daemon, then connect to its current boot. A dead daemon
        // leaves this token-checked worker able to restore the same session.
        std::thread::sleep(Duration::from_secs(20));
        "revert"
    } else {
        action
    };
    if let Some(result) = crate::daemon::settings_controller::client(action, &args, recovery)? {
        println!("{}", wire::dumps(&result, false)?);
        return Ok(0);
    }
    let paths = Paths::environment()?;
    let mut backend = Backend::new(paths.clone(), std::env::vars().collect())?;
    let mut settings = Settings::new(paths.clone(), Native::new(paths, &mut backend))?;
    println!("{}", wire::dumps(&settings.execute(action, &args)?, false)?);
    Ok(0)
}

pub fn monitor_records(all: &Value) -> Result<Value> {
    let mut result = Vec::new();
    for monitor in all.as_array().ok_or("Invalid monitor records")? {
        if monitor["disabled"].as_bool().unwrap_or(false) {
            continue;
        }
        let mut row = serde_json::Map::new();
        for key in [
            "name",
            "width",
            "height",
            "refreshRate",
            "scale",
            "x",
            "y",
            "transform",
            "disabled",
            "mirrorOf",
            "availableModes",
        ] {
            row.insert(key.into(), monitor.get(key).cloned().unwrap_or(Value::Null));
        }
        result.push(Value::Object(row));
    }
    Ok(json!(result))
}
