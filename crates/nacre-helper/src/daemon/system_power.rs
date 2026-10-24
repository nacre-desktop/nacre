//! UPower and power-profiles-daemon readback topics.
use super::{
    behaviours::{proxy, signals, PROFILE_SERVICES},
    hub::Shared,
};
use crate::Result;
use nacre_protocol::core_services::{
    BatteryState, CoreSnapshot, Entities, EntityId, Health, PowerProfile, PowerProfilesState,
    ProfileHold,
};
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};
use zbus::zvariant::OwnedValue;

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const BATTERY: &str = "battery";
const PROFILES: &str = "power.profiles";

fn unavailable_health() -> Health {
    Health {
        available: false,
        stale: false,
        error: None,
    }
}
fn empty_battery() -> BatteryState {
    BatteryState {
        health: unavailable_health(),
        percentage: None,
        charging: None,
        state: None,
        time_to_empty_seconds: None,
        time_to_full_seconds: None,
        on_battery: None,
    }
}
fn empty_profiles() -> PowerProfilesState {
    PowerProfilesState {
        health: unavailable_health(),
        active: None,
        profiles: Entities {
            items: BTreeMap::new(),
            order: vec![],
        },
        degraded: None,
        holds: vec![],
    }
}

pub fn initial(hub: &Shared) -> Result<()> {
    hub.borrow_mut()
        .publish(BATTERY, serde_json::to_value(empty_battery())?)?;
    hub.borrow_mut()
        .publish(PROFILES, serde_json::to_value(empty_profiles())?)?;
    Ok(())
}

async fn timed<T>(future: impl std::future::Future<Output = zbus::Result<T>>) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(2), future)
        .await
        .ok()?
        .ok()
}

async fn battery(system: &zbus::Connection) -> BatteryState {
    let (Some(device), Some(root)) = (
        timed(proxy(
            system,
            UPOWER,
            DISPLAY,
            "org.freedesktop.UPower.Device",
        ))
        .await,
        timed(proxy(system, UPOWER, UPOWER_PATH, UPOWER)).await,
    ) else {
        return empty_battery();
    };
    let (present, kind, percentage, state, empty, full, on_battery) = tokio::join!(
        timed(device.get_property::<bool>("IsPresent")),
        timed(device.get_property::<u32>("Type")),
        timed(device.get_property::<f64>("Percentage")),
        timed(device.get_property::<u32>("State")),
        timed(device.get_property::<i64>("TimeToEmpty")),
        timed(device.get_property::<i64>("TimeToFull")),
        timed(root.get_property::<bool>("OnBattery")),
    );
    let charging = match state {
        Some(1) => Some(true),
        Some(2..=4) => Some(false),
        _ => None,
    };
    let mut result = empty_battery();
    if present != Some(true) || kind != Some(2) {
        return result;
    }
    result.health.available =
        percentage.is_some_and(|p| p.is_finite() && (0.0..=100.0).contains(&p));
    result.percentage =
        percentage.filter(|value| value.is_finite() && (0.0..=100.0).contains(value));
    result.charging = charging;
    result.state = state.filter(|&s| s <= 6);
    result.time_to_empty_seconds = empty.filter(|&v| v > 0).map(|v| v as u64);
    result.time_to_full_seconds = full.filter(|&v| v > 0).map(|v| v as u64);
    result.on_battery = on_battery;
    result
}

async fn profiles(system: &zbus::Connection) -> PowerProfilesState {
    for (name, path) in PROFILE_SERVICES {
        let Some(service) = timed(proxy(system, name, path, name)).await else {
            continue;
        };
        let (active, available, degraded, holds) = tokio::join!(
            timed(service.get_property::<String>("ActiveProfile")),
            timed(
                service
                    .get_property::<Vec<std::collections::HashMap<String, OwnedValue>>>("Profiles")
            ),
            timed(service.get_property::<String>("PerformanceDegraded")),
            timed(service.get_property::<Vec<HashMap<String, OwnedValue>>>("ActiveProfileHolds")),
        );
        let Some(active) = active.filter(|a| EntityId(a.clone()).valid()) else {
            continue;
        };
        let Some(profiles) = available else { continue };
        let mut state = empty_profiles();
        for item in profiles.into_iter().take(2048) {
            let mut item = item;
            let Some(value) = item.remove("Profile") else {
                continue;
            };
            let Ok(id) = String::try_from(value) else {
                continue;
            };
            if !EntityId(id.clone()).valid()
                || state.profiles.items.contains_key(&EntityId(id.clone()))
            {
                continue;
            }
            let description = item
                .remove("Name")
                .and_then(|value| String::try_from(value).ok())
                .unwrap_or_else(|| id.clone());
            state.profiles.order.push(EntityId(id.clone()));
            state.profiles.items.insert(
                EntityId(id),
                PowerProfile {
                    name: description,
                    available: true,
                    reason: None,
                },
            );
        }
        if !state.profiles.items.contains_key(&EntityId(active.clone())) {
            continue;
        }
        state.active = Some(EntityId(active));
        state.degraded = degraded;
        state.holds = holds
            .unwrap_or_default()
            .into_iter()
            .take(128)
            .filter_map(|mut hold| {
                let application_id = String::try_from(hold.remove("ApplicationId")?).ok()?;
                let reason = String::try_from(hold.remove("Reason")?).ok()?;
                let profile = String::try_from(hold.remove("Profile")?).ok()?;
                if application_id.len() > 256 || reason.len() > 512 || profile.len() > 128 {
                    return None;
                }
                Some(ProfileHold {
                    profile,
                    reason,
                    application_id,
                })
            })
            .collect();
        state.health.available = true;
        return state;
    }
    empty_profiles()
}

fn publish(hub: &Shared, b: BatteryState, p: PowerProfilesState) -> Result<()> {
    debug_assert!(CoreSnapshot::Battery(b.clone()).valid());
    debug_assert!(CoreSnapshot::PowerProfiles(p.clone()).valid());
    hub.borrow_mut()
        .publish(BATTERY, serde_json::to_value(b)?)?;
    hub.borrow_mut()
        .publish(PROFILES, serde_json::to_value(p)?)?;
    Ok(())
}

async fn run_connection(hub: Shared, system: zbus::Connection) -> Result<()> {
    let mut events = tokio::time::timeout(
        Duration::from_secs(3),
        signals(
            &system,
            &[UPOWER, PROFILE_SERVICES[0].0, PROFILE_SERVICES[1].0],
            None,
            None,
        ),
    )
    .await??;
    hub.borrow_mut().service("system-power", "running", 0)?;
    loop {
        let (b, p) = tokio::join!(battery(&system), profiles(&system));
        publish(&hub, b, p)?;
        match futures_util::StreamExt::next(&mut events).await {
            Some(_) => {}
            None => return Err("Power D-Bus signal stream ended".into()),
        }
    }
}

pub async fn run(hub: Shared) -> Result<()> {
    loop {
        let system =
            match tokio::time::timeout(Duration::from_secs(3), zbus::Connection::system()).await {
                Ok(Ok(connection)) => connection,
                _ => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
        let result = run_connection(hub.clone(), system).await;
        if result.is_ok() {
            return Ok(());
        }
        initial(&hub)?;
        hub.borrow_mut().service("system-power", "starting", 0)?;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::hub::Hub;
    use std::{
        io::{BufRead, BufReader},
        path::Path,
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };
    use tokio::task::LocalSet;

    struct PrivateBus(Child, String);
    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn private_bus(dir: &Path) -> PrivateBus {
        let mut child = Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address",
                &format!("--address=unix:path={}", dir.join("bus").display()),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        PrivateBus(child, address.trim().to_owned())
    }
    type BatteryData = Arc<Mutex<(bool, u32, f64, i64, i64, bool)>>;
    #[derive(Clone)]
    struct BatteryFake(BatteryData);
    #[zbus::interface(name = "org.freedesktop.UPower.Device")]
    impl BatteryFake {
        #[zbus(property)]
        fn is_present(&self) -> bool {
            self.0.lock().unwrap().0
        }
        #[zbus(property, name = "Type")]
        fn kind(&self) -> u32 {
            2
        }
        #[zbus(property)]
        fn state(&self) -> u32 {
            self.0.lock().unwrap().1
        }
        #[zbus(property)]
        fn percentage(&self) -> f64 {
            self.0.lock().unwrap().2
        }
        #[zbus(property)]
        fn time_to_empty(&self) -> i64 {
            self.0.lock().unwrap().3
        }
        #[zbus(property)]
        fn time_to_full(&self) -> i64 {
            self.0.lock().unwrap().4
        }
    }
    struct UPowerFake(BatteryData);
    #[zbus::interface(name = "org.freedesktop.UPower")]
    impl UPowerFake {
        #[zbus(property)]
        fn on_battery(&self) -> bool {
            self.0.lock().unwrap().5
        }
    }
    struct ProfilesFake(Arc<Mutex<(String, String, String)>>);
    #[zbus::interface(name = "org.freedesktop.UPower.PowerProfiles")]
    impl ProfilesFake {
        #[zbus(property)]
        fn active_profile(&self) -> String {
            self.0.lock().unwrap().0.clone()
        }
        #[zbus(property)]
        fn profiles(&self) -> Vec<HashMap<String, OwnedValue>> {
            ["power-saver", "balanced", "performance"]
                .into_iter()
                .map(|name| {
                    [(
                        "Profile".to_owned(),
                        OwnedValue::try_from(zbus::zvariant::Value::new(name)).unwrap(),
                    )]
                    .into_iter()
                    .collect()
                })
                .collect()
        }
        #[zbus(property)]
        fn performance_degraded(&self) -> String {
            self.0.lock().unwrap().1.clone()
        }
        #[zbus(property)]
        fn active_profile_holds(&self) -> Vec<HashMap<String, OwnedValue>> {
            let (profile, reason, app) = self.0.lock().unwrap().clone();
            if app.is_empty() {
                return vec![];
            }
            let hold = [
                (
                    "Profile".into(),
                    OwnedValue::try_from(zbus::zvariant::Value::new(profile)).unwrap(),
                ),
                (
                    "Reason".into(),
                    OwnedValue::try_from(zbus::zvariant::Value::new(reason)).unwrap(),
                ),
                (
                    "ApplicationId".into(),
                    OwnedValue::try_from(zbus::zvariant::Value::new(app)).unwrap(),
                ),
            ]
            .into_iter()
            .collect::<HashMap<_, _>>();
            vec![hold]
        }
    }
    async fn wait_snapshot(
        hub: &Shared,
        healthy: bool,
        percentage: Option<f64>,
        active: Option<&str>,
    ) {
        for _ in 0..200 {
            let b = hub.borrow().snapshot(BATTERY).unwrap();
            let p = hub.borrow().snapshot(PROFILES).unwrap();
            if b["health"]["available"] == healthy
                && b["percentage"].as_f64() == percentage
                && p["health"]["available"] == healthy
                && p["active"].as_str() == active
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("power state did not reach expected snapshot");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn private_bus_power_topics_start_unavailable_then_follow_properties_changed() {
        let dir = tempfile::tempdir().unwrap();
        let bus = private_bus(dir.path());
        let hub = Hub::new("power-test".into(), false, Duration::from_millis(1)).shared();
        initial(&hub).unwrap();
        assert_eq!(
            hub.borrow().snapshot(BATTERY).unwrap()["health"]["available"],
            false
        );
        assert_eq!(
            hub.borrow().snapshot(PROFILES).unwrap()["health"]["available"],
            false
        );
        let service_name = PROFILE_SERVICES[0].0;
        let state = Arc::new(Mutex::new((true, 2, 56.0, 7200, 1800, true)));
        let profile_state = Arc::new(Mutex::new((
            "balanced".to_owned(),
            "".to_owned(),
            "".to_owned(),
        )));
        let client = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        LocalSet::new().run_until(async {
            let worker = tokio::task::spawn_local(run_connection(hub.clone(), client.clone()));
            // Register services after monitoring starts: name-owner signals must
            // recover the initially unavailable topics without a polling timer.
            tokio::time::sleep(Duration::from_millis(100)).await;
        let upower = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name(UPOWER)
            .unwrap()
            .serve_at(UPOWER_PATH, UPowerFake(state.clone()))
            .unwrap()
            .serve_at(DISPLAY, BatteryFake(state.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let profiles_conn = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name(service_name)
            .unwrap()
            .serve_at(PROFILE_SERVICES[0].1, ProfilesFake(profile_state.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();

            wait_snapshot(&hub, true, Some(56.0), Some("balanced")).await;
            assert_eq!(hub.borrow().snapshot(BATTERY).unwrap()["time_to_empty_seconds"], 7200);
            assert_eq!(hub.borrow().snapshot(BATTERY).unwrap()["time_to_full_seconds"], 1800);
            assert_eq!(hub.borrow().snapshot(BATTERY).unwrap()["on_battery"], true);
            *state.lock().unwrap() = (true, 1, 81.0, 0, 900, false);
            let server = upower.object_server();
            let device = server.interface::<_, BatteryFake>(DISPLAY).await.unwrap();
            device.get().await.percentage_changed(device.signal_emitter()).await.unwrap();
            let root = server.interface::<_, UPowerFake>(UPOWER_PATH).await.unwrap();
            root.get().await.on_battery_changed(root.signal_emitter()).await.unwrap();
            wait_snapshot(&hub, true, Some(81.0), Some("balanced")).await;
            *profile_state.lock().unwrap() = ("power-saver".into(), "thermal".into(), "org.example.app".into());
            let iface = profiles_conn.object_server().interface::<_, ProfilesFake>(PROFILE_SERVICES[0].1).await.unwrap();
            iface.get().await.active_profile_changed(iface.signal_emitter()).await.unwrap();
            iface.get().await.performance_degraded_changed(iface.signal_emitter()).await.unwrap();
            iface.get().await.active_profile_holds_changed(iface.signal_emitter()).await.unwrap();
            wait_snapshot(&hub, true, Some(81.0), Some("power-saver")).await;
            assert_eq!(hub.borrow().snapshot(PROFILES).unwrap()["degraded"], "thermal");
            assert_eq!(hub.borrow().snapshot(PROFILES).unwrap()["holds"][0], serde_json::json!({"profile":"power-saver","reason":"thermal","application_id":"org.example.app"}));
            let b: BatteryState = serde_json::from_value(hub.borrow().snapshot(BATTERY).unwrap()).unwrap();
            let p: PowerProfilesState = serde_json::from_value(hub.borrow().snapshot(PROFILES).unwrap()).unwrap();
            assert!(CoreSnapshot::Battery(b.clone()).valid());
            assert!(CoreSnapshot::PowerProfiles(p.clone()).valid());
            assert_eq!(b.state, Some(1));
            assert_eq!(b.charging, Some(true));
            assert_eq!(p.profiles.order.len(), 3);
            // DisplayDevice exists on machines without a battery, too.
            state.lock().unwrap().0 = false;
            assert!(!battery(&client).await.health.available);
            state.lock().unwrap().0 = true;
            state.lock().unwrap().1 = 0;
            assert_eq!(battery(&client).await.charging, None);
            upower.release_name(UPOWER).await.unwrap();
            profiles_conn.release_name(service_name).await.unwrap();
            wait_snapshot(&hub, false, None, None).await;
            worker.abort();
            let _ = worker.await;
        }).await;
    }
}
