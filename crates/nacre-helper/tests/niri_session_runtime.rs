use nacre_helper::Result;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    process::Command,
};
fn isolated(command: &mut Command, home: &Path, path: &Path) {
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", path)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-session", home.display()),
        )
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}/absent-system", home.display()),
        );
}
#[test]
fn original_cli_cases_run_with_an_empty_path() -> Result<()> {
    let home = tempfile::tempdir()?;
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/niri-session-runtime-cli-reference.json"
    ))?;
    for case in reference["cases"].as_array().ok_or("Missing cases")? {
        let args: Vec<String> = serde_json::from_value(case["arguments"].clone())?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
        command.arg("niri-session-runtime").args(args);
        isolated(&mut command, home.path(), home.path());
        let result = command.output()?;
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
fn compositor_exec_preserves_environment_and_code_only_namespace() -> Result<()> {
    let home = tempfile::tempdir()?;
    let shell = home
        .path()
        .join(".local/share/nacre/niri-on-pc/session/home-view/.local/share/nacre/shell");
    for name in [
        "source/shell.qml",
        "tools/nacre_native_compositor.py",
        "bin/qs",
    ] {
        let path = shell.join(name);
        fs::create_dir_all(path.parent().ok_or("Invalid fixture")?)?;
        fs::write(path, "")?;
    }
    let root = home.path().join(".local/share/nacre/niri-on-pc/session");
    fs::write(root.join("stock-niri"), "stock\n")?;
    let bins = home.path().join("bin");
    fs::create_dir(&bins)?;
    let wrapper = bins.join("bwrap");
    fs::write(&wrapper,"#!/bin/sh\nprintf '%s\\n' \"$@\"\nprintf 'socket=%s\\nhover=%s\\nconfig=%s\\n' \"$NIRI_SOCKET\" \"$NACRE_NIRI_HOVER_STREAM\" \"$NACRE_NIRI_CONFIG\"\nexit 23\n")?;
    fs::set_permissions(wrapper, fs::Permissions::from_mode(0o700))?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command.args(["niri-session-runtime", "compositor"]);
    isolated(&mut command, home.path(), &bins);
    command
        .env("NACRE_COMPOSITOR", "niri")
        .env("NIRI_SOCKET", "/private/fixture.sock");
    let result = command.output()?;
    assert_eq!(result.status.code(), Some(23));
    assert!(result.stderr.is_empty());
    let text = String::from_utf8(result.stdout)?;
    assert!(text.contains("--ro-bind\n"));
    assert!(text.contains("/usr/bin/niri\n--session\n"));
    assert!(text.contains("socket=/private/fixture.sock\nhover=0\n"));
    assert!(!text.contains("python"));
    assert!(!text.contains(".config/nacre\n"));
    Ok(())
}
#[test]
fn offline_stock_preparation_works_with_python_absent_from_path() -> Result<()> {
    if !Path::new("/usr/bin/niri").is_file() {
        eprintln!("Skipping offline stock preparation: /usr/bin/niri is not installed");
        return Ok(());
    }
    let home = tempfile::tempdir()?;
    let repo = home.path().join("repo");
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tools = repo.join("nacre/shell-tools");
    fs::create_dir_all(&tools)?;
    for name in [
        "compositor-contract.json",
        "compositor_backends/hyprland/panel-drag.lua",
        "compositor_backends/hyprland/session-cursor.lua",
        "compositor_backends/niri/palette.kdl.in",
    ] {
        let target = tools.join(name);
        fs::create_dir_all(target.parent().ok_or("Invalid fixture")?)?;
        fs::copy(original.join("nacre/shell-tools").join(name), target)?;
    }
    fs::copy(
        env!("CARGO_BIN_EXE_nacre-helper"),
        tools.join("nacre-helper"),
    )?;
    for name in ["nacre-shell.service", "nacre-niri-widths.service"] {
        fs::write(tools.join(name), "unit fixture\n")?;
    }
    fs::create_dir_all(repo.join("nacre/shell"))?;
    fs::write(repo.join("nacre/shell/shell.qml"), "shell fixture\n")?;
    fs::create_dir_all(repo.join("niri"))?;
    for name in ["wallpaper.kdl", "workspace-visitors.kdl"] {
        fs::copy(
            original.join("niri").join(name),
            repo.join("niri").join(name),
        )?;
    }
    fs::write(repo.join("VERSION"), "0.0.fixture\n")?;
    fs::create_dir_all(home.path().join(".config/nacre"))?;
    fs::copy(
        original.join("profiles/default.json"),
        home.path().join(".config/nacre/profile.json"),
    )?;
    fs::create_dir_all(home.path().join(".local/state/nacre"))?;
    fs::write(
        home.path().join(".local/state/nacre/scheme.json"),
        serde_json::to_vec(
            &json!({"colours":{"primary":"112233","secondary":"445566","outlineVariant":"778899","error":"aabbcc"}}),
        )?,
    )?;
    let bins = home.path().join("bin");
    fs::create_dir(&bins)?;
    symlink("/usr/bin/git", bins.join("git"))?;
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
        let mut git = Command::new("/usr/bin/git");
        git.args(args).current_dir(&repo);
        isolated(&mut git, home.path(), &bins);
        let result = git.output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .args([
            "niri-session-runtime",
            "prepare",
            "--stock-niri",
            "--backend",
        ])
        .arg(&repo);
    isolated(&mut command, home.path(), &bins);
    let result = command.output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let destination = home.path().join(".local/share/nacre/niri-on-pc/session");
    assert!(destination.join("source-revision").is_file());
    assert!(destination.join("stock-niri").is_file());
    let config = fs::read_to_string(destination.join("config/config.kdl"))?;
    assert_eq!(
        config
            .lines()
            .filter(|line| line.starts_with("include ") && line.contains("bead-pearls.kdl"))
            .count(),
        1
    );
    let window_rules = fs::read_to_string(destination.join("config/window-rules.kdl"))?;
    let pearls = fs::read_to_string(destination.join("config/bead-pearls.kdl"))?;
    let pearl_match = "match app-id=\"^bead[.]pearl([.]pending)?$\"";
    assert_eq!(pearls.matches(pearl_match).count(), 1);
    assert_eq!(
        window_rules.matches(pearl_match).count() + pearls.matches(pearl_match).count(),
        1
    );
    assert!(pearls.contains("default-floating-position x=0 y=0 relative-to=\"top-left\""));
    assert!(pearls.contains("open-focused false"));
    assert!(pearls.contains("border { off; }"));
    assert!(pearls.contains("focus-ring { off; }"));
    assert!(pearls.contains("shadow { off; }"));
    assert!(pearls.contains("match app-id=\"^bead[.]pearl[.]pending$\"\n    opacity 0.0\n"));
    assert!(pearls.contains("background-effect { blur false; }"));
    // N0103: the pearl and Widgets rules come after desktop.kdl's session-wide window rule.
    let include_at = |name: &str| {
        config
            .lines()
            .position(|line| line.starts_with("include ") && line.contains(name))
    };
    assert!(include_at("bead-pearls.kdl") > include_at("desktop.kdl"));
    assert_eq!(
        fs::read(destination.join("config/workspace-visitors.kdl"))?,
        fs::read(repo.join("niri/workspace-visitors.kdl"))?
    );
    assert!(destination
        .join("home-view/.local/share/nacre/shell/tools/nacre-helper")
        .is_file());
    assert_eq!(
        fs::metadata(destination.join("config/config.kdl"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(!home.path().join(".config/hypr").exists());
    assert!(String::from_utf8(result.stderr)?.contains("System keyboard discovery failed"));
    Ok(())
}
