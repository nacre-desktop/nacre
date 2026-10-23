//! N0049: the `capabilities` topic, the flags the Settings registry's `visibleWhen`/`enabledWhen`
//! reference. The values are the ones QML's `NacreSettingsCapabilities` computed, taken from what
//! the daemon already publishes (`compositor`, `settings.store.desktop`, `settings.store.profile`)
//! plus three probes made once per daemon start: input devices, `powerprofilesctl` and `pacman`.
//! A flag the registry names but QML never defined (`batterySaver`, `cursorShake`) is absent, which
//! consumers evaluate as false, exactly as QML did.
use super::{
    behaviours::changed,
    hub::{Hub, Shared},
};
use crate::{process, Result};
use nacre_protocol::{
    core_services::Health,
    system_settings::{CapabilitiesState, CAPABILITIES_TOPIC},
};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

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
    pub powerprofilesctl: String,
    pub which: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}
impl Env {
    fn production() -> Self {
        Self {
            devices: "/proc/bus/input/devices".into(),
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
    packages: bool,
    power_profiles: bool,
    performance_profile: bool,
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
    let mut flags = BTreeMap::from([
        ("ai".into(), component("ai")),
        ("brain".into(), component("brain")),
        ("packages".into(), probe.packages),
        ("powerProfiles".into(), probe.power_profiles),
        ("performanceProfile".into(), probe.performance_profile),
        ("touchpad".into(), probe.touchpad),
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
async fn run_with(hub: Shared, env: Env) -> Result<()> {
    let probe = tokio::task::spawn_blocking(move || probe(&env)).await?;
    let mut notices = hub.borrow().observe();
    hub.borrow_mut().service("capabilities", "running", 0)?;
    loop {
        publish(&hub, &probe)?;
        changed(
            &mut notices,
            &[
                "compositor",
                "settings.store.desktop",
                "settings.store.profile",
            ],
        )
        .await?;
    }
}

#[cfg(test)]
mod tests;
