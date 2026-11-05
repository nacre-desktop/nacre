//! Transactional, stepwise Brain vault migration.
//!
//! This module deliberately never enumerates `credentials/`; the walker prunes
//! it before descending. All note reads are bounded to regular markdown files.
use crate::{brain_data, brain_git, brain_registry, Result};
use chrono::Datelike;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[path = "brain_migrate_generated.rs"]
mod generated;
#[path = "brain_migrate_verify.rs"]
mod verification;

const STEPS: &[&str] = &[
    "meta", "inbox", "archive", "digests", "wiki", "links", "index",
];
const KEYS: &[&str] = &["Worlds", "Areas", "Topics", "Review"];

#[derive(Clone)]
struct Note {
    path: String,
    bytes: Vec<u8>,
    hash: String,
    mode: u32,
}
#[derive(Default)]
pub(crate) struct Plan {
    pub(crate) writes: BTreeMap<String, Vec<u8>>,
    pub(crate) moves: BTreeMap<String, String>,
    pub(crate) flags: Vec<(String, String)>,
    pub(crate) before_after: Vec<(String, String, String)>,
    samples: Vec<(String, String, String)>,
    wiki_new: Vec<String>,
    wiki_updated: Vec<String>,
    state_writes: BTreeMap<String, Vec<u8>>,
    state_hashes: BTreeMap<String, Option<String>>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
const NOTE_DIRS: &[&str] = &[
    "inbox",
    "checkpoints",
    "decisions",
    "runbooks",
    "projects",
    "devices",
    "personal",
    "wiki",
    "raw",
    "infrastructure",
    "archive",
    "digests",
];

const ROOT_DOCS: &[&str] = &["INDEX.md", "README.md", "AGENTS.md", "CLAUDE.md"];

fn include_entry(entry: &walkdir::DirEntry, root: &Path) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if entry.file_name() == "credentials" || entry.file_name().to_string_lossy().starts_with('.') {
        return false;
    }
    let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
    let first = relative
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())
        .unwrap_or("");
    if entry.depth() == 1 && entry.file_type().is_file() {
        return ROOT_DOCS.contains(&first);
    }
    NOTE_DIRS.contains(&first)
}

fn notes(root: &Path) -> Result<Vec<Note>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| include_entry(entry, root))
    {
        let e = entry?;
        if e.file_type().is_symlink() {
            let rel = e.path().strip_prefix(root).unwrap_or(e.path());
            let first = rel
                .components()
                .next()
                .and_then(|c| c.as_os_str().to_str())
                .unwrap_or("");
            if NOTE_DIRS.contains(&first) || e.path().extension().is_some_and(|x| x == "md") {
                return Err(
                    format!("symlinked migration note path: {}", e.path().display()).into(),
                );
            }
        }
        if !e.file_type().is_file() || e.path().extension().is_none_or(|x| x != "md") {
            continue;
        }
        let rel = e
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with(".brain-state/") || rel.starts_with("credentials/") {
            continue;
        }
        let meta = fs::symlink_metadata(e.path())?;
        if meta.file_type().is_symlink() || meta.nlink() != 1 || meta.len() > 5 * 1024 * 1024 {
            return Err(format!("unsafe migration input: {rel}").into());
        }
        let bytes = fs::read(e.path())?;
        out.push(Note {
            path: rel,
            hash: digest(&bytes),
            bytes,
            mode: meta.permissions().mode() & 0o777,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
fn field_lines(header: &str) -> Vec<(String, String)> {
    header
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                Some((k.to_owned(), v.trim().to_owned()))
            } else {
                None
            }
        })
        .collect()
}
pub(super) fn header_body(text: &str) -> (&str, &str, &str) {
    if !text.starts_with("# ") {
        return ("", "", text);
    }
    let title_end = text.find('\n').unwrap_or(text.len());
    let title = &text[..title_end];
    let mut offset = title_end + usize::from(title_end < text.len());
    let mut fields_start = offset;
    let mut fields_end = offset;
    let mut started = false;
    for line in text[offset..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let next = offset + line.len();
        if trimmed.is_empty() {
            if started {
                return (title, &text[fields_start..fields_end], &text[next..]);
            }
            fields_start = next;
            fields_end = next;
        } else if line.split_once(':').is_some_and(|(key, _)| {
            !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        }) {
            started = true;
            fields_end = offset + trimmed.len();
        } else {
            return (
                title,
                if started {
                    &text[fields_start..fields_end]
                } else {
                    ""
                },
                &text[offset..],
            );
        }
        offset = next;
    }
    (
        title,
        if started {
            &text[fields_start..fields_end]
        } else {
            ""
        },
        "",
    )
}

fn lists(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
fn rewrite_classification(
    text: &str,
    categories: &brain_registry::Categories,
    fields: &[(String, String)],
) -> String {
    let (title, raw_fields, body) = header_body(text);
    let title = if title.is_empty() {
        "# Untitled note"
    } else {
        title
    };
    let mut replacement = Vec::new();
    if !categories.worlds.is_empty() {
        replacement.push(format!("Worlds: {}", categories.worlds.join(", ")));
    }
    if !categories.areas.is_empty() {
        replacement.push(format!("Areas: {}", categories.areas.join(", ")));
    }
    if !categories.topics.is_empty() || fields.iter().any(|(k, _)| k == "Topics") {
        replacement.push(format!("Topics: {}", categories.topics.join(", ")));
    }
    if !categories.review.is_empty() {
        replacement.push(format!("Review: {}", categories.review.join(", ")));
    }
    let mut all = Vec::new();
    let mut inserted = false;
    for line in raw_fields.lines() {
        let Some((k, v)) = line.split_once(':') else {
            all.push(line.to_owned());
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if KEYS.contains(&key) {
            if !inserted {
                all.extend(replacement.clone());
                inserted = true;
            }
            continue;
        }
        if (key == "Context" || (key == "Project" && (value.len() > 40 || value.contains('/'))))
            && !inserted
        {
            all.extend(replacement.clone());
            inserted = true;
        }
        if key == "Project" && (value.len() > 40 || value.contains('/')) {
            all.push(format!("Context: {value}"));
        } else {
            all.push(line.to_owned());
        }
    }
    if !inserted {
        all.extend(replacement);
    }
    let mut out = title.to_owned();
    if !all.is_empty() {
        out.push_str("\n\n");
        out.push_str(&all.join("\n"));
    }
    out.push_str("\n\n");
    out.push_str(body);
    out
}
fn safe_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        private_dirs(parent)?;
    }
    if fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_file() || m.nlink() != 1)
    {
        return Err(format!("unsafe migration output: {}", path.display()).into());
    }
    let temp = path.with_file_name(format!(".pending-migrate-{}", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&temp)?;
    let result = (|| -> Result<()> {
        f.write_all(bytes)?;
        drop(f);
        fs::rename(&temp, path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}
// Write the whole plan first so fsync can coalesce the step's journal work.
// Every changed file and rename parent is durable before verification/commit.
fn sync_changes(root: &Path, paths: &BTreeSet<String>) -> Result<()> {
    let mut directories = BTreeSet::new();
    for relative in paths {
        let path = root.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() || meta.nlink() != 1 {
                    return Err("unsafe migration sync path".into());
                }
                OpenOptions::new()
                    .read(true)
                    .custom_flags(nix::libc::O_NOFOLLOW)
                    .open(&path)?
                    .sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        if let Some(parent) = path.parent() {
            directories.extend(
                parent
                    .ancestors()
                    .take_while(|directory| directory.starts_with(root))
                    .map(Path::to_path_buf),
            );
        }
    }
    for directory in directories.into_iter().rev() {
        match fs::symlink_metadata(&directory) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
                fs::File::open(&directory)?.sync_all()?
            }
            Ok(_) => return Err("unsafe migration sync directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn snapshot_file(root: &Path, rel: &str) -> Result<Option<(Vec<u8>, u32)>> {
    let path = root.join(rel);
    check_ancestors(&path)?;
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink()
        || !meta.is_file()
        || meta.nlink() != 1
        || meta.len() > 5 * 1024 * 1024
    {
        return Err(format!("unsafe migration output: {rel}").into());
    }
    Ok(Some((fs::read(path)?, meta.permissions().mode() & 0o777)))
}
fn load_moves(root: &Path) -> Result<BTreeMap<String, String>> {
    let rel = ".brain-state/migrate-moves.json";
    match snapshot_file(root, rel)? {
        Some((bytes, _)) => {
            serde_json::from_slice(&bytes).map_err(|_| "Corrupt migration move ledger".into())
        }
        None => Ok(BTreeMap::new()),
    }
}
fn private_dirs(path: &Path) -> Result<()> {
    check_ancestors(path)?;
    let mut missing = Vec::new();
    let mut cur = path;
    while !cur.exists() {
        missing.push(cur.to_path_buf());
        cur = cur.parent().ok_or("invalid directory path")?;
    }
    fs::create_dir_all(path)?;
    for dir in missing {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn check_ancestors(path: &Path) -> Result<()> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        if fs::symlink_metadata(&prefix).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!("symlinked migration ancestor: {}", prefix.display()).into());
        }
    }
    Ok(())
}
fn note_dirs(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut dirs = BTreeSet::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| include_entry(entry, root))
    {
        let entry = entry?;
        if entry.depth() > 0 && entry.file_type().is_dir() {
            dirs.insert(entry.path().strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(dirs)
}

fn date_from(path: &str, text: &str) -> Option<chrono::NaiveDate> {
    let fields = field_lines(header_body(text).1);
    let recorded = fields
        .iter()
        .find(|(k, _)| k == "Recorded")
        .or_else(|| fields.iter().find(|(k, _)| k == "Reviewed"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    if let Some(d) = recorded
        .get(..10)
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    {
        return Some(d);
    }
    let name = Path::new(path).file_stem()?.to_string_lossy();
    let digits: String = name.chars().take(8).collect();
    chrono::NaiveDate::parse_from_str(&digits, "%Y%m%d").ok()
}
fn remap_record(
    value: &mut serde_json::Value,
    key: &str,
    moves: &BTreeMap<String, String>,
    all_strings: bool,
) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            let mut changed = false;
            for (k, v) in map {
                changed |= remap_record(v, k, moves, all_strings);
            }
            changed
        }
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= remap_record(item, key, moves, all_strings);
            }
            changed
        }
        serde_json::Value::String(s) if all_strings || key == "path" || key == "source" => {
            for (old, new) in moves {
                if s == old {
                    *s = new.clone();
                    return true;
                }
                if !Path::new(s.as_str()).is_absolute() {
                    continue;
                }
                let suffix = format!("/{old}");
                if let Some(prefix) = s.strip_suffix(&suffix) {
                    *s = format!("{prefix}/{new}");
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}
fn inbox_target(text: &str) -> Option<&'static str> {
    let (title, head, body) = header_body(text);
    let fields = field_lines(head);
    let t = title.to_lowercase();
    for (dest, rx) in [
        (
            "checkpoints",
            r"handoff|checkpoint|transfer|\bresume|\blead\b|status|progress|paused|continu|awaiting|pending|in progress",
        ),
        (
            "personal",
            r"\bprefer|preference|dislike|owner wants|\blikes?\b",
        ),
        (
            "decisions",
            r"decid|decision|\bchose|authori[sz]|approved|policy|ratif",
        ),
        (
            "runbooks",
            r"how to|runbook|procedure|steps|recover|restore|setup|install|workflow for|command",
        ),
    ] {
        if regex::Regex::new(rx).unwrap().is_match(&t) {
            return Some(dest);
        }
    }
    let worlds = fields
        .iter()
        .find(|(k, _)| k == "Worlds")
        .map(|(_, v)| lists(v))
        .unwrap_or_default();
    if worlds.iter().any(|w| w == "Infrastructure")
        && !worlds.iter().any(|w| w == "Nacre" || w == "Newbringer")
    {
        return Some("infrastructure");
    }
    let areas = fields
        .iter()
        .find(|(k, _)| k == "Areas")
        .map(|(_, v)| lists(v))
        .unwrap_or_default();
    let dev = [
        "Guns",
        "HK416",
        "Smash-Board",
        "White motor",
        "Desktop PC",
        "Laptop",
        "Phone",
        "Peripherals",
        "Home server",
    ];
    if areas.iter().any(|a|dev.contains(&a.as_str()))&&regex::Regex::new(r"(?i)\b(ip|address|board|battery|cm4|gun|laptop|phone|serial|mac|firmware|hardware|pinout)\b").unwrap().is_match(&format!("{t} {}",body.chars().take(600).collect::<String>())){return Some("devices")}
    if !worlds.is_empty() {
        Some("projects")
    } else {
        None
    }
}
fn markdown_report(plan: &Plan, counts: &BTreeMap<&str, usize>) -> String {
    let mut s = String::from("# Brain migration dry-run\n\n## Step counts\n\n");
    for (k, v) in counts {
        s.push_str(&format!("- {k}: {v}\n"));
    }
    s.push_str("\n## World and area mapping\n\n| Before | After |\n|---|---|\n");
    let mappings: BTreeSet<_> = plan.before_after.iter().map(|(_, a, b)| (a, b)).collect();
    for (a, b) in mappings {
        s.push_str(&format!(
            "| {} | {} |\n",
            a.replace('|', "\\|"),
            b.replace('|', "\\|")
        ));
    }
    s.push_str("\n## Review flags\n\n");
    for (p, f) in plan.flags.iter().collect::<BTreeSet<_>>() {
        s.push_str(&format!("- `{p}` — {f}\n"));
    }
    s.push_str("\n## Wiki pages\n\n");
    s.push_str("Created:\n");
    for path in &plan.wiki_new {
        s.push_str(&format!("- `{path}`\n"));
    }
    s.push_str("Updated:\n");
    for path in &plan.wiki_updated {
        s.push_str(&format!("- `{path}`\n"));
    }
    s.push_str("\n## Sample headers\n\n");
    for (p, a, b) in plan.samples.iter().take(30) {
        s.push_str(&format!(
            "### `{p}`\n\nBefore:\n```text\n{a}\n```\nAfter:\n```text\n{b}\n```\n\n"
        ));
    }
    s
}

type ClusterCandidate = (String, String, String);

fn metadata_plan(
    notes: &[Note],
    registry: &[u8],
    today: &str,
    wiki: bool,
) -> Result<(Plan, Vec<ClusterCandidate>)> {
    let mut plan = Plan::default();
    let mut candidates = Vec::new();
    for n in notes {
        if (!wiki && n.path.starts_with("wiki/")) || n.path.starts_with("digests/") {
            continue;
        }
        if ROOT_DOCS.contains(&n.path.as_str()) {
            continue;
        }
        let txt = String::from_utf8(n.bytes.clone())
            .map_err(|_| format!("non UTF-8 note: {}", n.path))?;
        let (title, head, body) = header_body(&txt);
        let fields = field_lines(head);
        let worlds = fields
            .iter()
            .find(|(k, _)| k == "Worlds")
            .map(|(_, v)| lists(v))
            .unwrap_or_default();
        let areas = fields
            .iter()
            .find(|(k, _)| k == "Areas")
            .map(|(_, v)| lists(v))
            .unwrap_or_default();
        let topics = fields
            .iter()
            .find(|(k, _)| k == "Topics")
            .map(|(_, v)| lists(v))
            .unwrap_or_default();
        let mut labels = worlds.clone();
        if let Some((_, project)) = fields.iter().find(|(k, _)| k == "Project") {
            if project.len() <= 40 && !project.contains('/') {
                labels.push(project.clone());
            }
        }
        let mut cats = brain_registry::classify_migration_registry_at(
            registry, title, body, &labels, &areas, &topics, today,
        )?;
        if let Some((_, review)) = fields
            .iter()
            .find(|(k, value)| k == "Review" && !value.is_empty())
        {
            cats.review = vec![review.clone()];
        }
        if !cats.review.is_empty() && !cats.worlds.is_empty() {
            let day = date_from(&n.path, &txt)
                .map(|d| d.to_string())
                .unwrap_or_default();
            let subjects: BTreeMap<_, _> = topics
                .iter()
                .chain(labels.iter())
                .chain(areas.iter())
                .filter_map(|subject| brain_registry::migration_subject(subject))
                .collect();
            for subject in subjects.into_values() {
                candidates.push((subject, cats.worlds[0].clone(), day.clone()));
            }
        }
        let updated = rewrite_classification(&txt, &cats, &fields);
        if !cats.review.is_empty() {
            plan.flags.push((n.path.clone(), cats.review.join(", ")));
        }
        let old_areas = fields
            .iter()
            .find(|(k, _)| k == "Areas")
            .map(|(_, v)| lists(v))
            .unwrap_or_default();
        plan.before_after.push((
            n.path.clone(),
            format!(
                "{}{}",
                worlds.join(", "),
                if old_areas.is_empty() {
                    String::new()
                } else {
                    format!(" / {}", old_areas.join(", "))
                }
            ),
            format!(
                "{}{}",
                cats.worlds.join(", "),
                if cats.areas.is_empty() {
                    String::new()
                } else {
                    format!(" / {}", cats.areas.join(", "))
                }
            ),
        ));
        let before_header = format!("{}\n{}", title, head);
        let (after_title, after_head, _) = header_body(&updated);
        plan.samples.push((
            n.path.clone(),
            before_header,
            format!("{}\n{}", after_title, after_head),
        ));
        if updated != txt {
            plan.writes.insert(n.path.clone(), updated.into_bytes());
        }
    }
    Ok((plan, candidates))
}

/// Run one migration step or the full ordered sequence. Arguments are the
/// tokens after `nacre-brain migrate`.
pub fn run(args: &[String]) -> Result<i32> {
    let mut step = None;
    let mut apply = false;
    let mut dry = false;
    let mut report = None;
    let mut days = 21i64;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--step" => {
                i += 1;
                step = args.get(i).cloned();
            }
            "--dry-run" => dry = true,
            "--apply" => apply = true,
            "--report" => {
                i += 1;
                report = args.get(i).map(PathBuf::from);
            }
            "--archive-older-than-days" => {
                i += 1;
                days = args.get(i).ok_or("missing archive age")?.parse()?;
            }
            x => return Err(format!("unknown migrate argument: {x}").into()),
        }
        i += 1;
    }
    if dry == apply || days < 0 {
        return Err("choose exactly one of --dry-run or --apply; age must be non-negative".into());
    }
    let step = step.ok_or("missing --step")?;
    if step != "all" && !STEPS.contains(&step.as_str()) {
        return Err("invalid migration step".into());
    }
    let root = brain_data::configured_root_path()?;
    let report_path = if dry || report.is_some() {
        let path = report.unwrap_or_else(|| {
            root.parent()
                .unwrap_or(&root)
                .join("brain-migrate-report.md")
        });
        let absolute = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        };
        let parent = fs::canonicalize(absolute.parent().ok_or("invalid report path")?)?;
        let resolved = parent.join(absolute.file_name().ok_or("invalid report path")?);
        if resolved.starts_with(&root) {
            return Err("migration report must be outside the vault".into());
        }
        if fs::symlink_metadata(&resolved)
            .is_ok_and(|m| !m.is_file() || m.file_type().is_symlink() || m.nlink() != 1)
        {
            return Err("unsafe migration report path".into());
        }
        Some(resolved)
    } else {
        None
    };
    let selected: Vec<&str> = if step == "all" {
        STEPS.to_vec()
    } else {
        vec![step.as_str()]
    };
    // Dry-run does not create lock/state files. Apply serializes capture and note writes.
    let _lock = if apply {
        Some(brain_git::acquire_vault_lock(&root)?)
    } else {
        None
    };
    let originals = notes(&root)?;
    let mut virtual_state: BTreeMap<String, Vec<u8>> = originals
        .iter()
        .map(|n| (n.path.clone(), n.bytes.clone()))
        .collect();
    let registry_path = root.join("registry.json");
    let mut virtual_registry = snapshot_file(&root, "registry.json")?
        .map(|(bytes, _)| bytes)
        .unwrap_or_else(|| include_bytes!("../../../brain/registry-seed.json").to_vec());
    let mut virtual_records = BTreeMap::<String, Vec<u8>>::new();
    let mut virtual_moves = load_moves(&root)?;
    let mut all_counts = BTreeMap::new();
    let mut report_plan = Plan::default();
    for note in &originals {
        let text = std::str::from_utf8(&note.bytes)?;
        if let Some((_, flag)) = field_lines(header_body(text).1)
            .into_iter()
            .find(|(key, value)| key == "Review" && !value.is_empty())
        {
            report_plan.flags.push((note.path.clone(), flag));
        }
    }
    for current in selected {
        let registry_before = virtual_registry.clone();
        let current_notes: Vec<Note> = if dry {
            virtual_state
                .iter()
                .map(|(path, bytes)| Note {
                    path: path.clone(),
                    bytes: bytes.clone(),
                    hash: digest(bytes),
                    mode: 0o600,
                })
                .collect()
        } else {
            notes(&root)?
        };
        let mut plan = Plan::default();
        match current {
            "meta" => {
                let today = chrono::Local::now().date_naive().to_string();
                let (initial, candidates) =
                    metadata_plan(&current_notes, &virtual_registry, &today, false)?;
                let proposed = brain_registry::propose_migration_clusters(
                    &virtual_registry,
                    &candidates,
                    &chrono::Local::now().date_naive().to_string(),
                )?;
                // Resolve against the final proposed registry in this same pass;
                // the next run must not promote topics into areas a second time.
                plan = if proposed != virtual_registry {
                    metadata_plan(&current_notes, &proposed, &today, false)?.0
                } else {
                    initial
                };
                if proposed != virtual_registry || !root.join("registry.json").exists() {
                    plan.writes.insert("registry.json".into(), proposed.clone());
                    virtual_registry = proposed;
                }
            }
            "inbox" | "archive" => {
                let now = chrono::Local::now().date_naive();
                for n in &current_notes {
                    let text = String::from_utf8_lossy(&n.bytes);
                    let d = match date_from(&n.path, &text) {
                        Some(d) => d,
                        None => continue,
                    };
                    let age = (now - d).num_days();
                    let dest = if current == "archive"
                        && n.path.starts_with("checkpoints/")
                        && age >= days
                    {
                        Some(format!(
                            "archive/checkpoints/{:04}-{:02}/{}",
                            d.year(),
                            d.month(),
                            Path::new(&n.path).file_name().unwrap().to_string_lossy()
                        ))
                    } else if current == "inbox" && n.path.starts_with("inbox/") && age >= 3 {
                        inbox_target(&text).map(|folder| {
                            format!(
                                "{folder}/{}",
                                Path::new(&n.path).file_name().unwrap().to_string_lossy()
                            )
                        })
                    } else if current == "archive" && n.path.starts_with("inbox/") && age >= 30 {
                        Some(format!(
                            "archive/inbox/{:04}-{:02}/{}",
                            d.year(),
                            d.month(),
                            Path::new(&n.path).file_name().unwrap().to_string_lossy()
                        ))
                    } else {
                        None
                    };
                    if let Some(dest) = dest {
                        plan.moves.insert(n.path.clone(), dest);
                    }
                }
                let move_map = plan.moves.clone();
                let link_snapshot: BTreeMap<String, Vec<u8>> = current_notes
                    .iter()
                    .map(|n| (n.path.clone(), n.bytes.clone()))
                    .collect();
                for n in &current_notes {
                    let new_path = move_map.get(&n.path).unwrap_or(&n.path);
                    let txt = String::from_utf8(n.bytes.clone())
                        .map_err(|_| format!("non UTF-8 note: {}", n.path))?;
                    let rewritten = verification::rewrite_links(
                        &txt,
                        &n.path,
                        new_path,
                        &move_map,
                        &link_snapshot,
                    );
                    if rewritten != txt {
                        plan.writes.insert(new_path.clone(), rewritten.into_bytes());
                    }
                }
                if !move_map.is_empty() {
                    let ledger_path = ".brain-state/migrate-moves.json";
                    let prior = if dry {
                        virtual_moves.clone()
                    } else {
                        load_moves(&root)?
                    };
                    let prior_bytes = snapshot_file(&root, ledger_path)?;
                    let mut ledger = prior;
                    for target in ledger.values_mut() {
                        if let Some(next) = move_map.get(target) {
                            *target = next.clone();
                        }
                    }
                    ledger.extend(move_map.clone());
                    let state_rel = ledger_path.to_owned();
                    plan.state_hashes.insert(
                        state_rel.clone(),
                        prior_bytes.as_ref().map(|(bytes, _)| digest(bytes)),
                    );
                    plan.state_writes
                        .insert(state_rel, serde_json::to_vec_pretty(&ledger)?);
                    virtual_moves = ledger;
                    let state = root.join(".brain-state");
                    if state.is_dir() {
                        for ent in fs::read_dir(&state)? {
                            let ent = ent?;
                            let name = ent.file_name().to_string_lossy().into_owned();
                            if !(name.starts_with("capture") || name.contains("dedup"))
                                || !name.ends_with(".json")
                            {
                                continue;
                            }
                            let path = ent.path();
                            let meta = fs::symlink_metadata(&path)?;
                            if !meta.is_file()
                                || meta.file_type().is_symlink()
                                || meta.nlink() != 1
                                || meta.len() > 5 * 1024 * 1024
                            {
                                return Err("unsafe capture record in Brain state".into());
                            }
                            let state_key = format!(".brain-state/{name}");
                            let bytes = if dry {
                                virtual_records
                                    .get(&state_key)
                                    .cloned()
                                    .unwrap_or(fs::read(&path)?)
                            } else {
                                fs::read(&path)?
                            };
                            let Ok(mut record) =
                                serde_json::from_slice::<serde_json::Value>(&bytes)
                            else {
                                continue;
                            };
                            let changed =
                                remap_record(&mut record, "", &move_map, name.contains("dedup"));
                            if changed {
                                let state_rel = format!(".brain-state/{name}");
                                plan.state_hashes
                                    .insert(state_rel.clone(), Some(digest(&bytes)));
                                plan.state_writes
                                    .insert(state_rel, serde_json::to_vec_pretty(&record)?);
                            }
                        }
                    }
                }
            }
            "links" => {
                let map: BTreeMap<String, Vec<u8>> = current_notes
                    .iter()
                    .map(|n| (n.path.clone(), n.bytes.clone()))
                    .collect();
                let link_moves: BTreeMap<_, _> = (if dry {
                    virtual_moves.clone()
                } else {
                    load_moves(&root)?
                })
                .into_iter()
                .filter(|(old, new)| !map.contains_key(old) && map.contains_key(new))
                .collect();
                for n in &current_notes {
                    let text = String::from_utf8(n.bytes.clone())
                        .map_err(|_| format!("non UTF-8 note: {}", n.path))?;
                    let updated =
                        verification::rewrite_links(&text, &n.path, &n.path, &link_moves, &map);
                    if updated != text {
                        plan.writes.insert(n.path.clone(), updated.into_bytes());
                    }
                }
            }
            "digests" | "wiki" | "index" => {
                let map: BTreeMap<String, Vec<u8>> = current_notes
                    .iter()
                    .map(|n| (n.path.clone(), n.bytes.clone()))
                    .collect();
                let vocab: serde_json::Value =
                    serde_json::from_slice(include_bytes!("brain-migrate-vocabulary.json"))?;
                let registry: serde_json::Value = serde_json::from_slice(&virtual_registry)?;
                let generated = generated::generate(
                    current,
                    &map,
                    &registry,
                    &vocab,
                    &chrono::Local::now().date_naive().to_string(),
                )?;
                for path in generated.keys().filter(|p| p.starts_with("wiki/")) {
                    if virtual_state.contains_key(path) {
                        plan.wiki_updated.push(path.clone())
                    } else {
                        plan.wiki_new.push(path.clone())
                    }
                }
                plan.writes.extend(generated);
                if current == "wiki" {
                    let canonical: BTreeSet<_> = registry["worlds"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|world| world["name"].as_str())
                        .collect();
                    let mut unclassified = Vec::new();
                    for note in current_notes.iter().filter(|note| {
                        note.path.starts_with("wiki/") && !plan.writes.contains_key(&note.path)
                    }) {
                        let text = std::str::from_utf8(&note.bytes)?;
                        let fields = field_lines(header_body(text).1);
                        let worlds = fields.iter().find(|(key, _)| key == "Worlds");
                        let valid = worlds.is_some_and(|(_, value)| {
                            !value.is_empty()
                                && lists(value)
                                    .iter()
                                    .all(|world| canonical.contains(world.as_str()))
                        });
                        let review = fields
                            .iter()
                            .any(|(key, value)| key == "Review" && !value.is_empty());
                        if !valid && !(review && worlds.is_none()) {
                            unclassified.push(note.clone());
                        }
                    }
                    let (fallback, _) = metadata_plan(
                        &unclassified,
                        &virtual_registry,
                        &chrono::Local::now().date_naive().to_string(),
                        true,
                    )?;
                    plan.wiki_updated.extend(fallback.writes.keys().cloned());
                    plan.writes.extend(fallback.writes);
                    plan.flags.extend(fallback.flags);
                }
            }
            _ => unreachable!(),
        }
        let changed_paths: BTreeSet<_> = plan
            .writes
            .keys()
            .chain(plan.moves.keys())
            .chain(plan.moves.values())
            .chain(plan.state_writes.keys())
            .collect();
        all_counts.insert(current, changed_paths.len());
        for (path, bytes) in plan.writes.iter().filter(|(path, _)| path.ends_with(".md")) {
            let text = std::str::from_utf8(bytes)?;
            if let Some((_, flag)) = field_lines(header_body(text).1)
                .into_iter()
                .find(|(key, value)| key == "Review" && !value.is_empty())
            {
                report_plan.flags.push((path.clone(), flag));
            }
        }
        report_plan.writes.extend(plan.writes.clone());
        report_plan.moves.extend(plan.moves.clone());
        report_plan.flags.extend(plan.flags.clone());
        report_plan.before_after.extend(plan.before_after.clone());
        report_plan.samples.extend(plan.samples.clone());
        report_plan.wiki_new.extend(plan.wiki_new.clone());
        report_plan.wiki_updated.extend(plan.wiki_updated.clone());
        if dry {
            let mut changed = virtual_state.clone();
            for (old, new) in &plan.moves {
                if let Some(bytes) = changed.remove(old) {
                    changed.insert(new.clone(), bytes);
                }
            }
            changed.extend(
                plan.writes
                    .iter()
                    .filter(|(path, _)| path.ends_with(".md"))
                    .map(|(path, bytes)| (path.clone(), bytes.clone())),
            );
            virtual_state = changed;
            virtual_records.extend(plan.state_writes.clone());
            continue;
        }
        if plan.writes.is_empty() && plan.moves.is_empty() {
            // A previous attempt may have committed successfully while offline.
            brain_git::push_migration(&root)?;
            continue;
        }
        let output_paths: BTreeSet<String> = plan
            .writes
            .keys()
            .cloned()
            .chain(plan.moves.iter().flat_map(|(a, b)| [a.clone(), b.clone()]))
            .chain(plan.state_writes.keys().cloned())
            .collect();
        let mut snapshots = BTreeMap::<String, Option<(Vec<u8>, u32)>>::new();
        for path in &output_paths {
            snapshots.insert(path.clone(), snapshot_file(&root, path)?);
        }
        let baseline_paths = output_paths
            .iter()
            .filter(|path| !path.starts_with(".brain-state/") && snapshots[*path].is_some())
            .cloned()
            .collect();
        brain_git::require_migration_baseline(&root, &baseline_paths)?;
        // Complete all compare-and-swap and collision checks before the first write.
        for (old, new) in &plan.moves {
            let expected = current_notes
                .iter()
                .find(|n| n.path == *old)
                .ok_or_else(|| format!("missing move source: {old}"))?;
            if snapshots
                .get(old)
                .and_then(Option::as_ref)
                .is_none_or(|(bytes, _)| digest(bytes) != expected.hash)
            {
                return Err(format!("CAS changed: {old}").into());
            }
            if snapshots.get(new).is_some_and(Option::is_some) {
                return Err(format!("move destination exists: {new}").into());
            }
        }
        for rel in plan.writes.keys() {
            if let Some((old, _)) = plan.moves.iter().find(|(_, new)| *new == rel) {
                if snapshots.get(rel).is_some_and(Option::is_some) {
                    return Err(format!("move destination exists: {rel}").into());
                }
                let source = current_notes
                    .iter()
                    .find(|n| n.path == *old)
                    .ok_or("missing moved source")?;
                if snapshots
                    .get(old)
                    .and_then(Option::as_ref)
                    .is_none_or(|(bytes, _)| digest(bytes) != source.hash)
                {
                    return Err(format!("CAS changed: {old}").into());
                }
            } else if let Some(note) = current_notes.iter().find(|n| n.path == *rel) {
                if snapshots
                    .get(rel)
                    .and_then(Option::as_ref)
                    .is_none_or(|(bytes, _)| digest(bytes) != note.hash)
                {
                    return Err(format!("CAS changed: {rel}").into());
                }
            } else if rel == "registry.json" {
                let expected = if registry_path.exists() {
                    Some(digest(&registry_before))
                } else {
                    None
                };
                let actual = snapshots
                    .get(rel)
                    .and_then(Option::as_ref)
                    .map(|(bytes, _)| digest(bytes));
                if expected != actual {
                    return Err("CAS changed: registry.json".into());
                }
            } else if snapshots.get(rel).is_some_and(Option::is_some) {
                return Err(format!("refusing to overwrite existing file: {rel}").into());
            }
        }
        for (rel, expected) in &plan.state_hashes {
            let actual = snapshots
                .get(rel)
                .and_then(Option::as_ref)
                .map(|(bytes, _)| digest(bytes));
            if &actual != expected {
                return Err(format!("CAS changed: {rel}").into());
            }
        }
        let original_dirs = note_dirs(&root)?;
        let mut applied = BTreeSet::<String>::new();
        let mut committed = false;
        let commit_result = (|| -> Result<()> {
            for (old, new) in &plan.moves {
                let src = root.join(old);
                let dst = root.join(new);
                if snapshot_file(&root, old)? != snapshots[old]
                    || snapshot_file(&root, new)?.is_some()
                {
                    return Err(format!("CAS changed during move: {old}").into());
                }
                if let Some(parent) = dst.parent() {
                    private_dirs(parent)?;
                }
                applied.insert(old.clone());
                applied.insert(new.clone());
                fs::rename(src, dst)?;
            }
            for (rel, bytes) in &plan.writes {
                let old = plan
                    .moves
                    .iter()
                    .find(|(_, new)| *new == rel)
                    .map(|(old, _)| old.as_str())
                    .unwrap_or(rel);
                let mode = snapshots
                    .get(rel)
                    .and_then(Option::as_ref)
                    .map(|(_, mode)| *mode)
                    .or_else(|| current_notes.iter().find(|n| n.path == old).map(|n| n.mode))
                    .unwrap_or(0o600);
                let expected = if old != rel {
                    snapshots[old].clone()
                } else {
                    snapshots[rel].clone()
                };
                if snapshot_file(&root, rel)? != expected {
                    return Err(format!("CAS changed before replacement: {rel}").into());
                }
                applied.insert(rel.clone());
                safe_write(&root.join(rel), bytes, mode)?;
            }
            for (rel, bytes) in &plan.state_writes {
                let path = root.join(rel);
                let mode = snapshots[rel]
                    .as_ref()
                    .map(|(_, mode)| *mode)
                    .unwrap_or(0o600);
                if snapshot_file(&root, rel)? != snapshots[rel] {
                    return Err(format!("CAS changed before state replacement: {rel}").into());
                }
                applied.insert(rel.clone());
                safe_write(&path, bytes, mode)?;
            }
            sync_changes(&root, &applied)?;
            let after = notes(&root)?;
            let before_map: BTreeMap<String, Vec<u8>> = current_notes
                .iter()
                .map(|n| (n.path.clone(), n.bytes.clone()))
                .collect();
            let after_map: BTreeMap<String, Vec<u8>> = after
                .iter()
                .map(|n| (n.path.clone(), n.bytes.clone()))
                .collect();
            let registry: serde_json::Value = serde_json::from_slice(&virtual_registry)?;
            if current == "links" {
                let ledger = load_moves(&root)?
                    .into_iter()
                    .filter(|(old, new)| {
                        !before_map.contains_key(old) && before_map.contains_key(new)
                    })
                    .collect();
                verification::verify_links(&before_map, &after_map, &ledger, &registry)?;
            } else {
                verification::verify(&before_map, &after_map, &plan.moves, current, &registry)?;
            }
            if std::env::var_os("NACRE_BRAIN_MIGRATE_TEST_FAIL_VERIFY").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                return Err("injected migration verification failure".into());
            }
            let paths: BTreeSet<String> = plan
                .writes
                .keys()
                .cloned()
                .chain(plan.moves.iter().flat_map(|(a, b)| [a.clone(), b.clone()]))
                .collect();
            if brain_git::commit_migration_step(&root, current, &paths)?.is_some() {
                committed = true;
            }
            Ok(())
        })();
        if let Err(err) = commit_result {
            if committed {
                return Err(format!("step committed but follow-up failed: {err}").into());
            }
            for (old, new) in plan.moves.iter().rev() {
                if applied.contains(new) {
                    let dst = root.join(new);
                    let src = root.join(old);
                    if dst.exists() && !src.exists() {
                        private_dirs(src.parent().unwrap_or(&root))?;
                        fs::rename(dst, src)?;
                    }
                }
            }
            let mut rollback_errors = Vec::new();
            for rel in &applied {
                match snapshots.get(rel).cloned().flatten() {
                    Some((bytes, mode)) => {
                        if let Err(e) = safe_write(&root.join(rel), &bytes, mode) {
                            rollback_errors.push(format!("{rel}: {e}"));
                        }
                    }
                    None => {
                        if let Err(e) = fs::remove_file(root.join(rel)) {
                            if e.kind() != std::io::ErrorKind::NotFound {
                                rollback_errors.push(format!("{rel}: {e}"));
                            }
                        }
                    }
                }
            }
            if let Ok(dirs) = note_dirs(&root) {
                for rel in dirs
                    .difference(&original_dirs)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    if let Err(e) = fs::remove_dir(root.join(rel)) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            rollback_errors.push(format!("{}: {e}", rel.display()));
                        }
                    }
                }
            }
            if let Err(error) = sync_changes(&root, &applied) {
                rollback_errors.push(format!("rollback sync: {error}"));
            }
            if !rollback_errors.is_empty() {
                return Err(format!(
                    "{err}; rollback also failed: {}",
                    rollback_errors.join("; ")
                )
                .into());
            }
            return Err(err);
        }
        if committed {
            brain_git::push_migration(&root).map_err(|error| {
                format!("migration {current} committed locally; push failed: {error}")
            })?;
        }
    }
    if let Some(path) = report_path {
        let md = markdown_report(&report_plan, &all_counts);
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&path)?;
        output.write_all(md.as_bytes())?;
        output.sync_all()?;
        if dry {
            println!(
                "dry-run: {} notes scanned; report {}",
                originals.len(),
                path.display()
            );
        }
    }
    for (name, count) in &all_counts {
        println!("{name}: {count} changed paths");
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_rules_match_words_instead_of_substrings() {
        let note = |title| format!("# {title}\nWorlds: Nacre\n\nEvidence.\n");
        assert_eq!(inbox_target(&note("Unlikely scenario")), Some("projects"));
        assert_eq!(inbox_target(&note("Leaderboard feature")), Some("projects"));
        assert_eq!(inbox_target(&note("Lead handoff")), Some("checkpoints"));
        assert_eq!(
            inbox_target(&note("Owner prefers a compact layout")),
            Some("personal")
        );
    }

    #[test]
    fn capture_paths_repoint_without_rewriting_source_urls() {
        let mut record = serde_json::json!({"path":"/old-vault/inbox/a.md","source":"https://example.invalid/inbox/a.md","nested":[{"path":"inbox/a.md"}]});
        let moves = BTreeMap::from([("inbox/a.md".into(), "projects/a.md".into())]);
        assert!(remap_record(&mut record, "", &moves, false));
        assert_eq!(record["path"], "/old-vault/projects/a.md");
        assert_eq!(record["source"], "https://example.invalid/inbox/a.md");
        assert_eq!(record["nested"][0]["path"], "projects/a.md");
    }

    #[test]
    fn header_boundary_keeps_body_fields_and_crlf_payload_bytes() {
        let legacy =
            "# Legacy\n\nSource: fixture\nProject: /home/example/context\n\nOriginal body.\n";
        let categories = brain_registry::Categories {
            worlds: vec!["Nacre".into()],
            ..Default::default()
        };
        let rewritten =
            rewrite_classification(legacy, &categories, &field_lines(header_body(legacy).1));
        assert!(rewritten.find("Worlds: Nacre").unwrap() < rewritten.find("Context:").unwrap());
        for newline in ["\n", "\r\n"] {
            let text = format!("# Title{newline}{newline}Source: fixture{newline}Worlds:{newline}Topics: ci{newline}{newline}Project: /home/example/body{newline}Payload.  {newline}");
            let (_, head, body) = header_body(&text);
            assert!(head.contains("Topics: ci"));
            assert_eq!(
                body,
                format!("Project: /home/example/body{newline}Payload.  {newline}")
            );
            let cats = brain_registry::Categories {
                worlds: vec!["Nacre".into()],
                topics: vec!["ci".into()],
                ..Default::default()
            };
            let rewritten = rewrite_classification(&text, &cats, &field_lines(head));
            assert_eq!(header_body(&rewritten).2.as_bytes(), body.as_bytes());
            assert_eq!(
                rewrite_classification(&rewritten, &cats, &field_lines(header_body(&rewritten).1)),
                rewritten
            );
        }
    }
}
