//! Fakes only: scripts for every tool and for the helper engine, a temp `/sys` tree, a temp
//! HOME, and a private socket standing in for PipeWire. No real device, bus or session is used.
use super::*;
use crate::daemon::hub::Hub;
use std::{os::unix::fs::PermissionsExt, os::unix::net::UnixListener};
use tokio::task::LocalSet;

struct Fixture {
    dir: tempfile::TempDir,
    hub: Shared,
    missing: std::rc::Rc<RefCell<Vec<String>>>,
    _sockets: Vec<UnixListener>,
}
fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let d = root.display();
        // Runtime dir with a PipeWire socket.
        std::fs::create_dir_all(root.join("run")).unwrap();
        let pipewire = UnixListener::bind(root.join("run/pipewire-0")).unwrap();
        std::fs::create_dir_all(root.join("sys")).unwrap();
        let system_bus = UnixListener::bind(root.join("system_bus")).unwrap();
        let user_bus = UnixListener::bind(root.join("run/bus")).unwrap();
        write(&root.join("rfkill_dev"), "");
        // Light devices: a panel backlight (0..100) and a keyboard backlight (0..3).
        for (class, name, max, now) in [
            ("backlight", "intel_backlight", 100, 40),
            ("leds", "tpacpi::kbd_backlight", 3, 1),
        ] {
            write(
                &root.join(format!("sys/class/{class}/{name}/max_brightness")),
                &format!("{max}\n"),
            );
            write(
                &root.join(format!("sys/class/{class}/{name}/brightness")),
                &format!("{now}\n"),
            );
        }
        for (index, kind, soft) in [(0, "wlan", 0), (1, "bluetooth", 1), (2, "nfc", 0)] {
            let base = root.join(format!("sys/class/rfkill/rfkill{index}"));
            write(&base.join("type"), &format!("{kind}\n"));
            write(&base.join("soft"), &format!("{soft}\n"));
            write(&base.join("hard"), "0\n");
        }
        write(&root.join("profile"), "balanced\n");
        write(&root.join("night"), "false");
        write(
            &root.join("pw.json"),
            r#"[{"id":42,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"alsa_output.fake","media.class":"Audio/Sink"}}},
               {"id":43,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"video.fake","media.class":"Video/Source"}}},
               {"id":44,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"alsa_input.fake","media.class":"Audio/Source"}}},
               {"id":45,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"stream.fake","media.class":"Stream/Output/Audio"}}}]"#,
        );
        script(
            &root.join("wpctl"),
            &format!("[ -f '{d}/gone' ] && exit 1\ncase \"$1\" in get-volume) echo 'Volume: 0.50';; *) echo \"$@\" >> '{d}/wpctl.log';; esac\n"),
        );
        script(&root.join("pw-dump"), &format!("cat '{d}/pw.json'\n"));
        script(
            &root.join("rfkill"),
            &format!(
                "echo \"$@\" >> '{d}/rfkill.log'\nmode=$1; shift\nfor k in \"$@\"; do for r in '{d}'/sys/class/rfkill/*; do [ \"$(cat $r/type)\" = \"$k\" ] || continue; [ -f '{d}/hard' ] && continue; if [ $mode = block ]; then echo 1 > $r/soft; else echo 0 > $r/soft; fi; done; done\n"
            ),
        );
        script(
            &root.join("powerprofilesctl"),
            &format!("case \"$1\" in set) [ -f '{d}/refuse' ] || echo \"$2\" > '{d}/profile';; get) cat '{d}/profile';; esac\n"),
        );
        script(
            &root.join("systemctl"),
            &format!("for f in stay rec; do if [ -f \"{d}/$f\" ]; then echo active; else echo inactive; fi; done\nexit 3\n"),
        );
        script(
            &root.join("brightnessctl"),
            &format!("echo \"$@\" >> '{d}/light.log'\necho \"$6\" > '{d}'/sys/class/\"$2\"/\"$4\"/brightness\n"),
        );
        script(&root.join("ddcutil"), "exit 1\n");
        script(
            &root.join("engine"),
            &format!(
                r#"d='{d}'
echo "$@" >> $d/engine.log
[ -f $d/hold ] && sleep 0.4
case "$1 $2 $3" in
 "control-tools state ") if [ -f $d/external ]; then ext=true; else ext=false; fi
   echo "{{\"nightLightSupported\":true,\"nightLightExternal\":$ext,\"nightLightEnabled\":$(cat $d/night)}}";;
 "control-tools night-light on") echo true > $d/night; echo '{{}}';;
 "control-tools night-light off") echo false > $d/night; echo '{{}}';;
 "menu-backend tool stay-awake") if [ -f $d/stay ]; then rm $d/stay; else touch $d/stay; fi; echo '{{"message":""}}';;
 "menu-backend tool record-stop") rm -f $d/rec; echo '{{"message":""}}';;
 "menu-backend tool record-start") touch $d/rec;;
 "wallpaper-media theme "*) cat >> $d/theme.log; echo '{{}}';;
 "wallpaper-media catalog "*) echo '{{"entries":[{{"path":"/w/a.png","poster":"/w/a.png","name":"a","dynamic":false,"animated":false,"preview":null,"thumbnail":null}},{{"path":"/w/b.png","poster":"/w/b.png","name":"b","dynamic":false,"animated":false,"preview":null,"thumbnail":null}}]}}';;
 "wallpaper-media select "*) echo "{{\"path\":\"$3\",\"poster\":\"$3\",\"name\":\"n\",\"dynamic\":false,\"animated\":false,\"preview\":null,\"thumbnail\":null}}";;
esac
"#
            ),
        );
        let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
        let missing = std::rc::Rc::new(RefCell::new(Vec::<String>::new()));
        let fixture = Self {
            dir,
            hub,
            missing,
            _sockets: vec![pipewire, system_bus, user_bus],
        };
        fixture.install();
        fixture
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn env(&self) -> Env {
        let missing: Vec<String> = self.missing.borrow().clone();
        let p = |name: &str| self.path(name).display().to_string();
        Env {
            engine: self.path("engine"),
            wpctl: p("wpctl"),
            pw_dump: p("pw-dump"),
            rfkill: p("rfkill"),
            powerprofilesctl: p("powerprofilesctl"),
            systemctl: p("systemctl"),
            brightnessctl: p("brightnessctl"),
            ddcutil: p("ddcutil"),
            sys: self.path("sys"),
            runtime: Some(self.path("run")),
            system_bus: self.path("system_bus"),
            rfkill_dev: self.path("rfkill_dev"),
            paths: Paths::new(self.path("home")),
            which: Arc::new(move |name| !missing.iter().any(|m| m == name) && name != "none"),
            tick: Duration::from_millis(30),
        }
    }
    fn install(&self) {
        set_env(Some(self.env()));
    }
    /// Pretend these tools are not installed.
    fn without(&self, names: &[&str]) {
        *self.missing.borrow_mut() = names.iter().map(|n| (*n).to_owned()).collect();
        self.install();
    }
    fn text(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap_or_default()
    }
    async fn call(&self, method: &str, params: Value) -> std::result::Result<Value, Failure> {
        dispatch(self.hub.clone(), method, params).await
    }
}

async fn released() {
    for _ in 0..200 {
        if !CAPTURE_BUSY.with(Cell::get) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the capture lane was not released");
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(LocalSet::new().run_until(future))
}

#[test]
fn volume_is_linear_on_the_wire_and_cubic_for_wpctl() {
    run(async {
        let f = Fixture::new();
        f.call("audio.set_volume", json!({"target":"sink","value":0.125}))
            .await
            .unwrap();
        f.call("audio.set_volume", json!({"target":"source","value":1.0}))
            .await
            .unwrap();
        f.call("audio.set_mute", json!({"target":"sink","muted":true}))
            .await
            .unwrap();
        f.call("audio.set_mute", json!({"target":"source","muted":false}))
            .await
            .unwrap();
        f.call(
            "audio.set_volume",
            json!({"target":"node","node":"alsa_output.fake","value":0.0}),
        )
        .await
        .unwrap();
        f.call(
            "audio.set_mute",
            json!({"target":"node","node":"alsa_output.fake","muted":true}),
        )
        .await
        .unwrap();
        assert_eq!(
            f.text("wpctl.log"),
            "set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 0.5000\n\
             set-volume -l 1.0 @DEFAULT_AUDIO_SOURCE@ 1.0000\n\
             set-mute @DEFAULT_AUDIO_SINK@ 1\n\
             set-mute @DEFAULT_AUDIO_SOURCE@ 0\n\
             set-volume -l 1.0 42 0.0000\n\
             set-mute 42 1\n"
        );
    });
}

#[test]
fn audio_refuses_bad_targets_missing_nodes_and_vanished_devices() {
    run(async {
        let f = Fixture::new();
        for params in [
            json!({"target":"sink","value":1.5}),
            json!({"target":"sink","value":-0.1}),
            json!({"target":"node","value":0.5}),
            json!({"target":"sink","node":"x","value":0.5}),
            json!({"target":"node","node":"a b","value":0.5}),
            json!({"target":"node","node":"$(id)","value":0.5}),
            json!({"target":"sink","value":0.5,"extra":1}),
        ] {
            let error = f
                .call("audio.set_volume", params.clone())
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation, "{params}");
            assert_eq!(error.dispatched, Some(false));
        }
        // Unknown node and a non-audio node.
        for name in ["nope", "video.fake"] {
            let error = f
                .call(
                    "audio.set_mute",
                    json!({"target":"node","node":name,"muted":true}),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Unavailable, "{name}");
        }
        write(&f.path("gone"), "");
        let error = f
            .call("audio.set_volume", json!({"target":"sink","value":0.5}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(f.text("wpctl.log").is_empty(), "nothing was written");
    });
}

#[test]
fn the_default_device_is_a_named_sink_or_source_and_nothing_else() {
    run(async {
        let f = Fixture::new();
        for node in ["alsa_output.fake", "alsa_input.fake"] {
            let reply = f
                .call("audio.set_default", json!({ "node": node }))
                .await
                .unwrap();
            assert_eq!(reply, json!({"changed":true}));
        }
        assert_eq!(f.text("wpctl.log"), "set-default 42\nset-default 44\n");
        // A stream, a video node and an unknown name are unavailable; bad names never get that far.
        for node in ["stream.fake", "video.fake", "nope"] {
            let error = f
                .call("audio.set_default", json!({ "node": node }))
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Unavailable, "{node}");
        }
        for params in [
            json!({}),
            json!({"node":""}),
            json!({"node":"a b"}),
            json!({"node":"$(id)"}),
            json!({"node":"alsa_output.fake","target":"sink"}),
        ] {
            let error = f
                .call("audio.set_default", params.clone())
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation, "{params}");
            assert_eq!(error.dispatched, Some(false));
        }
        assert_eq!(
            f.text("wpctl.log").lines().count(),
            2,
            "nothing else written"
        );
    });
}

#[test]
fn keyboard_light_sets_an_absolute_step_and_refuses_beyond_the_maximum() {
    run(async {
        let f = Fixture::new();
        let reply = f
            .call("keyboard_light.set", json!({"level":3}))
            .await
            .unwrap();
        assert_eq!(reply, json!({"changed":true,"level":3,"max":3}));
        assert_eq!(
            f.text("sys/class/leds/tpacpi::kbd_backlight/brightness")
                .trim(),
            "3"
        );
        let same = f
            .call("keyboard_light.set", json!({"level":3}))
            .await
            .unwrap();
        assert_eq!(same["changed"], false);
        assert_eq!(
            f.text("light.log").lines().count(),
            1,
            "no write when unchanged"
        );
        let error = f
            .call("keyboard_light.set", json!({"level":4}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
        // The brightness lane is shared: a held lane makes the call busy.
        let held = bar_services::brightness_lane().try_lock_owned().unwrap();
        let error = f
            .call("keyboard_light.set", json!({"level":0}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy);
        drop(held);
        f.call("keyboard_light.set", json!({"level":0}))
            .await
            .unwrap();
    });
}

#[test]
fn power_profile_is_checked_after_the_write() {
    run(async {
        let f = Fixture::new();
        f.call("power.set_profile", json!({"profile":"power-saver"}))
            .await
            .unwrap();
        assert_eq!(f.text("profile").trim(), "power-saver");
        assert_eq!(
            f.call("power.set_profile", json!({"profile":"turbo"}))
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
        write(&f.path("refuse"), "");
        let error = f
            .call("power.set_profile", json!({"profile":"performance"}))
            .await
            .unwrap_err();
        assert_eq!((error.code, error.dispatched), (ErrorCode::Io, Some(true)));
    });
}

#[test]
fn airplane_blocks_every_radio_and_restores_only_what_was_on() {
    run(async {
        let f = Fixture::new();
        // wlan on, bluetooth already off, nfc is not a managed radio.
        f.call("airplane.set", json!({"enabled":true}))
            .await
            .unwrap();
        assert_eq!(f.text("rfkill.log").trim(), "block bluetooth wlan");
        assert_eq!(f.text("sys/class/rfkill/rfkill0/soft").trim(), "1");
        assert_eq!(
            f.text("sys/class/rfkill/rfkill2/soft").trim(),
            "0",
            "nfc untouched"
        );
        f.call("airplane.set", json!({"enabled":false}))
            .await
            .unwrap();
        assert_eq!(f.text("rfkill.log").lines().last().unwrap(), "unblock wlan");
        assert_eq!(f.text("sys/class/rfkill/rfkill0/soft").trim(), "0");
        assert_eq!(
            f.text("sys/class/rfkill/rfkill1/soft").trim(),
            "1",
            "bluetooth stays off"
        );
        // A hard block makes the write ineffective, which is an error.
        write(&f.path("hard"), "");
        let error = f
            .call("airplane.set", json!({"enabled":true}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Io);
    });
}

#[test]
fn night_light_toggles_through_the_helper_and_respects_external_control() {
    run(async {
        let f = Fixture::new();
        initial(&f.hub).await.unwrap();
        assert_eq!(
            f.call("night_light.set", json!({"enabled":true}))
                .await
                .unwrap()["changed"],
            true
        );
        assert_eq!(f.text("night").trim(), "true");
        assert_eq!(
            f.call("night_light.set", json!({"enabled":true}))
                .await
                .unwrap()["changed"],
            false
        );
        assert_eq!(
            f.call("night_light.set", json!({"enabled":false}))
                .await
                .unwrap()["changed"],
            true
        );
        write(&f.path("external"), "");
        let error = f
            .call("night_light.set", json!({"enabled":true}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(!f.text("engine.log").contains("night-light on\nwallpaper"));
    });
}

#[test]
fn stay_awake_and_recorder_follow_the_unit_state() {
    run(async {
        let f = Fixture::new();
        assert_eq!(
            f.call("stay_awake.set", json!({"enabled":true}))
                .await
                .unwrap()["changed"],
            true
        );
        assert!(f.path("stay").exists());
        assert_eq!(
            f.call("stay_awake.set", json!({"enabled":true}))
                .await
                .unwrap()["changed"],
            false
        );
        assert_eq!(
            f.call("stay_awake.set", json!({"enabled":false}))
                .await
                .unwrap()["changed"],
            true
        );
        assert!(!f.path("stay").exists());
        // Recorder: start spawns the interactive tool and returns; stop waits for the unit.
        assert_eq!(
            f.call("recorder.stop", json!({})).await.unwrap()["changed"],
            false
        );
        f.call("recorder.start", json!({"mode":"region"}))
            .await
            .unwrap();
        for _ in 0..100 {
            if f.path("rec").exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(f.path("rec").exists());
        let error = f
            .call("recorder.start", json!({"mode":"screen"}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Busy, "already recording");
        assert_eq!(
            f.call("recorder.stop", json!({})).await.unwrap()["changed"],
            true
        );
        assert!(!f.path("rec").exists());
    });
}

#[test]
fn capture_tools_start_one_at_a_time() {
    run(async {
        let f = Fixture::new();
        write(&f.path("hold"), "");
        f.call("screenshot.capture", json!({"mode":"region"}))
            .await
            .unwrap();
        for (method, params) in [
            ("color_picker.pick", json!({})),
            ("screenshot.capture", json!({"mode":"window"})),
            ("recorder.start", json!({"mode":"region"})),
        ] {
            assert_eq!(
                f.call(method, params).await.unwrap_err().code,
                ErrorCode::Busy,
                "{method}"
            );
        }
        released().await;
        std::fs::remove_file(f.path("hold")).unwrap();
        f.call("screenshot.capture", json!({"mode":"screen"}))
            .await
            .unwrap();
        released().await;
        f.call("screenshot.capture", json!({"mode":"window"}))
            .await
            .unwrap();
        released().await;
        f.call("color_picker.pick", json!({})).await.unwrap();
        released().await;
        let log = f.text("engine.log");
        for line in [
            "control-tools screenshot",
            "menu-backend tool capture-screen",
            "menu-backend tool capture-window",
            "control-tools color-picker",
        ] {
            assert!(log.contains(line), "{line}");
        }
        assert_eq!(
            f.call("screenshot.capture", json!({"mode":"area"}))
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
    });
}

#[test]
fn appearance_and_wallpaper_use_the_existing_commands() {
    run(async {
        let f = Fixture::new();
        for mode in ["light", "dark", "auto"] {
            f.call("appearance.set_mode", json!({"mode":mode}))
                .await
                .unwrap_err();
        }
        // The route is advertised only with a settings service; the helper call itself works.
        let env = f.env();
        for mode in [
            AppearanceMode::Light,
            AppearanceMode::Dark,
            AppearanceMode::Auto,
        ] {
            set_appearance(&env, mode).unwrap();
        }
        assert_eq!(
            f.text("theme.log"),
            "{\"paletteAutoMode\":false,\"paletteMode\":\"light\"}\n{\"paletteAutoMode\":false,\"paletteMode\":\"dark\"}\n{\"paletteAutoMode\":true}\n"
        );
        assert!(wallpaper_next(&f.hub, &env).await.unwrap());
        let log = f.text("engine.log");
        assert!(log.contains("wallpaper-media select /w/"), "{log}");
    });
}

#[test]
fn methods_are_advertised_only_with_their_backend_and_never_in_safe_mode() {
    run(async {
        let f = Fixture::new();
        initial(&f.hub).await.unwrap();
        let all = |f: &Fixture| {
            available_methods(&f.hub.borrow())
                .into_keys()
                .collect::<Vec<_>>()
        };
        let full = all(&f);
        for name in [
            "audio.set_volume",
            "audio.set_mute",
            "audio.set_default",
            "keyboard_light.set",
            "power.set_profile",
            "airplane.set",
            "night_light.set",
            "stay_awake.set",
            "recorder.start",
            "recorder.stop",
            "screenshot.capture",
            "color_picker.pick",
        ] {
            assert!(full.iter().any(|m| m == name), "{name} in {full:?}");
        }
        // Wallpaper and appearance need the settings service, absent in this fixture.
        assert!(!full
            .iter()
            .any(|m| m == "wallpaper.next" || m == "appearance.set_mode"));
        f.without(&["wf-recorder", "grim", "hyprpicker", "systemd-inhibit"]);
        let reduced = all(&f);
        for name in [
            "recorder.start",
            "recorder.stop",
            "screenshot.capture",
            "color_picker.pick",
            "stay_awake.set",
        ] {
            assert!(!reduced.iter().any(|m| m == name), "{name}");
        }
        assert!(reduced.iter().any(|m| m == "audio.set_volume"));
        let error = f.call("color_picker.pick", json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        // No keyboard backlight: the method disappears.
        std::fs::remove_dir_all(f.path("sys/class/leds")).unwrap();
        assert!(!all(&f).iter().any(|m| m == "keyboard_light.set"));
        // Tools without a session or a control device do not count as a backend.
        std::fs::remove_file(f.path("rfkill_dev")).unwrap();
        std::fs::remove_file(f.path("run/bus")).unwrap();
        f.install();
        let bare = all(&f);
        for name in ["airplane.set", "stay_awake.set", "recorder.start"] {
            assert!(!bare.iter().any(|m| m == name), "{name}");
        }
        // PipeWire socket gone: audio disappears.
        std::fs::remove_file(f.path("run/pipewire-0")).unwrap();
        assert!(!all(&f).iter().any(|m| m == "audio.set_mute"));
        f.hub.borrow_mut().safe = true;
        assert!(available_methods(&f.hub.borrow()).is_empty());
        for (method, params) in [
            ("audio.set_volume", json!({"target":"sink","value":0.5})),
            ("airplane.set", json!({"enabled":true})),
            ("power.set_profile", json!({"profile":"balanced"})),
            ("color_picker.pick", json!({})),
            ("wallpaper.next", json!({})),
        ] {
            let error = f.call(method, params).await.unwrap_err();
            assert_eq!(
                (error.code, error.dispatched),
                (ErrorCode::PermissionDenied, Some(false)),
                "{method}"
            );
        }
    });
}

#[test]
fn state_reads_every_backend_and_omits_the_missing_ones() {
    let f = Fixture::new();
    let probe = |slow| Probe {
        outputs: vec!["eDP-1".into(), "DP-1".into()],
        ddc: true,
        slow,
    };
    let state = collect(&f.env(), &probe(true), &empty_state());
    state.validate().unwrap();
    assert_eq!(
        state.brightness,
        BTreeMap::from([("eDP-1".into(), 40)]),
        "DP-1 has no DDC bus"
    );
    assert_eq!(state.keyboard, Some(KeyboardLight { level: 1, max: 3 }));
    assert_eq!(
        state.airplane,
        Some(Toggle { enabled: false }),
        "wlan is on"
    );
    assert_eq!(
        state.night_light,
        Some(NightLight {
            enabled: false,
            external: false
        })
    );
    assert_eq!(state.stay_awake, Some(Toggle { enabled: false }));
    assert_eq!(state.recorder, Some(Recorder { recording: false }));
    // A slow member keeps its prior value between slow reads.
    write(&f.path("night"), "true");
    write(&f.path("rec"), "");
    write(&f.path("sys/class/rfkill/rfkill0/soft"), "1\n");
    let next = collect(&f.env(), &probe(false), &state);
    assert_eq!(next.night_light, state.night_light);
    assert_eq!(next.recorder, Some(Recorder { recording: true }));
    assert_eq!(next.airplane, Some(Toggle { enabled: true }));
    assert!(
        collect(&f.env(), &probe(true), &next)
            .night_light
            .unwrap()
            .enabled
    );

    // Two internal panels cannot be mapped to the one backlight.
    let two = Probe {
        outputs: vec!["eDP-1".into(), "eDP-2".into()],
        ddc: false,
        slow: false,
    };
    assert!(collect(&f.env(), &two, &empty_state())
        .brightness
        .is_empty());
    // Missing backends leave their members out.
    std::fs::remove_dir_all(f.path("sys/class/leds")).unwrap();
    std::fs::remove_dir_all(f.path("sys/class/rfkill")).unwrap();
    f.without(&["wf-recorder", "systemd-inhibit"]);
    let bare = collect(&f.env(), &probe(false), &empty_state());
    assert!(bare.keyboard.is_none() && bare.airplane.is_none());
    assert!(bare.stay_awake.is_none() && bare.recorder.is_none());
}

#[test]
fn the_topic_is_produced_only_while_subscribed_and_follows_changes() {
    run(async {
        let f = Fixture::new();
        initial(&f.hub).await.unwrap();
        let task = tokio::task::spawn_local(serve(f.hub.clone(), f.env()));
        tokio::time::sleep(Duration::from_millis(150)).await;
        write(&f.path("rec"), "");
        tokio::time::sleep(Duration::from_millis(150)).await;
        let state = |f: &Fixture| -> ControlsState {
            serde_json::from_value(f.hub.borrow().snapshot(TOPIC).unwrap()).unwrap()
        };
        assert_eq!(
            state(&f).recorder,
            Some(Recorder { recording: false }),
            "no producer without a subscriber"
        );
        let (client, _) = f.hub.borrow_mut().add_client().unwrap();
        f.hub
            .borrow_mut()
            .subscribe(client, &[TOPIC.into()], false)
            .unwrap();
        let mut seen = false;
        for _ in 0..100 {
            if state(&f).recorder == Some(Recorder { recording: true }) {
                seen = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(seen, "a subscriber starts the producer");
        write(
            &f.path("sys/class/leds/tpacpi::kbd_backlight/brightness"),
            "2\n",
        );
        let mut moved = false;
        for _ in 0..200 {
            if state(&f).keyboard.map(|k| k.level) == Some(2) {
                moved = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(moved, "ticks pick up external changes");
        // A method forces an immediate re-read of the slow members.
        f.call("night_light.set", json!({"enabled":true}))
            .await
            .unwrap();
        let mut lit = false;
        for _ in 0..100 {
            if state(&f).night_light.map(|n| n.enabled) == Some(true) {
                lit = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(lit);
        f.hub.borrow_mut().remove_client(client);
        tokio::time::sleep(Duration::from_millis(100)).await;
        write(
            &f.path("sys/class/leds/tpacpi::kbd_backlight/brightness"),
            "0\n",
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            state(&f).keyboard.map(|k| k.level),
            Some(2),
            "stopped after the last subscriber"
        );
        task.abort();
    });
}
