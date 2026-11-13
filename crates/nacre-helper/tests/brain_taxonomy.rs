//! End-to-end checks for the Brain category resolver and writer.
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

#[test]
fn memory_topics_only_deduplicate_exact_category_slugs() {
    let cases = [
        (
            "AI and agents, Brain overlap",
            vec!["AI & agents"],
            vec!["Brain"],
            vec!["brain-overhaul"],
            vec!["brain-overhaul"],
            vec!["Brain"],
        ),
        (
            "Nacre with Brain topic",
            vec!["nacre"],
            vec![],
            vec!["Brain overhaul"],
            vec!["brain-overhaul"],
            vec![],
        ),
        (
            "Lead workflow with deploy",
            vec!["AI & agents"],
            vec!["Lead workflow"],
            vec!["deploy"],
            vec!["deploy"],
            vec!["Lead workflow"],
        ),
        (
            "Exact Brain duplicate",
            vec!["AI & agents"],
            vec!["Brain"],
            vec!["brain"],
            vec![],
            vec!["Brain"],
        ),
        (
            "Only exact duplicate removed",
            vec!["AI & agents"],
            vec!["Brain"],
            vec!["brain-overhaul", "brain", "deploy"],
            vec!["brain-overhaul", "deploy"],
            vec!["Brain"],
        ),
    ];
    for (index, (name, worlds, areas, topics, expected_topics, expected_areas)) in
        cases.into_iter().enumerate()
    {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let vault = root.path().join("vault");
        fs::create_dir_all(&home).unwrap();
        let mut args = vec![
            "nacre-ai-memory".to_owned(),
            "--title".to_owned(),
            format!("Topic case {index}"),
            "--source".to_owned(),
            "taxonomy parity fixture".to_owned(),
            "--kind".to_owned(),
            "checkpoints".to_owned(),
        ];
        for (flag, values) in [
            ("--world", &worlds),
            ("--area", &areas),
            ("--topic", &topics),
        ] {
            for value in values {
                args.extend([flag.to_owned(), (*value).to_owned()]);
            }
        }
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        let native = call(
            &home,
            &vault,
            "nacre-ai-memory",
            &refs[1..],
            "Synthetic topic deduplication fixture.",
        );
        assert!(
            native.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&native.stderr)
        );
        let native_json: serde_json::Value = serde_json::from_slice(&native.stdout).unwrap();
        assert_eq!(
            native_json["topics"],
            serde_json::json!(expected_topics),
            "{name}"
        );
        assert_eq!(
            native_json["areas"],
            serde_json::json!(expected_areas),
            "{name}"
        );
    }
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
