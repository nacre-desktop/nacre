//! Retained Brain CLI assertions from ai/tests/test_workflow.py.
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output, Stdio},
};

fn call(home: &Path, vault: &Path, args: &[&str], body: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .arg("nacre-brain")
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("NACRE_BRAIN", vault)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn concurrent_notes_are_distinct_private_and_validate_raw_metadata_and_body() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let vault = home.join("private");
    assert!(call(home, &vault, &["init"], "").status.success());
    let paths = std::thread::scope(|scope| {
        let jobs = (0..16)
            .map(|index| {
                let vault = &vault;
                scope.spawn(move || {
                    let output = call(
                        home,
                        vault,
                        &[
                            "note",
                            "--title",
                            "Concurrent fact",
                            "--source",
                            "test evidence",
                            "--confidence",
                            "verified",
                        ],
                        &format!("Observation {index}"),
                    );
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let path = String::from_utf8(output.stdout).unwrap().trim().to_owned();
                    assert!(fs::read_to_string(&path)
                        .unwrap()
                        .contains(&format!("Observation {index}")));
                    assert_eq!(
                        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                        0o600
                    );
                    path
                })
            })
            .collect::<Vec<_>>();
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<BTreeSet<_>>()
    });
    assert_eq!(paths.len(), 16);
    assert_eq!(
        fs::metadata(&vault).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for field in ["--title", "--source"] {
        let mut args = vec!["note", "--title", "Title", "--source", "Source"];
        let value = args.iter().position(|value| *value == field).unwrap() + 1;
        args[value] = "password: example-private-value";
        assert!(!call(home, &vault, &args, "Ordinary body").status.success());
    }
    assert!(!call(
        home,
        &vault,
        &["note", "--title", "Blocked", "--source", "test"],
        "refresh_token: test-private-value"
    )
    .status
    .success());
    assert_eq!(fs::read_dir(vault.join("inbox")).unwrap().count(), 16);
}
