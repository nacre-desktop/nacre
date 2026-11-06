//! Synthetic-vault acceptance tests for the native Brain migration command.
//! All note text here is invented fixture data; no owner vault is read.
use sha2::{Digest, Sha256};
use std::os::unix::fs::OpenOptionsExt;
use std::process::Stdio;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};

// The owner-approved Nacre world page; split so public-name checks stay exact.
const NACRE_WORLD_PAGE: &str = concat!("wiki/siver", "teh-ai.md");

const STEPS: &[&str] = &[
    "meta", "inbox", "archive", "digests", "wiki", "links", "index",
];

fn helper() -> &'static str {
    env!("CARGO_BIN_EXE_nacre-helper")
}

fn call(home: &Path, vault: &Path, step: &str, mode: &str, report: Option<&Path>) -> Output {
    let mut cmd = Command::new(helper());
    cmd.arg("nacre-brain")
        .arg("migrate")
        .arg("--step")
        .arg(step)
        .arg(mode)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("NACRE_BRAIN", vault);
    if let Some(path) = report {
        cmd.arg("--report").arg(path);
    }
    cmd.output().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "siverteh")
        .env(
            "GIT_AUTHOR_EMAIL",
            "92440237+Siverteh@users.noreply.github.com",
        )
        .env("GIT_COMMITTER_NAME", "siverteh")
        .env(
            "GIT_COMMITTER_EMAIL",
            "92440237+Siverteh@users.noreply.github.com",
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn remote(vault: &Path) {
    let bare = vault.with_extension("git");
    let out = Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git init --bare: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let url = bare.to_string_lossy();
    let has = Command::new("git")
        .arg("-C")
        .arg(vault)
        .args(["remote", "get-url", "origin"])
        .output()
        .unwrap();
    if has.status.success() {
        git(vault, &["remote", "set-url", "origin", &url]);
    } else {
        git(vault, &["remote", "add", "origin", &url]);
    }
    git(vault, &["push", "-u", "origin", "main"]);
}

fn fixture(root: &Path, count: usize) {
    fs::create_dir_all(root).unwrap();
    for dir in [
        "inbox",
        "checkpoints",
        "decisions",
        "runbooks",
        "projects",
        "devices",
        "personal",
        "infrastructure",
        "wiki",
        "raw",
        ".brain-state",
        "credentials",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(root.join("wiki/entity-nacre-desktop.md"), "# Desktop\n\nEntity: area\nName: Desktop\nParent: Nacre\nReviewed: 2026-01-01\nStatus: curated\n\n## Current facts\n\n- Synthetic current fact must survive migration. (confirmed 2026-01-01; [[devices/fixture-0005]])\n").unwrap();
    fs::write(root.join(NACRE_WORLD_PAGE), "# Nacre\n\nEntity: world\nName: Nacre\nReviewed: 2026-01-01\nStatus: curated\n\n## Current facts\n\n- Synthetic world fact must survive migration.\n").unwrap();
    // This decoy's bytes and inode timestamps must survive, and credentials must not
    // become part of the migration's input set.
    fs::write(
        root.join("credentials/decoy.txt"),
        b"synthetic credential decoy\n",
    )
    .unwrap();
    // Match the audit's shape: checkpoints dominate, inbox is about 14%,
    // and every other evidence layer has synthetic notes of its own.
    let targets = [
        "inbox",
        "checkpoints",
        "decisions",
        "runbooks",
        "projects",
        "devices",
        "raw",
        "personal",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "checkpoints",
        "inbox",
        "inbox",
        "inbox",
        "infrastructure",
        "projects",
    ];
    let mut without_worlds = 0;
    for i in 0..count {
        let folder = targets[i % targets.len()];
        let (title, weird) = match i % 25 {
            0 => ("Desktop compositor checkpoint", "Worlds:\nTopics:\n"),
            1 => ("Bead green project note", "Worlds: Bead green\n"),
            2 => (
                "Synthetic device reference",
                "Worlds: The project lives at /home/synthetic/project and needs review\n",
            ),
            3 => ("Synthetic note without categories", ""),
            4 => ("Synthetic old checkpoint", "Worlds: Nacre\n"),
            5 => (
                "Synthetic decision record",
                "Worlds: Nacre\nAreas: Helper\n",
            ),
            22 => ("Synthetic Nacre preference", "Worlds: Nacre\n"),
            23 => ("Desktop compositor reference", "Worlds: desktop\n"),
            24 => ("Synthetic runbook", "Worlds: Infrastructure\n"),
            6 if i <= 8 => (
                "Synthetic novel subject",
                "Worlds: Nacre\nTopics: Photon_Garden\nReview: low-confidence\n",
            ),
            7 if i <= 8 => (
                "Synthetic novel subject",
                "Worlds: Nacre\nTopics: photon-garden, PHOTON GARDEN\nReview: low-confidence\n",
            ),
            8 if i <= 8 => (
                "Synthetic novel subject",
                "Worlds: Nacre\nTopics: PHOTON GARDEN\nReview: low-confidence\n",
            ),
            _ => ("Synthetic note without categories", ""),
        };
        let file_name = if folder == "checkpoints" || folder == "inbox" {
            format!("202001{:02}-fixture-{i:04}.md", i % 28 + 1)
        } else {
            format!("fixture-{i:04}.md")
        };
        let path = root.join(folder).join(&file_name);
        let link = if i > 0 {
            let previous = i - 1;
            let previous_folder = targets[previous % targets.len()];
            let previous_name = if previous_folder == "checkpoints" || previous_folder == "inbox" {
                format!("202001{:02}-fixture-{previous:04}.md", previous % 28 + 1)
            } else {
                format!("fixture-{previous:04}.md")
            };
            let md = if previous_folder == folder {
                previous_name.clone()
            } else {
                format!("../{previous_folder}/{previous_name}")
            };
            format!("\nSee [[{previous_folder}/{previous_name}]] and [related]({md}).")
        } else {
            String::new()
        };
        let title_break = if i % 2 == 0 { "\n" } else { "\n\n" };
        let body_prefix = if i == 42 {
            "Project: /home/synthetic/body-first-line\n"
        } else {
            ""
        };
        let recorded = if i == 8 { "2020-01-02" } else { "2020-01-01" };
        let text = format!("# {title} {i}{title_break}Recorded: {recorded}\nConfidence: reported\nSource: synthetic-fixture\n{weird}Project: /home/synthetic/example\n\n{body_prefix}Synthetic body {i}; body checksum fixture.{link}\nProject: /home/synthetic/body-path\n");
        let text = if i == 3 {
            text.split_once('\n').unwrap().1.to_owned()
        } else {
            text
        };
        without_worlds += usize::from(!text.lines().any(|line| line.starts_with("Worlds:")));
        fs::write(path, text).unwrap();
    }
    if count == 2_000 {
        assert!(
            (1_350..=1_370).contains(&without_worlds),
            "fixture must keep about 68% of notes without a Worlds line"
        );
    }
    // Capture and dedup state deliberately refer to a note that may move.
    let capture_path = root.join("inbox/20200101-fixture-0000.md");
    let capture = serde_json::json!({"path":capture_path.to_string_lossy()});
    fs::write(
        root.join(".brain-state/capture-000.json"),
        serde_json::to_vec(&capture).unwrap(),
    )
    .unwrap();
    fs::write(
        root.join(".brain-state/dedup.json"),
        br#"{"source":"inbox/20200101-fixture-0000.md"}"#,
    )
    .unwrap();
    fs::write(root.join(".brain-state/lock"), b"").unwrap();
    fs::write(root.join(".brain-state/capture.lock"), b"").unwrap();
    fs::write(root.join(".gitignore"), ".brain-state/\ncredentials/\n").unwrap();
    fs::write(root.join("wiki/unregistered.md"),"# Unregistered synthetic wiki\n\nSource: synthetic-fixture\n\nCurated unregistered text stays byte-identical.  \n").unwrap();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.name", "siverteh"]);
    git(
        root,
        &[
            "config",
            "user.email",
            "92440237+Siverteh@users.noreply.github.com",
        ],
    );
    git(
        root,
        &[
            "add",
            "inbox",
            "checkpoints",
            "decisions",
            "runbooks",
            "projects",
            "devices",
            "personal",
            "infrastructure",
            "wiki",
            "raw",
            ".gitignore",
        ],
    );
    git(root, &["commit", "-m", "fixture [no-publish]"]);
    remote(root);
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    fn walk(root: &Path, here: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(here).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if e.file_type().unwrap().is_dir() {
                walk(root, &p, out);
            } else if e.file_type().unwrap().is_file() {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                if rel != ".brain-state/git-status.json" && rel != ".brain-state/git-queue.json" {
                    let mut bytes = fs::read(p).unwrap();
                    if rel == ".brain-state/capture-000.json" {
                        let normalized = String::from_utf8_lossy(&bytes)
                            .replace(&root.to_string_lossy().to_string(), "<VAULT>");
                        bytes = normalized.into_bytes();
                    }
                    out.insert(rel, bytes);
                }
            }
        }
    }
    walk(root, root, &mut files);
    files
}

fn normalized_body(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let marker = text.find("Synthetic body ").expect("fixture body marker");
    let start = text[..marker].rfind("\n\n").map(|i| i + 2).unwrap_or(0);
    let raw = &text[start..];
    let wiki = regex::Regex::new(r"\[\[[^\]]+\]\]")
        .unwrap()
        .replace_all(raw, "[[LINK]]");
    regex::Regex::new(r"\]\([^)]+\)")
        .unwrap()
        .replace_all(&wiki, "](LINK)")
        .into_owned()
}

fn body_hashes(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    files
        .iter()
        .filter(|(p, _)| {
            p.ends_with(".md")
                && !p.starts_with("wiki/")
                && !p.starts_with("digests/")
                && *p != "INDEX.md"
        })
        .flat_map(|(_, bytes)| {
            let text = String::from_utf8_lossy(bytes);
            text.split_once("Synthetic body ")
                .and_then(|(_, tail)| tail.split_once(';').map(|(n, _)| n.to_owned()))
                .map(|n| {
                    (
                        n,
                        format!("{:x}", Sha256::digest(normalized_body(bytes).as_bytes())),
                    )
                })
        })
        .collect()
}

fn normalized_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn copy_tree(src: &Path, dst: &Path) {
    for e in fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let from = e.path();
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            fs::create_dir_all(&to).unwrap();
            copy_tree(&from, &to);
        } else if e.file_type().unwrap().is_file() {
            fs::copy(from, to).unwrap();
        }
    }
}

fn success(out: Output) {
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn synthetic_vault_dry_run_then_all_is_idempotent_and_preserves_content() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let seed = temp.path().join("seed");
    fixture(&seed, 2_000);
    let sequential = temp.path().join("sequential");
    let together = temp.path().join("together");
    fs::create_dir_all(&sequential).unwrap();
    copy_tree(&seed, &sequential);
    fs::create_dir_all(&together).unwrap();
    copy_tree(&seed, &together);
    remote(&sequential);
    remote(&together);
    let report = temp.path().join("dry-run.md");
    let before = snapshot(&sequential);
    let original_body_hashes = body_hashes(&before);
    let credentials = sequential.join("credentials/decoy.txt");
    let credentials_dir = sequential.join("credentials");
    let touch = Command::new("touch")
        .args(["-a", "-d", "@1"])
        .arg(&credentials)
        .arg(&credentials_dir)
        .status()
        .unwrap();
    assert!(
        touch.success(),
        "failed to set old synthetic credential atimes"
    );
    let file_dry_before = fs::metadata(&credentials).unwrap();
    let dir_dry_before = fs::metadata(&credentials_dir).unwrap();
    success(call(&home, &sequential, "all", "--dry-run", Some(&report)));
    let file_dry_after = fs::metadata(&credentials).unwrap();
    let dir_dry_after = fs::metadata(&credentials_dir).unwrap();
    assert_eq!(
        (file_dry_before.atime(), file_dry_before.atime_nsec()),
        (file_dry_after.atime(), file_dry_after.atime_nsec()),
        "dry-run read credentials/decoy.txt"
    );
    assert_eq!(
        (dir_dry_before.atime(), dir_dry_before.atime_nsec()),
        (dir_dry_after.atime(), dir_dry_after.atime_nsec()),
        "dry-run enumerated credentials/"
    );
    assert_eq!(snapshot(&sequential), before, "dry-run changed the vault");
    let report_text = fs::read_to_string(report).unwrap();
    assert!(report_text.contains("Sample headers"));
    assert!(report_text.contains("Review flags"));

    let touch = Command::new("touch")
        .args(["-a", "-d", "@1"])
        .arg(&credentials)
        .arg(&credentials_dir)
        .status()
        .unwrap();
    assert!(
        touch.success(),
        "failed to set old synthetic credential atime"
    );
    let meta_before = fs::metadata(&credentials).unwrap();
    let credentials_dir_before = fs::metadata(&credentials_dir).unwrap();
    let start = Instant::now();
    for step in STEPS {
        success(call(&home, &sequential, step, "--apply", None));
    }
    eprintln!(
        "brain-migrate synthetic sequential 2,000-note run: {:.3}s",
        start.elapsed().as_secs_f64()
    );
    assert!(
        start.elapsed() < Duration::from_secs(60),
        "migration of 2,000 synthetic notes took {:?}",
        start.elapsed()
    );
    let meta_after = fs::metadata(&credentials).unwrap();
    assert_eq!(meta_before.ino(), meta_after.ino());
    assert_eq!(meta_before.atime(), meta_after.atime());
    assert_eq!(meta_before.atime_nsec(), meta_after.atime_nsec());
    assert_eq!(
        credentials_dir_before.ino(),
        fs::metadata(&credentials_dir).unwrap().ino()
    );
    let credentials_dir_after = fs::metadata(&credentials_dir).unwrap();
    assert_eq!(
        credentials_dir_before.atime(),
        credentials_dir_after.atime(),
        "apply enumerated credentials/"
    );
    assert_eq!(
        credentials_dir_before.atime_nsec(),
        credentials_dir_after.atime_nsec(),
        "apply enumerated credentials/"
    );
    assert_eq!(
        fs::read(&credentials).unwrap(),
        b"synthetic credential decoy\n"
    );

    let all_start = Instant::now();
    success(call(&home, &together, "all", "--apply", None));
    eprintln!(
        "brain-migrate synthetic --step all 2,000-note run: {:.3}s",
        all_start.elapsed().as_secs_f64()
    );
    assert!(
        all_start.elapsed() < Duration::from_secs(60),
        "all migration took {:?}",
        all_start.elapsed()
    );
    assert_eq!(
        snapshot(&sequential),
        snapshot(&together),
        "one-step-at-a-time tree differs from --step all"
    );
    let head_before = Command::new("git")
        .arg("-C")
        .arg(&together)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let stable = snapshot(&together);
    success(call(&home, &together, "all", "--apply", None));
    assert_eq!(stable, snapshot(&together), "rerun was not idempotent");
    let head_after = Command::new("git")
        .arg("-C")
        .arg(&together)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        head_before.stdout, head_after.stdout,
        "idempotent rerun created another commit"
    );

    // Each original payload marker must occur after migration, including moved notes.
    let resulting = snapshot(&together);
    let unknown_wiki = String::from_utf8_lossy(&resulting["wiki/unregistered.md"]);
    assert!(unknown_wiki.contains("Review:") || unknown_wiki.contains("Worlds:"));
    assert!(String::from_utf8_lossy(&resulting["wiki/unregistered.md"])
        .ends_with("Curated unregistered text stays byte-identical.  \n"));
    let registry: serde_json::Value = serde_json::from_slice(&resulting["registry.json"]).unwrap();
    assert!(
        registry["areas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|area| area["name"] == "Photon Garden" && area["status"] == "proposed"),
        "qualifying Review cluster was not proposed"
    );

    assert!(
        String::from_utf8_lossy(&resulting["wiki/entity-nacre-desktop.md"])
            .contains("Synthetic current fact must survive migration")
    );
    assert!(String::from_utf8_lossy(&resulting[NACRE_WORLD_PAGE])
        .contains("Synthetic world fact must survive migration"));
    assert!(
        resulting.values().any(|b| String::from_utf8_lossy(b)
            .contains("\n\nProject: /home/synthetic/body-first-line\nSynthetic body 42")),
        "body-leading Project line was not preserved byte-for-byte"
    );
    for i in 0..2_000 {
        let marker = format!("Synthetic body {i};");
        assert!(
            resulting
                .values()
                .any(|b| String::from_utf8_lossy(b).contains(&marker)),
            "body missing for fixture note {i}"
        );
    }
    assert_eq!(
        body_hashes(&resulting),
        original_body_hashes,
        "note body checksum changed outside rewritten link targets"
    );
    let vocabulary: serde_json::Value =
        serde_json::from_str(include_str!("../src/brain-migrate-vocabulary.json")).unwrap();
    let canonical_worlds: std::collections::BTreeSet<_> = vocabulary["worlds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w["name"].as_str())
        .collect();
    for (path, bytes) in resulting.iter().filter(|(path, _)| {
        path.ends_with(".md")
            && !path.starts_with("wiki/")
            && !path.starts_with("digests/")
            && *path != "INDEX.md"
    }) {
        let text = String::from_utf8_lossy(bytes);
        let worlds: Vec<_> = text
            .lines()
            .filter_map(|line| line.strip_prefix("Worlds:"))
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .collect();
        let has_world = !worlds.is_empty();
        assert!(
            worlds.iter().all(|w| canonical_worlds.contains(w)),
            "non-canonical world in {path}: {worlds:?}"
        );
        let has_review = text
            .lines()
            .any(|line| line.starts_with("Review:") && !line["Review:".len()..].trim().is_empty());
        assert!(
            has_world || has_review,
            "note has neither canonical world nor review flag: {path}"
        );
        assert!(
            !text.lines().any(|line| line == "Worlds:"),
            "empty Worlds line remains in {path}"
        );
    }
    let capture: serde_json::Value =
        serde_json::from_slice(&resulting[".brain-state/capture-000.json"]).unwrap();
    assert!(
        capture["path"]
            .as_str()
            .unwrap()
            .contains("archive/checkpoints/2020-01/20200101-fixture-0000.md"),
        "capture reference was not remapped: {capture}"
    );
    let dedup: serde_json::Value =
        serde_json::from_slice(&resulting[".brain-state/dedup.json"]).unwrap();
    assert!(
        dedup["source"]
            .as_str()
            .unwrap()
            .contains("archive/checkpoints/2020-01/20200101-fixture-0000.md"),
        "dedup reference was not remapped: {dedup}"
    );
    for (path, bytes) in resulting.iter().filter(|(path, _)| {
        path.ends_with(".md")
            && !path.starts_with("wiki/")
            && !path.starts_with("digests/")
            && *path != "INDEX.md"
    }) {
        let text = String::from_utf8_lossy(bytes);
        let Some((_, tail)) = text.split_once("See [[") else {
            continue;
        };
        let (wiki, tail) = tail.split_once("]] and [related](").unwrap();
        let (markdown, _) = tail.split_once(").").unwrap();
        let mut wiki_path = PathBuf::from(wiki);
        if wiki_path.extension().is_none() {
            wiki_path.set_extension("md");
        }
        assert!(
            resulting.contains_key(&wiki_path.to_string_lossy().to_string()),
            "broken wikilink {wiki:?} in {path}"
        );
        let md_path = normalized_path(&Path::new(path).parent().unwrap().join(markdown));
        assert!(
            resulting.contains_key(&md_path.to_string_lossy().to_string()),
            "broken markdown link {markdown:?} in {path}"
        );
    }
    let log = Command::new("git")
        .arg("-C")
        .arg(&together)
        .args(["log", "--format=%s", "-8"])
        .output()
        .unwrap();
    assert!(log.status.success());
    let commits = String::from_utf8_lossy(&log.stdout);
    let migration_commits: Vec<_> = commits
        .lines()
        .filter(|line| line.starts_with("brain: migrate "))
        .collect();
    assert!(
        (5..=7).contains(&migration_commits.len()),
        "expected a single commit for each changed step: {commits}"
    );
    for step in STEPS {
        let message = format!("brain: migrate {step}");
        assert!(
            migration_commits
                .iter()
                .filter(|line| **line == message.as_str())
                .count()
                <= 1,
            "duplicate migration commit for {step}: {commits}"
        );
    }
}

#[test]
fn injected_verify_failure_restores_tree_and_makes_no_commit() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let vault = temp.path().join("vault");
    fixture(&vault, 24);
    let before = snapshot(&vault);
    let out = Command::new(helper())
        .args(["nacre-brain", "migrate", "--step", "meta", "--apply"])
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .env("NACRE_BRAIN", &vault)
        .env("NACRE_BRAIN_MIGRATE_TEST_FAIL_VERIFY", "1")
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "injected verify failure unexpectedly succeeded"
    );
    assert_eq!(
        snapshot(&vault),
        before,
        "failed step did not restore its snapshot"
    );
    let log = Command::new("git")
        .arg("-C")
        .arg(&vault)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), "1");
}

#[test]
fn move_collision_preserves_preexisting_destination_and_rolls_back_prior_moves() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let vault = temp.path().join("vault");
    fixture(&vault, 56);
    let source = vault.join("inbox/zz-move-collision.md");
    let destination = vault.join("checkpoints/zz-move-collision.md");
    fs::write(&source,"# Synthetic checkpoint handoff\n\nRecorded: 2020-01-01\nWorlds: Nacre\n\nSynthetic source payload.\n").unwrap();
    fs::write(
        &destination,
        b"pre-existing destination must remain byte-identical\n",
    )
    .unwrap();
    git(
        &vault,
        &[
            "add",
            "inbox/zz-move-collision.md",
            "checkpoints/zz-move-collision.md",
        ],
    );
    git(
        &vault,
        &["commit", "-m", "brain: collision fixture [no-publish]"],
    );
    let before = snapshot(&vault);
    let out = call(&home, &vault, "inbox", "--apply", None);
    assert!(
        !out.status.success(),
        "move collision unexpectedly succeeded"
    );
    assert_eq!(
        snapshot(&vault),
        before,
        "collision failure did not roll back earlier moves"
    );
    assert_eq!(
        fs::read(&destination).unwrap(),
        b"pre-existing destination must remain byte-identical\n"
    );
    assert!(source.is_file());
    let log = Command::new("git")
        .arg("-C")
        .arg(&vault)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        "2",
        "collision failure created a commit"
    );
}

#[test]
fn duplicate_subject_spellings_are_not_counted_as_distinct_notes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let vault = temp.path().join("vault");
    fixture(&vault, 2);
    for day in 1..=2 {
        fs::write(vault.join(format!("projects/subject-{day}.md")),format!("# Novel subject {day}\nRecorded: 2020-01-0{day}\nWorlds: Nacre\nTopics: Photon Grove, PHOTON GROVE\nReview: low-confidence\n\nA distinct synthetic note.\n")).unwrap();
    }
    git(&vault, &["add", "projects"]);
    git(
        &vault,
        &["commit", "-m", "brain: subject fixture [no-publish]"],
    );
    success(call(&home, &vault, "meta", "--apply", None));
    let registry: serde_json::Value =
        serde_json::from_slice(&fs::read(vault.join("registry.json")).unwrap()).unwrap();
    assert!(
        !registry["areas"]
            .as_array()
            .unwrap()
            .iter()
            .any(|area| area["name"] == "Photon Grove"),
        "two notes must not satisfy a three-note threshold by repeating topic spellings"
    );
    let stable = snapshot(&vault);
    success(call(&home, &vault, "meta", "--apply", None));
    assert_eq!(snapshot(&vault), stable);
}

#[test]
fn pending_original_bodies_are_not_folded_into_migration_commits() {
    for untracked in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let vault = temp.path().join("vault");
        fixture(&vault, 24);
        let path = if untracked {
            vault.join("projects/pending.md")
        } else {
            vault.join("inbox/20200101-fixture-0000.md")
        };
        fs::write(&path,"# Pending original\n\nSource: synthetic-fixture\n\nThis original body must survive a rejected migration.\n").unwrap();
        let before = snapshot(&vault);
        let out = call(&home, &vault, "meta", "--apply", None);
        assert!(
            !out.status.success(),
            "migration silently committed a pre-existing pending body"
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("commit affected"));
        assert_eq!(snapshot(&vault), before);
        let head = Command::new("git")
            .arg("-C")
            .arg(&vault)
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), "1");
    }
}

#[test]
fn links_step_repairs_stale_paths_in_notes_arriving_after_the_first_pass() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let vault = temp.path().join("vault");
    fixture(&vault, 16);
    let old = vault.join("checkpoints/20200101-late-target.md");
    fs::write(&old,"# Synthetic old checkpoint target\n\nRecorded: 2020-01-01\nWorlds: Nacre\n\nSynthetic target payload.\n").unwrap();
    git(&vault, &["add", "checkpoints/20200101-late-target.md"]);
    git(
        &vault,
        &["commit", "-m", "brain: fixture capture [no-publish]"],
    );
    // Follow the production order so archive verification sees canonical metadata.
    success(call(&home, &vault, "meta", "--apply", None));
    success(call(&home, &vault, "archive", "--apply", None));
    assert!(!old.exists());
    let moved = vault.join("archive/checkpoints/2020-01/20200101-late-target.md");
    assert!(moved.is_file());
    success(call(&home, &vault, "links", "--apply", None));

    fs::write(vault.join("raw/late-stale-reference.md"),"# Synthetic late link reference\n\nWorlds: Nacre\n\nStale links: [[checkpoints/20200101-late-target]] and [the old note](../checkpoints/20200101-late-target.md).\n").unwrap();
    git(&vault, &["add", "raw/late-stale-reference.md"]);
    git(
        &vault,
        &["commit", "-m", "brain: late fixture capture [no-publish]"],
    );
    success(call(&home, &vault, "links", "--apply", None));
    let late = fs::read_to_string(vault.join("raw/late-stale-reference.md")).unwrap();
    assert!(
        late.contains("[[archive/checkpoints/2020-01/20200101-late-target]]"),
        "wikilink was not rewritten from the move ledger: {late}"
    );
    assert!(
        late.contains("[the old note](../archive/checkpoints/2020-01/20200101-late-target.md)"),
        "Markdown link was not rewritten from the move ledger: {late}"
    );
    assert!(!late.contains("[[checkpoints/20200101-late-target]]"));
    assert!(moved.is_file());
}

#[test]
fn apply_waits_for_capture_lock_and_includes_the_later_note() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let vault = temp.path().join("vault");
    fixture(&vault, 24);
    let capture_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(vault.join(".brain-state/capture.lock"))
        .unwrap();
    rustix::fs::flock(&capture_lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let mut child = Command::new(helper())
        .args(["nacre-brain", "migrate", "--step", "meta", "--apply"])
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .env("NACRE_BRAIN", &vault)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        child.try_wait().unwrap().is_none(),
        "migration did not block on capture.lock"
    );
    let mut writer = Command::new(helper())
        .args([
            "nacre-brain",
            "note",
            "--title",
            "Concurrent synthetic lock note",
            "--source",
            "migration lock acceptance fixture",
            "--world",
            "desktop",
        ])
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .env("NACRE_BRAIN", &vault)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writer
        .stdin
        .take()
        .unwrap()
        .write_all(b"Written by the native note command while the migration awaited the lock.\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        writer.try_wait().unwrap().is_none(),
        "native note command did not block on the vault lock"
    );
    rustix::fs::flock(&capture_lock, rustix::fs::FlockOperation::Unlock).unwrap();
    let out = child.wait_with_output().unwrap();
    success(out);
    let note_out = writer.wait_with_output().unwrap();
    success(note_out);
    let files = snapshot(&vault);
    assert!(files.values().any(|b| String::from_utf8_lossy(b)
        .contains("Written by the native note command while the migration awaited the lock.")));
    // If the native writer acquired the lock after migration, the next run must
    // classify the note; if it acquired first, this run already did so.
    success(call(&home, &vault, "meta", "--apply", None));
    let files = snapshot(&vault);
    let note = files
        .values()
        .find(|b| {
            String::from_utf8_lossy(b).contains(
                "Written by the native note command while the migration awaited the lock.",
            )
        })
        .unwrap();
    let note = String::from_utf8_lossy(note);
    assert!(note.lines().any(|line| line.starts_with("Worlds: Nacre")));
    assert!(!note.lines().any(|line| line == "Worlds:"));
}
