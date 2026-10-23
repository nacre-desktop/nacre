//! N0049: the `capabilities` topic, the flags the Settings registry's `visibleWhen`/`enabledWhen`
//! reference. The values are the ones QML's `NacreSettingsCapabilities` computed, taken from what
//! the daemon already publishes (`compositor`, `settings.store.desktop`, `settings.store.profile`)
//! plus three probes made once per daemon start: input devices, `powerprofilesctl` and `pacman`.
//! A flag the registry names but QML never defined (`batterySaver`, `cursorShake`) is absent, which
//! consumers evaluate as false, exactly as QML did.
//!
//! N0051 adds `bluetooth` (the `bluetooth` topic lists an adapter) and `battery` (a laptop battery
//! in `/sys/class/power_supply`, probed with the rest) and `capabilities.refresh`, which probes
//! again so a late-started `power-profiles-daemon` or a docked battery shows without a restart.
use super::{
    behaviours::changed,
    hub::{failure, Hub, Shared},
};
use crate::{process, Result};
use nacre_protocol::{
    core_services::Health,
    pill_controls::PillEmpty,
    system_settings::{CapabilitiesState, CAPABILITIES_TOPIC},
    ErrorCode, Failure, Method,
};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Notify;

const DESKTOP_OPTIONS: [&str; 8] = [
    "gapsIn",
    "gapsOut",
    "borderSize",
    "rounding",
    "blur",
    "shadow",
    "followMouse",
    "naturalScroll",
];
const PROBE_LIMIT: Duration = Duration::from_secs(5);
const DEVICES_LIMIT: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct Env {
    /// `/proc/bus/input/devices`
    pub devices: PathBuf,
    /// `/sys/class/power_supply`
    pub power_supply: PathBuf,
    pub powerprofilesctl: String,
    pub which: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}
impl Env {
    fn production() -> Self {
        Self {
            devices: "/proc/bus/input/devices".into(),
            power_supply: "/sys/class/power_supply".into(),
            powerprofilesctl: "powerprofilesctl".into(),
            which: Arc::new(|program| {
                if program.contains('/') {
                    return std::path::Path::new(program).is_file();
                }
                std::env::var_os("PATH").is_some_and(|path| {
                    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
                })
            }),
        }
    }
}
/// What does not change while the daemon runs.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Probe {
    touchpad: bool,
    battery: bool,
    packages: bool,
    power_profiles: bool,
    performance_profile: bool,
}
/// A power-supply device of type `Battery` that is not a peripheral's (`scope` is `Device` for a
/// mouse or a headset), as UPower's `isLaptopBattery` decides.
fn laptop_battery(dir: &std::path::Path) -> bool {
    let read = |entry: &std::path::Path, name: &str| {
        std::fs::read_to_string(entry.join(name)).map(|text| text.trim().to_owned())
    };
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().take(64).any(|entry| {
            let path = entry.path();
            read(&path, "type").is_ok_and(|kind| kind == "Battery")
                && !read(&path, "scope").is_ok_and(|scope| scope == "Device")
        })
    })
}
fn probe(env: &Env) -> Probe {
    use std::io::Read;
    let mut text = String::new();
    let touchpad = std::fs::File::open(&env.devices)
        .and_then(|file| file.take(DEVICES_LIMIT).read_to_string(&mut text))
        .is_ok()
        && {
            let lower = text.to_ascii_lowercase();
            lower.contains("touchpad") || lower.contains("trackpad")
        };
    let run = |args: &[&str]| {
        let mut command = std::process::Command::new(&env.powerprofilesctl);
        command.args(args).env_remove("NOTIFY_SOCKET");
        process::capture(command, None, PROBE_LIMIT)
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let power_profiles = (env.which)(&env.powerprofilesctl) && run(&["get"]).is_some();
    let performance_profile =
        power_profiles && run(&["list"]).is_some_and(|list| list.contains("performance"));
    Probe {
        touchpad,
        battery: laptop_battery(&env.power_supply),
        packages: (env.which)("pacman"),
        power_profiles,
        performance_profile,
    }
}

fn flags(probe: &Probe, hub: &Hub) -> BTreeMap<String, bool> {
    let compositor = hub.snapshot_ref("compositor").unwrap_or(&Value::Null);
    let capability = |name: &str| compositor["capabilities"][name] == true;
    let backend = compositor["backend"].as_str().unwrap_or("");
    let niri = backend == "niri";
    let store = |id: &str| {
        hub.snapshot_ref(&format!("settings.store.{id}"))
            .map_or(Value::Null, |store| store["data"].clone())
    };
    let components = store("profile")["components"].clone();
    let component = |name: &str| {
        components
            .as_array()
            .is_some_and(|all| all.iter().any(|item| item == name))
    };
    // The `bluetooth` topic lists an adapter while BlueZ is there (QML: `NacreBluetooth.available`).
    let bluetooth = hub.snapshot_ref("bluetooth").is_some_and(|state| {
        state["health"]["available"] == true
            && state["adapters"]["order"]
                .as_array()
                .is_some_and(|adapters| !adapters.is_empty())
    });
    let mut flags = BTreeMap::from([
        ("ai".into(), component("ai")),
        ("brain".into(), component("brain")),
        ("packages".into(), probe.packages),
        ("powerProfiles".into(), probe.power_profiles),
        ("performanceProfile".into(), probe.performance_profile),
        ("touchpad".into(), probe.touchpad),
        ("battery".into(), probe.battery),
        ("bluetooth".into(), bluetooth),
        ("panelDragging".into(), capability("nativePanelDrag")),
        ("windowCapture".into(), capability("windowCapture")),
        ("niriWorkspaces".into(), niri),
        (
            "niriFixedWorkspaces".into(),
            niri && store("desktop")["niriWorkspaceMode"] == "fixed-all",
        ),
        (
            "input.controls".into(),
            matches!(backend, "hyprland" | "niri"),
        ),
        ("input.touchpadSpeed".into(), niri),
    ]);
    // A compositor that lists no options (or none yet) offers all of them, as QML did.
    let options = compositor["capabilities"]["desktopOptions"].as_array();
    for key in DESKTOP_OPTIONS {
        flags.insert(
            format!("desktop.{key}"),
            options.is_none_or(|all| all.iter().any(|item| item == key)),
        );
    }
    flags
}
fn publish(hub: &Shared, probe: &Probe) -> Result<()> {
    let state = CapabilitiesState {
        health: Health {
            available: true,
            stale: false,
            error: None,
        },
        flags: flags(probe, &hub.borrow()),
    };
    debug_assert!(state.validate().is_ok());
    hub.borrow_mut()
        .publish(CAPABILITIES_TOPIC, serde_json::to_value(&state)?)?;
    Ok(())
}

/// A first value before the probes ran, so a client that subscribes at once finds the topic.
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub, &Probe::default())
}
pub async fn run(hub: Shared) -> Result<()> {
    run_with(hub, Env::production()).await
}
/// The Settings-page services (N0049, N0051) run in this daemon: not safe mode, and the
/// `capabilities` service started. The methods that need no backend of their own are advertised
/// only then, so a bare hub (tests, a partial daemon) offers none of them.
pub fn pages_up(hub: &Hub) -> bool {
    !hub.safe && hub.services.contains_key("capabilities")
}
/// `capabilities.refresh`: probe again. A request made while a probe runs is not lost.
static REFRESH: Notify = Notify::const_new();
pub fn handles(method: &str) -> bool {
    method == "capabilities.refresh"
}
pub fn dispatch(hub: &Hub, params: Value) -> std::result::Result<Value, Failure> {
    serde_json::from_value::<PillEmpty>(params).map_err(|_| {
        failure(ErrorCode::Validation, "Invalid capabilities request", false).dispatch_state(false)
    })?;
    if !pages_up(hub) {
        return Err(
            failure(ErrorCode::Unavailable, "Capabilities are unavailable", true)
                .dispatch_state(false),
        );
    }
    REFRESH.notify_one();
    Ok(serde_json::json!({"requested":true}))
}
/// Advertised while the service runs.
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    let mut methods = nacre_protocol::settings_asks::methods();
    methods.retain(|name, _| handles(name));
    if !pages_up(hub) {
        methods.clear();
    }
    methods
}
async fn run_with(hub: Shared, env: Env) -> Result<()> {
    let probing = env.clone();
    let mut probed = crate::process::offload(move || probe(&probing)).await?;
    let mut notices = hub.borrow().observe();
    hub.borrow_mut().service("capabilities", "running", 0)?;
    loop {
        publish(&hub, &probed)?;
        tokio::select! {
            result = changed(
                &mut notices,
                &[
                    "compositor",
                    "bluetooth",
                    "settings.store.desktop",
                    "settings.store.profile",
                ],
            ) => result?,
            () = REFRESH.notified() => {
                let probing = env.clone();
                probed = crate::process::offload(move || probe(&probing)).await?;
            }
        }
    }
}

#[cfg(test)]
mod tests;
