//! End-to-end checks for the Brain category resolver and writer parity.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn helper() -> &'static str {
    env!("CARGO_BIN_EXE_nacre-helper")
}

fn call(home: &Path, vault: &Path, tool: &str, args: &[&str], body: &str) -> Output {
    let mut command = Command::new(helper());
    command
        .arg(tool)
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("NACRE_BRAIN", vault);
    if tool == "nacre-ai-memory" {
        command.env("TZ", "UTC");
    }
    let mut child = command
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

fn note_path(output: &Output) -> PathBuf {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    PathBuf::from(text.trim())
}

fn fields(note: &str) -> Vec<String> {
    let header = nacre_helper::brain_discovery::header_fields(note);
    ["worlds", "areas", "topics", "review"]
        .into_iter()
        .filter_map(|key| {
            header
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(|value| {
                    let label = match key {
                        "worlds" => "Worlds",
                        "areas" => "Areas",
                        "topics" => "Topics",
                        _ => "Review",
                    };
                    format!("{label}: {value}")
                })
        })
        .collect()
}

#[test]
fn python_memory_and_native_note_share_canonical_headers() {
    let root = tempfile::tempdir().unwrap();
    let native_home = root.path().join("native-home");
    let python_home = root.path().join("python-home");
    fs::create_dir_all(&native_home).unwrap();
    fs::create_dir_all(&python_home).unwrap();
    let native_vault = native_home.join("brain");
    let python_vault = python_home.join("brain");
    let native = call(
        &native_home,
        &native_vault,
        "nacre-brain",
        &[
            "note",
            "--title",
            "Desktop compositor notes",
            "--source",
            "taxonomy parity fixture",
            "--world",
            "desktop",
            "--topic",
            "QML Runtime",
        ],
        "A synthetic note about Nacre compositors.",
    );
    let native_note = fs::read_to_string(note_path(&native)).unwrap();

    let writer = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bin/nacre-ai-memory.py.rollback"
    );
    let python = Command::new("python3")
        .args(["-c", "import argparse, importlib.machinery, importlib.util, json, sys; loader=importlib.machinery.SourceFileLoader('memory_writer', sys.argv[1]); spec=importlib.util.spec_from_loader(loader.name, loader); mod=importlib.util.module_from_spec(spec); loader.exec_module(mod); args=argparse.Namespace(title='Desktop compositor notes', source='taxonomy parity fixture', kind='inbox', confidence='reported', world=['desktop'], area=[], topic=['QML Runtime']); print(json.dumps(mod.remember(args, sys.stdin.read())))", writer])
        .env_clear()
        .env("HOME", &python_home)
        .env("NACRE_BRAIN", &python_vault)
        .env("NACRE_HELPER_BINARY", helper())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map(|mut child| {
            child.stdin.take().unwrap().write_all(b"A synthetic note about Nacre compositors.").unwrap();
            child.wait_with_output().unwrap()
        })
        .unwrap();
    assert!(
        python.status.success(),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&python.stdout).unwrap();
    let python_note = fs::read_to_string(result["path"].as_str().unwrap()).unwrap();
    assert_eq!(fields(&native_note), fields(&python_note));
    assert!(native_note.contains("Worlds: Nacre"));
    assert!(native_note.contains("Areas: Desktop"));
}

#[test]
fn invalid_category_shaped_inputs_do_not_create_junk_registry_entries() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let vault = home.join("brain");
    let invalid = [
        "Topics:",
        "/home/person/.config/nacre desktop setup",
        "https://example.invalid/Bead",
    ];
    for (index, value) in invalid.iter().enumerate() {
        let output = call(
            home,
            &vault,
            "nacre-brain",
            &[
                "note",
                "--title",
                &format!("Synthetic invalid {index}"),
                "--source",
                "taxonomy fixture",
                "--world",
                value,
            ],
            "Ordinary synthetic body with no matching category.",
        );
        // Invalid labels are rejected or routed to review, never serialized as a world.
        if output.status.success() {
            let path = note_path(&output);
            let note = fs::read_to_string(path).unwrap();
            assert!(!note.lines().any(|line| line == format!("Worlds: {value}")));
        }
    }
    let registry = fs::read_to_string(vault.join("registry.json")).unwrap();
    for junk in invalid {
        assert!(
            !registry.contains(junk),
            "registry contains invalid label {junk:?}"
        );
    }
}

#[test]
fn area_aliases_passed_as_worlds_supply_their_parent_world() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let vault = home.join("brain");
    for (value, expected_world, expected_area) in [
        ("Bead green", "Nacre", "Bead"),
        ("desktop", "Nacre", "Desktop"),
        ("dev-1", "Infrastructure", "dev-1"),
    ] {
        let output = call(
            home,
            &vault,
            "nacre-brain",
            &[
                "note",
                "--title",
                "Synthetic area alias",
                "--source",
                "taxonomy fixture",
                "--world",
                value,
            ],
            "Synthetic body with no unrelated subject.",
        );
        let note = fs::read_to_string(note_path(&output)).unwrap();
        assert!(
            note.lines()
                .any(|line| line == format!("Worlds: {expected_world}")),
            "{note}"
        );
        assert!(
            note.lines()
                .any(|line| line == format!("Areas: {expected_area}")),
            "{note}"
        );
        assert!(!note.lines().any(|line| line == "Worlds: "));
    }
}

#[test]
fn title_mention_classifies_nacre_compositors() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let vault = home.join("brain");
    let output = call(
        home,
        &vault,
        "nacre-brain",
        &[
            "note",
            "--title",
            "Nacre compositors",
            "--source",
            "taxonomy fixture",
        ],
        "A synthetic note about the compositor stack.",
    );
    let note = fs::read_to_string(note_path(&output)).unwrap();
    assert!(note.lines().any(|line| line == "Worlds: Nacre"), "{note}");
    assert!(
        note.lines().any(|line| line == "Areas: Compositors"),
        "{note}"
    );
}

#[test]
fn concurrent_explicit_area_creation_has_one_registry_entry() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let vault = home.join("brain");
    let mut children = Vec::new();
    for index in 0..2 {
        children.push(
            Command::new(helper())
                .args([
                    "nacre-brain",
                    "note",
                    "--title",
                    if index == 0 {
                        "Concurrent area one"
                    } else {
                        "Concurrent area two"
                    },
                    "--source",
                    "taxonomy race fixture",
                    "--world",
                    "Nacre",
                    "--area",
                    "Codex Sandbox",
                ])
                .env_clear()
                .env("HOME", home)
                .env("NACRE_BRAIN", &vault)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"Synthetic concurrent category creation.")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let registry: serde_json::Value =
        serde_json::from_slice(&fs::read(vault.join("registry.json")).unwrap()).unwrap();
    let entries = registry["areas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|area| area["name"] == "Codex Sandbox")
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "{registry}");
    assert_eq!(entries[0]["status"], "auto");
    assert_eq!(entries[0]["parent"], "Nacre");
}
