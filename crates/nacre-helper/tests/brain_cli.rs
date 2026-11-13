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

#[test]
fn git_cli_init_status_and_staged_guard_are_parsed_and_routed() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let vault = home.join("private");
    assert!(call(home, &vault, &["init"], "").status.success());
    for _ in 0..2 {
        let output = call(home, &vault, &["git", "init"], "");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let status = call(home, &vault, &["git", "status", "--json"], "");
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(value.get("pending_paths").is_some());
    let guard = call(home, &vault, &["guard", "--staged"], "");
    assert!(
        guard.status.success(),
        "{}",
        String::from_utf8_lossy(&guard.stderr)
    );
    assert!(!call(home, &vault, &["guard"], "").status.success());
}

#[test]
#[ignore = "spawned by the two-process batching acceptance test"]
fn brain_writer_child_process() {
    assert_eq!(
        std::env::var("NACRE_BRAIN_WRITER_CHILD").as_deref(),
        Ok("1")
    );
    for index in 0..10 {
        nacre_helper::brain_data::write_note(
            "inbox",
            &format!("child observation {index}"),
            "two-process acceptance fixture",
            "verified",
            &format!("Complete child note {index}."),
        )
        .unwrap();
    }
}

#[test]
fn two_process_writes_batch_without_lost_or_partial_notes() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path();
    let vault = home.join("private");
    let remote = home.join("brain.git");
    let init_remote = Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .arg(&remote)
        .output()
        .unwrap();
    assert!(init_remote.status.success());
    let initialized = call(
        home,
        &vault,
        &["git", "init", "--remote", remote.to_str().unwrap()],
        "",
    );
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );

    let executable = std::env::current_exe().unwrap();
    let children = (0..2)
        .map(|_| {
            Command::new(&executable)
                .args([
                    "--exact",
                    "brain_writer_child_process",
                    "--ignored",
                    "--nocapture",
                ])
                .env("HOME", home)
                .env("NACRE_BRAIN", &vault)
                .env("NACRE_BRAIN_WRITER_CHILD", "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let queue_path = vault.join(".brain-state/git-queue.json");
    let mut queue: serde_json::Value =
        serde_json::from_slice(&fs::read(&queue_path).unwrap()).unwrap();
    queue["updated"] = serde_json::json!(0);
    fs::write(&queue_path, serde_json::to_vec(&queue).unwrap()).unwrap();
    // Advance fixture mtimes past the sweep's active-writer safety window.
    let settled = std::time::SystemTime::now() - std::time::Duration::from_secs(11);
    // Classification also queues registry.json. Settle every queued path;
    // a faster classifier must not turn CPU time into an implicit sleep.
    for entry in queue["entries"].as_array().unwrap() {
        fs::File::open(vault.join(entry["path"].as_str().unwrap()))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(settled))
            .unwrap();
    }
    nacre_helper::brain_git::flush(&vault).unwrap();
    let mut queue: serde_json::Value =
        serde_json::from_slice(&fs::read(&queue_path).unwrap()).unwrap();
    queue["sweep_updated"] = serde_json::json!(0);
    fs::write(&queue_path, serde_json::to_vec(&queue).unwrap()).unwrap();
    nacre_helper::brain_git::flush(&vault).unwrap();

    let notes = fs::read_dir(vault.join("inbox"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "md")
        })
        .map(|entry| fs::read_to_string(entry.path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(notes.len(), 20);
    assert!(notes
        .iter()
        .all(|note| note.contains("Complete child note ")));
    let count = Command::new("git")
        .arg("-C")
        .arg(&vault)
        .args(["rev-list", "--count", "main"])
        .output()
        .unwrap();
    assert!(count.status.success());
    let commits_after_baseline = String::from_utf8_lossy(&count.stdout)
        .trim()
        .parse::<usize>()
        .unwrap()
        - 1;
    assert!(commits_after_baseline <= 2);
    let status: serde_json::Value =
        serde_json::from_str(&nacre_helper::brain_git::status_json(&vault).unwrap()).unwrap();
    assert!(status["last_push"].as_str().is_some());
    assert!(status["pending_paths"].as_array().unwrap().is_empty());
    assert!(status["error"].is_null());
    let remote_notes = Command::new("git")
        .arg("--git-dir")
        .arg(&remote)
        .args(["ls-tree", "--name-only", "main:inbox"])
        .output()
        .unwrap();
    assert!(remote_notes.status.success());
    assert_eq!(
        String::from_utf8_lossy(&remote_notes.stdout)
            .lines()
            .count(),
        20
    );
}
