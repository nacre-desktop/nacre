//! Deterministic Brain consolidation (N0145; DESIGN §7–9, §16), no model calls.
//!
//! One run archives and triages old notes, repairs links with the migration's
//! shared resolver, refreshes digests, wiki evidence blocks and the INDEX,
//! keeps the self-growing taxonomy tidy, flags stale and contradicting facts in
//! the derived index and writes small context packs. Everything a run changes
//! in the vault is one verified, revertable commit made through the migration
//! transaction; derived flags live in `.brain-state/index/`, never in prose.
use super::{
    date_from, digest, field_lines, generated, header_body, lists, notes, report_target,
    retention_target, snapshot_file, state_updates, transact, verification, write_report, Apply,
    Note, Plan,
};
use crate::{brain_data, brain_git, brain_registry, brain_search, brain_web, Result};
use chrono::NaiveDate;
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::LazyLock,
};

type Notes = BTreeMap<String, Vec<u8>>;

const CHECKPOINT_DAYS: i64 = 14;
const ADDRESS_DAYS: i64 = 14;
const VERSION_DAYS: i64 = 30;
const PACK_LIMIT: usize = 4096;
const EVIDENCE: &[&str] = &[
    "inbox",
    "checkpoints",
    "decisions",
    "runbooks",
    "projects",
    "devices",
    "personal",
    "infrastructure",
    "archive",
    "raw",
];
/// Evidence that still describes the present (archive, raw and digests do not).
const ACTIVE: &[&str] = &[
    "inbox",
    "checkpoints",
    "decisions",
    "runbooks",
    "projects",
    "devices",
    "personal",
    "infrastructure",
];
const ADDRESS_WORDS: &[&str] = &["tailscale", "tailnet", "network", "lan", "wifi", "vpn"];
const SEED: &[u8] = include_bytes!("../../../brain/registry-seed.json");
const VOCABULARY: &[u8] = include_bytes!("brain-migrate-vocabulary.json");

fn rx(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid consolidation pattern")
}
static IPV4: LazyLock<Regex> =
    LazyLock::new(|| rx(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b"));
static REVISION: LazyLock<Regex> = LazyLock::new(|| rx(r"\br(\d{2,6})\b"));
static VERSION: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)\b(?:v|version\s+|release\s+)(\d+\.\d+(?:\.\d+)*)\b"));
static DEPLOYED: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)\b(?:deployed|installed|running|pinned)\b"));
static HASH: LazyLock<Regex> = LazyLock::new(|| rx(r"\b[0-9a-f]{7,40}\b"));
/// Words before a hex token that make it a serial, digest or id, not a commit.
static NOT_COMMIT: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)(?:serial|sha-?\d*|digest|boot|uuid|\bid)\W*$"));
/// Update wording: a newer value with it supersedes an older one (DESIGN §7).
static CUE: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)\b(?:now|currently|updated?|upgraded?|changed|moved|migrated|switched|replaced|renumbered|reassigned|rotated|bumped|re-?deploy\w*|deploy\w*|install\w*|became|new|latest|from\s+\S+\s+to)\b",
    )
});
/// A repo, branch, package or host token (`helios-web`, `owner/repo`, `newbringer-v2-…`).
static IDENT: LazyLock<Regex> =
    LazyLock::new(|| rx(r"\b[A-Za-z][A-Za-z0-9]*(?:[-_/@][A-Za-z0-9]+)+\b"));
/// Notes this many days apart asserting different values contradict each other;
/// further apart, a newer value is ordinary progress (addresses excepted).
const WINDOW_DAYS: i64 = 3;
static RUNS_ON: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)\bruns? on (?:the )?([a-z0-9][a-z0-9._-]*[a-z0-9])"));
static STATE: LazyLock<Regex> = LazyLock::new(|| rx(r"(?i)\b(not\s+)?(enabled|disabled)\b"));
static PROVENANCE: LazyLock<Regex> =
    LazyLock::new(|| rx(r"\s*\(([^()]*\b(\d{4}-\d{2}-\d{2})\b[^()]*)\)\.?\s*$"));
static DECISION_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"(?i)\b(?:decid\w*|decision|prefer\w*|policy|owner wants|always|never)\b")
});
static ADDRESS_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)\b(?:ip|ipv4|ipv6|address(?:es)?|tailscale|tailnet|hostname|mac|online|offline|reachable|unreachable|ssh|dns|port)\b",
    )
});
static VERSION_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)\b(?:r\d{2,6}|v\d+\.\d+\S*|version|revision|release|deploy\w*|install\w*|pinned|commit)\b",
    )
});
static REVERSAL: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)\b(?:supersed\w*|revert\w*|disabl\w*|roll(?:s|ed)? back|rollback|undo(?:es|ne)?|reverse[sd]?)\b",
    )
});
static WIKILINK: LazyLock<Regex> = LazyLock::new(|| rx(r"\[\[([^\]|#]+)[^\]]*\]\]"));
static OPEN_HEADING: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)^#+\s*(?:open (?:questions|loops)|todo|next steps?|pending)\b"));
static DO_HEADING: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)^#+\s*do(?:\s*(?:and|/|&)\s*)don'?t"));
static DO_LINE: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)^(?:do:|don'?t\b|do not\b|never\b|always\b)"));

/// Today, or `NACRE_BRAIN_TODAY` (YYYY-MM-DD) so tests can inject the clock.
pub(super) fn today() -> Result<NaiveDate> {
    match std::env::var("NACRE_BRAIN_TODAY") {
        Ok(value) if !value.is_empty() => NaiveDate::parse_from_str(&value, "%Y-%m-%d")
            .map_err(|_| "NACRE_BRAIN_TODAY expects YYYY-MM-DD".into()),
        _ => Ok(chrono::Local::now().date_naive()),
    }
}

fn root_of(path: &str) -> &str {
    path.split_once('/').map_or("", |(root, _)| root)
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or("")
}

fn clip(value: &str, limit: usize) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() <= limit {
        return value;
    }
    let mut out: String = value.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn link(path: &str) -> String {
    format!("[[{}]]", path.trim_end_matches(".md"))
}

/// Body lines outside the header and the generated evidence block, numbered from 1.
fn prose_lines(text: &str) -> Vec<(usize, &str)> {
    let body = header_body(text).2;
    let skip = text[..text.len() - body.len()].matches('\n').count();
    let mut generated = false;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.starts_with(generated::START.split(" (").next().unwrap_or("")) {
            generated = true;
        } else if line.starts_with(generated::END) {
            generated = false;
        } else if i >= skip && !generated {
            out.push((i + 1, line));
        }
    }
    out
}

/// The summary paragraph at the top of a page's curated block (N0148), if any.
fn curated_summary(text: &str) -> Option<String> {
    let start = text.find(generated::CURATED_START)?;
    let after = &text[start..];
    let paragraph: Vec<&str> = after
        .lines()
        .skip(1)
        .map(str::trim)
        .take_while(|l| !l.is_empty() && !l.starts_with(['-', '*', '#', '<']))
        .collect();
    (!paragraph.is_empty()).then(|| paragraph.join(" "))
}

/// Registry, vocabulary and wiki names that identify what a fact is about.
struct Names {
    long: Option<Regex>,
    short: Option<Regex>,
    canonical: HashMap<String, (String, u8)>,
}

impl Names {
    fn new(registry: &Value, vocabulary: &Value) -> Self {
        let mut labels = Vec::<(String, String, u8)>::new();
        let follow = |name: &str| {
            let mut name = name.to_owned();
            for _ in 0..64 {
                let next = registry["areas"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|a| a["name"].as_str() == Some(name.as_str()))
                    .and_then(|a| a["merged_into"].as_str())
                    .filter(|t| !t.is_empty());
                match next {
                    Some(target) => name = target.to_owned(),
                    None => break,
                }
            }
            name
        };
        let strings = |v: &Value| -> Vec<String> {
            v.as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        };
        for area in registry["areas"].as_array().into_iter().flatten() {
            let name = area["name"].as_str().unwrap_or_default();
            let target = follow(name);
            for label in std::iter::once(name.to_owned()).chain(strings(&area["aliases"])) {
                labels.push((label, target.clone(), 0));
            }
        }
        for entity in vocabulary["entities"].as_array().into_iter().flatten() {
            let name = entity["name"].as_str().unwrap_or_default().to_owned();
            for label in std::iter::once(name.clone()).chain(strings(&entity["aliases"])) {
                labels.push((label, name.clone(), 0));
            }
        }
        for world in registry["worlds"].as_array().into_iter().flatten() {
            let name = world["name"].as_str().unwrap_or_default().to_owned();
            for label in std::iter::once(name.clone()).chain(strings(&world["aliases"])) {
                // Short world aliases (os, me, job) are ordinary words.
                if label.chars().count() > 3 {
                    labels.push((label, name.clone(), 1));
                }
            }
        }
        let mut canonical = HashMap::new();
        let mut long = Vec::new();
        let mut short = Vec::new();
        labels.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
        for (label, name, priority) in labels {
            let trimmed = label.trim();
            if trimmed.chars().count() < 2
                || !trimmed
                    .chars()
                    .all(|c| c.is_alphanumeric() || " &-_.".contains(c))
            {
                continue;
            }
            let key = trimmed.to_lowercase();
            if canonical.contains_key(&key) {
                continue;
            }
            canonical.insert(key, (name, priority));
            if trimmed.chars().count() <= 3 {
                short.push(regex::escape(trimmed));
            } else {
                long.push(regex::escape(trimmed));
            }
        }
        let build = |parts: Vec<String>, flags: &str| {
            (!parts.is_empty()).then(|| rx(&format!(r"{flags}\b(?:{})\b", parts.join("|"))))
        };
        Self {
            long: build(long, "(?i)"),
            short: build(short, ""),
            canonical,
        }
    }

    /// (start, end, canonical name, priority: 0 area/entity, 1 world).
    fn find(&self, line: &str) -> Vec<(usize, usize, String, u8)> {
        let mut out = Vec::new();
        for re in self.long.iter().chain(self.short.iter()) {
            for m in re.find_iter(line) {
                if let Some((name, priority)) = self.canonical.get(&m.as_str().to_lowercase()) {
                    out.push((m.start(), m.end(), name.clone(), *priority));
                }
            }
        }
        out
    }

    fn canonical(&self, label: &str) -> Option<String> {
        self.canonical
            .get(&label.trim().to_lowercase())
            .map(|(name, _)| name.clone())
    }
}

#[derive(Clone, Debug)]
struct FactLine {
    path: String,
    line: usize,
    text: String,
    date: String,
    class: &'static str,
    stale: bool,
    age: i64,
    superseded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Observation {
    entity: String,
    attribute: String,
    /// Repo/host/device name near the value: one subject per qualifier.
    qualifier: String,
    date: String,
    wiki: bool,
    path: String,
    line: usize,
    value: String,
    /// The line uses update wording (now, updated, deployed, …).
    cue: bool,
}

impl Observation {
    fn json(&self) -> Value {
        json!({"entity":self.entity,"attribute":self.attribute,"qualifier":self.qualifier,"value":self.value,"path":self.path,"line":self.line,"date":self.date,"source":if self.wiki {"wiki"} else {"note"}})
    }

    /// "dev-1 tailscale-ip" or "Desktop deployed-commit (helios-web)".
    fn subject(&self) -> String {
        if self.qualifier.is_empty() {
            format!("{} {}", self.entity, self.attribute)
        } else {
            format!("{} {} ({})", self.entity, self.attribute, self.qualifier)
        }
    }
}

/// One typed value on a line: (entity, attribute, qualifier, value).
#[derive(Debug, PartialEq, Eq)]
struct Typed {
    entity: String,
    attribute: String,
    qualifier: String,
    value: String,
}

#[derive(Default)]
struct Facts {
    lines: Vec<FactLine>,
    current: Vec<Observation>,
    /// (newer, older): values that disagree without an update in between.
    contradictions: Vec<(Observation, Observation)>,
    /// (newer, older): a later value replacing an older one (normal progress).
    supersessions: Vec<(Observation, Observation)>,
    reversals: Vec<(String, String, String, String)>,
}

fn fact_class(text: &str) -> &'static str {
    if DECISION_CLASS.is_match(text) {
        "decision"
    } else if IPV4.is_match(text) || ADDRESS_CLASS.is_match(text) {
        "address"
    } else if VERSION_CLASS.is_match(text)
        || HASH
            .find_iter(text)
            .any(|m| m.as_str().chars().any(|c| c.is_ascii_digit()))
    {
        "version"
    } else {
        "other"
    }
}

fn attribute_days(attribute: &str) -> i64 {
    if ["revision", "version", "deployed-commit"].contains(&attribute) {
        VERSION_DAYS
    } else {
        ADDRESS_DAYS
    }
}

/// A commit hash on a deploy line: 7–12 or 40 hex digits with a deploy word
/// close by, not glued to a longer token and not a serial, digest or id.
fn commit_hash(line: &str) -> Option<regex::Match<'_>> {
    let near = |from: usize, to: usize| {
        let from = line.floor_char_boundary(from);
        let to = line.ceil_char_boundary(to.min(line.len()));
        DEPLOYED.is_match(&line[from..to])
    };
    HASH.find_iter(line).find(|m| {
        let token = m.as_str();
        let glued = |c: Option<char>| matches!(c, Some('-' | '_' | '.' | ':' | '/' | '@'));
        // Sentence punctuation after a hash is fine; `.g56…` or `:ab…` is not.
        let mut after = line[m.end()..].chars();
        let glued_after = match after.next() {
            Some('.' | ':') => after.next().is_some_and(|c| c.is_ascii_alphanumeric()),
            next => glued(next),
        };
        matches!(token.len(), 7..=12 | 40)
            && token.chars().any(|c| c.is_ascii_digit())
            && token.chars().any(|c| c.is_ascii_alphabetic())
            && !glued(line[..m.start()].chars().next_back())
            && !glued_after
            && !NOT_COMMIT
                .is_match(&line[line.floor_char_boundary(m.start().saturating_sub(16))..m.start()])
            && (near(m.start().saturating_sub(48), m.start()) || near(m.end(), m.end() + 24))
    })
}

/// The repo/host/device name qualifying the value at `start`: the name part of
/// the token the value is embedded in (`nacre-niri-26.04.r3063`), else the
/// nearest other name or identifier-like token between the previous value on
/// the line (or the clause start, at most 64 bytes back) and the value.
fn qualifier(line: &str, floor: usize, start: usize, entity: &str, names: &Names) -> String {
    let own = |label: &str| {
        names
            .canonical(label)
            .is_some_and(|name| name.eq_ignore_ascii_case(entity))
            || label.eq_ignore_ascii_case(entity)
    };
    let label = |token: &str| {
        names
            .canonical(token)
            .unwrap_or_else(|| token.to_lowercase())
    };
    let prefix_len: usize = line[..start]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || "-_./".contains(*c))
        .map(char::len_utf8)
        .sum();
    let token_start = start - prefix_len;
    let name = line[token_start..start]
        .trim_end_matches(|c: char| c.is_ascii_digit() || "-_./".contains(c));
    if name.chars().filter(char::is_ascii_alphabetic).count() >= 3 && !own(name) {
        return label(name);
    }
    let clause = ["; ", ". ", " — ", " | "]
        .iter()
        .filter_map(|sep| line[..token_start].rfind(sep).map(|i| i + sep.len()))
        .max()
        .unwrap_or(0);
    // Overlapping values (`version 1.2.3.4`) must not push the floor past the value.
    let from = line.floor_char_boundary(
        floor
            .max(clause)
            .max(token_start.saturating_sub(64))
            .min(token_start),
    );
    let mut best: Option<(usize, String)> = None;
    for (s, e, name, _) in names.find(line) {
        if s >= from
            && e <= token_start
            && !own(&name)
            && !ADDRESS_WORDS.contains(&line[s..e].to_lowercase().as_str())
            && best.as_ref().is_none_or(|(at, _)| e > *at)
        {
            best = Some((e, name));
        }
    }
    for m in IDENT.find_iter(&line[from..token_start]) {
        let token = m.as_str().trim_end_matches(['-', '_', '/', '@']);
        let end = from + m.end();
        if !own(token) && best.as_ref().is_none_or(|(at, _)| end > *at) {
            best = Some((end, label(token)));
        }
    }
    best.map(|(_, label)| label).unwrap_or_default()
}

/// Typed facts on one line. The entity is the nearest registry/vocabulary name
/// (areas and entities before worlds), else `fallback`; the qualifier names the
/// repo, host or device the value belongs to when the line says so.
fn observe(line: &str, names: &Names, fallback: Option<&str>) -> Vec<Typed> {
    let mut values = Vec::<(usize, usize, String, String)>::new();
    for c in IPV4.captures_iter(line) {
        let m = c.get(0).unwrap();
        let octets: Vec<u32> = (1..=4).filter_map(|i| c[i].parse().ok()).collect();
        let before = line[..m.start()].chars().next_back();
        let after = &line[m.end()..];
        if octets.len() != 4
            || octets.iter().any(|o| *o > 255)
            || [0, 127, 255].contains(&octets[0])
            || octets[0] >= 224
            || matches!(before, Some('.' | 'v' | 'V'))
            || after.starts_with('/')
            || (after.starts_with('.') && after[1..].starts_with(|ch: char| ch.is_ascii_digit()))
        {
            continue;
        }
        let attribute = if octets[0] == 100 && (64..128).contains(&octets[1]) {
            "tailscale-ip"
        } else {
            "ip"
        };
        values.push((m.start(), m.end(), attribute.into(), m.as_str().into()));
    }
    for c in REVISION.captures_iter(line) {
        let m = c.get(0).unwrap();
        values.push((m.start(), m.end(), "revision".into(), m.as_str().into()));
    }
    for c in VERSION.captures_iter(line) {
        let m = c.get(0).unwrap();
        values.push((m.start(), m.end(), "version".into(), c[1].to_owned()));
    }
    if let Some(m) = commit_hash(line) {
        values.push((
            m.start(),
            m.end(),
            "deployed-commit".into(),
            m.as_str()[..7].into(),
        ));
    }
    for c in RUNS_ON.captures_iter(line) {
        let host = c.get(1).unwrap();
        let value = names.canonical(host.as_str()).or_else(|| {
            let lower = host.as_str().to_lowercase();
            (lower.contains('.')
                || lower
                    .split_once('-')
                    .is_some_and(|(a, b)| !a.is_empty() && b.chars().all(|c| c.is_ascii_digit())))
            .then_some(lower)
        });
        if let Some(value) = value {
            let m = c.get(0).unwrap();
            values.push((m.start(), m.start(), "host".into(), value));
        }
    }
    for c in STATE.captures_iter(line) {
        let m = c.get(0).unwrap();
        let value = if c.get(1).is_some() || c[2].eq_ignore_ascii_case("disabled") {
            "disabled"
        } else {
            "enabled"
        };
        let qualifier = line[..m.start()]
            .split(|ch: char| !ch.is_alphanumeric() && ch != '-')
            .rev()
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .find(|w| {
                ![
                    "is",
                    "was",
                    "are",
                    "were",
                    "now",
                    "been",
                    "be",
                    "currently",
                    "still",
                    "remains",
                    "stays",
                    "got",
                    "has",
                    "have",
                    "the",
                    "and",
                    "stay",
                    "remain",
                ]
                .contains(&w.as_str())
            })
            .unwrap_or_default();
        values.push((
            m.start(),
            m.end(),
            format!("state:{qualifier}"),
            value.into(),
        ));
    }
    if values.is_empty() {
        return Vec::new();
    }
    values.sort();
    let entities = names.find(line);
    let mut out = Vec::new();
    let mut floor = 0;
    for (start, end, attribute, value) in values {
        // Network words name the kind of address, not what it belongs to.
        let address = attribute.ends_with("ip");
        let usable: Vec<_> = entities
            .iter()
            .filter(|e| {
                !(address && ADDRESS_WORDS.contains(&line[e.0..e.1].to_lowercase().as_str()))
            })
            .collect();
        let best = usable.iter().map(|e| e.3).min();
        let candidates: Vec<_> = usable.iter().filter(|e| Some(e.3) == best).collect();
        let entity = candidates
            .iter()
            .filter(|e| e.1 <= start)
            .max_by_key(|e| e.1)
            .or_else(|| candidates.iter().filter(|e| e.0 >= end).min_by_key(|e| e.0))
            .map(|e| e.2.clone())
            .or_else(|| fallback.map(str::to_owned));
        let previous = floor;
        floor = floor.max(end);
        let Some(entity) = entity else {
            continue;
        };
        if attribute == "host" && entity.eq_ignore_ascii_case(&value) {
            continue;
        }
        let attribute = match attribute.strip_prefix("state:") {
            Some(q) if q.is_empty() || entity.to_lowercase().split(' ').any(|w| w == q) => {
                "state".to_owned()
            }
            _ => attribute,
        };
        // Enabled/disabled already carry the word they qualify.
        let qualifier = if attribute.starts_with("state") || attribute == "host" {
            String::new()
        } else {
            qualifier(line, previous, start, &entity, names)
        };
        let row = Typed {
            entity,
            attribute,
            qualifier,
            value,
        };
        if !out.contains(&row) {
            out.push(row);
        }
    }
    out
}

/// Fact lines, typed observations, contradictions and reversals (DESIGN §7).
fn scan_facts(vault: &Notes, names: &Names, today: NaiveDate) -> Facts {
    let mut facts = Facts::default();
    let mut observations = Vec::<Observation>::new();
    for (path, bytes) in vault {
        let wiki = path.starts_with("wiki/");
        if !wiki && !ACTIVE.contains(&root_of(path)) {
            continue;
        }
        let text = text(bytes);
        let note_date = date_from(path, text).map(|d| d.to_string());
        let page = wiki.then(|| {
            let fields = field_lines(header_body(text).1);
            fields
                .iter()
                .find(|(k, _)| k == "Name")
                .and_then(|(_, v)| names.canonical(v).or(Some(v.clone())))
                .unwrap_or_default()
        });
        for (number, line) in prose_lines(text) {
            let item = line.trim_start();
            let date = if wiki {
                let Some(body) = item.strip_prefix("- ").or_else(|| item.strip_prefix("* ")) else {
                    continue;
                };
                let Some(provenance) = PROVENANCE.captures(body) else {
                    continue;
                };
                let date = provenance[2].to_owned();
                let Ok(day) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
                    continue;
                };
                let claim = &body[..provenance.get(0).unwrap().start()];
                let superseded = body.contains("~~") || body.to_lowercase().contains("superseded");
                let class = fact_class(claim);
                let age = (today - day).num_days();
                let limit = match class {
                    "address" => Some(ADDRESS_DAYS),
                    "version" => Some(VERSION_DAYS),
                    _ => None,
                };
                facts.lines.push(FactLine {
                    path: path.clone(),
                    line: number,
                    text: clip(claim.trim_matches('~'), 300),
                    date: date.clone(),
                    class,
                    stale: !superseded && limit.is_some_and(|days| age > days),
                    age,
                    superseded,
                });
                if superseded {
                    continue;
                }
                date
            } else {
                match &note_date {
                    Some(date) => date.clone(),
                    None => break,
                }
            };
            let fallback = page.as_deref().filter(|p| !p.is_empty());
            let cue = CUE.is_match(line);
            for typed in observe(line, names, fallback) {
                observations.push(Observation {
                    entity: typed.entity,
                    attribute: typed.attribute,
                    qualifier: typed.qualifier,
                    date: date.clone(),
                    wiki,
                    path: path.clone(),
                    line: number,
                    value: typed.value,
                    cue,
                });
            }
        }
    }
    observations.sort();
    observations.dedup_by(|a, b| {
        (&a.entity, &a.attribute, &a.qualifier, &a.path, &a.value)
            == (&b.entity, &b.attribute, &b.qualifier, &b.path, &b.value)
    });
    let mut keys = BTreeMap::<(String, String, String), Vec<Observation>>::new();
    for o in observations {
        keys.entry((o.entity.clone(), o.attribute.clone(), o.qualifier.clone()))
            .or_default()
            .push(o);
    }
    let day = |d: &str| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
    for (_, mut list) in keys {
        // Newest last; on the same day a wiki fact (curated) outranks a note.
        list.sort_by(|a, b| {
            (&a.date, a.wiki, &a.path, a.line).cmp(&(&b.date, b.wiki, &b.path, b.line))
        });
        let newest = list.last().cloned().expect("non-empty key");
        if !newest.wiki {
            if let Some(older) = list
                .iter()
                .rev()
                .skip(1)
                .find(|o| o.value != newest.value && o.path != newest.path)
            {
                // A later value with update wording supersedes; so does any
                // later value outside the window, except a host's address,
                // which must not change silently. Within the window (or on the
                // same day) different values contradict.
                let later = newest.date > older.date;
                let gap = day(&newest.date)
                    .zip(day(&older.date))
                    .map_or(0, |(n, o)| (n - o).num_days());
                let pair = (newest.clone(), older.clone());
                if later && newest.cue {
                    facts.supersessions.push(pair);
                } else if newest.attribute.ends_with("ip") || gap <= WINDOW_DAYS {
                    facts.contradictions.push(pair);
                } else {
                    facts.supersessions.push(pair);
                }
            }
        }
        facts.current.push(newest);
    }
    // Reversals: a decision that supersedes, reverts or disables something an
    // earlier decision about the same entity (or its Supersedes link) decided.
    let mut decisions = Vec::new();
    for (path, bytes) in vault.iter().filter(|(p, _)| p.starts_with("decisions/")) {
        let text = text(bytes);
        let Some(date) = date_from(path, text) else {
            continue;
        };
        let (title, header, body) = header_body(text);
        let title = title.trim_start_matches("# ").to_owned();
        let own: BTreeSet<String> = names
            .find(&title)
            .into_iter()
            .filter(|e| e.3 == 0)
            .map(|e| e.2)
            .collect();
        let supersedes = field_lines(header)
            .into_iter()
            .find(|(k, _)| k == "Supersedes")
            .map(|(_, v)| v)
            .unwrap_or_default();
        decisions.push((
            date.to_string(),
            path.clone(),
            title,
            own,
            supersedes,
            body.to_owned(),
        ));
    }
    decisions.sort();
    for (i, (date, path, title, _, supersedes, body)) in decisions.iter().enumerate() {
        // Only a decision's title, its `Supersedes:` header or a body line that
        // links the decision it reverses count; reversal words in passing do not.
        let titled = REVERSAL.is_match(title);
        let linked: String = body
            .lines()
            .filter(|line| REVERSAL.is_match(line))
            .flat_map(|line| WIKILINK.find_iter(line).map(|m| m.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        if !titled && linked.is_empty() && supersedes.is_empty() {
            continue;
        }
        let mut found = BTreeSet::new();
        for target in WIKILINK.captures_iter(&format!("{supersedes} {linked}")) {
            let stem = target[1].trim().trim_end_matches(".md");
            let stem = stem.rsplit('/').next().unwrap_or(stem);
            if let Some((_, older, _, _, _, _)) = decisions[..i]
                .iter()
                .find(|d| d.1.trim_end_matches(".md").rsplit('/').next() == Some(stem))
            {
                found.insert((older.clone(), "supersedes".to_owned()));
            }
        }
        let entities: BTreeSet<String> = if titled {
            names
                .find(title)
                .into_iter()
                .filter(|e| e.3 == 0)
                .map(|e| e.2)
                .collect()
        } else {
            BTreeSet::new()
        };
        for entity in entities {
            if let Some(older) = decisions[..i].iter().rev().find(|d| d.3.contains(&entity)) {
                if !found.iter().any(|(p, _)| *p == older.1) {
                    found.insert((older.1.clone(), entity));
                }
            }
        }
        for (older, entity) in found {
            facts
                .reversals
                .push((path.clone(), date.clone(), older, entity));
        }
    }
    facts
}

/// Context pack path for a world or area (vault-unique file name: the viewer
/// keys notes by file name, and wiki pages already use `<world>[-<area>].md`).
pub(super) fn pack_path(world: &str, area: Option<&str>) -> String {
    match area {
        Some(area) => format!(
            "packs/pack-{}-{}.md",
            generated::slug(world),
            generated::slug(area)
        ),
        None => format!("packs/pack-{}.md", generated::slug(world)),
    }
}

struct Scope<'a> {
    world: &'a str,
    area: Option<&'a str>,
}

/// An evidence note with its resolved categories.
struct Member {
    path: String,
    worlds: BTreeSet<String>,
    areas: BTreeSet<String>,
    date: String,
    title: String,
}

struct PackInputs<'a> {
    vault: &'a Notes,
    registry: &'a Value,
    shown: &'a Value,
    pages: BTreeMap<(String, String), String>,
    categories: Vec<Member>,
    facts: &'a Facts,
    today: NaiveDate,
}

fn render_pack(inputs: &PackInputs<'_>, scope: &Scope<'_>) -> Option<String> {
    let PackInputs {
        vault,
        registry,
        shown,
        pages,
        categories,
        facts,
        today,
    } = inputs;
    let (kind, name) = match scope.area {
        Some(area) => ("area", area),
        None => ("world", scope.world),
    };
    let page = pages
        .get(&(kind.to_owned(), name.to_owned()))
        .filter(|p| vault.contains_key(*p))
        .cloned();
    let in_scope = |worlds: &BTreeSet<String>, areas: &BTreeSet<String>| match scope.area {
        Some(area) => areas.contains(area),
        None => worlds.contains(scope.world),
    };
    let mut members: Vec<&Member> = categories
        .iter()
        .filter(|m| in_scope(&m.worlds, &m.areas))
        .collect();
    let curated = |p: &String| facts.lines.iter().any(|f| f.path == *p);
    // An uncurated stub page alone is not worth a pack.
    if members.is_empty() && !page.as_ref().is_some_and(curated) {
        return None;
    }
    members.sort_by(|a, b| (&b.date, &b.path).cmp(&(&a.date, &a.path)));
    let member_paths: BTreeSet<&str> = members.iter().map(|m| m.path.as_str()).collect();
    let mut scope_pages: BTreeSet<String> = page.iter().cloned().collect();
    if scope.area.is_none() {
        // A world pack also draws on its areas' pages.
        scope_pages.extend(
            shown["areas"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| a["parent"].as_str() == Some(scope.world))
                .filter_map(|a| pages.get(&("area".into(), a["name"].as_str()?.into())))
                .filter(|p| vault.contains_key(*p))
                .cloned(),
        );
    }
    let relevant = |path: &str| member_paths.contains(path) || scope_pages.contains(path);
    let page_text = page.as_ref().map(|p| text(&vault[p])).unwrap_or("");
    let page_fields = field_lines(header_body(page_text).1);
    let field = |key: &str| {
        page_fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    // Identity: the page's first prose paragraph, aliases and placement.
    let mut identity = Vec::new();
    let prose = |l: &&str| !l.is_empty() && !l.starts_with(['#', '<', '-', '*']);
    let paragraph: Vec<&str> = prose_lines(page_text)
        .into_iter()
        .map(|(_, l)| l.trim())
        .skip_while(|l| !prose(l))
        .take_while(prose)
        .collect();
    // A curated summary (N0148) describes the page best; else its first paragraph.
    if let Some(summary) = curated_summary(page_text) {
        identity.push(clip(&summary, 400));
    } else if !paragraph.is_empty() {
        identity.push(clip(&paragraph.join(" "), 400));
    }
    let entry = registry[if scope.area.is_some() {
        "areas"
    } else {
        "worlds"
    }]
    .as_array()
    .into_iter()
    .flatten()
    .find(|e| e["name"].as_str() == Some(name));
    let aliases: Vec<&str> = entry
        .and_then(|e| e["aliases"].as_array())
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(6)
        .collect();
    let mut placement = match scope.area {
        Some(_) => format!("Area of {}.", scope.world),
        None => {
            let children: Vec<&str> = shown["areas"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| a["parent"].as_str() == Some(scope.world))
                .filter_map(|a| a["name"].as_str())
                .collect();
            if children.is_empty() {
                "World.".to_owned()
            } else {
                format!("World; areas: {}.", children.join(", "))
            }
        }
    };
    if !aliases.is_empty() {
        placement.push_str(&format!(" Also called: {}.", aliases.join(", ")));
    }
    identity.push(clip(&placement, 400));
    // Current state.
    let active = members
        .iter()
        .filter(|m| ACTIVE.contains(&root_of(&m.path)))
        .count();
    let newest = members
        .iter()
        .map(|m| m.date.as_str())
        .find(|d| !d.is_empty());
    let mut state = Vec::new();
    if let Some(status) = field("Status") {
        let reviewed = field("Reviewed")
            .map(|r| format!(" (reviewed {r})"))
            .unwrap_or_default();
        state.push(clip(&format!("Status: {status}{reviewed}."), 200));
    }
    let stale: Vec<&FactLine> = facts
        .lines
        .iter()
        .filter(|f| f.stale && scope_pages.contains(&f.path))
        .collect();
    let contradictions: Vec<&(Observation, Observation)> = facts
        .contradictions
        .iter()
        .filter(|(newer, older)| relevant(&newer.path) || relevant(&older.path))
        .collect();
    let reversals: Vec<&(String, String, String, String)> = facts
        .reversals
        .iter()
        .filter(|r| relevant(&r.0) || relevant(&r.2))
        .collect();
    state.push(format!(
        "{} notes ({active} active){}; {} open review items.",
        members.len(),
        newest.map(|d| format!(", newest {d}")).unwrap_or_default(),
        stale.len() + contradictions.len() + reversals.len()
    ));
    // Wiki facts whose value a newer note replaced: the page needs an update.
    let outdated: Vec<&(Observation, Observation)> = facts
        .supersessions
        .iter()
        .filter(|(_, older)| older.wiki && scope_pages.contains(&older.path))
        .collect();
    // Key facts: newest current wiki facts, then typed observations.
    let contradicted: BTreeSet<(&str, usize)> = contradictions
        .iter()
        .chain(outdated.iter())
        .filter(|(_, older)| older.wiki)
        .map(|(_, older)| (older.path.as_str(), older.line))
        .collect();
    let mut key_facts = Vec::<(String, String)>::new();
    for fact in facts.lines.iter().filter(|f| {
        scope_pages.contains(&f.path)
            && !f.stale
            && !f.superseded
            && !contradicted.contains(&(f.path.as_str(), f.line))
    }) {
        let source = vault
            .get(&fact.path)
            .and_then(|b| text(b).lines().nth(fact.line - 1))
            .and_then(|line| WIKILINK.captures(line))
            .and_then(|c| {
                let target = c[1].trim().trim_end_matches(".md").to_owned();
                vault
                    .contains_key(&format!("{target}.md"))
                    .then_some(format!("{target}.md"))
            })
            .unwrap_or_else(|| fact.path.clone());
        key_facts.push((
            fact.date.clone(),
            format!(
                "- {}: {} {}",
                fact.date,
                clip(&fact.text, 170),
                link(&source)
            ),
        ));
    }
    for o in facts
        .current
        .iter()
        .filter(|o| !o.wiki && member_paths.contains(o.path.as_str()))
    {
        let fresh = NaiveDate::parse_from_str(&o.date, "%Y-%m-%d")
            .is_ok_and(|d| (*today - d).num_days() <= attribute_days(&o.attribute));
        if fresh {
            key_facts.push((
                o.date.clone(),
                format!(
                    "- {}: {}: {} {}",
                    o.date,
                    o.subject(),
                    o.value,
                    link(&o.path)
                ),
            ));
        }
    }
    key_facts.sort_by(|a, b| (&b.0, &a.1).cmp(&(&a.0, &b.1)));
    key_facts.dedup_by(|a, b| a.1 == b.1);
    let key_facts: Vec<String> = key_facts.into_iter().take(20).map(|(_, l)| l).collect();
    // Open questions.
    let mut open = Vec::new();
    for fact in &stale {
        open.push(format!(
            "- Recheck {} fact from {}: {} {}",
            fact.class,
            fact.date,
            clip(&fact.text, 120),
            link(&fact.path)
        ));
    }
    for (newer, older) in &outdated {
        open.push(format!(
            "- Update {}: {} is now {} ({}) {}, was {}",
            link(&older.path),
            newer.subject(),
            newer.value,
            newer.date,
            link(&newer.path),
            older.value
        ));
    }
    for (newer, older) in &contradictions {
        open.push(format!(
            "- {}: {} ({}) {} vs {} ({}) {}",
            newer.subject(),
            newer.value,
            newer.date,
            link(&newer.path),
            older.value,
            older.date,
            link(&older.path)
        ));
    }
    for (decision, _, older, entity) in &reversals {
        open.push(format!(
            "- {} reverses {} ({entity})",
            link(decision),
            link(older)
        ));
    }
    let mut dos = Vec::new();
    let mut section_open = false;
    let mut section_do = false;
    for (_, line) in prose_lines(page_text) {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            section_open = OPEN_HEADING.is_match(trimmed);
            section_do = DO_HEADING.is_match(trimmed);
            continue;
        }
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .unwrap_or("");
        if let Some(task) = item.strip_prefix("[ ] ") {
            open.push(format!("- {}", clip(task, 200)));
        } else if section_open && !item.is_empty() {
            open.push(format!("- {}", clip(item, 200)));
        } else if !item.is_empty() && (section_do || DO_LINE.is_match(item)) {
            dos.push(format!("- {}", clip(item, 200)));
        }
    }
    // Where to look.
    let mut look = Vec::new();
    if let Some(page) = &page {
        look.push(format!(
            "- Wiki: [[{}|{name}]]",
            page.trim_end_matches(".md")
        ));
    }
    let digests: Vec<&String> = vault
        .iter()
        .filter(|(p, b)| {
            p.starts_with("digests/") && {
                let f = field_lines(header_body(text(b)).1);
                let has = |key: &str, value: &str| {
                    f.iter()
                        .any(|(k, v)| k == key && lists(v).iter().any(|x| x == value))
                };
                match scope.area {
                    Some(area) => has("Areas", area),
                    None => has("Worlds", scope.world),
                }
            }
        })
        .map(|(p, _)| p)
        .rev()
        .take(3)
        .collect();
    if !digests.is_empty() {
        look.push(format!(
            "- Digests: {}",
            digests
                .iter()
                .map(|p| link(p))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let search = match scope.area {
        Some(area) => format!("--area \"{area}\""),
        None => format!("--world \"{}\"", scope.world),
    };
    look.push(format!("- Search: `nacre-brain search {search} WORDS`"));
    for m in members
        .iter()
        .filter(|m| ACTIVE.contains(&root_of(&m.path)))
        .take(5)
    {
        look.push(format!(
            "- {} [[{}|{}]]",
            if m.date.is_empty() { "?" } else { &m.date },
            m.path.trim_end_matches(".md"),
            clip(&m.title, 90).replace(['[', ']', '|'], "")
        ));
    }
    // Assemble under the hard cap, trimming the least important lines first.
    let label = match scope.area {
        Some(area) => format!("{} › {area}", scope.world),
        None => scope.world.to_owned(),
    };
    let recorded = members
        .iter()
        .map(|m| m.date.clone())
        .chain(
            facts
                .lines
                .iter()
                .filter(|f| scope_pages.contains(&f.path))
                .map(|f| f.date.clone()),
        )
        .chain(field("Reviewed").map(|r| r.chars().take(10).collect()))
        .filter(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        .max();
    let mut head = format!("# Context pack: {label}\n\n");
    if let Some(recorded) = recorded {
        head.push_str(&format!("Recorded: {recorded}\n"));
    }
    head.push_str(&format!(
        "Source: generated by nacre-brain-maintain consolidate; edit {} instead\nWorlds: {}\n",
        page.as_deref().unwrap_or("the wiki"),
        scope.world
    ));
    if let Some(area) = scope.area {
        head.push_str(&format!("Areas: {area}\n"));
    }
    head.push_str(&format!(
        "\nLoad this first, then search for specifics, then open the evidence. Save new facts with `nacre-brain note {search}`.\n"
    ));
    let mut sections: Vec<(&str, Vec<String>)> = vec![
        ("Identity", identity),
        ("Current state", state),
        ("Key facts", key_facts),
        ("Open questions", open.into_iter().take(8).collect()),
        ("Where to look", look),
        ("Do and don't", dos.into_iter().take(6).collect()),
    ];
    let render = |sections: &[(&str, Vec<String>)]| {
        let mut out = head.clone();
        for (title, lines) in sections.iter().filter(|(_, l)| !l.is_empty()) {
            out.push_str(&format!("\n## {title}\n\n{}\n", lines.join("\n")));
        }
        out
    };
    for (index, keep) in [
        (4, 3),
        (3, 3),
        (5, 2),
        (2, 10),
        (3, 0),
        (5, 0),
        (4, 1),
        (2, 3),
        (0, 1),
    ] {
        while render(&sections).len() > PACK_LIMIT && sections[index].1.len() > keep {
            sections[index].1.pop();
        }
    }
    let mut out = render(&sections);
    while out.len() > PACK_LIMIT {
        let Some((_, lines)) = sections.iter_mut().rev().find(|(_, l)| !l.is_empty()) else {
            break;
        };
        lines.pop();
        out = render(&sections);
    }
    Some(out)
}

/// Facts and context packs derived from one vault snapshot (shared with curation).
pub(super) struct Derived {
    facts: Facts,
    pub(super) packs: Notes,
}

/// Scan facts and render every world/area context pack for `vault`.
pub(super) fn derive(vault: &Notes, reg: &Value, today: NaiveDate) -> Result<Derived> {
    let mut shown = reg.clone();
    if let Some(areas) = shown["areas"].as_array_mut() {
        areas.retain(|a| a["status"].as_str() != Some("proposed"));
    }
    let vocabulary: Value = serde_json::from_slice(VOCABULARY)?;
    let names = Names::new(reg, &vocabulary);
    let facts = scan_facts(vault, &names, today);
    let pages = generated::roles(&vocabulary, &shown)
        .into_iter()
        .filter(|(_, kind, _, _, _)| kind == "world" || kind == "area")
        .map(|(page, kind, name, _, _)| ((kind, name), page))
        .collect();
    let resolver = brain_search::Categories::new(reg);
    let categories = vault
        .iter()
        .filter(|(p, _)| EVIDENCE.contains(&root_of(p)))
        .map(|(path, bytes)| {
            let text = text(bytes);
            let (worlds, areas) = resolver.resolve(&brain_web::fields(text));
            Member {
                path: path.clone(),
                worlds,
                areas,
                date: date_from(path, text)
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                title: header_body(text).0.trim_start_matches("# ").to_owned(),
            }
        })
        .collect();
    let inputs = PackInputs {
        vault,
        registry: reg,
        shown: &shown,
        pages,
        categories,
        facts: &facts,
        today,
    };
    let mut rendered = Notes::new();
    for world in reg["worlds"].as_array().into_iter().flatten() {
        let world_name = world["name"].as_str().unwrap_or_default();
        if let Some(pack) = render_pack(
            &inputs,
            &Scope {
                world: world_name,
                area: None,
            },
        ) {
            rendered.insert(pack_path(world_name, None), pack.into_bytes());
        }
        for area in shown["areas"].as_array().into_iter().flatten().filter(|a| {
            a["parent"].as_str() == Some(world_name)
                && a["status"].as_str() != Some("merged-into")
                && a["merged_into"].as_str().is_none_or(str::is_empty)
        }) {
            let area_name = area["name"].as_str().unwrap_or_default();
            let scope = Scope {
                world: world_name,
                area: Some(area_name),
            };
            if let Some(pack) = render_pack(&inputs, &scope) {
                rendered.insert(pack_path(world_name, Some(area_name)), pack.into_bytes());
            }
        }
    }
    let mut combined = vault.clone();
    combined.extend(rendered.clone());
    let broken_pack_links: Vec<_> = verification::broken_links(&combined)
        .into_iter()
        .filter(|(path, _)| path.starts_with("packs/"))
        .collect();
    if let Some((path, target)) = broken_pack_links.first() {
        return Err(format!("generated pack {path} has a broken link: {target}").into());
    }
    drop(inputs);
    Ok(Derived {
        facts,
        packs: rendered,
    })
}

/// What one run changes, flags and reports.
struct Run {
    plan: Plan,
    registry: Vec<u8>,
    changes: Vec<String>,
    facts: Value,
    review: Value,
    counts: BTreeMap<&'static str, usize>,
    deferred: Vec<(String, String)>,
    broken: Vec<(String, String)>,
    pack_paths: Vec<String>,
}

/// Plan one run on a lock-held snapshot. `held` paths have uncommitted edits:
/// they are neither moved, rewritten nor regenerated (their moves wait).
fn plan(
    notes: &[Note],
    registry: &[u8],
    packs: &Notes,
    today: NaiveDate,
    held: &BTreeSet<String>,
) -> Result<Run> {
    let state: Notes = notes
        .iter()
        .map(|n| (n.path.clone(), n.bytes.clone()))
        .collect();
    let mut deferred = Vec::new();
    // Retention (§9): triage old inbox notes by rule, archive old checkpoints
    // and unclear inbox notes, keeping file names.
    let mut moves = BTreeMap::<String, String>::new();
    let mut targets = BTreeSet::new();
    for n in notes {
        let text = String::from_utf8_lossy(&n.bytes);
        let triaged = retention_target(&n.path, &text, today, CHECKPOINT_DAYS, "inbox");
        let from = triaged.as_deref().unwrap_or(&n.path);
        let Some(dest) =
            retention_target(from, &text, today, CHECKPOINT_DAYS, "archive").or(triaged)
        else {
            continue;
        };
        if held.contains(&n.path) {
            deferred.push((n.path.clone(), "uncommitted edits".to_owned()));
        } else if state.contains_key(&dest) || !targets.insert(dest.clone()) {
            deferred.push((n.path.clone(), format!("destination exists: {dest}")));
        } else {
            moves.insert(n.path.clone(), dest);
        }
    }
    for path in held.iter().filter(|p| state.contains_key(*p)) {
        for target in verification::link_targets(&state, path) {
            if moves.remove(&target).is_some() {
                deferred.push((target, format!("linked from uncommitted {path}")));
            }
        }
    }
    let relink = |moves: &BTreeMap<String, String>| -> Result<(Notes, BTreeSet<String>)> {
        let relinker = verification::Relinker::new(&state, moves);
        let mut out = Notes::new();
        let mut changed = BTreeSet::new();
        for (path, bytes) in &state {
            let new = moves.get(path).unwrap_or(path);
            let text = std::str::from_utf8(bytes).map_err(|_| format!("non UTF-8 note: {path}"))?;
            let rewritten = relinker.rewrite(text, path, new);
            if rewritten != text {
                changed.insert(new.clone());
            }
            out.insert(new.clone(), rewritten.into_bytes());
        }
        Ok((out, changed))
    };
    let (mut vault, mut relinked) = relink(&moves)?;
    if relinked.iter().any(|p| held.contains(p)) {
        for (old, _) in std::mem::take(&mut moves) {
            deferred.push((old, "an uncommitted note links here".to_owned()));
        }
        (vault, relinked) = relink(&moves)?;
    }
    // Taxonomy upkeep (§16) over the evidence notes as written.
    let usage: Vec<brain_registry::Usage<'_>> = vault
        .iter()
        .filter(|(p, _)| EVIDENCE.contains(&root_of(p)))
        .map(|(path, bytes)| {
            let text = text(bytes);
            let (title, header, body) = header_body(text);
            let labels = field_lines(header)
                .into_iter()
                .filter(|(k, _)| k == "Worlds" || k == "Areas")
                .flat_map(|(_, v)| lists(&v))
                .collect();
            brain_registry::Usage {
                title: title.trim_start_matches("# "),
                body,
                labels,
                date: date_from(path, text)
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            }
        })
        .collect();
    let upkeep = if held.contains("registry.json") {
        deferred.push(("registry.json".into(), "uncommitted taxonomy edits".into()));
        brain_registry::Upkeep {
            registry: registry.to_vec(),
            changes: Vec::new(),
            flags: Vec::new(),
        }
    } else {
        brain_registry::upkeep(registry, &usage, today)?
    };
    drop(usage);
    // Digests, wiki evidence blocks and the INDEX, through the migration
    // generators. Proposed areas get no pages until they are promoted.
    let reg: Value = serde_json::from_slice(&upkeep.registry)?;
    let mut shown = reg.clone();
    if let Some(areas) = shown["areas"].as_array_mut() {
        areas.retain(|a| a["status"].as_str() != Some("proposed"));
    }
    let vocabulary: Value = serde_json::from_slice(VOCABULARY)?;
    let mut generated_paths = BTreeSet::new();
    for (step, taxonomy) in [("digests", &reg), ("wiki", &shown), ("index", &shown)] {
        for (path, bytes) in
            generated::generate(step, &vault, taxonomy, &vocabulary, &today.to_string())?
        {
            if held.contains(&path) {
                deferred.push((path, "uncommitted edits".to_owned()));
                continue;
            }
            generated_paths.insert(path.clone());
            vault.insert(path, bytes);
        }
    }
    let mut plan = Plan::default();
    let mut counts = BTreeMap::new();
    let sources: BTreeMap<&String, &String> = moves.iter().map(|(old, new)| (new, old)).collect();
    for path in relinked.iter().chain(generated_paths.iter()) {
        let original = sources.get(path).copied().unwrap_or(path);
        if state.get(original) != vault.get(path) {
            plan.writes.insert(path.clone(), vault[path].clone());
        }
    }
    *counts.entry("links repaired").or_default() += relinked
        .iter()
        .filter(|p| !generated_paths.contains(*p))
        .count();
    for path in plan.writes.keys().filter(|p| generated_paths.contains(*p)) {
        let key = if path.starts_with("digests/") {
            "digests"
        } else if path.starts_with("wiki/") {
            "wiki pages"
        } else {
            "index"
        };
        *counts.entry(key).or_default() += 1;
    }
    for dest in moves.values() {
        *counts
            .entry(if dest.starts_with("archive/") {
                "archived"
            } else {
                "triaged"
            })
            .or_default() += 1;
    }
    plan.moves = moves;
    if upkeep.registry != registry {
        plan.writes
            .insert("registry.json".into(), upkeep.registry.clone());
    }
    // Freshness and contradictions (§7) go to the derived index only; context
    // packs (§8) are rendered from the same facts.
    let Derived {
        facts,
        packs: rendered,
    } = derive(&vault, &reg, today)?;
    for (path, bytes) in &rendered {
        if packs.get(path) == Some(bytes) {
            continue;
        }
        if held.contains(path) {
            deferred.push((path.clone(), "uncommitted edits".into()));
            continue;
        }
        plan.state_hashes
            .insert(path.clone(), packs.get(path).map(|b| digest(b)));
        plan.writes.insert(path.clone(), bytes.clone());
        *counts.entry("packs").or_default() += 1;
    }
    for (path, bytes) in packs {
        if rendered.contains_key(path) {
            continue;
        }
        if held.contains(path) {
            deferred.push((path.clone(), "uncommitted edits".into()));
            continue;
        }
        plan.state_hashes.insert(path.clone(), Some(digest(bytes)));
        plan.deletes.insert(path.clone());
        *counts.entry("packs removed").or_default() += 1;
    }
    // Review queue and derived facts index.
    let mut items = Vec::new();
    for fact in facts.lines.iter().filter(|f| f.stale) {
        items.push(json!({"kind":"stale-fact","path":fact.path,"line":fact.line,"class":fact.class,"date":fact.date,"ageDays":fact.age,"text":fact.text}));
    }
    for (newer, older) in &facts.contradictions {
        items.push(json!({"kind":"contradiction","entity":newer.entity,"attribute":newer.attribute,"qualifier":newer.qualifier,"paths":[newer.path,older.path],"values":[newer.value,older.value],"dates":[newer.date,older.date]}));
    }
    for (decision, date, older, entity) in &facts.reversals {
        items.push(json!({"kind":"reversal","paths":[decision,older],"date":date,"entity":entity}));
    }
    items.extend(upkeep.flags.iter().cloned());
    for change in &upkeep.changes {
        let page = if let Some(name) = change.strip_prefix("retired proposed area ") {
            format!("wiki/area-{}.md", generated::slug(name))
        } else if let Some(rest) = change.strip_prefix("folded world ") {
            let name = rest.split(" into ").next().unwrap_or(rest);
            format!("wiki/world-{}.md", generated::slug(name))
        } else {
            continue;
        };
        if vault.contains_key(&page) {
            items.push(json!({"kind":"orphan-page","path":page,"reason":change}));
        }
    }
    for (path, reason) in &deferred {
        items.push(json!({"kind":"deferred","path":path,"reason":reason}));
    }
    let facts_json = json!({
        "schema": 1,
        "date": today.to_string(),
        "facts": facts.lines.iter().map(|f| json!({"path":f.path,"line":f.line,"text":f.text,"date":f.date,"class":f.class,"stale":f.stale,"ageDays":f.age,"superseded":f.superseded})).collect::<Vec<_>>(),
        "current": facts.current.iter().map(Observation::json).collect::<Vec<_>>(),
        "contradictions": facts.contradictions.iter().map(|(n, o)| json!({"entity":n.entity,"attribute":n.attribute,"qualifier":n.qualifier,"newer":n.json(),"older":o.json()})).collect::<Vec<_>>(),
        "supersessions": facts.supersessions.iter().map(|(n, o)| json!({"entity":n.entity,"attribute":n.attribute,"qualifier":n.qualifier,"newer":n.json(),"older":o.json()})).collect::<Vec<_>>(),
        "reversals": facts.reversals.iter().map(|(d, date, o, e)| json!({"decision":d,"date":date,"reverses":o,"entity":e})).collect::<Vec<_>>(),
    });
    let review = json!({"schema": 1, "date": today.to_string(), "items": items});
    Ok(Run {
        plan,
        registry: upkeep.registry,
        changes: upkeep.changes,
        facts: facts_json,
        review,
        counts,
        deferred,
        broken: verification::broken_links(&state),
        pack_paths: rendered.into_keys().collect(),
    })
}

fn read_packs(root: &Path) -> Result<Notes> {
    let mut out = Notes::new();
    let dir = root.join("packs");
    match fs::symlink_metadata(&dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(error) => return Err(error.into()),
        Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
            return Err("unsafe Brain packs directory".into())
        }
        Ok(_) => (),
    }
    for entry in fs::read_dir(&dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".md") || name.starts_with('.') {
            continue;
        }
        let rel = format!("packs/{name}");
        if let Some((bytes, _)) = snapshot_file(root, &rel)? {
            out.insert(rel, bytes);
        }
    }
    Ok(out)
}

/// Replace a derived index file privately when its content changed.
fn write_index(root: &Path, name: &str, value: &Value) -> Result<()> {
    let dir = root.join(".brain-state/index");
    brain_web::private_dir(&root.join(".brain-state"))?;
    brain_web::private_dir(&dir)?;
    let path = dir.join(name);
    brain_web::reject_links(&path)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    if fs::read(&path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let mut file = tempfile::NamedTempFile::new_in(&dir)?;
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(&path)?;
    Ok(())
}

fn summary(run: &Run) -> String {
    let parts: Vec<String> = run
        .counts
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(k, n)| format!("{n} {k}"))
        .chain((!run.changes.is_empty()).then(|| format!("{} taxonomy changes", run.changes.len())))
        .collect();
    if parts.is_empty() {
        "no changes".into()
    } else {
        parts.join(", ")
    }
}

fn markdown(run: &Run, outcome: &str) -> String {
    let mut s = format!(
        "# Brain consolidation\n\nResult: {outcome}\n\nSummary: {}\n\n## Moves\n\n",
        summary(run)
    );
    for (old, new) in &run.plan.moves {
        s.push_str(&format!("- `{old}` → `{new}`\n"));
    }
    s.push_str("\n## Taxonomy changes\n\n");
    for change in &run.changes {
        s.push_str(&format!("- {change}\n"));
    }
    s.push_str("\n## Changed files\n\n");
    for path in run.plan.writes.keys().chain(run.plan.deletes.iter()) {
        s.push_str(&format!("- `{path}`\n"));
    }
    s.push_str("\n## Review queue\n\n");
    for item in run.review["items"].as_array().into_iter().flatten() {
        s.push_str(&format!("- {item}\n"));
    }
    s.push_str("\n## Pre-existing broken links\n\nLeft exactly as written; consolidation verifies only that it adds none.\n\n");
    for (path, target) in &run.broken {
        s.push_str(&format!("- `{path}` → `{target}`\n"));
    }
    if run.broken.is_empty() {
        s.push_str("- none\n");
    }
    s
}

/// `nacre-brain-maintain consolidate [--dry-run] [--report PATH]`.
pub fn run(args: &[String]) -> Result<i32> {
    let mut dry = false;
    let mut report = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" => dry = true,
            "--report" => {
                i += 1;
                report = Some(PathBuf::from(args.get(i).ok_or("missing report path")?));
            }
            other => return Err(format!("unknown consolidate argument: {other}").into()),
        }
        i += 1;
    }
    let root = brain_data::configured_root_path()?;
    if !root.is_dir() {
        return Err("Brain vault not found; run nacre-brain init".into());
    }
    let report = report
        .map(|path| report_target(&root, Some(path), "brain-consolidate-report.md"))
        .transpose()?;
    let versioned = root.join(".git").exists();
    if !dry && !versioned {
        return Err("Brain git history is required; run nacre-brain git init".into());
    }
    // Dry run takes no lock and writes nothing; a run holds both vault locks.
    let _lock = (!dry)
        .then(|| brain_git::acquire_vault_lock(&root))
        .transpose()?;
    if let Some(ms) = std::env::var("NACRE_BRAIN_CONSOLIDATE_TEST_HOLD_MS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    let today = today()?;
    let notes = notes(&root)?;
    let registry = snapshot_file(&root, "registry.json")?
        .map(|(bytes, _)| bytes)
        .unwrap_or_else(|| SEED.to_vec());
    let packs = read_packs(&root)?;
    let mut held = BTreeSet::new();
    let mut run = plan(&notes, &registry, &packs, today, &held)?;
    for _ in 0..4 {
        if !versioned {
            break;
        }
        let existing: BTreeSet<String> = run
            .plan
            .moves
            .keys()
            .chain(run.plan.deletes.iter())
            .chain(run.plan.writes.keys().filter(|p| {
                notes.iter().any(|n| n.path == **p)
                    || packs.contains_key(*p)
                    || (p.as_str() == "registry.json" && root.join("registry.json").exists())
            }))
            .filter(|p| !held.contains(*p))
            .cloned()
            .collect();
        let unclean = brain_git::unclean_paths(&root, &existing)?;
        if unclean.is_empty() {
            break;
        }
        held.extend(unclean);
        run = plan(&notes, &registry, &packs, today, &held)?;
    }
    let message = {
        let mut m = format!("brain: consolidate — {}", summary(&run));
        if !run.changes.is_empty() {
            m.push_str("\n\nTaxonomy:\n");
            for change in &run.changes {
                m.push_str(&format!("- {change}\n"));
            }
        }
        m.push_str("\nAgent: nacre-brain-maintain consolidate");
        m
    };
    let empty =
        run.plan.writes.is_empty() && run.plan.moves.is_empty() && run.plan.deletes.is_empty();
    let stale = run.review["items"].as_array().map_or(0, |items| {
        items.iter().filter(|i| i["kind"] == "stale-fact").count()
    });
    println!("consolidate: {}", summary(&run));
    for change in &run.changes {
        println!("taxonomy: {change}");
    }
    println!(
        "facts: {} fact lines, {stale} stale; {} contradictions; {} reversals; {} packs",
        run.facts["facts"].as_array().map_or(0, Vec::len),
        run.facts["contradictions"].as_array().map_or(0, Vec::len),
        run.facts["reversals"].as_array().map_or(0, Vec::len),
        run.pack_paths.len()
    );
    for (path, reason) in &run.deferred {
        println!("deferred: {path} ({reason})");
    }
    if !run.broken.is_empty() {
        eprintln!(
            "nacre-brain: {} pre-existing broken links are left as written (listed by --report)",
            run.broken.len()
        );
    }
    if dry {
        if let Some(path) = &report {
            write_report(path, &markdown(&run, "dry run; nothing written"))?;
        }
        println!("dry-run: nothing written");
        return Ok(0);
    }
    let mut committed = false;
    if !empty {
        let mut plan = std::mem::take(&mut run.plan);
        if !plan.moves.is_empty() {
            let moves = plan.moves.clone();
            state_updates(&root, &moves, None, &mut plan)?;
        }
        committed = transact(
            &root,
            Apply {
                step: "consolidate",
                notes: &notes,
                plan: &plan,
                registry_before: &registry,
                registry: &run.registry,
                message: Some(&message),
            },
        )?;
        run.plan = plan;
    }
    write_index(&root, "facts.json", &run.facts)?;
    write_index(&root, "review.json", &run.review)?;
    let outcome = if committed {
        let head = brain_git::status_json(&root)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|s| s["last_commit"].as_str().map(str::to_owned))
            .unwrap_or_default();
        format!("committed {head}")
    } else {
        "no commit (nothing changed)".to_owned()
    };
    if let Some(path) = &report {
        write_report(path, &markdown(&run, &outcome))?;
    }
    println!("{outcome}");
    if brain_git::has_origin(&root) {
        brain_git::push_migration(&root)
            .map_err(|error| format!("consolidation committed locally; push failed: {error}"))?;
    }
    Ok(0)
}

/// `nacre-brain pack <world|area> [--json]`: the nearest context pack.
pub fn pack(args: &[String]) -> Result<i32> {
    let json_output = args.iter().any(|a| a == "--json");
    let name = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if name.trim().is_empty() {
        return Err("usage: nacre-brain pack <world|area> [--json]".into());
    }
    let root = brain_data::configured_root_path()?;
    let registry = snapshot_file(&root, "registry.json")?
        .map(|(bytes, _)| bytes)
        .unwrap_or_else(|| SEED.to_vec());
    let (world, area) = brain_registry::resolve_label(&registry, &name)?
        .ok_or_else(|| format!("unknown world or area: {name}"))?;
    let candidates = area
        .as_deref()
        .map(|a| pack_path(&world, Some(a)))
        .into_iter()
        .chain([pack_path(&world, None)]);
    for (index, path) in candidates.enumerate() {
        let Some((bytes, _)) = snapshot_file(&root, &path)? else {
            continue;
        };
        let body = String::from_utf8(bytes).map_err(|_| "non UTF-8 context pack")?;
        let fallback = index > 0 && area.is_some();
        if json_output {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "query": name,
                    "world": world,
                    "area": area,
                    "path": path,
                    "file": root.join(&path),
                    "fallback": fallback,
                    "bytes": body.len(),
                    "text": body,
                }))?
            );
        } else {
            if fallback {
                eprintln!("nacre-brain: no pack for {name}; showing the {world} pack");
            }
            print!("{body}");
        }
        return Ok(0);
    }
    Err(format!("no context pack for {name} yet; run nacre-brain-maintain consolidate").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Names {
        let registry: Value = serde_json::from_slice(SEED).unwrap();
        let vocabulary: Value = serde_json::from_slice(VOCABULARY).unwrap();
        Names::new(&registry, &vocabulary)
    }

    fn rows(line: &str, fallback: Option<&str>) -> Vec<(String, String, String, String)> {
        observe(line, &names(), fallback)
            .into_iter()
            .map(|t| (t.entity, t.attribute, t.qualifier, t.value))
            .collect()
    }

    fn row(
        entity: &str,
        attribute: &str,
        qualifier: &str,
        value: &str,
    ) -> (String, String, String, String) {
        (
            entity.into(),
            attribute.into(),
            qualifier.into(),
            value.into(),
        )
    }

    #[test]
    fn extractors_key_typed_facts_by_nearest_entity() {
        let found = rows("dev-1 answers on 100.64.0.5 and prod on 10.1.2.3", None);
        assert!(found.contains(&row("dev-1", "tailscale-ip", "", "100.64.0.5")));
        assert!(
            found.contains(&row("prod", "ip", "", "10.1.2.3")),
            "{found:?}"
        );
        assert_eq!(
            rows("dev-1 tailscale address is now 100.64.0.9", None),
            vec![row("dev-1", "tailscale-ip", "", "100.64.0.9")]
        );
        let found = rows("Bead pinned at r412 and deployed 3cc38598ab", None);
        assert!(
            found.contains(&row("Bead", "revision", "", "r412")),
            "{found:?}"
        );
        assert!(found.contains(&row("Bead", "deployed-commit", "", "3cc3859")));
        let found = rows("The Brain viewer runs on dev-1", None);
        assert!(found.contains(&row("Brain", "host", "", "dev-1")));
        let found = rows("CI: the nightly timer is not enabled", None);
        assert!(found.contains(&row("CI", "state:timer", "", "disabled")));
        // Versions, subnets and loopback are not addresses.
        assert!(
            rows("dev-1 runs v1.2.3.4 on 10.0.0.0/24 and 127.0.0.1", None)
                .iter()
                .all(|(_, a, _, _)| a != "ip")
        );
        assert!(rows("no entity here 10.9.8.7", None).is_empty());
        assert_eq!(
            rows("- address 10.9.8.7", Some("Laptop")),
            vec![row("Laptop", "ip", "", "10.9.8.7")]
        );
    }

    #[test]
    fn qualifiers_name_the_repo_or_host_and_commits_need_a_deploy_word() {
        // The repo or branch a commit belongs to qualifies it.
        assert_eq!(
            rows("Desktop: deployed helios-web at 1a2b3c4d.", None),
            vec![row("Desktop", "deployed-commit", "helios-web", "1a2b3c4")]
        );
        // A package revision belongs to its package, qualified by the host.
        let found = rows("Laptop installed nacre-niri-26.04.r3063.g568232cc", None);
        assert!(
            found.contains(&row("Compositors", "revision", "Laptop", "r3063")),
            "{found:?}"
        );
        // A value inside a larger token is qualified by the token's name part.
        assert_eq!(
            rows("the kiosk build helios-web-r42 shipped", Some("Desktop")),
            vec![row("Desktop", "revision", "helios-web", "r42")]
        );
        // Serials, digests, glued tokens and hashes far from a deploy word are
        // not deployed commits.
        for line in [
            "Laptop deployed; serial 10000000344fe947 verified",
            "Laptop installed SHA256 2b880c14f44da31aac04640212de41cff6cb0a4b043152b0",
            "Laptop deployed boot id 0455e1f7-c6d6-446b-a372-1cf90fb65a4",
            "Laptop pushed 914412f97397eedf54fed3a568ab641ba59ffc15 to the review branch and much later, in a separate step that is unrelated, deployed",
        ] {
            assert!(
                rows(line, None).iter().all(|(_, a, _, _)| a != "deployed-commit"),
                "{line}: {:?}",
                rows(line, None)
            );
        }
    }

    fn note(path: &str, date: &str, body: &str) -> (String, Vec<u8>) {
        (
            path.to_owned(),
            format!(
                "# {path}\n\nRecorded: {date}T09:00:00+00:00\nConfidence: reported\n\n{body}\n"
            )
            .into_bytes(),
        )
    }

    fn scan(notes: &[(String, Vec<u8>)]) -> Facts {
        let vault: Notes = notes.iter().cloned().collect();
        scan_facts(
            &vault,
            &names(),
            NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
        )
    }

    #[test]
    fn realistic_shapes_flag_only_real_contradictions() {
        // Commits for different repos in one area on consecutive days: no flag.
        let facts = scan(&[
            note(
                "checkpoints/a.md",
                "2026-06-10",
                "Desktop: deployed helios-web at 1a2b3c4d.",
            ),
            note(
                "checkpoints/b.md",
                "2026-06-11",
                "Desktop: deployed atlas-api at 5e6f7a8b.",
            ),
            // The same repo two days later: ordinary progress.
            note(
                "checkpoints/c.md",
                "2026-06-12",
                "Desktop: deployed helios-web at 9c8d7e6f.",
            ),
        ]);
        assert!(
            facts.contradictions.is_empty(),
            "{:?}",
            facts.contradictions
        );
        assert_eq!(facts.supersessions.len(), 1);
        assert_eq!(facts.supersessions[0].0.value, "9c8d7e6");
        // A host's Tailscale address changing with "now" wording: supersession.
        let facts = scan(&[
            note(
                "devices/a.md",
                "2026-06-01",
                "dev-1 tailscale address is 100.64.0.5.",
            ),
            note(
                "devices/b.md",
                "2026-06-12",
                "dev-1 tailscale address is now 100.64.0.9.",
            ),
        ]);
        assert!(facts.contradictions.is_empty());
        assert_eq!(
            (
                facts.supersessions[0].0.value.as_str(),
                facts.supersessions[0].1.value.as_str()
            ),
            ("100.64.0.9", "100.64.0.5")
        );
        // The same change without update wording weeks later still flags: an
        // address must not change silently.
        let facts = scan(&[
            note(
                "devices/a.md",
                "2026-06-01",
                "dev-1 tailscale address is 100.64.0.5.",
            ),
            note(
                "devices/b.md",
                "2026-06-12",
                "dev-1 tailscale address is 100.64.0.9.",
            ),
        ]);
        assert_eq!(facts.contradictions.len(), 1);
        // Conflicting addresses on the same day: flag.
        let facts = scan(&[
            note(
                "devices/a.md",
                "2026-06-12",
                "prod LAN address is 10.30.0.4.",
            ),
            note(
                "devices/b.md",
                "2026-06-12",
                "prod LAN address is 10.30.0.5.",
            ),
        ]);
        assert_eq!(facts.contradictions.len(), 1, "{:?}", facts.contradictions);
        assert_eq!(facts.contradictions[0].0.subject(), "prod ip");
        // Different revisions of one package within the window: flag; far apart: progress.
        let facts = scan(&[
            note("projects/a.md", "2026-06-10", "Laptop runs Bead r100."),
            note("projects/b.md", "2026-06-11", "Laptop runs Bead r120."),
        ]);
        assert_eq!(facts.contradictions.len(), 1);
        let facts = scan(&[
            note("projects/a.md", "2026-05-01", "Laptop runs Bead r100."),
            note("projects/b.md", "2026-06-11", "Laptop runs Bead r120."),
        ]);
        assert!(facts.contradictions.is_empty());
    }

    #[test]
    fn reversals_need_a_title_header_or_link_not_a_word_in_passing() {
        let decision = |path: &str, date: &str, title: &str, body: &str| {
            (
                path.to_owned(),
                format!("# {title}\n\nRecorded: {date}T09:00:00+00:00\n\n{body}\n").into_bytes(),
            )
        };
        let facts = scan(&[
            decision(
                "decisions/a.md",
                "2026-06-01",
                "Bead preview stays on the laptop",
                "Keep it.",
            ),
            decision(
                "decisions/b.md",
                "2026-06-02",
                "Bead dashboard header layout",
                "The old Bead header stays disabled while the new one ships.",
            ),
            decision(
                "decisions/c.md",
                "2026-06-03",
                "Revert the Bead preview decision",
                "Undo it.",
            ),
            decision(
                "decisions/d.md",
                "2026-06-04",
                "Laptop sync timer",
                "This reverts [[decisions/a]] for the laptop.",
            ),
        ]);
        let pairs: Vec<(&str, &str)> = facts
            .reversals
            .iter()
            .map(|(d, _, o, _)| (d.as_str(), o.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("decisions/c.md", "decisions/b.md"),
                ("decisions/d.md", "decisions/a.md")
            ]
        );
    }

    #[test]
    fn fact_classes_and_prose_skip_generated_blocks() {
        assert_eq!(
            fact_class("dev-1 tailscale address is 100.64.0.5"),
            "address"
        );
        assert_eq!(fact_class("Bead is pinned at r412"), "version");
        assert_eq!(fact_class("Owner prefers dark panels"), "decision");
        assert_eq!(fact_class("The board has four buttons"), "other");
        let page = format!(
            "# Page\n\nEntity: area\nName: Bead\n\nIntro.\n\n{} \n- 2026-01-01 [[x]]\n{}\n- tail\n",
            generated::START,
            generated::END
        );
        let lines: Vec<&str> = prose_lines(&page).into_iter().map(|(_, l)| l).collect();
        assert!(lines.contains(&"Intro.") && lines.contains(&"- tail"));
        assert!(!lines
            .iter()
            .any(|l| l.contains("[[x]]") || l.starts_with("Name:")));
    }

    #[test]
    fn pack_paths_never_collide_with_wiki_or_digest_names() {
        assert_eq!(pack_path("Newbringer", None), "packs/pack-newbringer.md");
        assert_eq!(
            pack_path("Nacre", Some("Compositors")),
            "packs/pack-nacre-compositors.md"
        );
    }
}
