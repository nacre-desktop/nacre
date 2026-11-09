//! Synthetic-vault acceptance tests for `nacre-brain-maintain consolidate`.
//! All note text is invented fixture data; the clock is injected with
//! `NACRE_BRAIN_TODAY`; no owner vault is read.
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

// Owner-approved Git attribution, split so public-name checks stay exact.
const OWNER: &str = concat!("siver", "teh");
const OWNER_EMAIL: &str = concat!("92440237+Siver", "teh@users.noreply.github.com");
const TODAY: &str = "2026-06-15";

fn helper() -> &'static str {
    env!("CARGO_BIN_EXE_nacre-helper")
}

fn command(home: &Path, vault: &Path, tool: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(helper());
    cmd.arg(tool)
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("NACRE_BRAIN", vault)
        .env("NACRE_BRAIN_TODAY", TODAY);
    cmd
}

fn consolidate(home: &Path, vault: &Path, args: &[&str]) -> Output {
    let mut all = vec!["consolidate"];
    all.extend_from_slice(args);
    command(home, vault, "nacre-brain-maintain", &all)
        .output()
        .unwrap()
}

fn ok(out: &Output) -> String {
    assert!(
        out.status.success(),
        "exit {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", OWNER)
        .env("GIT_AUTHOR_EMAIL", OWNER_EMAIL)
        .env("GIT_COMMITTER_NAME", OWNER)
        .env("GIT_COMMITTER_EMAIL", OWNER_EMAIL)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}

fn note(title: &str, recorded: &str, header: &str, body: &str) -> String {
    format!("# {title}\n\nRecorded: {recorded}T09:00:00+00:00\nConfidence: reported\nSource: synthetic-fixture\n{header}\n{body}\n")
}

fn registry() -> String {
    let mut registry: Value =
        serde_json::from_str(include_str!("../../../brain/registry-seed.json")).unwrap();
    let area = |name: &str, parent: &str, status: &str, created: &str| serde_json::json!({"name":name,"aliases":[],"patterns":[],"status":status,"created":created,"updated":created,"parent":parent,"title_only":false,"uses":[],"merged_into":null});
    let areas = registry["areas"].as_array_mut().unwrap();
    areas.push(area("Photon Garden", "Nacre", "proposed", "2026-06-01"));
    areas.push(area("Empty Idea", "Nacre", "proposed", "2026-04-01"));
    areas.push(area("Smash Boards", "Newbringer", "auto", "2026-05-01"));
    areas.push(area("Kite Lines", "Kite Club", "auto", "2026-01-02"));
    registry["worlds"].as_array_mut().unwrap().push(serde_json::json!({"name":"Kite Club","aliases":["Kite club"],"patterns":[],"status":"auto","created":"2026-01-01","updated":"2026-01-01","parent":null,"title_only":false,"uses":[],"merged_into":null}));
    serde_json::to_string_pretty(&registry).unwrap()
}

const OLD: &str = "checkpoints/20260501T100000Z-old-handoff.md";
const OLD_ARCHIVED: &str = "archive/checkpoints/2026-05/20260501T100000Z-old-handoff.md";
const FRESH: &str = "checkpoints/20260610T100000Z-fresh-handoff.md";

fn fixture(root: &Path) {
    write(root, "registry.json", &registry());
    write(root, ".gitignore", ".brain-state/\ncredentials/\n");
    write(
        root,
        OLD,
        &note(
            "Old Bead handoff",
            "2026-05-01",
            "Worlds: Nacre\nAreas: Bead\n",
            "Bead preview handed over; see [[decisions/20260301T090000Z-enable-sync]].",
        ),
    );
    write(root, FRESH, &note("Fresh Bead handoff", "2026-06-10", "Worlds: Nacre\nAreas: Bead\n", "Continues [[checkpoints/20260501T100000Z-old-handoff]] and [the old one](20260501T100000Z-old-handoff.md).\nA pre-existing broken link: [[does-not-exist]]."));
    write(
        root,
        "decisions/20260601T090000Z-name-link.md",
        &note(
            "Name link keeper",
            "2026-06-01",
            "Worlds: Nacre\n",
            "By name: [[20260501T100000Z-old-handoff]].",
        ),
    );
    // Inbox triage: one note per rule, plus unclear old and recent items.
    let inbox = [
        (
            "20260608T090000Z-handoff-status.md",
            "Handoff: status of the bead build",
            "2026-06-08",
            "Worlds: Nacre\nAreas: Bead\n",
            "Build continues.",
        ),
        (
            "20260605T090000Z-owner-prefers.md",
            "Owner prefers dark panels",
            "2026-06-05",
            "Worlds: Personal\n",
            "Noted in a session.",
        ),
        (
            "20260605T091000Z-decided-timer.md",
            "Decided to keep the check timer",
            "2026-06-05",
            "Worlds: Nacre\n",
            "Keep it daily.",
        ),
        (
            "20260605T092000Z-restore.md",
            "How to restore the preview",
            "2026-06-05",
            "Worlds: Nacre\nAreas: Bead\n",
            "Steps follow.",
        ),
        (
            "20260605T093000Z-battery.md",
            "Laptop battery replaced",
            "2026-06-05",
            "Worlds: Home & devices\nAreas: Laptop\n",
            "The battery serial is on the sticker.",
        ),
        (
            "20260605T094000Z-widgets.md",
            "Widget ideas for the bar",
            "2026-06-05",
            "Worlds: Nacre\n",
            "Some ideas.",
        ),
        (
            "20260501T090000Z-unclear.md",
            "A loose thought",
            "2026-05-01",
            "Review: unsorted\n",
            "Nothing to file yet.",
        ),
        (
            "20260610T090000Z-unclear-recent.md",
            "Another loose thought",
            "2026-06-10",
            "Review: unsorted\n",
            "Still nothing.",
        ),
    ];
    for (file, title, day, header, body) in inbox {
        write(
            root,
            &format!("inbox/{file}"),
            &note(title, day, header, body),
        );
    }
    // Promotion: three notes over two days for the proposed area.
    for (i, day) in ["2026-06-02", "2026-06-02", "2026-06-03"]
        .iter()
        .enumerate()
    {
        write(
            root,
            &format!("projects/photon-{i}.md"),
            &note(
                &format!("Photon garden sketch {i}"),
                day,
                "Worlds: Nacre\nAreas: Photon Garden\n",
                "Sketch.",
            ),
        );
    }
    // A tiny auto world that fits Newbringer; its old checkpoint is archived too.
    for (i, folder) in ["projects", "projects", "checkpoints"].iter().enumerate() {
        write(
            root,
            &format!("{folder}/20260201T09000{i}Z-kite-{i}.md"),
            &note(
                &format!("Newbringer kite flight {i}"),
                "2026-02-01",
                "Worlds: Kite Club\n",
                "The Newbringer pilots fly guns at the venue.",
            ),
        );
    }
    // Facts with provenance; stale and fresh by class.
    write(root, "wiki/entity-dev-1.md", "# dev-1\n\nEntity: area\nName: dev-1\nParent: Infrastructure\nReviewed: 2026-06-10\nStatus: curated\nSource: synthetic-fixture\n\nThe shared synthetic build host.\n\n## Current facts\n\n- dev-1 tailscale address is 100.64.0.5. (confirmed 2026-05-01; [[devices/dev-1-network]])\n- dev-1 runs Bead r100. (confirmed 2026-05-01)\n- dev-1 LAN address is 10.20.0.7. (confirmed 2026-06-10)\n- ~~dev-1 old LAN address 10.20.0.1~~ (superseded 2026-04-01 by [[devices/dev-1-network]])\n- Owner decided dev-1 stays the build host. (decided 2026-01-01)\n\n## Do and don't\n\n- Never run captures on dev-1.\n\n## Open questions\n\n- Is the GPU needed?\n");
    write(
        root,
        "devices/dev-1-network.md",
        &note(
            "dev-1 network",
            "2026-06-12",
            "Worlds: Infrastructure\nAreas: dev-1\n",
            "dev-1 tailscale address is now 100.64.0.9.",
        ),
    );
    // Two notes on one day disagree about prod's address: a real contradiction.
    for (file, address) in [("prod-a", "10.30.0.4"), ("prod-b", "10.30.0.5")] {
        write(
            root,
            &format!("devices/{file}.md"),
            &note(
                "prod network",
                "2026-06-12",
                "Worlds: Infrastructure\nAreas: prod\n",
                &format!("prod LAN address is {address}."),
            ),
        );
    }
    write(
        root,
        "decisions/20260301T090000Z-enable-sync.md",
        &note(
            "Enable the sync timer on the laptop",
            "2026-03-01",
            "Worlds: Home & devices\nAreas: Laptop\n",
            "Enabled for now.",
        ),
    );
    write(
        root,
        "decisions/20260601T090000Z-disable-sync.md",
        &note(
            "Disable the sync timer on the laptop",
            "2026-06-01",
            "Worlds: Home & devices\nAreas: Laptop\n",
            "The laptop sync timer is disabled.",
        ),
    );
    write(
        root,
        "credentials/decoy.txt",
        "synthetic credential decoy\n",
    );
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "fixture [no-publish]"]);
    let bare = root.with_extension("git");
    let out = Command::new("git")
        .args(["init", "-q", "--bare"])
        .arg(&bare)
        .output()
        .unwrap();
    assert!(out.status.success());
    git(root, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(root, &["push", "-q", "-u", "origin", "main"]);
}

fn setup(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let vault = dir.path().join(name);
    fs::create_dir_all(&home).unwrap();
    fixture(&vault);
    (dir, home, vault)
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        e.depth() == 0 || (name != ".git" && name != ".brain-state")
    }) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(root).unwrap();
            out.insert(
                rel.to_string_lossy().into_owned(),
                fs::read(entry.path()).unwrap(),
            );
        }
    }
    out
}

/// Seed entries with serde defaults filled in, as the registry writer stores them.
fn seeds(registry: &Value) -> Vec<Value> {
    ["worlds", "areas"]
        .iter()
        .flat_map(|k| registry[*k].as_array().unwrap().clone())
        .filter(|e| e["status"] == "seed")
        .map(|mut e| {
            for (key, default) in [
                ("parent", Value::Null),
                ("title_only", Value::Bool(false)),
                ("uses", Value::Array(Vec::new())),
                ("merged_into", Value::Null),
            ] {
                e.as_object_mut().unwrap().entry(key).or_insert(default);
            }
            e
        })
        .collect()
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn json(root: &Path, path: &str) -> Value {
    serde_json::from_str(&read(root, path)).unwrap()
}

#[test]
fn consolidation_archives_flags_tidies_and_packs_in_one_commit() {
    let (_dir, home, vault) = setup("vault");
    let base = git(&vault, &["rev-parse", "HEAD"]);
    let seeds_before = seeds(&json(&vault, "registry.json"));
    let out = consolidate(&home, &vault, &[]);
    let stdout = ok(&out);
    assert!(String::from_utf8_lossy(&out.stderr).contains("pre-existing broken links"));
    // Exactly one commit with the summary and every taxonomy change.
    assert_eq!(git(&vault, &["rev-parse", "HEAD~1"]), base);
    let message = git(&vault, &["log", "-1", "--format=%B"]);
    assert!(message.starts_with("brain: consolidate — "), "{message}");
    for change in [
        "promoted area Photon Garden",
        "retired proposed area Empty Idea",
        "merged area Smash Boards into Smash-Board (alias)",
        "folded world Kite Club into Newbringer as area Kite Club (alias kept)",
        "moved area Kite Lines from Kite Club to Newbringer",
    ] {
        assert!(message.contains(change), "missing {change}:\n{message}");
        assert!(
            stdout.contains(change),
            "missing {change} in stdout:\n{stdout}"
        );
    }
    assert_eq!(git(&vault, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&vault, &["rev-parse", "HEAD"]),
        git(&vault, &["rev-parse", "origin/main"])
    );
    // Retention: old checkpoint archived, inbox triaged by rule, unclear kept or archived.
    assert!(!vault.join(OLD).exists() && vault.join(OLD_ARCHIVED).exists());
    for (path, exists) in [
        ("checkpoints/20260608T090000Z-handoff-status.md", true),
        ("personal/20260605T090000Z-owner-prefers.md", true),
        ("decisions/20260605T091000Z-decided-timer.md", true),
        ("runbooks/20260605T092000Z-restore.md", true),
        ("devices/20260605T093000Z-battery.md", true),
        ("projects/20260605T094000Z-widgets.md", true),
        ("archive/inbox/2026-05/20260501T090000Z-unclear.md", true),
        ("inbox/20260610T090000Z-unclear-recent.md", true),
        (FRESH, true),
        ("inbox/20260501T090000Z-unclear.md", false),
    ] {
        assert_eq!(vault.join(path).exists(), exists, "{path}");
    }
    let unclear = read(&vault, "archive/inbox/2026-05/20260501T090000Z-unclear.md");
    assert!(unclear.contains("Review: unsorted"));
    assert!(vault
        .join("digests/2026-05/2026-05-review-unsorted.md")
        .exists());
    // Digests use canonical names: the folded world's checkpoint lands under
    // its new world and area.
    let kite = read(&vault, "digests/2026-02/2026-02-newbringer-kite-club.md");
    assert!(
        kite.contains("\nWorlds: Newbringer\nAreas: Kite Club\n"),
        "{kite}"
    );
    let digest = read(&vault, "digests/2026-05/2026-05-nacre-bead.md");
    assert!(digest.contains(&format!("[[{}|", OLD_ARCHIVED.trim_end_matches(".md"))));
    // Links repaired in their own style; name links and broken links untouched.
    let fresh = read(&vault, FRESH);
    assert!(fresh.contains("[[archive/checkpoints/2026-05/20260501T100000Z-old-handoff]]"));
    assert!(fresh.contains("](../archive/checkpoints/2026-05/20260501T100000Z-old-handoff.md)"));
    assert!(fresh.contains("[[does-not-exist]]"));
    assert!(read(&vault, "decisions/20260601T090000Z-name-link.md")
        .contains("[[20260501T100000Z-old-handoff]]"));
    // Taxonomy: seed entries untouched, auto entries reshaped.
    let registry = json(&vault, "registry.json");
    let entry = |kind: &str, name: &str| {
        registry[kind]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name)
            .cloned()
    };
    assert_eq!(seeds_before, seeds(&registry));
    assert_eq!(entry("areas", "Photon Garden").unwrap()["status"], "auto");
    assert!(entry("areas", "Empty Idea").is_none());
    let merged = entry("areas", "Smash Boards").unwrap();
    assert_eq!(
        (&merged["status"], &merged["merged_into"]),
        (&Value::from("merged-into"), &Value::from("Smash-Board"))
    );
    assert!(entry("worlds", "Kite Club").is_none());
    let folded = entry("areas", "Kite Club").unwrap();
    assert_eq!(folded["parent"], "Newbringer");
    assert_eq!(folded["aliases"][0], "Kite club");
    assert_eq!(
        entry("areas", "Kite Lines").unwrap()["parent"],
        "Newbringer"
    );
    // Freshness and contradictions live in the derived index, not in prose.
    let facts = json(&vault, ".brain-state/index/facts.json");
    let fact = |needle: &str| {
        facts["facts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["text"].as_str().unwrap().contains(needle))
            .cloned()
            .unwrap_or_else(|| panic!("no fact {needle}"))
    };
    assert_eq!(
        (
            fact("100.64.0.5")["class"].clone(),
            fact("100.64.0.5")["stale"].clone()
        ),
        (Value::from("address"), Value::from(true))
    );
    assert_eq!(
        (fact("r100")["class"].clone(), fact("r100")["stale"].clone()),
        (Value::from("version"), Value::from(true))
    );
    assert_eq!(fact("10.20.0.7")["stale"], false);
    assert_eq!(fact("stays the build host")["class"], "decision");
    assert_eq!(fact("stays the build host")["stale"], false);
    assert_eq!(fact("old LAN address")["superseded"], true);
    assert_eq!(fact("old LAN address")["stale"], false);
    assert!(!read(&vault, "wiki/entity-dev-1.md").contains("stale"));
    let review = json(&vault, ".brain-state/index/review.json");
    let items = review["items"].as_array().unwrap();
    // dev-1's address changed with "now" wording: a supersession, not a flag.
    assert!(!items
        .iter()
        .any(|i| i["kind"] == "contradiction" && i["entity"] == "dev-1"));
    let supersession = facts["supersessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["entity"] == "dev-1" && s["attribute"] == "tailscale-ip")
        .expect("supersession");
    assert_eq!(
        (
            &supersession["newer"]["path"],
            &supersession["older"]["path"]
        ),
        (
            &Value::from("devices/dev-1-network.md"),
            &Value::from("wiki/entity-dev-1.md")
        )
    );
    let contradiction = items
        .iter()
        .find(|i| i["kind"] == "contradiction")
        .expect("contradiction item");
    assert_eq!(
        (&contradiction["entity"], &contradiction["attribute"]),
        (&Value::from("prod"), &Value::from("ip"))
    );
    assert_eq!(
        contradiction["paths"],
        serde_json::json!(["devices/prod-b.md", "devices/prod-a.md"])
    );
    assert_eq!(
        contradiction["values"],
        serde_json::json!(["10.30.0.5", "10.30.0.4"])
    );
    assert_eq!(
        items
            .iter()
            .filter(|i| i["kind"] == "contradiction")
            .count(),
        1,
        "{items:#?}"
    );
    let reversal = items
        .iter()
        .find(|i| i["kind"] == "reversal")
        .expect("reversal item");
    assert_eq!(
        reversal["paths"],
        serde_json::json!([
            "decisions/20260601T090000Z-disable-sync.md",
            "decisions/20260301T090000Z-enable-sync.md"
        ])
    );
    assert_eq!(
        items.iter().filter(|i| i["kind"] == "stale-fact").count(),
        2,
        "{items:#?}"
    );
    // Context packs: capped, newest current facts, stale ones only as questions.
    let pack = read(&vault, "packs/pack-infrastructure-dev-1.md");
    assert!(pack.len() <= 4096, "{} bytes", pack.len());
    let key_facts = pack
        .split("## Key facts")
        .nth(1)
        .unwrap()
        .split("\n## ")
        .next()
        .unwrap();
    assert!(key_facts.contains("10.20.0.7") && key_facts.contains("stays the build host"));
    assert!(key_facts.contains("100.64.0.9"), "{pack}");
    assert!(!key_facts.contains("100.64.0.5") && !key_facts.contains("r100"));
    let open = pack.split("## Open questions").nth(1).unwrap();
    assert!(open.contains("Recheck address fact") && open.contains("Is the GPU needed?"));
    assert!(pack.contains("## Do and don't\n\n- Never run captures on dev-1."));
    assert!(pack.contains("[[wiki/entity-dev-1|dev-1]]"));
    for entry in fs::read_dir(vault.join("packs")).unwrap() {
        let size = entry.unwrap().metadata().unwrap().len();
        assert!(size <= 4096);
    }
    // Consolidation and the filtered CLI expose the same source-backed loop.
    let loops = json(&vault, ".brain-state/index/loops.json");
    assert!(loops["loops"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["text"] == "Is the GPU needed?"
            && item["kind"] == "question"
            && item["source"].as_str().unwrap().starts_with("note:")));
    let filtered: Value = serde_json::from_str(&ok(&command(
        &home,
        &vault,
        "nacre-brain",
        &[
            "loops",
            "--area",
            "dev-1",
            "--world",
            "Infrastructure",
            "--kind",
            "question",
            "--json",
        ],
    )
    .output()
    .unwrap()))
    .unwrap();
    let selected = filtered["loops"].as_array().unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["text"], "Is the GPU needed?");
    assert_eq!(selected[0]["status"], "open");
    let empty: Value = serde_json::from_str(&ok(&command(
        &home,
        &vault,
        "nacre-brain",
        &["loops", "--area", "Missing Area", "--json"],
    )
    .output()
    .unwrap()))
    .unwrap();
    assert_eq!(empty["loops"], serde_json::json!([]));
    // The CLI prints the nearest pack: the area's own, or its world's.
    let printed = ok(&command(&home, &vault, "nacre-brain", &["pack", "dev-1"])
        .output()
        .unwrap());
    assert_eq!(printed, pack);
    let fallback: Value = serde_json::from_str(&ok(&command(
        &home,
        &vault,
        "nacre-brain",
        &["pack", "--json", "Stations"],
    )
    .output()
    .unwrap()))
    .unwrap();
    assert_eq!(
        (&fallback["path"], &fallback["fallback"]),
        (&Value::from("packs/pack-newbringer.md"), &Value::from(true))
    );
    let merged_pack = command(
        &home,
        &vault,
        "nacre-brain",
        &["pack", "Smash Boards", "--json"],
    )
    .output()
    .unwrap();
    let merged_pack: Value = serde_json::from_str(&ok(&merged_pack)).unwrap();
    assert_eq!(merged_pack["area"], "Smash-Board");
    // A rerun is a no-op: same tree, no commit.
    let before = tree(&vault);
    let head = git(&vault, &["rev-parse", "HEAD"]);
    let rerun = ok(&consolidate(&home, &vault, &[]));
    assert!(rerun.contains("consolidate: no changes"), "{rerun}");
    assert_eq!(git(&vault, &["rev-parse", "HEAD"]), head);
    assert_eq!(tree(&vault), before);
}

#[test]
fn dry_run_is_read_only_and_uncommitted_notes_defer_their_moves() {
    let (dir, home, vault) = setup("vault");
    let before = tree(&vault);
    let head = git(&vault, &["rev-parse", "HEAD"]);
    let report = dir.path().join("report.md");
    let out = ok(&consolidate(
        &home,
        &vault,
        &["--dry-run", "--report", report.to_str().unwrap()],
    ));
    assert!(out.contains("dry-run: nothing written") && out.contains("archived"));
    assert_eq!(tree(&vault), before);
    assert!(!vault.join(".brain-state").exists());
    assert_eq!(git(&vault, &["rev-parse", "HEAD"]), head);
    assert!(fs::read_to_string(&report).unwrap().contains(OLD_ARCHIVED));
    // A report inside the vault is refused.
    let inside = vault.join("report.md");
    assert!(!consolidate(
        &home,
        &vault,
        &["--dry-run", "--report", inside.to_str().unwrap()]
    )
    .status
    .success());
    // An injected verification failure restores every file and commits nothing.
    let failed = command(&home, &vault, "nacre-brain-maintain", &["consolidate"])
        .env("NACRE_BRAIN_MIGRATE_TEST_FAIL_VERIFY", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(tree(&vault), before);
    assert_eq!(git(&vault, &["rev-parse", "HEAD"]), head);
    // An uncommitted edit to the note linking the old checkpoint defers that move.
    let edited = format!("{}\nOwner edit, not yet committed.\n", read(&vault, FRESH));
    write(&vault, FRESH, &edited);
    let out = ok(&consolidate(&home, &vault, &[]));
    assert!(out.contains(&format!("deferred: {OLD}")), "{out}");
    assert!(vault.join(OLD).exists() && !vault.join(OLD_ARCHIVED).exists());
    assert_eq!(read(&vault, FRESH), edited);
    assert!(git(&vault, &["status", "--porcelain"]).contains(FRESH));
    let review = json(&vault, ".brain-state/index/review.json");
    assert!(review["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["kind"] == "deferred" && i["path"] == OLD));
    // Once the edit is committed, the next run archives it.
    git(&vault, &["commit", "-qam", "owner edit [no-publish]"]);
    ok(&consolidate(&home, &vault, &[]));
    assert!(vault.join(OLD_ARCHIVED).exists());
    assert!(read(&vault, FRESH)
        .contains("[[archive/checkpoints/2026-05/20260501T100000Z-old-handoff]]"));
}

#[test]
fn a_concurrent_note_write_waits_for_the_consolidation_lock() {
    let (_dir, home, vault) = setup("vault");
    let started = Instant::now();
    let mut run = command(&home, &vault, "nacre-brain-maintain", &["consolidate"])
        .env("NACRE_BRAIN_CONSOLIDATE_TEST_HOLD_MS", "2000")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(600));
    let mut writer = command(
        &home,
        &vault,
        "nacre-brain",
        &[
            "note",
            "--title",
            "Concurrent synthetic note",
            "--source",
            "test",
            "--world",
            "Nacre",
        ],
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
    writer
        .stdin
        .take()
        .unwrap()
        .write_all(b"Written while consolidation holds the lock.\n")
        .unwrap();
    // The note may finish only after the run's 2-second lock hold has ended.
    let finished = loop {
        if writer.try_wait().unwrap().is_some() {
            break started.elapsed();
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "note write hung"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        finished >= Duration::from_millis(2000),
        "note write did not wait for the consolidation lock: {finished:?}"
    );
    assert!(run.wait().unwrap().success());
    let out = writer.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    assert!(Path::new(&path).exists());
    let committed = git(&vault, &["show", "--name-only", "--format=", "HEAD"]);
    let name = Path::new(&path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        !committed.contains(&name),
        "the later note must not join the run's commit"
    );
}
