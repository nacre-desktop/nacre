//! Capability-gated action plans, reviewed and confirmed in a visible terminal.
use crate::{i18n::tr, paths::Paths, process, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, IsTerminal, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    time::Duration,
};

pub fn tool(name: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|root| {
        let path = root.join(name);
        path.is_file() && fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    })
}
pub fn read(program: &str, args: &[&str]) -> Option<String> {
    if !tool(program) {
        return None;
    }
    process::run(
        program,
        &args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        None,
        Duration::from_secs(3),
    )
    .ok()
    .filter(|o| o.status.success())
    .and_then(|o| String::from_utf8(o.stdout).ok())
}
fn unit(name: &str, user: bool) -> bool {
    let args = if user {
        vec!["--user", "show", name, "-p", "LoadState", "--value"]
    } else {
        vec!["show", name, "-p", "LoadState", "--value"]
    };
    read("systemctl", &args).is_some_and(|s| s.trim() == "loaded")
}
fn touchpads() -> Vec<String> {
    if std::env::var("NACRE_COMPOSITOR").is_ok_and(|s| s != "hyprland")
        || std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none()
    {
        return vec![];
    }
    let mut confirmed = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir("/sys/class/input") {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(device) = fs::read_to_string(path.join("dev")).ok() else {
                continue;
            };
            let tags =
                fs::read_to_string(Path::new("/run/udev/data").join(format!("c{}", device.trim())))
                    .unwrap_or_default();
            if tags.lines().any(|line| line == "E:ID_INPUT_TOUCHPAD=1") {
                if let Ok(name) = fs::read_to_string(path.join("device/name")) {
                    confirmed.insert(name.trim().to_lowercase().replace(' ', "-"));
                }
            }
        }
    }
    let data = read("hyprctl", &["devices", "-j"])
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(Value::Null);
    data["mice"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let name = r["name"].as_str()?;
            let touchpad = r["type"] == "touchpad"
                || r["isTouchpad"] == true
                || confirmed.contains(&name.to_lowercase());
            (touchpad
                && name.len() <= 160
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.:/".contains(c)))
            .then(|| name.to_string())
        })
        .collect()
}
fn directory_entries(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut r| r.next().is_some())
}
pub fn facts() -> Value {
    let wifi = fs::read_dir("/sys/class/net")
        .is_ok_and(|r| r.flatten().any(|e| e.path().join("wireless").is_dir()));
    let firmware = read("fwupdmgr", &["get-devices", "--json"])
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .is_some_and(|v| {
            v["Devices"].as_array().is_some_and(|rows| {
                rows.iter().any(|r| {
                    r["Flags"]
                        .as_array()
                        .is_some_and(|flags| flags.iter().any(|f| f == "updatable"))
                })
            })
        });
    json!({"audio":unit("pipewire.service",true)&&unit("wireplumber.service",true),"pulse":unit("pipewire-pulse.service",true),"wifi":wifi&&tool("nmcli")&&unit("NetworkManager.service",false),"bluetooth":directory_entries(Path::new("/sys/class/bluetooth"))&&unit("bluetooth.service",false),"touchpads":touchpads(),"firmware":firmware,"packages":tool("pacman"),"cache":tool("paccache"),"admin":tool("pkexec")||tool("sudo"),"terminal":tool("kitty")})
}
pub fn health_rows(facts: &Value) -> Vec<Value> {
    let mut rows = vec![];
    for (id, label, description, capability, root) in [
        (
            "audio-restart",
            tr("software.health.audio", "Restart audio"),
            tr(
                "software.health.audio-detail",
                "Restart PipeWire and WirePlumber; audio will briefly stop",
            ),
            "audio",
            false,
        ),
        (
            "wifi-restart",
            tr("software.health.wifi", "Restart Wi-Fi"),
            tr(
                "software.health.wifi-detail",
                "Restart NetworkManager; active connections will disconnect",
            ),
            "wifi",
            true,
        ),
        (
            "bluetooth-restart",
            tr("software.health.bluetooth", "Restart Bluetooth"),
            tr(
                "software.health.bluetooth-detail",
                "Restart Bluetooth; connected devices will disconnect",
            ),
            "bluetooth",
            true,
        ),
        (
            "firmware-update",
            tr("software.health.firmware", "Firmware updates"),
            tr(
                "software.health.firmware-detail",
                "Review and apply fwupd updates; a reboot may be required",
            ),
            "firmware",
            true,
        ),
        (
            "orphans-remove",
            tr("software.health.orphans", "Remove orphaned packages"),
            tr(
                "software.health.orphans-detail",
                "Review unused dependency packages before removal",
            ),
            "packages",
            true,
        ),
        (
            "cache-clean",
            tr("software.health.cache", "Clean package caches"),
            tr(
                "software.health.cache-detail",
                "Keep the newest three cached package versions",
            ),
            "cache",
            true,
        ),
    ] {
        if facts[capability] == true
            && facts["terminal"] == true
            && (!root || facts["admin"] == true)
        {
            rows.push(json!({"id":id,"name":label,"description":description,"section":"health.fixits","root":root}))
        }
    }
    if facts["touchpads"].as_array().is_some_and(|p| !p.is_empty()) && facts["terminal"] == true {
        rows.push(json!({"id":"touchpad-restart","name":tr("software.health.touchpad","Restart touchpad input"),"description":tr("software.health.touchpad-detail","Disable and re-enable supported touchpads in the compositor"),"section":"health.fixits","root":false}));
    }
    rows
}
pub fn snapshot() -> Value {
    let facts = facts();
    json!({"health":health_rows(&facts),"facts":facts})
}
fn step(args: &[&str], root: bool) -> Value {
    json!({"arguments":args,"root":root})
}
pub fn health_plan(id: &str, facts: &Value) -> Result<Value> {
    if !health_rows(facts).iter().any(|r| r["id"] == id) {
        return Err("This action is unavailable on the current hardware or tools".into());
    }
    let mut steps = vec![];
    match id {
        "audio-restart" => {
            let mut args = vec![
                "systemctl",
                "--user",
                "restart",
                "pipewire.service",
                "wireplumber.service",
            ];
            if facts["pulse"] == true {
                args.push("pipewire-pulse.service")
            }
            steps.push(step(&args, false));
        }
        "wifi-restart" => steps.push(step(
            &["systemctl", "restart", "NetworkManager.service"],
            true,
        )),
        "bluetooth-restart" => {
            steps.push(step(&["systemctl", "restart", "bluetooth.service"], true))
        }
        "touchpad-restart" => {
            for pad in facts["touchpads"].as_array().unwrap() {
                let name = pad.as_str().ok_or("Invalid touchpad")?;
                if !name.is_ascii() || name.chars().any(|c| c.is_control()) {
                    return Err("Invalid touchpad identity".into());
                }
                let quoted = serde_json::to_string(name)?;
                for enabled in [false, true] {
                    let expression = format!("hl.device({{name={quoted},enabled={enabled}}})");
                    steps.push(step(&["hyprctl", "eval", &expression], false));
                }
            }
        }
        "firmware-update" => {
            steps.push(step(&["fwupdmgr", "refresh"], true));
            steps.push(step(&["fwupdmgr", "update"], true));
        }
        "orphans-remove" => {
            steps.push(json!({"arguments":["pacman","-Rns","--"],"root":true,"orphans":true}))
        }
        "cache-clean" => steps.push(step(&["paccache", "--remove", "--keep", "3"], true)),
        _ => return Err("Unknown Health action".into()),
    }
    Ok(json!({"id":id,"steps":steps}))
}
pub fn run_health(id: &str) -> Result<i32> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("System actions require a visible interactive terminal".into());
    }
    let facts = facts();
    let mut plan = health_plan(id, &facts)?;
    if id == "orphans-remove" {
        let packages = read("pacman", &["-Qdtq"]).unwrap_or_default();
        let names = packages
            .lines()
            .filter(|s| !s.is_empty())
            .map(crate::menu::package_name)
            .collect::<Result<Vec<_>>>()?;
        if names.is_empty() {
            println!(
                "{}",
                tr("software.health.no-orphans", "No orphaned packages found.")
            );
            return Ok(0);
        }
        plan["steps"][0]["arguments"]
            .as_array_mut()
            .unwrap()
            .extend(names.into_iter().map(|n| json!(n)));
    }
    println!("{}", id);
    for step in plan["steps"].as_array().unwrap() {
        println!(
            "{}{}",
            if step["root"] == true {
                tr(
                    "software.actions.admin-prefix",
                    "Administrator permission: ",
                )
            } else {
                String::new()
            },
            serde_json::to_string(&step["arguments"])?
        );
    }
    print!(
        "{} ",
        tr(
            "software.actions.confirm",
            "Type yes to run these commands:"
        )
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if answer.trim() != "yes" {
        return Ok(0);
    }
    if plan["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["root"] == true)
        && fs::read_to_string("/proc/self/uid_map")
            .is_ok_and(|map| map.split_whitespace().collect::<Vec<_>>() != ["0", "0", "4294967295"])
    {
        if !tool("systemd-run") {
            return Err("The normal user-session action bridge is unavailable".into());
        }
        return Ok(Command::new("systemd-run")
            .args(["--user", "--wait", "--pty", "--collect", "--quiet", "--"])
            .arg(std::env::current_exe()?)
            .args(["software-actions", "run", id])
            .status()?
            .code()
            .unwrap_or(1));
    }
    let mut code = 0;
    for step in plan["steps"].as_array().unwrap() {
        let args = step["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        let mut command = if step["root"] == true {
            let mut c = Command::new(if tool("pkexec") { "pkexec" } else { "sudo" });
            c.arg(args[0]);
            c
        } else {
            Command::new(args[0])
        };
        command.args(&args[1..]);
        code = command.status()?.code().unwrap_or(1);
        if code != 0 {
            break;
        }
    }
    println!(
        "{}",
        tr(
            "software.actions.finished",
            "Finished. Press Enter to close."
        )
    );
    answer.clear();
    io::stdin().read_line(&mut answer)?;
    Ok(code)
}
pub fn open(paths: &Paths, id: &str) -> Result<Value> {
    if !health_rows(&facts()).iter().any(|r| r["id"] == id) {
        return Err("Action is not available".into());
    }
    let helper = std::env::current_exe()?.to_string_lossy().into_owned();
    let args = vec![
        "--class".into(),
        "nacre-control".into(),
        "--title".into(),
        id.into(),
        "--".into(),
        helper,
        "software-actions".into(),
        "run".into(),
        id.into(),
    ];
    let _ = paths;
    process::spawn_detached_quiet("kitty", &args)?;
    Ok(
        json!({"message":tr("software.actions.opened","Review and confirm the action in the terminal")}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_gates_and_privilege_boundaries_are_explicit() -> Result<()> {
        let absent = json!({"terminal":true,"admin":true});
        assert!(health_rows(&absent).is_empty());
        let present = json!({"terminal":true,"admin":true,"audio":true,"wifi":true,"bluetooth":true,"firmware":true,"packages":true,"cache":true,"touchpads":[]});
        assert_eq!(health_rows(&present).len(), 6);
        assert_eq!(
            health_plan("audio-restart", &present)?["steps"][0]["root"],
            false
        );
        assert_eq!(
            health_plan("wifi-restart", &present)?["steps"][0]["root"],
            true
        );
        assert!(health_plan("touchpad-restart", &present).is_err());
        let cache = health_plan("cache-clean", &present)?;
        assert_eq!(
            cache["steps"][0]["arguments"],
            json!(["paccache", "--remove", "--keep", "3"])
        );
        Ok(())
    }
}
