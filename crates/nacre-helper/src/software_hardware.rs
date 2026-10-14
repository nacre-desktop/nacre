//! Supported hardware controls; never invent device support or disable the last output.
use crate::{
    i18n::tr,
    paths::Paths,
    software_actions::{read, tool},
    Result,
};
use serde_json::{json, Value};
use std::fs;

fn builtin(name: &str) -> bool {
    name.starts_with("eDP-") || name.starts_with("LVDS-") || name.starts_with("DSI-")
}
pub fn facts(paths: &Paths) -> Value {
    let compositor = std::env::var("NACRE_COMPOSITOR").unwrap_or_else(|_| {
        if std::env::var_os("NIRI_SOCKET").is_some() {
            "niri".into()
        } else if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            "hyprland".into()
        } else {
            String::new()
        }
    });
    let mut outputs = vec![];
    if compositor == "niri" {
        if let Some(data) = read("niri", &["msg", "--json", "outputs"])
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        {
            if let Some(entries) = data.as_object() {
                for (name, value) in entries {
                    outputs.push(json!({"name":name,"builtin":builtin(name),"active":!value["logical"].is_null()}));
                }
            }
        }
    } else if compositor == "hyprland" {
        if let Some(data) = read("hyprctl", &["monitors", "all", "-j"])
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        {
            if let Some(entries) = data.as_array() {
                for v in entries {
                    if let Some(name) = v["name"].as_str() {
                        outputs.push(json!({"name":name,"builtin":builtin(name),"active":v["disabled"]!=true}));
                    }
                }
            }
        }
    }
    let physical = fs::read_dir("/run/udev/data").is_ok_and(|files| {
        files.flatten().any(|f| {
            fs::read_to_string(f.path())
                .is_ok_and(|s| s.lines().any(|l| l == "E:ID_INPUT_TOUCHPAD=1"))
        })
    });
    let pads = crate::software_actions::touchpads();
    let managed = paths
        .config
        .join(if compositor == "niri" {
            "niri/desktop.kdl"
        } else {
            "desktop.lua"
        })
        .is_file();
    let modes = read("supergfxctl", &["--supported"])
        .map(|s| {
            s.split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|s| matches!(*s, "Integrated" | "Hybrid" | "AsusMuxDgpu"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let gpu_count = fs::read_dir("/sys/class/drm")
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    name.starts_with("card")
                        && !name.contains('-')
                        && e.path().join("device/vendor").is_file()
                })
                .count()
        })
        .unwrap_or(0);
    let can = read(
        "busctl",
        &[
            "--system",
            "--json=short",
            "call",
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "CanHibernate",
        ],
    )
    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    .is_some_and(|v| {
        v["data"]
            .as_array()
            .is_some_and(|a| a.iter().any(|s| s == "yes" || s == "challenge"))
    });
    let resume = fs::read_to_string("/sys/power/resume").is_ok_and(|s| s.trim() != "0:0")
        || fs::read_to_string("/proc/cmdline")
            .is_ok_and(|s| s.split_whitespace().any(|w| w.starts_with("resume=")));
    let swap = fs::read_to_string("/proc/swaps").is_ok_and(|s| s.lines().count() > 1);
    json!({"compositor":compositor,"outputs":outputs,"pads":pads,"touchpad":physical&&managed&&tool(if compositor=="niri"{"niri"}else{"hyprctl"})&&(compositor=="niri"||!pads.is_empty()),"gpuModes":if gpu_count>=2{modes}else{vec![]},"hibernate":can&&resume&&swap&&tool("systemctl"),"terminal":tool("kitty"),"admin":tool("pkexec")||tool("sudo")})
}
pub fn rows(f: &Value) -> Vec<Value> {
    let mut rows = vec![];
    if f["terminal"] != true {
        return rows;
    }
    let mut add = |id: String, name: &str, description: &str, root: bool| {
        rows.push(json!({"id":id,"name":name,"description":description,"section":"setup.hardware","root":root}));
    };
    if f["touchpad"] == true {
        add(
            "touchpad-enable".into(),
            &tr("software.hardware.touchpad-enable", "Enable touchpad"),
            "Apply the managed touchpad input preference",
            false,
        );
        add(
            "touchpad-disable".into(),
            &tr("software.hardware.touchpad-disable", "Disable touchpad"),
            "Keyboard navigation remains available",
            false,
        );
    }
    let external = f["outputs"].as_array().is_some_and(|o| {
        o.iter()
            .any(|r| r["builtin"] == false && r["active"] == true)
    });
    if external
        && f["outputs"]
            .as_array()
            .is_some_and(|o| o.iter().any(|r| r["builtin"] == true))
    {
        add(
            "laptop-display-off".into(),
            &tr(
                "software.hardware.laptop-display-off",
                "Turn off laptop display",
            ),
            "Requires an active external display",
            false,
        );
        add(
            "laptop-display-on".into(),
            &tr(
                "software.hardware.laptop-display-on",
                "Turn on laptop display",
            ),
            "Restore the built-in output",
            false,
        );
        if f["compositor"] == "hyprland" {
            add(
                "laptop-mirror".into(),
                &tr("software.hardware.laptop-mirror", "Mirror laptop display"),
                "Mirror to the connected external display",
                false,
            );
        }
    }
    if f["admin"] == true {
        for mode in f["gpuModes"].as_array().into_iter().flatten() {
            let mode = mode.as_str().unwrap();
            add(
                format!("gpu-{mode}"),
                &tr("software.hardware.gpu", "GPU mode: {mode}").replace("{mode}", mode),
                "Uses the detected controller; logout or reboot may be needed",
                true,
            );
        }
        if f["hibernate"] == true {
            add(
                "hibernate".into(),
                &tr("software.hardware.hibernate", "Hibernate"),
                "Save the session to configured swap and power off",
                true,
            );
        }
    }
    rows
}
fn valid_output(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}
pub fn plan(id: &str, f: &Value) -> Result<Value> {
    if !rows(f).iter().any(|r| r["id"] == id) {
        return Err("Hardware action is unavailable".into());
    }
    let mut steps = vec![];
    if id.starts_with("touchpad-") {
        steps.push(json!({"arguments":["Nacre touchpad preference",id],"root":false,"touchpad":id=="touchpad-enable","devices":f["pads"]}));
    } else if id == "hibernate" {
        steps.push(json!({"arguments":["systemctl","hibernate"],"root":true}));
    } else if let Some(mode) = id.strip_prefix("gpu-") {
        if !f["gpuModes"].as_array().unwrap().iter().any(|m| m == mode) {
            return Err("Unsupported GPU mode".into());
        }
        steps.push(json!({"arguments":["supergfxctl","--mode",mode],"root":true}));
    } else {
        let outputs = f["outputs"].as_array().unwrap();
        let external = outputs
            .iter()
            .find(|o| o["builtin"] == false && o["active"] == true)
            .and_then(|o| o["name"].as_str())
            .ok_or("An active external display is required")?;
        if !valid_output(external) {
            return Err("Invalid output identity".into());
        }
        for output in outputs.iter().filter(|o| o["builtin"] == true) {
            let name = output["name"]
                .as_str()
                .filter(|n| valid_output(n))
                .ok_or("Invalid output identity")?;
            let args = if f["compositor"] == "niri" {
                vec![
                    "niri",
                    "msg",
                    "output",
                    name,
                    if id == "laptop-display-off" {
                        "off"
                    } else {
                        "on"
                    },
                ]
            } else {
                let value = if id == "laptop-display-off" {
                    format!(
                        "hl.monitor({{output={},disabled=true}})",
                        serde_json::to_string(name)?
                    )
                } else if id == "laptop-mirror" {
                    format!(
                        "hl.monitor({{output={},disabled=false,mirror={}}})",
                        serde_json::to_string(name)?,
                        serde_json::to_string(external)?
                    )
                } else {
                    format!(
                        "hl.monitor({{output={},disabled=false}})",
                        serde_json::to_string(name)?
                    )
                };
                steps.push(json!({"arguments":["hyprctl","eval",value],"root":false}));
                continue;
            };
            steps.push(json!({"arguments":args,"root":false}));
        }
    }
    Ok(json!({"id":id,"steps":steps}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_last_output_off_or_unsupported_mirror_or_unconfigured_hibernate() -> Result<()> {
        let f = json!({"terminal":true,"admin":true,"compositor":"niri","outputs":[{"name":"eDP-1","builtin":true,"active":true}],"gpuModes":[],"hibernate":false});
        assert!(rows(&f).is_empty());
        assert!(plan("laptop-display-off", &f).is_err());
        let mut f = f;
        f["outputs"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"DP-1","builtin":false,"active":true}));
        assert_eq!(
            plan("laptop-display-off", &f)?["steps"][0]["arguments"],
            json!(["niri", "msg", "output", "eDP-1", "off"])
        );
        assert!(plan("laptop-mirror", &f).is_err());
        assert!(plan("hibernate", &f).is_err());
        Ok(())
    }
}
