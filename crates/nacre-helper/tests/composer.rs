//! Exercise the actual QML process boundary using private homes and fake tools.
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

#[test]
fn qml_request_completes_without_closing_stdin() {
    let home = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["composer", "load"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "{}", json!({"key":"early-typing"})).unwrap();
    input.flush().unwrap();
    let output = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        BufReader::new(output).read_line(&mut line).unwrap();
        let _ = tx.send(line);
    });
    let result = rx.recv_timeout(Duration::from_secs(5));
    // This exact PID belongs to this test, including on failure.
    let _ = child.kill();
    child.wait().unwrap();
    reader.join().unwrap();
    let result: Value = serde_json::from_str(&result.expect("QML stdin remained open")).unwrap();
    assert_eq!(result, json!({"text":"","attachments":[]}));
    drop(input);
}

#[test]
fn fake_area_capture_writes_into_a_private_file() {
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for (name, script) in [
        ("slurp", "#!/bin/sh\nprintf '0,0 10x10\\n'\n"),
        (
            "grim",
            "#!/bin/sh\n[ \"$1\" = -g ] && [ \"$2\" = '0,0 10x10' ] && [ \"$3\" = - ] || exit 2\n[ \"$(stat -Lc %a /proc/self/fd/1)\" = 600 ] || exit 3\nprintf fixture-image\n",
        ),
    ] {
        let path = bin.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["composer", "screenshot"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    let reply: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(reply.get("error").is_none(), "{reply}");
    let path = std::path::Path::new(reply["attachments"][0]["path"].as_str().unwrap());
    assert!(path.starts_with(home.path()));
    assert_eq!(fs::read(path).unwrap(), b"fixture-image");
    assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap().parent().unwrap())
            .unwrap()
            .mode()
            & 0o777,
        0o700
    );
}
