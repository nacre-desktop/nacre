//! Deadline and privacy controls use a private bus, never the host system bus.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Manager {
    stall: bool,
    oversized_interface: bool,
    healthy: bool,
    large_scalar: bool,
    large_devices: bool,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    async fn wireless_enabled(&self) -> zbus::zvariant::OwnedValue {
        if self.stall {
            std::future::pending::<()>().await;
        }
        if self.oversized_interface || self.healthy || self.large_devices {
            return zbus::zvariant::OwnedValue::from(true);
        }
        // A malformed oversized scalar must not leak into the error response.
        zbus::zvariant::OwnedValue::from(zbus::zvariant::Str::from(if self.large_scalar {
            "sensitive".repeat(240_000)
        } else {
            "sensitive".into()
        }))
    }
    fn get_devices(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
        if self.large_devices {
            return (0..14_000)
                .map(|index| {
                    zbus::zvariant::OwnedObjectPath::try_from(format!(
                        "/org/freedesktop/NetworkManager/Devices/{}_{index}",
                        "N".repeat(160)
                    ))
                    .unwrap()
                })
                .collect();
        }
        vec![
            zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Devices/1")
                .unwrap(),
        ]
    }
}
struct Device {
    oversized: bool,
}
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn interface(&self) -> String {
        if self.oversized {
            "sensitive".repeat(240_000)
        } else {
            "wlan0".into()
        }
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        30
    }
}
struct Wireless;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::zvariant::OwnedObjectPath {
        zbus::zvariant::OwnedObjectPath::try_from("/").unwrap()
    }
    #[zbus(property)]
    fn access_points(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
        vec![]
    }
}
fn probe(
    stall: bool,
    oversized_interface: bool,
    healthy: bool,
    large_scalar: bool,
    large_devices: bool,
) {
    let home = tempfile::tempdir().unwrap();
    let mut daemon = Daemon(
        Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address",
                &format!("--address=unix:path={}", home.path().join("bus").display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(daemon.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let _server = zbus::blocking::connection::Builder::address(address.trim())
        .unwrap()
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager",
            Manager {
                stall,
                oversized_interface,
                healthy,
                large_scalar,
                large_devices,
            },
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Devices/1",
            Device {
                oversized: oversized_interface,
            },
        )
        .unwrap()
        .serve_at("/org/freedesktop/NetworkManager/Devices/1", Wireless)
        .unwrap()
        .build()
        .unwrap();
    let began = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .arg("network-state")
        .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim())
        .env("HOME", home.path())
        .env("PATH", "")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if child
        .wait_timeout(Duration::from_secs(20))
        .unwrap()
        .is_none()
    {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("Network snapshot did not respect its method deadline");
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(if healthy { 0 } else { 1 }));
    let reply: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected = if oversized_interface || large_scalar || large_devices {
        "NetworkManager returned too much data."
    } else if stall {
        "NetworkManager is unavailable or did not respond."
    } else {
        "NetworkManager returned an unknown Wi-Fi radio state."
    };
    if healthy {
        assert_eq!(
            reply,
            serde_json::json!({"wifiEnabled":true,"wifiAvailable":true,"wifiInterface":"","networks":[]})
        );
    } else {
        assert_eq!(reply, serde_json::json!({"error":expected}));
    }
    assert!(!String::from_utf8_lossy(&output.stderr).contains("sensitive"));
    if stall {
        assert!(
            began.elapsed() >= Duration::from_secs(7),
            "The fixture must reach the real eight-second deadline"
        );
    }
    if !stall && !oversized_interface {
        use std::io::Write;
        let mut service = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .arg("serve")
            .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim())
            .env("HOME", home.path())
            .env("PATH", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(
            service.stdin.take().unwrap(),
            "{}",
            serde_json::json!({"id":1,"tool":"network-state","arguments":[],"input":null})
        )
        .unwrap();
        if service
            .wait_timeout(Duration::from_secs(20))
            .unwrap()
            .is_none()
        {
            service.kill().unwrap();
            service.wait().unwrap();
            panic!("Hosted network query did not finish");
        }
        let output = service.wait_with_output().unwrap();
        assert!(output.status.success());
        let packets: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0], serde_json::json!({"ready":true,"version":1}));
        assert_eq!(packets[1]["id"], 1);
        assert_eq!(packets[1]["code"], if healthy { 0 } else { 1 });
        assert_eq!(
            serde_json::from_str::<Value>(packets[1]["output"].as_str().unwrap()).unwrap(),
            reply
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("sensitive"));
    }
}
#[test]
fn stalled_private_networkmanager_respects_deadline() {
    probe(true, false, false, false, false);
}
#[test]
fn oversized_malformed_property_is_rejected_without_sensitive_output() {
    probe(false, false, false, true, false);
}
#[test]
fn oversized_well_typed_interface_is_rejected_without_sensitive_output() {
    probe(false, true, false, false, false);
}
#[test]
fn native_network_cli_and_hosted_dispatch_match() {
    probe(false, false, true, false, false);
}
#[test]
fn malformed_small_property_is_rejected_without_sensitive_output() {
    probe(false, false, false, false, false);
}
#[test]
fn oversized_well_typed_device_array_is_rejected_before_queries() {
    probe(false, false, false, false, true);
}
