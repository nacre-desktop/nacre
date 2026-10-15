use std::{fs, os::unix::net::UnixListener, process::Command};

#[test]
fn environment_query_never_dumps_inherited_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = temp.path().join("runtime");
    fs::create_dir(&runtime).unwrap();
    let _socket = UnixListener::bind(runtime.join("wayland-test")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["shell-environment"])
        .env_clear()
        .env("HOME", temp.path())
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("NACRE_COMPOSITOR", "niri")
        .env("OPENAI_API_KEY", "synthetic-canary-do-not-emit")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("OPENAI_API_KEY"));
    assert!(!text.contains("synthetic-canary"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap(),
        serde_json::json!({"WAYLAND_DISPLAY":"wayland-test", "XDG_SESSION_TYPE":"wayland"})
    );
}
