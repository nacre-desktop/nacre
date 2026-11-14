// SPDX-License-Identifier: GPL-3.0-only
//! Reconnecting daemon projections for the always-visible bar status icons.
use bead::core::{
    Value,
    source::{self, Snapshot, Source, Subscription},
};
use nacre_protocol::core_services::{
    AudioKind, AudioState, BatteryState, BluetoothState, CoreSnapshot, NetworkState,
};
use std::{collections::BTreeMap, sync::Arc};

const KEYS: [&str; 12] = [
    "bead.desktop.network",
    "bead.desktop.bluetooth",
    "bead.desktop.battery",
    "bead.desktop.battery.value",
    "bead.desktop.battery.value.width",
    "bead.desktop.audio",
    "bead.desktop.audio.percent",
    "bead.desktop.audio.device",
    "bead.desktop.audio.label",
    "bead.desktop.audio.available",
    "bead.desktop.network.vpn",
    "bead.desktop.bluetooth.value",
];

fn unavailable() -> BTreeMap<String, Value> {
    BTreeMap::from([
        (KEYS[0].into(), Value::Text("wifi_off".into())),
        (KEYS[1].into(), Value::Text("bluetooth_disabled".into())),
        (KEYS[2].into(), Value::Text("battery_unknown".into())),
        (KEYS[3].into(), Value::Text(String::new())),
        (KEYS[4].into(), Value::Number(3.)),
        (KEYS[5].into(), Value::Text("volume_mute".into())),
        (KEYS[6].into(), Value::Number(0.)),
        (KEYS[7].into(), Value::Text("Output unavailable".into())),
        (KEYS[8].into(), Value::Text("Unavailable".into())),
        (KEYS[9].into(), Value::Bool(false)),
        (KEYS[10].into(), Value::Bool(false)),
        (KEYS[11].into(), Value::Text(String::new())),
    ])
}

fn network(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let state = serde_json::from_value::<NetworkState>((*snapshot.data).clone())
        .ok()
        .filter(|state| {
            snapshot.connected
                && state.health.available
                && !state.health.stale
                && CoreSnapshot::Network(state.clone()).valid()
        });
    let connected = |kinds: &[&str]| {
        state.as_ref().is_some_and(|state| {
            state
                .devices
                .items
                .values()
                .any(|device| device.connected && kinds.contains(&device.kind.as_str()))
        })
    };
    let wifi = state.as_ref().is_some_and(|state| state.wireless_enabled)
        && connected(&["wifi", "wireless"]);
    // D1 (spec 0237): a cable shows `lan` even with Wi-Fi off; an active VPN is a lock on the glyph.
    let glyph = if wifi {
        "wifi"
    } else if connected(&["ethernet", "wired"]) {
        "lan"
    } else {
        "wifi_off"
    };
    let vpn = state.as_ref().is_some_and(|state| {
        state.connections.items.values().any(|connection| {
            connection.active && matches!(connection.kind.as_str(), "vpn" | "wireguard")
        })
    });
    BTreeMap::from([
        (KEYS[0].into(), Value::Text(glyph.into())),
        (KEYS[10].into(), Value::Bool(vpn)),
    ])
}

fn bluetooth(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let state = serde_json::from_value::<BluetoothState>((*snapshot.data).clone())
        .ok()
        .filter(|state| {
            snapshot.connected
                && CoreSnapshot::Bluetooth(state.clone()).valid()
                && state.health.available
                && !state.health.stale
        });
    let powered = state
        .as_ref()
        .is_some_and(|state| state.adapters.items.values().any(|adapter| adapter.powered));
    // The connected device's battery, when one reports it (D1 "bluetooth_connected 72 %").
    let device = state.as_ref().and_then(|state| {
        state
            .devices
            .items
            .values()
            .filter(|device| device.connected)
            .max_by_key(|device| device.battery.is_some())
    });
    let glyph = if device.is_some() {
        "bluetooth_connected"
    } else if powered {
        "bluetooth"
    } else {
        "bluetooth_disabled"
    };
    BTreeMap::from([
        (KEYS[1].into(), Value::Text(glyph.into())),
        (
            KEYS[11].into(),
            Value::Text(
                device
                    .and_then(|device| device.battery)
                    .map_or(String::new(), |battery| battery.to_string()),
            ),
        ),
    ])
}

fn battery(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let state = serde_json::from_value::<BatteryState>((*snapshot.data).clone())
        .ok()
        .filter(|state| {
            snapshot.connected
                && CoreSnapshot::Battery(state.clone()).valid()
                && state.health.available
                && !state.health.stale
        });
    let Some(state) = state else {
        return unavailable()
            .into_iter()
            .filter(|(key, _)| KEYS[2..5].contains(&key.as_str()))
            .collect();
    };
    let value = state
        .percentage
        .map_or(String::new(), |percentage| format!("{percentage:.0}"));
    BTreeMap::from([
        (
            KEYS[2].into(),
            Value::Text(
                battery_glyph(
                    state.charging == Some(true) || state.on_battery == Some(false),
                    state.percentage,
                )
                .into(),
            ),
        ),
        (KEYS[3].into(), Value::Text(value.clone())),
        (
            KEYS[4].into(),
            Value::Number(if value.is_empty() {
                3.
            } else {
                7. + 7.5 * value.len() as f64
            }),
        ),
    ])
}

/// D1 icon family: bolt while plugged in (charging, or on AC at full / a charge threshold, which UPower
/// reports as not charging), alert at <= 20 %, empty at <= 8 %, full otherwise.
pub(crate) fn battery_glyph(plugged: bool, percentage: Option<f64>) -> &'static str {
    if plugged {
        "battery_android_bolt"
    } else if percentage.is_some_and(|value| value <= 8.) {
        "battery_android_0"
    } else if percentage.is_some_and(|value| value <= 20.) {
        "battery_android_alert"
    } else {
        "battery_android_full"
    }
}

fn audio(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let state = serde_json::from_value::<AudioState>((*snapshot.data).clone())
        .ok()
        .filter(|state| {
            snapshot.connected
                && CoreSnapshot::Audio(state.clone()).valid()
                && state.health.available
                && !state.health.stale
        });
    let node = state.as_ref().and_then(|state| {
        state
            .default_output
            .as_ref()
            .and_then(|id| state.nodes.items.get(id))
            .filter(|node| node.kind == AudioKind::Output)
    });
    let level = node.and_then(|node| {
        if let Some(readback) = &node.readback {
            readback
                .perceptual_volume
                .filter(|value| value.is_finite() && *value >= 0.)
        } else if node.ready {
            Some(node.volume.max(0.).cbrt())
        } else {
            None
        }
    });
    let muted = node.and_then(|node| {
        if let Some(readback) = &node.readback {
            readback.muted
        } else if node.ready {
            Some(node.muted)
        } else {
            None
        }
    });
    let percent = level.map_or(0., |value| value * 100.);
    let label = level.map_or_else(
        || "Unavailable".into(),
        |value| format!("{:.0}%", value * 100.),
    );
    // D1: a headset sink shows the headphones glyph.
    let headset = node.is_some_and(|node| {
        [node.icon.as_deref().unwrap_or_default(), node.name.as_str()]
            .iter()
            .any(|text| {
                let text = text.to_ascii_lowercase();
                text.contains("headphone") || text.contains("headset")
            })
    });
    let icon = if muted == Some(true) {
        "volume_off"
    } else if level.is_none() {
        "volume_mute"
    } else if headset {
        "headphones"
    } else {
        "volume_up"
    };
    BTreeMap::from([
        (KEYS[5].into(), Value::Text(icon.into())),
        (KEYS[6].into(), Value::Number(percent)),
        (
            KEYS[7].into(),
            Value::Text(node.map_or_else(
                || "Output unavailable".into(),
                |node| {
                    if node.name.trim().is_empty() {
                        "Default output".into()
                    } else {
                        node.name.clone()
                    }
                },
            )),
        ),
        (KEYS[8].into(), Value::Text(label)),
        (KEYS[9].into(), Value::Bool(level.is_some())),
    ])
}

fn project(name: &str, snapshot: &Snapshot) -> BTreeMap<String, Value> {
    match name {
        "network" => network(snapshot),
        "bluetooth" => bluetooth(snapshot),
        "battery" => battery(snapshot),
        "audio" => audio(snapshot),
        _ => BTreeMap::new(),
    }
}

pub struct BarDaemon {
    topics: Vec<(&'static str, Arc<dyn Source>)>,
}
impl BarDaemon {
    pub fn new(client: &crate::nacre_source::Client) -> source::Result<Self> {
        let topics = ["network", "bluetooth", "battery", "audio"]
            .into_iter()
            .map(|name| client.view_topic(name).map(|source| (name, source)))
            .collect::<source::Result<Vec<_>>>()?;
        Ok(Self { topics })
    }
    pub fn values(&self) -> BTreeMap<String, Value> {
        let mut values = unavailable();
        for (name, source) in &self.topics {
            values.extend(project(name, &source.snapshot()));
        }
        values
    }
    pub fn subscribe(
        &self,
        wake: Arc<dyn Fn(BTreeMap<String, Value>) + Send + Sync>,
    ) -> source::Result<Vec<Subscription>> {
        let mut subscriptions = Vec::new();
        for (name, source) in &self.topics {
            let name = *name;
            let wake = wake.clone();
            subscriptions.push(source.subscribe(Arc::new(move |snapshot| {
                wake(project(name, &snapshot));
            }))?);
        }
        Ok(subscriptions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(data: serde_json::Value, connected: bool) -> Snapshot {
        Snapshot {
            epoch: "fixture-boot".into(),
            revision: 0,
            data: Arc::new(data),
            connected,
        }
    }

    #[test]
    fn network_and_battery_projections_recover_after_disconnection() {
        let network = serde_json::json!({
            "health":{"available":true,"stale":false}, "wireless_enabled":true,
            "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
            "access_points":{"items":{"ap0":{"device":"wifi0","name":"Home Wi-Fi","strength":86,"secured":true,"active":true,"connection":"wifi-connection"}},"order":["ap0"]},
            "connections":{"items":{"wifi-connection":{"name":"Wi-Fi profile","kind":"wireless","active":true}},"order":["wifi-connection"]},
            "primary_connection":"wifi-connection"
        });
        assert_eq!(
            super::network(&snapshot(network.clone(), false))[KEYS[0]],
            Value::Text("wifi_off".into())
        );
        let live = super::network(&snapshot(network.clone(), true));
        assert_eq!(live[KEYS[0]], Value::Text("wifi".into()));
        assert_eq!(live[KEYS[10]], Value::Bool(false));
        let mut vpn = network;
        vpn["connections"]["items"]["tunnel"] =
            serde_json::json!({"name":"Office","kind":"wireguard","active":true});
        vpn["connections"]["order"] = serde_json::json!(["wifi-connection", "tunnel"]);
        assert_eq!(
            super::network(&snapshot(vpn, true))[KEYS[10]],
            Value::Bool(true)
        );

        let battery_data = serde_json::json!({
            "health":{"available":true,"stale":false}, "percentage":83.0, "charging":true,
            "state":1, "time_to_full_seconds":5400, "on_battery":false
        });
        let charging = battery(&snapshot(battery_data.clone(), true));
        assert_eq!(charging[KEYS[3]], Value::Text("83".into()));
        assert_eq!(
            charging[KEYS[2]],
            Value::Text("battery_android_bolt".into())
        );
        // Full or threshold-held on AC: UPower says "not charging", the bar still shows the bolt.
        let mut held = battery_data.clone();
        held["charging"] = false.into();
        held["state"] = 4.into();
        assert_eq!(
            battery(&snapshot(held, true))[KEYS[2]],
            Value::Text("battery_android_bolt".into())
        );
        assert_eq!(
            battery(&snapshot(battery_data, false))[KEYS[2]],
            Value::Text("battery_unknown".into())
        );
        // D1 icon family: alert at <= 20 %, empty at <= 8 %, full otherwise.
        for (percentage, charging, glyph) in [
            (56., false, "battery_android_full"),
            (20., false, "battery_android_alert"),
            (8., false, "battery_android_0"),
            (8., true, "battery_android_bolt"),
        ] {
            assert_eq!(
                super::battery_glyph(charging, Some(percentage)),
                glyph,
                "{percentage} % charging={charging}"
            );
        }
    }

    #[test]
    fn connected_wifi_device_does_not_require_a_profile_or_access_point() {
        // Real daemon shape: device kind is "wifi"; profile/AP discovery may be empty.
        let data = serde_json::json!({
            "health":{"available":true,"stale":false}, "wireless_enabled":true,
            "devices":{"items":{"wlan0":{"name":"wlan0","kind":"wifi","connected":true}},"order":["wlan0"]},
            "access_points":{"items":{},"order":[]},
            "connections":{"items":{},"order":[]}, "primary_connection":null
        });
        assert_eq!(
            super::network(&snapshot(data.clone(), true))[KEYS[0]],
            Value::Text("wifi".into())
        );
        for (field, value, glyph) in [
            ("wireless_enabled", serde_json::json!(false), "wifi_off"),
            (
                "health",
                serde_json::json!({"available":true,"stale":true}),
                "wifi_off",
            ),
            // D1: a cable shows `lan` even with Wi-Fi off.
            (
                "devices",
                serde_json::json!({"items":{"eth0":{"name":"Wired","kind":"ethernet","connected":true}},"order":["eth0"]}),
                "lan",
            ),
        ] {
            let mut off = data.clone();
            off[field] = value;
            assert_eq!(
                super::network(&snapshot(off, true))[KEYS[0]],
                Value::Text(glyph.into())
            );
        }
        let mut disconnected = data;
        disconnected["devices"]["items"]["wlan0"]["connected"] = serde_json::json!(false);
        assert_eq!(
            super::network(&snapshot(disconnected, true))[KEYS[0]],
            Value::Text("wifi_off".into())
        );
    }

    #[test]
    fn absent_or_stale_topics_project_unavailable_icons() {
        let unavailable = snapshot(serde_json::json!({}), false);
        assert_eq!(
            super::network(&unavailable)[KEYS[0]],
            Value::Text("wifi_off".into())
        );
        assert_eq!(
            bluetooth(&unavailable)[KEYS[1]],
            Value::Text("bluetooth_disabled".into())
        );
        assert_eq!(
            battery(&unavailable)[KEYS[2]],
            Value::Text("battery_unknown".into())
        );
    }

    #[test]
    fn bluetooth_and_audio_project_fresh_values_and_recover_after_disconnect() {
        let bluetooth_data = serde_json::json!({
            "health":{"available":true,"stale":false},
            "adapters":{"items":{"adapter":{"name":"Adapter","powered":true,"discovering":false}},"order":["adapter"]},
            "devices":{"items":{},"order":[]}
        });
        assert_eq!(
            bluetooth(&snapshot(bluetooth_data.clone(), true))[KEYS[1]],
            Value::Text("bluetooth".into())
        );
        assert_eq!(
            bluetooth(&snapshot(bluetooth_data.clone(), false))[KEYS[1]],
            Value::Text("bluetooth_disabled".into())
        );
        // D1: a connected device shows bluetooth_connected and its battery when it reports one.
        let mut with_device = bluetooth_data;
        with_device["devices"] = serde_json::json!({"items":{"buds":{"adapter":"adapter","name":"Buds","connected":true,"paired":true,"trusted":true,"pairing":false,"battery":72}},"order":["buds"]});
        let connected = bluetooth(&snapshot(with_device, true));
        assert_eq!(
            connected[KEYS[1]],
            Value::Text("bluetooth_connected".into())
        );
        assert_eq!(connected[KEYS[11]], Value::Text("72".into()));

        let audio_data = serde_json::json!({
            "health":{"available":true,"stale":false}, "default_output":"out",
            "default_input":null,
            "nodes":{"items":{"out":{"name":"Studio speakers","kind":"output","volume":0.125,"muted":false,"ready":true,"readback":{"linear_volume":0.125,"perceptual_volume":0.5,"muted":false}}},"order":["out"]}
        });
        let fresh = audio(&snapshot(audio_data.clone(), true));
        assert_eq!(fresh[KEYS[5]], Value::Text("volume_up".into()));
        assert_eq!(fresh[KEYS[6]], Value::Number(50.));
        assert_eq!(fresh[KEYS[7]], Value::Text("Studio speakers".into()));
        assert_eq!(fresh[KEYS[9]], Value::Bool(true));
        let mut headset = audio_data.clone();
        headset["nodes"]["items"]["out"]["name"] = serde_json::json!("WH-1000 headphones");
        assert_eq!(
            audio(&snapshot(headset, true))[KEYS[5]],
            Value::Text("headphones".into())
        );
        let down = audio(&snapshot(audio_data, false));
        assert_eq!(down[KEYS[5]], Value::Text("volume_mute".into()));
        assert_eq!(down[KEYS[8]], Value::Text("Unavailable".into()));
        assert_eq!(down[KEYS[9]], Value::Bool(false));
    }
}
