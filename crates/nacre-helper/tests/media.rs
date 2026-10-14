//! The installed native media boundary needs neither Python nor the Orient CLI.
use nacre_helper::{json, orient};
use serde_json::{json as value, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use wait_timeout::ChildExt;

fn files(path: &Path) -> Vec<(String, Vec<u8>, u32)> {
    let mut rows: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
                entry.metadata().unwrap().permissions().mode() & 0o777,
            )
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}
fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .env("HOME", home)
        .env("PATH", "")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_STATE_HOME")
        .env_remove("XDG_CACHE_HOME");
    command
}
// The formula is the historical Orient CLI storage contract, not a call to
// Nacre's cache implementation. Each run uses its actual file metadata.
fn cache_path(poster: &Path, settings: &Value, folder: &Path) -> PathBuf {
    let meta = fs::metadata(poster).unwrap();
    let pin: Value = serde_json::from_str(include_str!(
        "../../../nacre/shell-cli/src/nacre_shell/engine-pin.json"
    ))
    .unwrap();
    let identity = value!([
        poster.canonicalize().unwrap(),
        meta.dev(),
        meta.ino(),
        meta.size(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
        settings,
        pin["engine"],
        pin["decoder"]
    ]);
    folder.join(format!(
        "rust-{:x}.json",
        Sha256::digest(serde_json::to_vec(&identity).unwrap())
    ))
}
fn check_media_cache(home: &Path, media: &Value) -> (PathBuf, Vec<u8>) {
    let poster = home
        .join(".cache/nacre/dashboard-media")
        .join(format!("{}.png", media["key"].as_str().unwrap()));
    let settings = orient_core::engine::settings(&value!({"mode":"dark"})).unwrap();
    let path = cache_path(&poster, &settings, &home.join(".cache/orient/palettes"));
    let bytes = fs::read(&path).unwrap();
    let data: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&data).unwrap());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(data["engine"], orient::engine_id());
    assert_eq!(data["input"], settings);
    for role in ["surface", "onSurface", "primary", "secondary", "tertiary"] {
        assert_eq!(
            media["palette"][role],
            format!("#{}", data["colours"][role].as_str().unwrap())
        );
    }
    (path, bytes)
}
#[test]
fn media_art_service_matches_cli_without_python_or_external_orient() {
    let home = tempfile::tempdir().unwrap();
    let art = home.path().join("art.ppm");
    fs::write(&art, "P3\n2 1\n255\n35 90 170 240 170 45\n").unwrap();
    let metadata = value!({"lyrics":false,"palette":true,"art":format!("file://{}",art.display()),"mode":"dark"});
    let cli = command(home.path())
        .args(["dashboard-media", &json::encode(&metadata).unwrap()])
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let actual: Value = serde_json::from_slice(&cli.stdout).unwrap();
    let palette = actual["palette"].as_object().unwrap();
    assert_eq!(palette.len(), 5);
    for role in ["surface", "onSurface", "primary", "secondary", "tertiary"] {
        assert!(palette[role]
            .as_str()
            .is_some_and(|v| v.len() == 7 && v.starts_with('#')));
    }
    let cache = home.path().join(".cache/nacre/dashboard-media");
    let before = files(&cache);
    let (previous_cache, previous_bytes) = check_media_cache(home.path(), &actual);
    assert_eq!(before.len(), 2);
    assert!(before.iter().all(|row| row.2 == 0o600));
    assert!(before
        .iter()
        .any(|row| row.0.ends_with(".png") && row.1.starts_with(b"\x89PNG\r\n\x1a\n")));
    fs::remove_dir_all(&cache).unwrap();
    let mut service = command(home.path())
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = value!({"id":1,"tool":"dashboard-media","arguments":[json::encode(&metadata).unwrap()],"input":null});
    writeln!(
        service.stdin.take().unwrap(),
        "{}",
        json::encode(&request).unwrap()
    )
    .unwrap();
    if service
        .wait_timeout(Duration::from_secs(20))
        .unwrap()
        .is_none()
    {
        service.kill().unwrap();
        service.wait().unwrap();
        panic!("Native media service failed to exit after EOF");
    }
    let output = service.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let packets: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0], value!({"ready":true,"version":1}));
    assert_eq!(packets[1]["id"], 1);
    assert_eq!(packets[1]["code"], 0);
    assert_eq!(packets[1]["output"], String::from_utf8(cli.stdout).unwrap());
    assert_eq!(files(&cache), before);
    check_media_cache(home.path(), &actual);
    assert_eq!(fs::read(previous_cache).unwrap(), previous_bytes);
}

#[test]
fn existing_valid_orient_cache_is_read_and_invalid_entries_are_replaced() {
    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("image.ppm");
    fs::write(&input, "P3\n2 1\n255\n35 90 170 240 170 45\n").unwrap();
    let poster = folder.path().join("poster.png");
    fs::write(&poster, orient_decoder::artwork_png(&input).unwrap()).unwrap();
    let options = value!({"mode":"dark","cache_dir":folder.path().join("palettes")});
    let fresh = orient::from_image(&poster, &options).unwrap();
    let path = cache_path(&poster, &fresh["input"], &folder.path().join("palettes"));
    let mut cached = fresh.clone();
    cached["fromCache"] = value!("existing valid entry");
    let bytes = serde_json::to_vec(&cached).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert_eq!(orient::from_image(&poster, &options).unwrap(), cached);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    for invalid in [
        value!({}),
        value!({"engine":"old"}),
        value!({"formatVersion":-1}),
        value!({"name":"wrong"}),
        value!({"input":{}}),
        value!({"colours":{"primary":"invalid"}}),
    ] {
        let mut entry = cached.clone();
        if let Some(properties) = invalid.as_object() {
            if properties.is_empty() {
                entry = invalid;
            } else {
                for (key, value) in properties {
                    entry[key] = value.clone();
                }
            }
        }
        fs::write(&path, serde_json::to_vec(&entry).unwrap()).unwrap();
        assert_eq!(orient::from_image(&poster, &options).unwrap(), fresh);
        assert_eq!(
            fs::read(&path).unwrap(),
            serde_json::to_vec(&fresh).unwrap()
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    fs::write(&path, "not json").unwrap();
    assert_eq!(orient::from_image(&poster, &options).unwrap(), fresh);
    assert_eq!(
        fs::read(&path).unwrap(),
        serde_json::to_vec(&fresh).unwrap()
    );
}
