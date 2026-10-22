//! Device controls; NetworkManager remains the credential agent.
use crate::{i18n::tr, Result};
use serde_json::{json, Value};
use std::{process::Command, time::Duration};

fn valid_interface(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .enumerate()
            .all(|(i, c)| c.is_ascii_alphanumeric() || (i > 0 && "_.-".contains(c)))
}
fn daemon_network(
    method: &str,
    select: impl FnOnce(&Value) -> Option<Value>,
) -> Result<Option<Value>> {
    use crate::daemon::client::{self, ConnectError};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut client = match client::connect().await {
            Ok(client) => client,
            Err(ConnectError::Absent) => return Ok(None),
            Err(ConnectError::Failed(error)) => return Err(error),
        };
        if !client.methods.contains_key(method) {
            return Ok(None);
        }
        use nacre_protocol::{framing, Message};
        framing::write(
            &mut client.writer,
            &Message::Sub {
                topics: vec!["network".into()],
            },
        )
        .await?;
        let data = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match client.reader.next().await? {
                    Some(Message::Snap { topic, data, .. }) if topic == "network" => {
                        break Ok(data)
                    }
                    Some(Message::Error { err }) => break Err(crate::Error::from(err)),
                    None => break Err(crate::Error::from("Daemon disconnected")),
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| "Timed out reading daemon network state")??;
        let Some(params) = select(&data) else {
            return Ok(None);
        };
        client.call(method, params, None).await?;
        Ok(Some(json!({"message":"Device updated"})))
    })
}

fn network_status() -> Result<Value> {
    let mut radio_command = Command::new("nmcli");
    radio_command.env("LC_ALL", "C").args(["radio", "wifi"]);
    let radio = crate::process::capture(radio_command, None, Duration::from_secs(5))?;
    if !radio.status.success() {
        return Err(String::from_utf8_lossy(&radio.stderr)
            .trim()
            .to_owned()
            .into());
    }
    let enabled = String::from_utf8_lossy(&radio.stdout).trim() == "enabled";
    let mut devices_command = Command::new("nmcli");
    devices_command
        .env("LC_ALL", "C")
        .args(["-t", "-f", "DEVICE,TYPE,STATE", "device"]);
    let output = crate::process::capture(devices_command, None, Duration::from_secs(5))?;
    if !output.status.success() {
        return Err("NetworkManager is unavailable".into());
    }
    let interface = String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            (fields.len() == 3
                && fields[1] == "wifi"
                && fields[2] == "connected"
                && valid_interface(fields[0]))
            .then(|| fields[0].to_owned())
        })
        .unwrap_or_default();
    Ok(json!({"enabled":enabled,"interface":interface}))
}
fn run_command(program: &str, args: &[&str], timeout: u64) -> Result<String> {
    let arguments = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let output = crate::process::run(program, &arguments, None, Duration::from_secs(timeout))?;
    checked_output(output)
}
fn checked_output(output: std::process::Output) -> Result<String> {
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_owned()
            .into());
    }
    let value = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    let lower = value.to_lowercase();
    if ["failed", "not available", "not ready"]
        .iter()
        .any(|word| lower.contains(word))
    {
        return Err(value.trim().chars().take(400).collect::<String>().into());
    }
    Ok(value)
}
fn valid_ssid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && !value.chars().any(char::is_control)
}
fn command(action: &str, value: &str) -> Result<Vec<String>> {
    let args = match action {
        "wifi-scan" => vec![
            "nmcli", "--wait", "15", "--colors", "no", "--fields", "IN-USE", "device", "wifi",
            "list", "--rescan", "yes",
        ],
        "wifi-radio" if matches!(value, "on" | "off") => vec!["nmcli", "radio", "wifi", value],
        "wifi-disconnect" if valid_interface(value) => vec!["nmcli", "device", "disconnect", value],
        "wifi-connect" if valid_ssid(value) => {
            vec!["nmcli", "--ask", "device", "wifi", "connect", value]
        }
        "bluetooth-power" if matches!(value, "on" | "off") => vec!["bluetoothctl", "power", value],
        "bluetooth-connect" | "bluetooth-disconnect" | "bluetooth-trust" | "bluetooth-untrust"
            if valid_mac(value) =>
        {
            vec![
                "bluetoothctl",
                match action {
                    "bluetooth-connect" => "connect",
                    "bluetooth-disconnect" => "disconnect",
                    "bluetooth-trust" => "trust",
                    _ => "untrust",
                },
                value,
            ]
        }
        _ => {
            return Err(tr(
                "helper.device-actions.unsupported-device-action",
                "Unsupported device action",
            )
            .into())
        }
    };
    Ok(args.into_iter().map(str::to_owned).collect())
}
pub fn execute(action: &str, value: &str) -> Result<Option<Value>> {
    let result = match action {
        "network-status" => return Ok(Some(network_status()?)),
        "wifi-scan" => {
            let args = command(action, value)?;
            run_command(
                &args[0],
                &args[1..].iter().map(String::as_str).collect::<Vec<_>>(),
                20,
            )
            .map(|_| wifi_scan_message().to_owned())
        }
        "wifi-radio" if matches!(value, "on" | "off") => {
            let enabled = value == "on";
            match daemon_network("network.set_wifi", |_| Some(json!({"enabled":enabled})))? {
                Some(result) => return Ok(Some(result)),
                None => {
                    let args = command(action, value)?;
                    run_command(
                        &args[0],
                        &args[1..].iter().map(String::as_str).collect::<Vec<_>>(),
                        20,
                    )
                }
            }
            .map(|text| text.trim().to_owned())
        }
        "wifi-disconnect" if valid_interface(value) => {
            match daemon_network("network.disconnect", |state| {
                let device = state["devices"]["items"]
                    .as_object()?
                    .iter()
                    .find(|(_, row)| row["name"] == value)?
                    .0
                    .clone();
                Some(json!({"device":device}))
            })? {
                Some(result) => return Ok(Some(result)),
                None => {
                    let args = command(action, value)?;
                    run_command(
                        &args[0],
                        &args[1..].iter().map(String::as_str).collect::<Vec<_>>(),
                        20,
                    )
                }
            }
            .map(|text| text.trim().to_owned())
        }
        "wifi-connect" if valid_ssid(value) => {
            match daemon_network("network.connect", |state| {
                let (id, point) = state["access_points"]["items"]
                    .as_object()?
                    .iter()
                    .find(|(_, row)| row["name"] == value)?;
                Some(json!({"device":point["device"],"target":{"kind":"access_point","id":id}}))
            })? {
                Some(_) => return Ok(None),
                None => {
                    // nmcli's agent prompts through --ask; the passphrase is never an argv value.
                    let args = command(action, value)?;
                    let status = Command::new(&args[0]).args(&args[1..]).status()?;
                    return wifi_connect_status(status.success());
                }
            }
        }
        "bluetooth-power"
        | "bluetooth-connect"
        | "bluetooth-disconnect"
        | "bluetooth-trust"
        | "bluetooth-untrust" => {
            let args = command(action, value)?;
            run_command(
                &args[0],
                &args[1..].iter().map(String::as_str).collect::<Vec<_>>(),
                20,
            )
            .map(|text| text.trim().to_owned())
        }
        _ => {
            return Err(tr(
                "helper.device-actions.unsupported-device-action",
                "Unsupported device action",
            )
            .into())
        }
    }?;
    let message = if result.is_empty() {
        "Device updated".to_owned()
    } else {
        result.chars().take(400).collect::<String>()
    };
    Ok(Some(json!({"message":message})))
}
fn valid_mac(value: &str) -> bool {
    value.len() == 17
        && value.split(':').count() == 6
        && value
            .split(':')
            .all(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_hexdigit()))
}
fn failure_json(error: &str) -> Value {
    json!({"error":error})
}
fn wifi_connect_status(success: bool) -> Result<Option<Value>> {
    if success {
        Ok(None)
    } else {
        Err("Wi-Fi connection failed".into())
    }
}
fn failure_exit(action: &str) -> i32 {
    if action == "wifi-connect" {
        1
    } else {
        0
    }
}
fn wifi_scan_message() -> &'static str {
    "Wi-Fi scan complete."
}
fn wifi_connect_prompt() -> &'static str {
    "\nCould not connect to Wi-Fi. Check the network and credentials.\n\nPress Enter to close."
}
pub fn run(args: &[String]) -> Result<i32> {
    let action = args.first().map(String::as_str).unwrap_or("");
    let value = args.get(1).map(String::as_str).unwrap_or("");
    match execute(action, value) {
        Ok(Some(value)) => {
            println!("{}", crate::json::encode(&value)?);
            Ok(0)
        }
        Ok(None) => Ok(0),
        Err(_error) if failure_exit(action) == 1 => {
            println!("{}", wifi_connect_prompt());
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            Ok(1)
        }
        Err(error) => {
            println!(
                "{}",
                crate::json::encode(&failure_json(&error.to_string()))?
            );
            Ok(failure_exit(action))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    #[test]
    fn invalid_actions_are_rejected() {
        assert!(command("unknown", "").is_err());
    }
    #[test]
    fn wifi_scan_waits_for_fresh_results_without_connecting() {
        assert_eq!(
            command("wifi-scan", "").unwrap(),
            vec![
                "nmcli", "--wait", "15", "--colors", "no", "--fields", "IN-USE", "device", "wifi",
                "list", "--rescan", "yes"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
        );
    }
    #[test]
    fn wifi_ssids_use_utf8_byte_limit_and_keep_secrets_with_nm() {
        for ssid in ["x".repeat(32), "é".repeat(16), "😀".repeat(8)] {
            assert_eq!(
                command("wifi-connect", &ssid).unwrap(),
                vec!["nmcli", "--ask", "device", "wifi", "connect", ssid.as_str()]
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            );
        }
        for ssid in [
            "x".repeat(33),
            "é".repeat(17),
            "😀".repeat(9),
            format!("{}x", "é".repeat(16)),
            "fixture\nnetwork".to_owned(),
            String::new(),
        ] {
            assert!(!valid_ssid(&ssid));
            assert!(command("wifi-connect", &ssid).is_err());
        }
    }
    #[test]
    fn empty_and_control_character_ssids_are_rejected() {
        for ssid in ["", "fixture\nnetwork", "fixture\0network"] {
            assert!(command("wifi-connect", ssid).is_err());
        }
    }
    #[test]
    fn bluetooth_failures_remain_visible_in_the_json_error_path() {
        assert_eq!(
            command("bluetooth-connect", "AA:BB:CC:DD:EE:FF").unwrap(),
            vec!["bluetoothctl", "connect", "AA:BB:CC:DD:EE:FF"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(valid_mac("AA:BB:CC:DD:EE:FF"), true);
        assert!(!valid_interface("bad/name"));
        assert_eq!(
            failure_json("Failed to connect"),
            json!({"error":"Failed to connect"})
        );
        assert_eq!(failure_exit("bluetooth-connect"), 0);
    }
    #[test]
    fn wifi_connect_failure_stays_interactive_and_exits_nonzero() {
        assert_eq!(failure_exit("wifi-connect"), 1);
        assert_eq!(
            command("wifi-connect", "Guest").unwrap(),
            vec!["nmcli", "--ask", "device", "wifi", "connect", "Guest"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert!(wifi_connect_status(true).unwrap().is_none());
        assert!(wifi_connect_status(false).is_err());
    }
    #[test]
    fn wifi_scan_reports_only_completion_and_keeps_failures_visible() {
        let ok = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"IN-USE\n*\n \n".to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(checked_output(ok).unwrap(), "IN-USE\n*\n \n");
        assert_eq!(wifi_scan_message(), "Wi-Fi scan complete.");
        let failed = std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: b"Scan failed".to_vec(),
        };
        assert_eq!(
            checked_output(failed).unwrap_err().to_string(),
            "Scan failed"
        );
        assert!(wifi_connect_prompt().contains("Could not connect"));
    }
}
