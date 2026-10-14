use nacre_helper::Result;
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, process::Command};
#[test]
fn original_preview_cli_cases_run_with_empty_path() -> Result<()> {
    let home = tempfile::tempdir()?;
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/niri-preview-cli-reference.json"))?;
    for case in reference["cases"]
        .as_array()
        .ok_or("Missing parser cases")?
    {
        let args: Vec<String> = serde_json::from_value(case["args"].clone())?;
        let result = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .arg("niri-preview")
            .args(args)
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", home.path())
            .output()?;
        assert_eq!(
            result.status.code(),
            case["exit"].as_i64().map(|code| code as i32)
        );
        assert_eq!(String::from_utf8(result.stdout)?, case["stdout"]);
        assert_eq!(String::from_utf8(result.stderr)?, case["stderr"]);
    }
    Ok(())
}
#[test]
fn preview_launch_keeps_child_exit_and_cleans_runtime_without_owner_bus_access() -> Result<()> {
    let home = tempfile::tempdir()?;
    let destination = home.path().join("preview");
    fs::create_dir(&destination)?;
    fs::write(destination.join("manifest.json"), "{}")?;
    let outer = home.path().join("runtime");
    fs::create_dir(&outer)?;
    let bin = home.path().join("bin");
    fs::create_dir(&bin)?;
    let wrapper = bin.join("bwrap");
    fs::write(&wrapper,"#!/bin/sh\nprintf '%s\\n' \"$@\"\nprintf 'system=%s\\nsession=%s\\nnotify=%s\\n' \"$DBUS_SYSTEM_BUS_ADDRESS\" \"${DBUS_SESSION_BUS_ADDRESS-unset}\" \"${NOTIFY_SOCKET-unset}\"\nexit 17\n")?;
    fs::set_permissions(wrapper, fs::Permissions::from_mode(0o700))?;
    let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["niri-preview", "run", "--destination"])
        .arg(&destination)
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", &bin)
        .env("XDG_RUNTIME_DIR", &outer)
        .env("WAYLAND_DISPLAY", "wayland-fixture")
        .env("DBUS_SESSION_BUS_ADDRESS", "owner-session-sentinel")
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            "unix:path=/owner-system-sentinel",
        )
        .env("NOTIFY_SOCKET", "owner-notify-sentinel")
        .output()?;
    assert_eq!(output.status.code(), Some(17));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout)?;
    assert!(text.contains("--ro-bind\n/\n/\n"));
    assert!(text.contains("--unshare-pid\n"));
    assert!(text.contains("dbus-run-session\n--\nniri\n-c\n"));
    assert!(text.contains("absent-system-bus\nsession=unset\nnotify=unset\n"));
    for forbidden in [
        "owner-system-sentinel",
        "owner-session-sentinel",
        "owner-notify-sentinel",
        "--session",
        "python",
    ] {
        assert!(!text.contains(forbidden));
    }
    assert_eq!(fs::read_dir(&outer)?.count(), 0);
    Ok(())
}
#[test]
fn offline_preparation_preserves_explicit_permissions_and_excludes_credentials() -> Result<()> {
    let home = tempfile::tempdir()?;
    let repo = home.path().join("repo");
    let tools = repo.join("nacre/shell-tools");
    fs::create_dir_all(&tools)?;
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in [
        "compositor-contract.json",
        "compositor_backends/hyprland/panel-drag.lua",
        "compositor_backends/hyprland/session-cursor.lua",
        "compositor_backends/niri/palette.kdl.in",
    ] {
        let target = tools.join(name);
        fs::create_dir_all(target.parent().ok_or("Invalid fixture path")?)?;
        fs::copy(original.join("nacre/shell-tools").join(name), target)?;
    }
    fs::copy(
        env!("CARGO_BIN_EXE_nacre-helper"),
        tools.join("nacre-helper"),
    )?;
    for name in ["nacre-shell.service", "nacre-niri-widths.service"] {
        fs::write(tools.join(name), "fixture unit\n")?;
    }
    fs::create_dir_all(repo.join("nacre/shell"))?;
    fs::write(repo.join("nacre/shell/shell.qml"), "fixture shell\n")?;
    fs::create_dir_all(repo.join("niri"))?;
    fs::copy(
        original.join("niri/wallpaper.kdl"),
        repo.join("niri/wallpaper.kdl"),
    )?;
    fs::write(repo.join("VERSION"), "0.0.fixture\n")?;
    let config = home.path().join(".config/nacre");
    fs::create_dir_all(&config)?;
    let mut profile: Value =
        serde_json::from_slice(&fs::read(original.join("profiles/default.json"))?)?;
    profile["aiFullAccess"] = json!(true);
    let profile_bytes = serde_json::to_vec(&profile)?;
    fs::write(config.join("profile.json"), &profile_bytes)?;
    fs::write(config.join("auth.json"), "excluded fixture\n")?;
    let state = home.path().join(".local/state/nacre");
    fs::create_dir_all(&state)?;
    fs::write(
        state.join("scheme.json"),
        serde_json::to_vec(
            &json!({"colours":{"primary":"112233","secondary":"445566","outlineVariant":"778899","error":"aabbcc"}}),
        )?,
    )?;
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        let result = Command::new("/usr/bin/git")
            .args(args)
            .current_dir(&repo)
            .env_clear()
            .env("HOME", home.path())
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let destination = home.path().join("preview");
    let tools_path = home.path().join("only-git");
    fs::create_dir(&tools_path)?;
    std::os::unix::fs::symlink("/usr/bin/git", tools_path.join("git"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["niri-preview", "prepare", "--backend"])
        .arg(&repo)
        .arg("--destination")
        .arg(&destination)
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", &tools_path)
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}/absent-system", home.path().display()),
        )
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(config.join("profile.json"))?, profile_bytes);
    assert_eq!(
        fs::read(destination.join("home-view/.config/nacre/profile.json"))?,
        profile_bytes
    );
    assert!(!destination
        .join("home-view/.config/nacre/auth.json")
        .exists());
    let manifest: Value = serde_json::from_slice(&fs::read(destination.join("manifest.json"))?)?;
    assert_eq!(manifest["activated"], false);
    assert_eq!(manifest["mode"], "windowed-preview");
    assert_eq!(
        fs::metadata(destination.join("bin/pkexec"))?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(String::from_utf8(output.stdout)?.ends_with(
        "Prepared private Nacre/niri preview; original profile and runtime untouched.\n"
    ));
    Ok(())
}
