//! Fakes only: a temp input-device list and a script for `powerprofilesctl`.
use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn hub() -> Shared {
    Hub::new("caps".into(), false, Duration::from_millis(1)).shared()
}
fn env(dir: &std::path::Path, devices: &str, profiles: &str, pacman: bool) -> Env {
    std::fs::write(dir.join("devices"), devices).unwrap();
    let ctl = dir.join("powerprofilesctl");
    std::fs::write(&ctl, format!("#!/bin/sh\n{profiles}")).unwrap();
    std::fs::set_permissions(&ctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    Env {
        devices: dir.join("devices"),
        power_supply: dir.join("power_supply"),
        powerprofilesctl: ctl.display().to_string(),
        which: Arc::new(move |program| {
            if program == "pacman" {
                pacman
            } else {
                std::path::Path::new(program).is_file()
            }
        }),
    }
}
fn supply(dir: &std::path::Path, name: &str, kind: &str, scope: Option<&str>) {
    let device = dir.join("power_supply").join(name);
    std::fs::create_dir_all(&device).unwrap();
    std::fs::write(device.join("type"), format!("{kind}\n")).unwrap();
    if let Some(scope) = scope {
        std::fs::write(device.join("scope"), format!("{scope}\n")).unwrap();
    }
}
fn compositor(backend: &str, options: Value) -> Value {
    json!({"backend":backend,"capabilities":{"windowCapture":backend=="hyprland","nativePanelDrag":backend=="hyprland","desktopOptions":options}})
}
fn state(hub: &Shared) -> CapabilitiesState {
    serde_json::from_value(hub.borrow().snapshot(CAPABILITIES_TOPIC).unwrap()).unwrap()
}

#[test]
fn probes_read_devices_power_profiles_and_pacman() {
    let dir = tempfile::tempdir().unwrap();
    let both = "case \"$1\" in get) echo balanced;; list) printf '* balanced:\\n\\n  performance:\\n';; esac\n";
    // A laptop battery counts; a mouse's battery (`scope` Device) and mains do not.
    supply(dir.path(), "AC", "Mains", None);
    supply(dir.path(), "hidpp_battery_0", "Battery", Some("Device"));
    assert!(!probe(&env(dir.path(), "", both, true)).battery);
    supply(dir.path(), "BAT0", "Battery", Some("System"));
    let p = probe(&env(dir.path(), "N: Name=\"Elan Touchpad\"\n", both, true));
    assert_eq!(
        p,
        Probe {
            touchpad: true,
            battery: true,
            packages: true,
            power_profiles: true,
            performance_profile: true
        }
    );
    std::fs::remove_dir_all(dir.path().join("power_supply/BAT0")).unwrap();
    supply(dir.path(), "BAT1", "Battery", None);
    assert!(
        probe(&env(dir.path(), "", both, true)).battery,
        "no scope file: still a battery"
    );
    std::fs::remove_dir_all(dir.path().join("power_supply")).unwrap();
    let saver = "case \"$1\" in get) echo balanced;; list) printf '* balanced:\\n\\n  power-saver:\\n';; esac\n";
    let p = probe(&env(dir.path(), "N: Name=\"Keyboard\"\n", saver, false));
    assert_eq!(
        p,
        Probe {
            touchpad: false,
            battery: false,
            packages: false,
            power_profiles: true,
            performance_profile: false
        }
    );
    // A failing or missing tool means no power profiles.
    assert!(!probe(&env(dir.path(), "", "exit 1\n", true)).power_profiles);
    let gone = env(dir.path(), "", both, true);
    std::fs::remove_file(&gone.powerprofilesctl).unwrap();
    assert!(!probe(&gone).power_profiles);
}

#[test]
fn flags_follow_the_compositor_and_the_stores() {
    let hub = hub();
    let probe = Probe {
        touchpad: true,
        ..Probe::default()
    };
    // Nothing published yet: no compositor flags, every desktop option offered (as QML).
    let flags_now = flags(&probe, &hub.borrow());
    assert!(flags_now["desktop.blur"] && flags_now["touchpad"]);
    assert!(!flags_now["niriWorkspaces"] && !flags_now["input.controls"] && !flags_now["ai"]);

    assert!(!flags_now["bluetooth"] && !flags_now["battery"]);
    // A battery is a probe; Bluetooth follows the topic: an adapter must be listed.
    let bluetooth = |available: bool, adapters: Value| json!({"health":{"available":available,"stale":false},"adapters":{"items":{},"order":adapters},"devices":{"items":{},"order":[]}});
    hub.borrow_mut()
        .publish("bluetooth", bluetooth(true, json!([])))
        .unwrap();
    assert!(!flags(&probe, &hub.borrow())["bluetooth"]);
    hub.borrow_mut()
        .publish("bluetooth", bluetooth(false, json!(["hci0"])))
        .unwrap();
    assert!(!flags(&probe, &hub.borrow())["bluetooth"]);
    hub.borrow_mut()
        .publish("bluetooth", bluetooth(true, json!(["hci0"])))
        .unwrap();
    assert!(flags(&probe, &hub.borrow())["bluetooth"]);
    let battery = Probe {
        battery: true,
        ..probe
    };
    assert!(flags(&battery, &hub.borrow())["battery"]);
    hub.borrow_mut()
        .publish("compositor", compositor("niri", json!(["gapsIn", "blur"])))
        .unwrap();
    hub.borrow_mut()
        .publish(
            "settings.store.desktop",
            json!({"data":{"niriWorkspaceMode":"fixed-all"}}),
        )
        .unwrap();
    hub.borrow_mut()
        .publish(
            "settings.store.profile",
            json!({"data":{"components":["configs","ai"]}}),
        )
        .unwrap();
    let niri = flags(&probe, &hub.borrow());
    assert!(niri["niriWorkspaces"] && niri["niriFixedWorkspaces"] && niri["input.touchpadSpeed"]);
    assert!(niri["input.controls"] && niri["ai"] && !niri["brain"]);
    assert!(niri["desktop.gapsIn"] && niri["desktop.blur"] && !niri["desktop.shadow"]);
    assert!(!niri["windowCapture"] && !niri["panelDragging"]);

    hub.borrow_mut()
        .publish("compositor", compositor("hyprland", json!(["shadow"])))
        .unwrap();
    hub.borrow_mut()
        .publish(
            "settings.store.desktop",
            json!({"data":{"niriWorkspaceMode":"dynamic"}}),
        )
        .unwrap();
    let hypr = flags(&probe, &hub.borrow());
    assert!(
        !hypr["niriWorkspaces"] && !hypr["niriFixedWorkspaces"] && !hypr["input.touchpadSpeed"]
    );
    assert!(hypr["input.controls"] && hypr["windowCapture"] && hypr["panelDragging"]);
    assert!(hypr["desktop.shadow"] && !hypr["desktop.gapsIn"]);
    // Every flag the registry's conditions name and QML defined is published.
    for name in [
        "ai",
        "brain",
        "packages",
        "powerProfiles",
        "performanceProfile",
        "touchpad",
        "panelDragging",
        "windowCapture",
        "niriWorkspaces",
        "niriFixedWorkspaces",
        "input.controls",
        "input.touchpadSpeed",
        "bluetooth",
        "battery",
    ] {
        assert!(hypr.contains_key(name), "{name}");
    }
    assert_eq!(hypr.len(), 22);
}

#[test]
fn the_topic_is_published_and_follows_its_inputs() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::task::LocalSet::new()
            .run_until(tokio::time::timeout(Duration::from_secs(30), async {
                let dir = tempfile::tempdir().unwrap();
                let hub = hub();
                initial(&hub).unwrap();
                assert!(!state(&hub).flags["niriWorkspaces"]);
                let task = tokio::task::spawn_local(run_with(
                    hub.clone(),
                    env(
                        dir.path(),
                        "N: Name=\"Touchpad\"\n",
                        "echo balanced\n",
                        true,
                    ),
                ));
                for _ in 0..200 {
                    if state(&hub).flags["touchpad"] {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                assert!(state(&hub).flags["touchpad"]);
                hub.borrow_mut()
                    .publish("compositor", compositor("niri", json!(["blur"])))
                    .unwrap();
                for _ in 0..200 {
                    if state(&hub).flags["niriWorkspaces"] {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let current = state(&hub);
                current.validate().unwrap();
                assert!(current.flags["niriWorkspaces"] && !current.flags["niriFixedWorkspaces"]);
                // `capabilities.refresh` probes again: a battery that appeared is now seen.
                assert!(!current.flags["battery"]);
                assert!(available_methods(&hub.borrow()).contains_key("capabilities.refresh"));
                supply(dir.path(), "BAT0", "Battery", Some("System"));
                assert_eq!(
                    dispatch(&hub.borrow(), json!({})).unwrap(),
                    json!({"requested":true})
                );
                for _ in 0..200 {
                    if state(&hub).flags["battery"] {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                assert!(state(&hub).flags["battery"]);
                assert_eq!(
                    dispatch(&hub.borrow(), json!({"x":1})).unwrap_err().code,
                    ErrorCode::Validation
                );
                task.abort();
            }))
            .await
            .unwrap();
    });
}
