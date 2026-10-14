//! Setup workflows: real device discovery, typed arguments and owned settings APIs.
use crate::{
    i18n::tr,
    paths::Paths,
    software_actions::{read, tool},
    Result,
};
use serde_json::{json, Value};
use std::{
    io::{self, Write},
    net::IpAddr,
};

pub fn facts(paths: &Paths) -> Value {
    let fingerprint = read(
        "busctl",
        &[
            "--system",
            "--json=short",
            "call",
            "net.reactivated.Fprint",
            "/net/reactivated/Fprint/Manager",
            "net.reactivated.Fprint.Manager",
            "GetDevices",
        ],
    )
    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    .is_some_and(|v| {
        v["data"].as_array().is_some_and(|a| {
            a.iter()
                .any(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        })
    });
    let keys = read("fido2-token", &["-L"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let name = line.split(':').next()?;
            name.strip_prefix("/dev/hidraw")
                .filter(|n| !n.is_empty() && n.len() < 5 && n.chars().all(|c| c.is_ascii_digit()))
                .map(|_| name.to_string())
        })
        .collect::<Vec<_>>();
    let network = read(
        "nmcli",
        &[
            "--terse",
            "--fields",
            "UUID,TYPE,DEVICE",
            "connection",
            "show",
            "--active",
        ],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|line| {
        let row = line.split(':').collect::<Vec<_>>();
        if row.len() != 3 {
            return None;
        }
        let id = row[0];
        if id.len() != 36 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return None;
        }
        let wifi = matches!(row[1], "802-11-wireless" | "wifi");
        (wifi || matches!(row[1], "802-3-ethernet" | "ethernet"))
            .then(|| json!({"id":id,"wifi":wifi,"device":row[2]}))
    })
    .collect::<Vec<_>>();
    let compositor = std::env::var("NACRE_COMPOSITOR").unwrap_or_else(|_| {
        if std::env::var_os("NIRI_SOCKET").is_some() {
            "niri".into()
        } else if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            "hyprland".into()
        } else {
            String::new()
        }
    });
    let keyboard = matches!(compositor.as_str(), "niri" | "hyprland")
        && tool(if compositor == "niri" {
            "niri"
        } else {
            "hyprctl"
        })
        && tool("localectl")
        && paths
            .config
            .join(if compositor == "niri" {
                "niri/desktop.kdl"
            } else {
                "desktop.lua"
            })
            .is_file();
    let ssh = read(
        "systemctl",
        &["show", "sshd.service", "-p", "LoadState", "--value"],
    )
    .is_some_and(|s| s.trim() == "loaded");
    json!({"fingerprint":fingerprint&&tool("fprintd-enroll"),"keys":keys,"network":network,"ssh":ssh,"keyboard":keyboard,"fcitx":tool("fcitx5-configtool"),"ibus":tool("ibus-setup"),"qr":tool("qrencode"),"terminal":tool("kitty"),"admin":tool("pkexec")||tool("sudo")})
}
pub fn rows(facts: &Value) -> Vec<Value> {
    let mut rows = vec![];
    if facts["terminal"] != true {
        return rows;
    }
    let network = facts["network"].as_array().is_some_and(|a| !a.is_empty());
    for (id, name, description, available, root) in [
        (
            "fingerprint-enroll",
            tr("software.setup.fingerprint-enroll", "Enroll a fingerprint"),
            tr(
                "software.setup.fingerprint-enroll.detail",
                "Use fprintd's enrollment workflow",
            ),
            facts["fingerprint"] == true,
            false,
        ),
        (
            "fido-info",
            tr("software.setup.fido-info", "FIDO2 keys"),
            tr(
                "software.setup.fido-info.detail",
                "Inspect a connected hardware key",
            ),
            facts["keys"].as_array().is_some_and(|a| !a.is_empty()),
            false,
        ),
        (
            "fido-pin",
            tr("software.setup.fido-pin", "Set FIDO2 key PIN"),
            tr(
                "software.setup.fido-pin.detail",
                "PIN entry stays in the hardware-key tool",
            ),
            facts["keys"].as_array().is_some_and(|a| !a.is_empty()),
            false,
        ),
        (
            "dns-dhcp",
            tr("software.setup.dns-dhcp", "Automatic DNS (DHCP)"),
            tr(
                "software.setup.dns-dhcp.detail",
                "Restore automatic DNS on a selected active connection",
            ),
            network,
            true,
        ),
        (
            "dns-cloudflare",
            tr("software.setup.dns-cloudflare", "Cloudflare DNS"),
            tr(
                "software.setup.dns-cloudflare.detail",
                "Use Cloudflare resolver addresses",
            ),
            network,
            true,
        ),
        (
            "dns-google",
            tr("software.setup.dns-google", "Google DNS"),
            tr(
                "software.setup.dns-google.detail",
                "Use Google resolver addresses",
            ),
            network,
            true,
        ),
        (
            "dns-custom",
            tr("software.setup.dns-custom", "Custom DNS"),
            tr(
                "software.setup.dns-custom.detail",
                "Enter and validate resolver IP addresses",
            ),
            network,
            true,
        ),
        (
            "wifi-qr",
            tr("software.setup.wifi-qr", "Share Wi-Fi as a QR code"),
            tr(
                "software.setup.wifi-qr.detail",
                "Display a QR without printing the password",
            ),
            facts["network"]
                .as_array()
                .is_some_and(|a| a.iter().any(|r| r["wifi"] == true))
                && facts["qr"] == true,
            true,
        ),
        (
            "ssh-enable",
            tr("software.setup.ssh-enable", "Enable SSH server"),
            tr(
                "software.setup.ssh-enable.detail",
                "Enable and start OpenSSH's server",
            ),
            facts["ssh"] == true,
            true,
        ),
        (
            "ssh-disable",
            tr("software.setup.ssh-disable", "Disable SSH server"),
            tr(
                "software.setup.ssh-disable.detail",
                "Stop and disable OpenSSH's server",
            ),
            facts["ssh"] == true,
            true,
        ),
        (
            "keyboard-layouts",
            tr("software.setup.keyboard-layouts", "Keyboard layouts"),
            tr(
                "software.setup.keyboard-layouts.detail",
                "Choose comma-separated XKB layouts",
            ),
            facts["keyboard"] == true,
            false,
        ),
        (
            "input-methods",
            tr("software.setup.input-methods", "Input methods"),
            tr(
                "software.setup.input-methods.detail",
                "Open the installed input-method configuration tool",
            ),
            facts["fcitx"] == true || facts["ibus"] == true,
            false,
        ),
    ] {
        if available && (!root || facts["admin"] == true) {
            rows.push(json!({"id":id,"name":name,"description":description,"section":"setup.system","root":root}))
        }
    }
    rows
}
fn step(args: &[&str], root: bool) -> Value {
    json!({"arguments":args,"root":root})
}
pub fn dns_addresses(text: &str) -> Result<(String, String)> {
    let mut v4 = vec![];
    let mut v6 = vec![];
    for word in text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
    {
        match word
            .parse::<IpAddr>()
            .map_err(|_| "Use IP addresses for custom DNS")?
        {
            IpAddr::V4(ip) => v4.push(ip.to_string()),
            IpAddr::V6(ip) => v6.push(ip.to_string()),
        }
    }
    if v4.len() + v6.len() == 0 || v4.len() + v6.len() > 8 {
        return Err("Choose between one and eight DNS addresses".into());
    }
    Ok((v4.join(","), v6.join(",")))
}
fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}
fn select(rows: &[Value]) -> Result<Value> {
    for (index, row) in rows.iter().enumerate() {
        println!(
            "{}: {}",
            index + 1,
            row["device"]
                .as_str()
                .or_else(|| row["id"].as_str())
                .unwrap_or("")
        );
    }
    let index = if rows.len() == 1 {
        0
    } else {
        prompt("Connection number")?
            .parse::<usize>()
            .map_err(|_| "Choose a connection number")?
            .checked_sub(1)
            .ok_or("Choose a connection number")?
    };
    rows.get(index)
        .cloned()
        .ok_or_else(|| "Choose an available connection".into())
}
pub fn plan(id: &str, facts: &Value, request: &Value) -> Result<Value> {
    if !rows(facts).iter().any(|r| r["id"] == id) {
        return Err("Setup action is not available on this host".into());
    }
    let mut steps = vec![];
    match id {
        "fingerprint-enroll" => steps.push(step(&["fprintd-enroll"], false)),
        "fido-info" | "fido-pin" => {
            let key = request["key"]
                .as_str()
                .ok_or("Select a connected FIDO2 key")?;
            if !facts["keys"].as_array().unwrap().iter().any(|k| k == key) {
                return Err("Hardware key is no longer available".into());
            }
            steps.push(step(
                &[
                    "fido2-token",
                    if id == "fido-info" { "-I" } else { "-S" },
                    key,
                ],
                false,
            ));
        }
        "ssh-enable" => steps.push(step(
            &["systemctl", "enable", "--now", "sshd.service"],
            true,
        )),
        "ssh-disable" => steps.push(step(
            &["systemctl", "disable", "--now", "sshd.service"],
            true,
        )),
        "input-methods" => steps.push(step(
            &[if facts["fcitx"] == true {
                "fcitx5-configtool"
            } else {
                "ibus-setup"
            }],
            false,
        )),
        "keyboard-layouts" => {
            let layouts = request["layouts"]
                .as_str()
                .ok_or("Choose keyboard layouts")?;
            if layouts.len() > 160
                || layouts.split(',').any(|s| {
                    s.is_empty()
                        || !s
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
                })
            {
                return Err("Invalid XKB layouts".into());
            }
            steps.push(json!({"arguments":["Nacre input preferences",layouts],"root":false,"input":layouts}));
        }
        "wifi-qr" => {
            let id = request["connection"]
                .as_str()
                .ok_or("Select an active Wi-Fi connection")?;
            if !facts["network"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == id && r["wifi"] == true)
            {
                return Err("Wi-Fi connection is no longer active".into());
            }
            steps.push(json!({"arguments":["Read the selected Wi-Fi connection and display a QR",id],"root":true,"wifiQr":id}));
        }
        _ if id.starts_with("dns-") => {
            let connection = request["connection"]
                .as_str()
                .ok_or("Select an active connection")?;
            if !facts["network"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == connection)
            {
                return Err("Connection is no longer active".into());
            }
            let automatic = id == "dns-dhcp";
            let (v4, v6) = if automatic {
                (String::new(), String::new())
            } else {
                dns_addresses(match id {
                    "dns-cloudflare" => "1.1.1.1,1.0.0.1,2606:4700:4700::1111,2606:4700:4700::1001",
                    "dns-google" => "8.8.8.8,8.8.4.4,2001:4860:4860::8888,2001:4860:4860::8844",
                    _ => request["servers"].as_str().ok_or("Choose DNS servers")?,
                })?
            };
            let ignore = if automatic { "no" } else { "yes" };
            steps.push(step(
                &[
                    "nmcli",
                    "connection",
                    "modify",
                    connection,
                    "ipv4.ignore-auto-dns",
                    ignore,
                    "ipv4.dns",
                    &v4,
                    "ipv6.ignore-auto-dns",
                    ignore,
                    "ipv6.dns",
                    &v6,
                ],
                true,
            ));
            steps.push(step(&["nmcli", "connection", "up", connection], true));
        }
        _ => return Err("Unknown Setup action".into()),
    }
    Ok(json!({"id":id,"steps":steps}))
}
pub fn request(id: &str, facts: &Value) -> Result<Value> {
    let mut value = json!({});
    if id.starts_with("dns-") || id == "wifi-qr" {
        let rows = facts["network"]
            .as_array()
            .ok_or("Network connections are unavailable")?
            .iter()
            .filter(|r| id != "wifi-qr" || r["wifi"] == true)
            .cloned()
            .collect::<Vec<_>>();
        value["connection"] = select(&rows)?["id"].clone();
        if id == "dns-custom" {
            value["servers"] = json!(prompt("DNS IP addresses")?);
        }
    }
    if id.starts_with("fido-") {
        let keys = facts["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| json!({"id":k}))
            .collect::<Vec<_>>();
        value["key"] = select(&keys)?["id"].clone();
    }
    if id == "keyboard-layouts" {
        let available = read("localectl", &["list-x11-keymap-layouts"])
            .ok_or("XKB layout list is unavailable")?;
        println!("{}", available);
        let layouts = prompt("Layouts (for example us,no)")?;
        if layouts
            .split(',')
            .any(|part| !available.lines().any(|line| line == part))
        {
            return Err("Choose layouts from the displayed list".into());
        }
        value["layouts"] = json!(layouts);
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn root_dns_is_typed_and_hardware_enrollment_is_gated() -> Result<()> {
        let empty = json!({"terminal":true,"admin":true});
        assert!(rows(&empty).is_empty());
        let f = json!({"terminal":true,"admin":true,"network":[{"id":"fixture-connection","wifi":true}],"keys":[],"fingerprint":false});
        let p = plan("dns-dhcp", &f, &json!({"connection":"fixture-connection"}))?;
        assert_eq!(p["steps"][0]["root"], true);
        assert_eq!(p["steps"][0]["arguments"][5], "no");
        assert!(plan("fingerprint-enroll", &f, &json!({})).is_err());
        assert!(dns_addresses("$(bad)").is_err());
        assert!(dns_addresses("example.org").is_err());
        assert_eq!(
            dns_addresses("1.1.1.1,2606:4700:4700::1111")?,
            ("1.1.1.1".into(), "2606:4700:4700::1111".into())
        );
        Ok(())
    }
}
