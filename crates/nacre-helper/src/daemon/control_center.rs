//! N0046: control-centre actions and the `controls` state topic.
//!
//! Every action reuses the tool the QML control centre ran: `wpctl`, `brightnessctl` through
//! `light_devices`, `powerprofilesctl`, `rfkill`, and the helper's own `control-tools`,
//! `menu-backend` and `wallpaper-media` commands (the daemon's executable, fixed argv). Callers
//! choose a value from a closed set; no caller text reaches a command line except a validated
//! PipeWire node name that is resolved to a numeric id first.
//!
//! The state topic is produced only while a client subscribes (like `osd`); nothing polls
//! otherwise. A method is advertised only while its backend is present (`available_methods`).
use super::{
    bar_services, behaviours,
    hub::{failure, Hub, Shared},
    osd, pill_controls,
};
use crate::{light_devices, paths::Paths, process, Result};
use nacre_protocol::{
    control_center::*, core_services::Health, pill_controls::PillEmpty, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    cell::Cell,
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";
const STAY_AWAKE_UNIT: &str = "nacre-stay-awake.service";
const RECORDING_UNIT: &str = "nacre-screen-recording.service";
/// Output cap for tools whose output is parsed.
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
/// Slow members (night light, appearance) are re-read on every fifth tick.
const SLOW_EVERY: u32 = 5;
/// An interactive capture tool (area selection, picker) never runs longer than this.
const CAPTURE_LIMIT: Duration = Duration::from_secs(200);

#[derive(Clone)]
pub struct Env {
    /// The daemon's own executable, which hosts the `control-tools`/`menu-backend` commands.
    pub engine: PathBuf,
    pub wpctl: String,
    pub pactl: String,
    pub pw_dump: String,
    pub rfkill: String,
    pub powerprofilesctl: String,
    pub systemctl: String,
    pub brightnessctl: String,
    pub ddcutil: String,
    /// `/sys`
    pub sys: PathBuf,
    /// `$XDG_RUNTIME_DIR` (the PipeWire socket).
    pub runtime: Option<PathBuf>,
    pub system_bus: PathBuf,
    /// `/dev/rfkill`
    pub rfkill_dev: PathBuf,
    pub paths: Paths,
    /// Presence of a plain tool (`grim`, `slurp`, `wf-recorder`, ...).
    pub which: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub tick: Duration,
}
impl Env {
    pub fn production() -> Result<Self> {
        Ok(Self {
            engine: std::env::current_exe()?,
            wpctl: "wpctl".into(),
            pactl: "pactl".into(),
            pw_dump: "pw-dump".into(),
            rfkill: "rfkill".into(),
            powerprofilesctl: "powerprofilesctl".into(),
            systemctl: "systemctl".into(),
            brightnessctl: "brightnessctl".into(),
            ddcutil: "ddcutil".into(),
            sys: light_devices::sys_root(),
            runtime: std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
            system_bus: "/run/dbus/system_bus_socket".into(),
            rfkill_dev: "/dev/rfkill".into(),
            paths: Paths::environment()?,
            which: Arc::new(executable),
            tick: Duration::from_secs(2),
        })
    }
    fn has(&self, program: &str) -> bool {
        (self.which)(program)
    }
}
thread_local! {
    static OVERRIDE: RefCell<Option<Env>> = const { RefCell::new(None) };
    static REFRESH: std::rc::Rc<tokio::sync::Notify> = std::rc::Rc::new(tokio::sync::Notify::new());
    /// One interactive capture tool (area selection, picker, recorder start) at a time.
    static CAPTURE_BUSY: Cell<bool> = const { Cell::new(false) };
}
#[cfg(test)]
fn set_env(env: Option<Env>) {
    OVERRIDE.with(|slot| *slot.borrow_mut() = env);
}
fn env() -> std::result::Result<Env, Failure> {
    if let Some(env) = OVERRIDE.with(|slot| slot.borrow().clone()) {
        return Ok(env);
    }
    Env::production().map_err(|_| unavailable("Control tools are unavailable"))
}
/// Ask the producer (if a client subscribes) for an immediate full re-read.
pub fn changed() {
    REFRESH.with(|notify| notify.notify_one());
}

pub(crate) fn executable(program: &str) -> bool {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<(std::ffi::OsString, HashMap<String, bool>)>> = OnceLock::new();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let cache = CACHE.get_or_init(|| Mutex::new((path.clone(), HashMap::new())));
    let mut cache = cache.lock().expect("executable cache poisoned");
    if cache.0 != path {
        cache.0 = path.clone();
        cache.1.clear();
    }
    if let Some(found) = cache.1.get(program) {
        return *found;
    }
    use std::os::unix::fs::PermissionsExt;
    let usable = |path: &Path| {
        path.is_file() && std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    };
    let found = if program.contains('/') {
        usable(Path::new(program))
    } else {
        std::env::split_paths(&path).any(|dir| usable(&dir.join(program)))
    };
    cache.1.insert(program.to_owned(), found);
    found
}
fn socket_present(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(path).is_ok_and(|m| m.file_type().is_socket())
}

// ------------------------------------------------------------------ errors

fn invalid() -> Failure {
    failure(ErrorCode::Validation, "Invalid control action", false).dispatch_state(false)
}
fn unavailable(message: &str) -> Failure {
    failure(ErrorCode::Unavailable, message, true).dispatch_state(false)
}
fn busy(message: &str) -> Failure {
    failure(ErrorCode::Busy, message, true).dispatch_state(false)
}
fn action_error() -> Failure {
    failure(
        ErrorCode::Io,
        "Control action failed; it may already have completed",
        false,
    )
    .dispatch_state(true)
}

// ------------------------------------------------------------- availability

fn pipewire(env: &Env) -> bool {
    env.has(&env.wpctl)
        && env
            .runtime
            .as_ref()
            .is_some_and(|runtime| socket_present(&runtime.join("pipewire-0")))
}
/// Radios that can be switched: the tool, the control device and a managed radio.
fn rfkill_present(env: &Env) -> bool {
    env.has(&env.rfkill) && env.rfkill_dev.exists() && !radios(&env.sys).is_empty()
}
/// A reachable user manager: the tools alone say nothing about a session.
fn systemd_present(env: &Env) -> bool {
    env.has(&env.systemctl)
        && env.has("systemd-inhibit")
        && env
            .runtime
            .as_ref()
            .is_some_and(|runtime| socket_present(&runtime.join("bus")))
}
/// N0067: from the last light scan (`scan_lights`); never reads sysfs on the async thread.
fn keyboard_present(env: &Env) -> bool {
    env.has(&env.brightnessctl) && light_devices::keyboard_cached(&env.sys)
}
fn backend_present(hub: &Hub, env: &Env, method: &str) -> bool {
    let controls = |member: &str| {
        hub.snapshot_ref(TOPIC)
            .is_some_and(|state| !state[member].is_null())
    };
    match method {
        "audio.set_volume" | "audio.set_mute" | "audio.set_default" => pipewire(env),
        "keyboard_light.set" => keyboard_present(env),
        "power.set_profile" => env.has(&env.powerprofilesctl) && socket_present(&env.system_bus),
        "airplane.set" => rfkill_present(env),
        "night_light.set" => controls("night_light"),
        "stay_awake.set" => systemd_present(env),
        "recorder.start" | "recorder.stop" => {
            systemd_present(env) && env.has("wf-recorder") && env.has("slurp")
        }
        "screenshot.capture" => env.has("grim") && env.has("slurp"),
        "color_picker.pick" => env.has("hyprpicker"),
        "wallpaper.next" | "wallpaper.previous" => {
            hub.settings.is_some() && hub.snapshot_ref(super::active_wallpaper::TOPIC).is_some()
        }
        "appearance.set_mode" => hub.settings.is_some(),
        _ => false,
    }
}
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    let mut methods = methods();
    if hub.safe {
        methods.clear();
    } else if let Ok(env) = env() {
        methods.retain(|name, _| backend_present(hub, &env, name));
    } else {
        methods.clear();
    }
    methods
}
pub fn handles(method: &str) -> bool {
    matches!(
        method,
        "audio.set_volume"
            | "audio.set_mute"
            | "audio.set_default"
            | "keyboard_light.set"
            | "power.set_profile"
            | "airplane.set"
            | "night_light.set"
            | "stay_awake.set"
            | "recorder.start"
            | "recorder.stop"
            | "screenshot.capture"
            | "color_picker.pick"
            | "wallpaper.next"
            | "wallpaper.previous"
            | "appearance.set_mode"
    )
}

// ---------------------------------------------------------------- commands

/// A child process never inherits the service-manager socket: systemd's own tools (`systemctl`)
/// and the helper report their errors to it, which the daemon's supervisor would read as its own.
fn command(program: impl AsRef<std::ffi::OsStr>, args: &[String]) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.args(args).env_remove("NOTIFY_SOCKET");
    command
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}
/// A bounded tool run whose output is parsed; `None` on any failure.
fn tool(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let output = process::capture(command(program, &strings(args)), None, timeout).ok()?;
    (output.status.success() && output.stdout.len() <= OUTPUT_LIMIT)
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}
/// A helper command of the daemon's own executable: `(exit code, last JSON line)`.
fn engine(
    env: &Env,
    args: &[&str],
    input: Option<&str>,
    timeout: Duration,
) -> Option<(i32, Value)> {
    let command = command(&env.engine, &strings(args));
    let output = process::capture(command, input.map(str::as_bytes), timeout).ok()?;
    let stdout = output.stdout.get(..64 * 1024).unwrap_or(&output.stdout);
    let text = String::from_utf8_lossy(stdout).into_owned();
    let value = text
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
        .unwrap_or(Value::Null);
    Some((output.status.code().unwrap_or(1), value))
}
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> std::result::Result<T, Failure> + Send + 'static,
) -> std::result::Result<T, Failure> {
    crate::process::offload(work)
        .await
        .map_err(|_| action_error())?
}

// ---------------------------------------------------------------- dispatch

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Control actions are disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    let env = env()?;
    if !backend_present(&hub.borrow(), &env, method) {
        return Err(unavailable("Control backend is unavailable"));
    }
    let reply = match method {
        "audio.set_volume" => {
            let request: AudioSetVolume = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            audio(
                env,
                request.target,
                request.node,
                Audio::Volume(request.value),
            )
            .await?;
            json!({"changed":true})
        }
        "audio.set_mute" => {
            let request: AudioSetMute = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            audio(
                env,
                request.target,
                request.node,
                Audio::Mute(request.muted),
            )
            .await?;
            json!({"changed":true})
        }
        "audio.set_default" => {
            let request: AudioSetDefault = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            set_default(env, request.node).await?;
            json!({"changed":true})
        }
        "keyboard_light.set" => {
            let request: KeyboardLightSet =
                serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            let guard = bar_services::brightness_lane()
                .try_lock_owned()
                .map_err(|_| busy("Brightness controls are busy"))?;
            let (level, max, changed) = blocking(move || {
                let _guard = guard;
                set_keyboard(&env, i64::from(request.level))
            })
            .await?;
            osd::keyboard(&hub, level, max);
            json!({"changed":changed,"level":level,"max":max})
        }
        "power.set_profile" => {
            let request: PowerSetProfile = serde_json::from_value(params).map_err(|_| invalid())?;
            blocking(move || set_profile(&env, request.profile)).await?;
            json!({"changed":true})
        }
        "airplane.set" => {
            let request: EnabledSet = serde_json::from_value(params).map_err(|_| invalid())?;
            blocking(move || set_airplane(&env, request.enabled)).await?;
            json!({"changed":true})
        }
        "night_light.set" => {
            let request: EnabledSet = serde_json::from_value(params).map_err(|_| invalid())?;
            let changed = blocking(move || set_night_light(&env, request.enabled)).await?;
            json!({"changed":changed})
        }
        "stay_awake.set" => {
            let request: EnabledSet = serde_json::from_value(params).map_err(|_| invalid())?;
            let changed = blocking(move || set_stay_awake(&env, request.enabled)).await?;
            json!({"changed":changed})
        }
        "recorder.start" => {
            let request: RecorderStart = serde_json::from_value(params).map_err(|_| invalid())?;
            let recording = {
                let env = env.clone();
                blocking(move || Ok(units(&env).1)).await?
            };
            if recording == Some(true) {
                return Err(busy("A recording is already running"));
            }
            let screen = request.mode == RecorderMode::Screen;
            match request.audio {
                // Sound goes through the launcher's recorder (virtual mix, cleaned up on stop).
                Some(audio @ (RecorderAudio::System | RecorderAudio::Mixed)) => {
                    let audio = if audio == RecorderAudio::System {
                        "system"
                    } else {
                        "mixed"
                    };
                    spawn_capture_with(
                        &env,
                        &[
                            "launcher-workflows",
                            "record",
                            if screen { "screen" } else { "area" },
                        ],
                        Some(json!({ "audio": audio }).to_string()),
                    )?;
                }
                _ => spawn_capture(
                    &env,
                    &[
                        "menu-backend",
                        "tool",
                        if screen {
                            "record-screen"
                        } else {
                            "record-start"
                        },
                    ],
                )?,
            }
            json!({"changed":true})
        }
        "recorder.stop" => {
            let _: PillEmpty = serde_json::from_value(params).map_err(|_| invalid())?;
            let changed = blocking(move || stop_recording(&env)).await?;
            json!({"changed":changed})
        }
        "screenshot.capture" => {
            let request: ScreenshotCapture =
                serde_json::from_value(params).map_err(|_| invalid())?;
            let args: &[&str] = match request.mode {
                ScreenshotMode::Region => &["control-tools", "screenshot"],
                ScreenshotMode::Screen => &["menu-backend", "tool", "capture-screen"],
                ScreenshotMode::Window => &["menu-backend", "tool", "capture-window"],
            };
            spawn_capture(&env, args)?;
            json!({"changed":true})
        }
        "color_picker.pick" => {
            let _: PillEmpty = serde_json::from_value(params).map_err(|_| invalid())?;
            spawn_capture(&env, &["control-tools", "color-picker"])?;
            json!({"changed":true})
        }
        "wallpaper.next" => {
            let _: PillEmpty = serde_json::from_value(params).map_err(|_| invalid())?;
            json!({"changed":wallpaper_next(&hub, &env).await?})
        }
        "wallpaper.previous" => {
            let _: PillEmpty = serde_json::from_value(params).map_err(|_| invalid())?;
            json!({"changed":wallpaper_previous(&hub, &env).await?})
        }
        "appearance.set_mode" => {
            let request: AppearanceSet = serde_json::from_value(params).map_err(|_| invalid())?;
            blocking(move || set_appearance(&env, request.mode)).await?;
            json!({"changed":true})
        }
        _ => {
            return Err(failure(
                ErrorCode::UnknownMethod,
                "Unknown control method",
                false,
            ))
        }
    };
    changed();
    Ok(reply)
}

// ------------------------------------------------------------------- audio

enum Audio {
    Volume(f64),
    Mute(bool),
}
async fn audio(
    env: Env,
    target: AudioTarget,
    node: Option<String>,
    action: Audio,
) -> std::result::Result<(), Failure> {
    let guard = pill_controls::audio_lock()
        .try_lock_owned()
        .map_err(|_| busy("Audio control is busy"))?;
    blocking(move || {
        let _guard = guard;
        let selector = match target {
            AudioTarget::Sink => SINK.to_owned(),
            AudioTarget::Source => SOURCE.to_owned(),
            AudioTarget::Node => resolve_node(&env, node.as_deref().unwrap_or_default(), false)?,
            AudioTarget::Stream => {
                return set_stream(&env, node.as_deref().unwrap_or_default(), &action)
            }
        };
        // A missing target is "unavailable", not a failed write.
        tool(
            &env.wpctl,
            &["get-volume", &selector],
            Duration::from_secs(5),
        )
        .filter(|out| out.starts_with("Volume:"))
        .ok_or_else(|| unavailable("Audio device is unavailable"))?;
        let done = match action {
            // wpctl takes the cubic (perceptual) scale; the contract value is linear.
            Audio::Volume(value) => tool(
                &env.wpctl,
                &[
                    "set-volume",
                    "-l",
                    "1.0",
                    &selector,
                    &format!("{:.4}", value.cbrt()),
                ],
                Duration::from_secs(5),
            ),
            Audio::Mute(muted) => tool(
                &env.wpctl,
                &["set-mute", &selector, if muted { "1" } else { "0" }],
                Duration::from_secs(5),
            ),
        };
        done.map(|_| ()).ok_or_else(action_error)
    })
    .await
}
/// N0061: one application stream, addressed by its `audio` topic id. Only the numeric pactl
/// index reaches the command line. pactl takes raw volume units (65536 = 100 % of the
/// perceptual scale), so the contract's linear value is converted with a cube root.
fn set_stream(env: &Env, id: &str, action: &Audio) -> std::result::Result<(), Failure> {
    let (kind, index) = stream_id(id).ok_or_else(invalid)?;
    let index = index.to_string();
    let done = match action {
        Audio::Volume(value) => {
            let raw = (value.cbrt() * 65536.0).round().to_string();
            tool(
                &env.pactl,
                &[&format!("set-{kind}-volume"), &index, &raw],
                Duration::from_secs(5),
            )
        }
        Audio::Mute(muted) => tool(
            &env.pactl,
            &[
                &format!("set-{kind}-mute"),
                &index,
                if *muted { "1" } else { "0" },
            ],
            Duration::from_secs(5),
        ),
    };
    // A stream that ended is the usual failure: report it as unavailable, not as a bad write.
    done.map(|_| ())
        .ok_or_else(|| unavailable("Audio stream is unavailable"))
}
/// N0051: make a device the default output or input. The name must be a PipeWire `Audio/Sink` or
/// `Audio/Source` node; only its numeric id reaches `wpctl`.
async fn set_default(env: Env, node: String) -> std::result::Result<(), Failure> {
    let guard = pill_controls::audio_lock()
        .try_lock_owned()
        .map_err(|_| busy("Audio control is busy"))?;
    blocking(move || {
        let _guard = guard;
        let id = resolve_node(&env, &node, true)?;
        tool(&env.wpctl, &["set-default", &id], Duration::from_secs(5))
            .map(|_| ())
            .ok_or_else(action_error)
    })
    .await
}
/// The numeric PipeWire id of an audio node named `name` (nothing else is ever passed on).
/// `device` narrows it to a sink or source, the only nodes that can be a default.
fn resolve_node(env: &Env, name: &str, device: bool) -> std::result::Result<String, Failure> {
    let dump = tool(&env.pw_dump, &[], Duration::from_secs(5))
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .ok_or_else(|| unavailable("Audio devices are unavailable"))?;
    dump.as_array()
        .into_iter()
        .flatten()
        .find(|object| {
            object["type"] == "PipeWire:Interface:Node"
                && object["info"]["props"]["node.name"] == name
                && object["info"]["props"]["media.class"]
                    .as_str()
                    .is_some_and(|class| {
                        if device {
                            matches!(class, "Audio/Sink" | "Audio/Source")
                        } else {
                            class.starts_with("Audio/") || class.contains("/Audio")
                        }
                    })
        })
        .and_then(|object| object["id"].as_u64())
        .map(|id| id.to_string())
        .ok_or_else(|| unavailable("Audio device is unavailable"))
}

// ---------------------------------------------------- brightness and keyboard

fn light_run<'a>(env: &'a Env) -> impl FnMut(&str, &[String], Duration) -> Result<String> + 'a {
    move |program, args, timeout| {
        let program = match program {
            "brightnessctl" => &env.brightnessctl,
            "ddcutil" => &env.ddcutil,
            _ => return Err("Unsupported light tool".into()),
        };
        Ok(String::from_utf8(
            process::checked(program, args, None, timeout)?.stdout,
        )?)
    }
}
fn set_keyboard(env: &Env, level: i64) -> std::result::Result<(i64, i64, bool), Failure> {
    let mut run = light_run(env);
    let found = light_devices::discover(&[], &env.sys, &mut run).map_err(|_| action_error())?;
    let device = found
        .get("keyboard")
        .filter(|row| !row.is_null())
        .ok_or_else(|| unavailable("Keyboard backlight is unavailable"))?;
    let (Some(max), Some(current)) = (
        device["maximum"].as_i64().filter(|max| *max > 0),
        device["current"].as_i64(),
    ) else {
        return Err(action_error());
    };
    if level > max {
        return Err(invalid());
    }
    if level == current {
        return Ok((current, max, false));
    }
    let written =
        light_devices::write(device, level, &env.sys, &mut run).map_err(|_| action_error())?;
    let now = written["current"].as_i64().ok_or_else(action_error)?;
    Ok((now, max, true))
}

// ---------------------------------------------------------- power profile

fn set_profile(env: &Env, profile: PowerProfileName) -> std::result::Result<(), Failure> {
    let name = profile.name();
    tool(
        &env.powerprofilesctl,
        &["set", name],
        Duration::from_secs(5),
    )
    .ok_or_else(action_error)?;
    // The service may refuse a profile it does not offer without failing the tool.
    match tool(&env.powerprofilesctl, &["get"], Duration::from_secs(5)) {
        Some(active) if active.trim() == name => Ok(()),
        _ => Err(action_error()),
    }
}

// ---------------------------------------------------------------- airplane

struct Radio {
    kind: &'static str,
    soft: bool,
}
fn radios(sys: &Path) -> Vec<Radio> {
    let Ok(entries) = std::fs::read_dir(sys.join("class/rfkill")) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten().take(64) {
        let read = |name: &str| {
            std::fs::read_to_string(entry.path().join(name))
                .map(|text| text.trim().to_owned())
                .unwrap_or_default()
        };
        let kind = match read("type").as_str() {
            "wlan" => "wlan",
            "bluetooth" => "bluetooth",
            _ => continue,
        };
        found.push(Radio {
            kind,
            soft: read("soft") == "1",
        });
    }
    found
}
fn kinds(radios: &[Radio]) -> Vec<&'static str> {
    let mut kinds = radios.iter().map(|radio| radio.kind).collect::<Vec<_>>();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}
fn set_airplane(env: &Env, enabled: bool) -> std::result::Result<(), Failure> {
    let before = radios(&env.sys);
    let present = kinds(&before);
    if present.is_empty() {
        return Err(unavailable("No radio can be switched"));
    }
    let record = env.paths.state.join("controls-airplane.json");
    let targets: Vec<&str> = if enabled {
        // Remember what was on so that leaving airplane mode restores exactly that.
        let on = present
            .iter()
            .copied()
            .filter(|kind| before.iter().any(|r| r.kind == *kind && !r.soft))
            .collect::<Vec<_>>();
        if !on.is_empty() {
            crate::state::write(&record, &json!({"restore":on})).map_err(|_| action_error())?;
        }
        present.clone()
    } else {
        let saved = crate::state::read(&record, json!({})).unwrap_or_else(|_| json!({}));
        let restore = present
            .iter()
            .copied()
            .filter(|kind| {
                saved["restore"]
                    .as_array()
                    .is_none_or(|list| list.iter().any(|v| v == kind))
            })
            .collect::<Vec<_>>();
        if restore.is_empty() {
            present.clone()
        } else {
            restore
        }
    };
    let mut args = vec![if enabled { "block" } else { "unblock" }];
    args.extend(&targets);
    tool(&env.rfkill, &args, Duration::from_secs(5)).ok_or_else(action_error)?;
    let after = radios(&env.sys);
    if !targets.iter().all(|kind| {
        after
            .iter()
            .filter(|r| r.kind == *kind)
            .all(|r| r.soft == enabled)
    }) {
        return Err(action_error());
    }
    if !enabled {
        let _ = std::fs::remove_file(&record);
    }
    Ok(())
}

// -------------------------------------------------- units and night light

/// `(stay awake active, recording active)`; `None` when systemd cannot be asked.
fn units(env: &Env) -> (Option<bool>, Option<bool>) {
    let args = strings(&["--user", "is-active", STAY_AWAKE_UNIT, RECORDING_UNIT]);
    // A non-zero exit only says that some unit is inactive; the lines carry the states.
    let Ok(output) = process::capture(command(&env.systemctl, &args), None, Duration::from_secs(5))
    else {
        return (None, None);
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = text.lines().map(str::trim);
    let (first, second) = (lines.next(), lines.next());
    match (first, second) {
        (Some(a), Some(b)) => (Some(a == "active"), Some(b == "active")),
        _ => (None, None),
    }
}
fn set_stay_awake(env: &Env, enabled: bool) -> std::result::Result<bool, Failure> {
    let current = units(env)
        .0
        .ok_or_else(|| unavailable("Systemd is unavailable"))?;
    if current == enabled {
        return Ok(false);
    }
    // The helper command toggles and refuses a unit it does not own.
    let (code, _) = engine(
        env,
        &["menu-backend", "tool", "stay-awake"],
        None,
        Duration::from_secs(15),
    )
    .ok_or_else(action_error)?;
    if code != 0 || units(env).0 != Some(enabled) {
        return Err(action_error());
    }
    Ok(true)
}
fn stop_recording(env: &Env) -> std::result::Result<bool, Failure> {
    if units(env).1 != Some(true) {
        return Ok(false);
    }
    let (code, _) = engine(
        env,
        &["menu-backend", "tool", "record-stop"],
        None,
        Duration::from_secs(25),
    )
    .ok_or_else(action_error)?;
    if code != 0 || units(env).1 == Some(true) {
        return Err(action_error());
    }
    Ok(true)
}
fn night_state(env: &Env) -> Option<Value> {
    let (code, state) = engine(
        env,
        &["control-tools", "state"],
        None,
        Duration::from_secs(20),
    )?;
    (code == 0 && state["nightLightSupported"].is_boolean()).then_some(state)
}
fn set_night_light(env: &Env, enabled: bool) -> std::result::Result<bool, Failure> {
    let state = night_state(env).ok_or_else(|| unavailable("Night light is unavailable"))?;
    if state["nightLightSupported"] != true {
        return Err(unavailable("Night light is unavailable"));
    }
    if state["nightLightExternal"] == true {
        return Err(unavailable("Night light is managed by another controller"));
    }
    if (state["nightLightEnabled"] == true) == enabled {
        return Ok(false);
    }
    let (code, _) = engine(
        env,
        &[
            "control-tools",
            "night-light",
            if enabled { "on" } else { "off" },
        ],
        None,
        Duration::from_secs(25),
    )
    .ok_or_else(action_error)?;
    if code != 0 {
        return Err(action_error());
    }
    Ok(true)
}

// ------------------------------------------------- interactive capture tools

/// Start an interactive tool (area selection, picker, recorder start) without waiting for the
/// user: the reply means "started"; the tool notifies the user itself and is stopped after
/// `CAPTURE_LIMIT`. One at a time. The caller closes its own panels first.
fn spawn_capture(env: &Env, args: &[&str]) -> std::result::Result<(), Failure> {
    spawn_capture_with(env, args, None)
}
/// `spawn_capture` with one JSON document on the tool's stdin (the helper's `input`).
fn spawn_capture_with(
    env: &Env,
    args: &[&str],
    input: Option<String>,
) -> std::result::Result<(), Failure> {
    if CAPTURE_BUSY.with(|busy| busy.replace(true)) {
        return Err(busy("A capture tool is already running"));
    }
    let child = tokio::process::Command::new(&env.engine)
        .args(args)
        // The helper must not talk to the daemon's service-manager socket.
        .env_remove("NOTIFY_SOCKET")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(_) => {
            CAPTURE_BUSY.with(|busy| busy.set(false));
            return Err(action_error());
        }
    };
    let stdin = child.stdin.take();
    tokio::task::spawn_local(async move {
        if let (Some(text), Some(mut stdin)) = (input, stdin) {
            use tokio::io::AsyncWriteExt;
            // Dropping the pipe is the helper's end of input.
            let _ = stdin.write_all(text.as_bytes()).await;
        }
        if tokio::time::timeout(CAPTURE_LIMIT, child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
        CAPTURE_BUSY.with(|busy| busy.set(false));
        changed();
    });
    Ok(())
}

// ------------------------------------------------------------ appearance

fn set_appearance(env: &Env, mode: AppearanceMode) -> std::result::Result<(), Failure> {
    let request = match mode {
        AppearanceMode::Auto => json!({"paletteAutoMode":true}),
        AppearanceMode::Light => json!({"paletteAutoMode":false,"paletteMode":"light"}),
        AppearanceMode::Dark => json!({"paletteAutoMode":false,"paletteMode":"dark"}),
    };
    let line = format!("{request}\n");
    let (code, reply) = engine(
        env,
        &["wallpaper-media", "theme"],
        Some(&line),
        Duration::from_secs(100),
    )
    .ok_or_else(action_error)?;
    if code == 0 && reply["error"].is_null() {
        Ok(())
    } else {
        Err(action_error())
    }
}
async fn read_appearance(hub: &Shared) -> Option<Appearance> {
    let data = behaviours::settings_data(hub, "wallpaper-picker").await?;
    let mode = if data["paletteAutoMode"] == true {
        AppearanceMode::Auto
    } else if data["paletteMode"] == "light" {
        AppearanceMode::Light
    } else {
        AppearanceMode::Dark
    };
    Some(Appearance { mode })
}

// --------------------------------------------------------- next wallpaper

/// The QML `advanceRotation(true)`: the next wallpaper of the configured kind, shuffled or in
/// order, through the same selection command the picker and the rotation use.
async fn wallpaper_next(hub: &Shared, env: &Env) -> std::result::Result<bool, Failure> {
    let has_active_topic = hub
        .borrow()
        .snapshot(super::active_wallpaper::TOPIC)
        .is_some();
    if has_active_topic {
        let available = hub
            .borrow()
            .snapshot(super::active_wallpaper::TOPIC)
            .and_then(|data| {
                serde_json::from_value::<nacre_protocol::wallpaper::WallpaperActive>(data).ok()
            })
            .is_some_and(|state| state.available && !state.stale);
        if !available {
            return Ok(false);
        }
        super::active_wallpaper::Producer::default()
            .refresh(hub, &env.paths)
            .map_err(|_| action_error())?;
    }
    let active = hub
        .borrow()
        .snapshot(super::active_wallpaper::TOPIC)
        .and_then(|data| {
            serde_json::from_value::<nacre_protocol::wallpaper::WallpaperActive>(data).ok()
        });
    if active
        .as_ref()
        .is_some_and(|state| !state.available || state.stale || !state.previous_available)
    {
        return Ok(false);
    }
    let now = chrono::Utc::now().timestamp_millis();
    let plan = behaviours::read_plan(hub, &env.paths, now)
        .await
        .map_err(|_| unavailable("Wallpaper settings are unavailable"))?;
    if active.is_some()
        && super::active_wallpaper::committed_source(hub).as_deref() != Some(plan.current.as_str())
    {
        super::active_wallpaper::mark_source_unstable(hub).map_err(|_| action_error())?;
        return Ok(false);
    }
    let pool = behaviours::catalog_pool(&env.engine, &plan.kind)
        .await
        .ok_or_else(|| unavailable("The wallpaper library is unavailable"))?;
    if pool.len() < 2 {
        super::active_wallpaper::update_pool_readiness(hub, false).map_err(|_| action_error())?;
        return Ok(false);
    }
    let selection = super::active_wallpaper::SelectionGuard::acquire_tracked(hub)?;
    if active.is_some() && !super::active_wallpaper::selection_environment_ready(hub) {
        return Ok(false);
    }
    if active.is_some()
        && super::active_wallpaper::committed_source(hub).as_deref() != Some(plan.current.as_str())
    {
        super::active_wallpaper::mark_source_unstable(hub).map_err(|_| action_error())?;
        return Ok(false);
    }
    let random = (uuid::Uuid::new_v4().as_u128() >> 75) as f64 / (1u64 << 53) as f64;
    let (choice, _) = behaviours::rotation_next(&pool, &plan.current, plan.shuffle, &[], random);
    let Some(path) = choice else {
        return Ok(false);
    };
    let selected = behaviours::select(&env.engine, &path).await;
    drop(selection);
    if selected {
        if hub
            .borrow()
            .snapshot(super::active_wallpaper::TOPIC)
            .is_some()
        {
            super::active_wallpaper::Producer::default()
                .refresh(hub, &env.paths)
                .map_err(|_| action_error())?;
        }
        Ok(true)
    } else {
        Err(action_error())
    }
}

/// QML `previousWallpaper()`: traverse the committed, bounded history or wrap
/// to the preceding item in the existing behavior worker's configured pool.
async fn wallpaper_previous(hub: &Shared, env: &Env) -> std::result::Result<bool, Failure> {
    let was_available = hub
        .borrow()
        .snapshot(super::active_wallpaper::TOPIC)
        .and_then(|data| {
            serde_json::from_value::<nacre_protocol::wallpaper::WallpaperActive>(data).ok()
        })
        .is_some_and(|state| state.available && !state.stale);
    if !was_available {
        return Ok(false);
    }
    super::active_wallpaper::Producer::default()
        .refresh(hub, &env.paths)
        .map_err(|_| action_error())?;
    let state: nacre_protocol::wallpaper::WallpaperActive = hub
        .borrow()
        .snapshot(super::active_wallpaper::TOPIC)
        .and_then(|data| serde_json::from_value(data).ok())
        .ok_or_else(|| unavailable("Wallpaper is unavailable"))?;
    if !state.available || state.stale || !state.previous_available {
        return Ok(false);
    }
    // Check before tracked admission marks the topic temporarily unavailable.
    let selection = super::active_wallpaper::SelectionGuard::acquire_tracked(hub)?;
    let plan = behaviours::read_plan(hub, &env.paths, chrono::Utc::now().timestamp_millis())
        .await
        .map_err(|_| unavailable("Wallpaper settings are unavailable"))?;
    let Some(current) = super::active_wallpaper::committed_source(hub) else {
        super::active_wallpaper::mark_source_unstable(hub).map_err(|_| action_error())?;
        return Ok(false);
    };
    if current != plan.current {
        super::active_wallpaper::mark_source_unstable(hub).map_err(|_| action_error())?;
        return Ok(false);
    }
    let Some(pool) = behaviours::catalog_pool(&env.engine, &plan.kind).await else {
        super::active_wallpaper::update_pool_readiness(hub, false).map_err(|_| action_error())?;
        return Err(unavailable("The wallpaper library is unavailable"));
    };
    if pool.len() < 2 {
        super::active_wallpaper::update_pool_readiness(hub, false).map_err(|_| action_error())?;
        return Ok(false);
    }
    if !super::active_wallpaper::selection_environment_ready(hub) {
        return Ok(false);
    }
    if super::active_wallpaper::committed_source(hub).as_deref() != Some(current.as_str()) {
        super::active_wallpaper::mark_source_unstable(hub).map_err(|_| action_error())?;
        return Ok(false);
    }
    let Some(target) = super::active_wallpaper::previous_target(hub, &pool, &current) else {
        super::active_wallpaper::update_pool_readiness(hub, false).map_err(|_| action_error())?;
        return Ok(false);
    };
    let selected = behaviours::select(&env.engine, &target).await;
    if !selected {
        super::active_wallpaper::previous_failed(hub, &target);
        drop(selection);
        return Err(action_error());
    }
    drop(selection);
    super::active_wallpaper::Producer::default()
        .refresh(hub, &env.paths)
        .map_err(|_| action_error())?;
    Ok(true)
}

// ----------------------------------------------------------------- state

fn percent(current: i64, maximum: i64) -> u8 {
    ((current.clamp(0, maximum) * 100 + maximum / 2) / maximum) as u8
}
fn internal(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("edp") || name.starts_with("lvds")
}
struct Probe {
    outputs: Vec<String>,
    /// Read external (DDC) outputs too.
    ddc: bool,
    /// Re-read night light.
    slow: bool,
}
fn light_externals(env: &Env, probe: &Probe) -> Vec<Value> {
    if probe.ddc && env.has(&env.ddcutil) {
        probe
            .outputs
            .iter()
            .filter(|name| !internal(name))
            .map(|name| json!(name))
            .collect()
    } else {
        Vec::new()
    }
}
/// N0067: backlight, keyboard-light and DDC reads can hang on slow firmware. They run on their own
/// thread, one scan at a time: while a scan is still stuck the next ones are skipped (`None`, the
/// prior values stay), and a scan that takes longer than the bound is abandoned by the caller.
async fn scan_lights(env: &Env, probe: &Probe) -> Option<Value> {
    static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    use std::sync::atomic::Ordering::SeqCst;
    if BUSY.swap(true, SeqCst) {
        return None;
    }
    let externals = light_externals(env, probe);
    let bound = Duration::from_secs(if externals.is_empty() { 2 } else { 8 });
    let env = env.clone();
    let scan = crate::process::offload(move || {
        let found = light_devices::discover(&externals, &env.sys, &mut light_run(&env)).ok();
        if let Some(found) = &found {
            light_devices::note_keyboard(&env.sys, !found["keyboard"].is_null());
        }
        BUSY.store(false, SeqCst);
        found
    });
    tokio::time::timeout(bound, scan).await.ok()?.ok()?
}

/// One read of everything but `appearance` (settings, async). Members whose backend is absent
/// stay `None`; slow or DDC members not re-read keep their `prior` value.
fn collect(
    env: &Env,
    probe: &Probe,
    prior: &ControlsState,
    lights: Option<Value>,
) -> ControlsState {
    let mut state = ControlsState {
        health: Health {
            available: true,
            stale: false,
            error: None,
        },
        brightness: prior
            .brightness
            .iter()
            .filter(|(name, _)| probe.outputs.contains(name))
            .map(|(name, level)| (name.clone(), *level))
            .collect(),
        keyboard: None,
        airplane: None,
        night_light: prior.night_light.clone(),
        stay_awake: None,
        recorder: None,
        appearance: prior.appearance.clone(),
    };
    if let Some(found) = lights {
        let panels = probe
            .outputs
            .iter()
            .filter(|name| internal(name))
            .collect::<Vec<_>>();
        if let ([panel], Some(maximum), Some(current)) = (
            panels.as_slice(),
            found["backlight"]["maximum"].as_i64().filter(|m| *m > 0),
            found["backlight"]["current"].as_i64(),
        ) {
            state
                .brightness
                .insert((*panel).clone(), percent(current, maximum));
        }
        for row in found["ddc"].as_array().into_iter().flatten() {
            if let (Some(screen), Some(maximum), Some(current)) = (
                row["screen"].as_str(),
                row["maximum"].as_i64().filter(|m| *m > 0),
                row["current"].as_i64(),
            ) {
                state
                    .brightness
                    .insert(screen.to_owned(), percent(current, maximum));
            }
        }
        if let (Some(maximum), Some(current)) = (
            found["keyboard"]["maximum"]
                .as_i64()
                .filter(|m| (1..=65535).contains(m)),
            found["keyboard"]["current"].as_i64(),
        ) {
            state.keyboard = Some(KeyboardLight {
                level: current.clamp(0, maximum) as u32,
                max: maximum as u32,
            });
        }
    }
    let present = radios(&env.sys);
    if rfkill_present(env) {
        state.airplane = Some(Toggle {
            enabled: present.iter().all(|radio| radio.soft),
        });
    }
    let (stay, recording) = units(env);
    if systemd_present(env) {
        state.stay_awake = stay.map(|enabled| Toggle { enabled });
    }
    if systemd_present(env) && env.has("wf-recorder") && env.has("slurp") {
        state.recorder = recording.map(|recording| Recorder { recording });
    }
    if probe.slow {
        state.night_light = night_state(env).and_then(|night| {
            (night["nightLightSupported"] == true).then(|| NightLight {
                enabled: night["nightLightEnabled"] == true,
                external: night["nightLightExternal"] == true,
            })
        });
    }
    state
}
fn output_names(hub: &Shared) -> Vec<String> {
    let snapshot = hub.borrow().snapshot("compositor").unwrap_or(Value::Null);
    let mut names = snapshot["outputs"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, row)| {
            let name = row["name"].as_str().unwrap_or(key);
            (!name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control))
                .then(|| name.to_owned())
        })
        .collect::<Vec<_>>();
    names.truncate(MAX_OUTPUTS);
    names
}

fn empty_state() -> ControlsState {
    ControlsState {
        health: Health {
            available: true,
            stale: false,
            error: None,
        },
        brightness: BTreeMap::new(),
        keyboard: None,
        airplane: None,
        night_light: None,
        stay_awake: None,
        recorder: None,
        appearance: None,
    }
}
fn publish(hub: &Shared, state: &ControlsState) -> Result<()> {
    debug_assert!(state.validate().is_ok());
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

/// The registry-relevant members (night light) need one read before any client subscribes.
pub async fn initial(hub: &Shared) -> Result<()> {
    if hub.borrow().snapshot(TOPIC).is_none() {
        publish(hub, &empty_state())?;
    }
    let env = env().map_err(|_| "Control environment is unavailable")?;
    let probe = Probe {
        outputs: output_names(hub),
        ddc: false,
        slow: true,
    };
    let prior = empty_state();
    let lights = scan_lights(&env, &probe).await;
    let state = crate::process::offload(move || collect(&env, &probe, &prior, lights)).await?;
    publish(hub, &state)
}

/// One subscribed period: read now, then on each tick and each `changed()`.
async fn session(hub: Shared, env: Env) -> Result<()> {
    let refresh = REFRESH.with(std::rc::Rc::clone);
    let mut ticker = tokio::time::interval(env.tick);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let (mut count, mut full) = (0u32, true);
    let mut state = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|data| serde_json::from_value::<ControlsState>(data).ok())
        .unwrap_or_else(empty_state);
    loop {
        let slow = full || count.is_multiple_of(SLOW_EVERY);
        let probe = Probe {
            outputs: output_names(&hub),
            ddc: full,
            slow,
        };
        let lights = scan_lights(&env, &probe).await;
        let (read_env, prior) = (env.clone(), state.clone());
        state = crate::process::offload(move || collect(&read_env, &probe, &prior, lights)).await?;
        if slow {
            state.appearance = read_appearance(&hub).await;
        }
        publish(&hub, &state)?;
        full = false;
        tokio::select! {
            _ = ticker.tick() => count = count.wrapping_add(1),
            () = refresh.notified() => full = true,
        }
    }
}
async fn idle(interest: &mut tokio::sync::watch::Receiver<bool>) -> Result<()> {
    while *interest.borrow() {
        interest.changed().await?;
    }
    Ok(())
}
pub async fn run(hub: Shared) -> Result<()> {
    let env = env().map_err(|_| "Control environment is unavailable")?;
    serve(hub, env).await
}
async fn serve(hub: Shared, env: Env) -> Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    hub.borrow_mut().service("controls", "running", 0)?;
    loop {
        while !*interest.borrow() {
            interest.changed().await?;
        }
        tokio::select! {
            result = session(hub.clone(), env.clone()) => result?,
            result = idle(&mut interest) => result?,
        }
    }
}

#[cfg(test)]
mod tests;
