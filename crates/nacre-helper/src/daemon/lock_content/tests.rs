//! Fake topic inputs -> the prepared files, with the helper's real renderer and fixture fonts.
use super::*;
use crate::daemon::hub::Hub;
use std::{collections::BTreeSet, fs, path::Path, rc::Rc};
use tokio::task::LocalSet;

const SECRET: &str = "launch-code-4711-secret-body";

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn options(debounce: u64) -> Options {
    Options {
        debounce: Duration::from_millis(debounce),
        tick: Duration::from_secs(3600),
        fixtures: Some((
            manifest().join("tests/fixtures/lock-fonts"),
            manifest().join("../../nacre/shell-tools"),
        )),
    }
}
fn palette() -> Value {
    let raw = fs::read(manifest().join("../../nacre/shell-tools/reference-style.json")).unwrap();
    serde_json::from_slice::<Value>(&raw).unwrap()["colours"].clone()
}
fn home() -> (tempfile::TempDir, Paths, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    let image = home.path().join("poster.png");
    image::RgbaImage::from_pixel(64, 36, image::Rgba([20, 90, 160, 255]))
        .save(&image)
        .unwrap();
    (home, paths, image)
}
fn publish_all(hub: &Shared, image: &Path, title: &str, contents: bool) {
    let mut hub = hub.borrow_mut();
    let wallpaper = json!({"available":true,"stale":false,"revision":1,"outputs":{},
        "current":{"path":image,"palette":{"mode":"dark","colours":palette()}}});
    hub.publish("wallpaper.active", wallpaper).unwrap();
    hub.publish("dashboard.player", json!({"active":"player-1","players":{"order":["player-1"],
        "items":{"player-1":{"identity":"Fixture","status":"playing","title":title,"artist":"Artist",
        "album":"Album","can_play_pause":true}}}})).unwrap();
    hub.publish("dashboard.weather", json!({"health":{"available":true,"stale":false},"current":{
        "location":"Oslo","wmo":61,"description":"Light rain","temperature":4.5,"feels_like":2.0,
        "high":6.0,"low":1.0}})).unwrap();
    hub.publish("notifications", json!({"history":{"order":["a","b"],"items":{
        "a":{"app_name":"Mail","summary":"Older","body":"older body","created_ms":1_800_000_000_000_u64},
        "b":{"app_name":"Chat","summary":"Newest","body":SECRET,"created_ms":1_800_000_100_000_u64}}},
        "active":{"order":[],"items":{}},"history_count":2})).unwrap();
    hub.publish("settings.store.desktop", json!({"exists":true,"error":null,"data":{
        "lockNotificationContents":contents}})).unwrap();
}
fn inputs(contents: bool) -> Inputs {
    let (_home, _paths, image) = home();
    let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
    publish_all(&hub, &image, "Song", contents);
    Inputs::read(&hub)
}
fn tree(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            tree(&path, out);
        } else {
            out.push(path);
        }
    }
}
fn leaked(root: &Path) -> bool {
    let mut files = Vec::new();
    tree(root, &mut files);
    files
        .iter()
        .any(|file| fs::read(file).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(SECRET)))
}
async fn eventually(mut check: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("condition not reached");
}
fn snapshot_title(paths: &Paths) -> Option<String> {
    let raw = fs::read(paths.cache.join("lock-ready/snapshot.json")).ok()?;
    serde_json::from_slice::<Value>(&raw).ok()?["media"]["title"]
        .as_str()
        .map(str::to_owned)
}

#[test]
fn presentation_has_the_qml_shape_from_topics() {
    let data = presentation(&inputs(true), &json!({}), "en");
    assert_eq!(data["colors"], palette());
    assert!(data["wallpaper"].as_str().unwrap().ends_with("poster.png"));
    assert_eq!(data["weather"]["temperature"], "5°C");
    assert_eq!(data["weather"]["detail"], "Feels like 2°C");
    assert_eq!(data["weather"]["range"], "High 6°C · Low 1°C");
    assert_eq!(data["weather"]["icon"], "rainy");
    assert_eq!(data["weather"]["description"], "Light rain");
    assert_eq!(data["weather"]["stale"], false);
    assert_eq!(data["media"]["title"], "Song");
    assert_eq!(data["media"]["playing"], true);
    assert_eq!(data["media"]["canToggle"], true);
    let rows = data["notifications"].as_array().unwrap();
    assert_eq!(rows[0]["app"], "Chat");
    assert_eq!(rows[0]["body"], SECRET);
    assert_eq!(rows[1]["app"], "Mail");
    assert_eq!(data["count"], 2);
    assert_eq!(data["preferences"]["lockNotificationContents"], true);
    assert_eq!(data["preferences"]["lockMedia"], true);
}

#[test]
fn stale_weather_is_marked_cached_and_fahrenheit_follows_the_setting() {
    let mut input = inputs(false);
    input.weather["health"]["stale"] = json!(true);
    input.desktop["weatherFahrenheit"] = json!(true);
    let data = presentation(&input, &json!({}), "en");
    assert_eq!(data["weather"]["temperature"], "41°F (cached)");
    assert_eq!(data["weather"]["stale"], true);
    // No topic data at all: honest, empty content rather than a crash.
    let empty = Inputs {
        wallpaper: Value::Null,
        player: Value::Null,
        weather: Value::Null,
        notices: Value::Null,
        desktop: json!({}),
        system: Value::Null,
    };
    let data = presentation(&empty, &json!({}), "en");
    assert_eq!(data["media"], Value::Null);
    assert_eq!(data["notifications"], json!([]));
    assert_eq!(data["weather"]["icon"], "cloud_off");
}

#[test]
fn notice_text_never_enters_the_presentation_when_contents_are_off() {
    let off = presentation(&inputs(false), &json!({}), "en");
    assert!(!off.to_string().contains(SECRET));
    assert!(!off.to_string().contains("older body"));
    assert_eq!(off["notifications"][0]["summary"], "");
    assert_eq!(off["notifications"][0]["app"], "Chat");
    assert_eq!(off["preferences"]["lockNotificationContents"], false);
    // A missing setting is the privacy-safe default.
    let mut missing = inputs(true);
    missing.desktop = json!({});
    assert!(!presentation(&missing, &json!({}), "en").to_string().contains(SECRET));
}

#[test]
fn prepared_files_follow_inputs_and_the_privacy_off_path_writes_no_bodies() {
    for contents in [false, true] {
        let (_home, paths, image) = home();
        let input = {
            let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
            publish_all(&hub, &image, "Song", contents);
            Inputs::read(&hub)
        };
        let key = prepare(&paths, &options(1), &input, None).unwrap().unwrap();
        // The same inputs do not render again.
        assert_eq!(prepare(&paths, &options(1), &input, Some(&key)).unwrap(), None);
        let ready = paths.cache.join("lock-ready");
        for name in ["weather", "media", "status", "notifications-safe", "notifications-private"] {
            assert!(ready.join(format!("{name}.txt")).is_file(), "{name}");
        }
        assert!(fs::read_to_string(ready.join("weather.txt")).unwrap().contains("5°C"));
        assert!(fs::read_to_string(ready.join("media.txt")).unwrap().contains("Song"));
        assert_eq!(leaked(&paths.cache), contents, "contents={contents}");
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(ready.join("snapshot.json")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn changes_are_debounced_and_followed() {
    let (_home, paths, image) = home();
    let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
    publish_all(&hub, &image, "first", false);
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(lock_content(hub.clone(), paths.clone(), options(250)));
            eventually(|| snapshot_title(&paths).as_deref() == Some("first")).await;
            // A burst of changes inside the quiet period renders only its last state.
            let seen = Rc::new(std::cell::RefCell::new(BTreeSet::new()));
            let watcher = {
                let (seen, paths) = (seen.clone(), paths.clone());
                tokio::task::spawn_local(async move {
                    loop {
                        if let Some(title) = snapshot_title(&paths) {
                            seen.borrow_mut().insert(title);
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
            };
            for title in ["burst-1", "burst-2", "burst-3", "burst-4"] {
                publish_all(&hub, &image, title, false);
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
            eventually(|| snapshot_title(&paths).as_deref() == Some("burst-4")).await;
            watcher.abort();
            let seen = seen.borrow().clone();
            assert!(
                seen.iter().all(|title| title == "first" || title == "burst-4"),
                "intermediate states were rendered: {seen:?}"
            );
            assert!(!leaked(&paths.cache));
            task.abort();
        })
        .await;
}
