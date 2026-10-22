//! Retain the historical repeated real-helper CLI/service equivalence test.
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use wait_timeout::ChildExt;
struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .env("HOME", home)
        .env("PATH", "")
        .env(
            "NACRE_HELPER_TOOLS",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
        )
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_CACHE_HOME")
        .env_remove("XDG_STATE_HOME");
    command
}
#[test]
fn cli_and_service_results_match_across_repeated_native_operations() {
    let home = tempfile::tempdir().unwrap();
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let mut worker = Worker(
        command(home.path())
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let pid = worker.0.id();
    let stdout = worker.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            sender
                .send(serde_json::from_str::<Value>(&line).unwrap())
                .unwrap();
        }
    });
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
        json!({"ready":true,"version":1})
    );
    let operations = [
        ("launcher-preferences", vec!["load"], Value::Null),
        (
            "launcher-preferences",
            vec!["favorite"],
            json!({"id":"fixture.desktop","enabled":true}),
        ),
        (
            "notification-history",
            vec!["save"],
            json!([{"key":"fixture","appName":"fixture","summary":"test"}]),
        ),
        ("notification-history", vec!["load"], Value::Null),
        ("dashboard-data", vec!["read"], Value::Null),
        (
            "dashboard-data",
            vec!["preferences", r#"{"focusMinutes":30}"#],
            Value::Null,
        ),
        ("menu-backend", vec!["convert", "invalid"], Value::Null),
    ];
    for (index, (tool, args, input)) in operations.iter().enumerate() {
        let id = index + 1;
        writeln!(
            worker.0.stdin.as_mut().unwrap(),
            "{}",
            json!({"id":id,"tool":tool,"arguments":args,"input":input})
        )
        .unwrap();
        worker.0.stdin.as_mut().unwrap().flush().unwrap();
        let reply = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(reply["id"], id);
        assert_eq!(reply["code"], 0, "{reply}");
        let mut cli = command(home.path())
            .arg(tool)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        writeln!(cli.stdin.take().unwrap(), "{input}").unwrap();
        if cli.wait_timeout(Duration::from_secs(10)).unwrap().is_none() {
            cli.kill().unwrap();
            cli.wait().unwrap();
            panic!("CLI operation timed out");
        }
        let output = cli.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if output.stdout.is_empty() {
            assert_eq!(reply["output"], "");
        } else {
            let mut expected: Value = serde_json::from_slice(&output.stdout).unwrap();
            let mut actual: Value =
                serde_json::from_str(reply["output"].as_str().unwrap()).unwrap();
            // This is the same one timestamp exclusion as the historical test.
            if let (Some(expected), Some(actual)) =
                (expected.as_object_mut(), actual.as_object_mut())
            {
                expected.remove("checked");
                actual.remove("checked");
            }
            assert_eq!(actual, expected);
        }
        assert_eq!(worker.0.id(), pid);
    }
    assert!(home.path().join(".config/nacre/launcher.json").is_file());
    drop(worker.0.stdin.take());
    assert!(worker
        .0
        .wait_timeout(Duration::from_secs(10))
        .unwrap()
        .is_some());
    assert!(worker.0.wait().unwrap().success());
    reader.join().unwrap();
}
