//! Canonical, self-growing Brain worlds and areas.
//!
//! Callers hold `brain_git::VaultLock` for the duration of this function.
use crate::{brain_git, Result};
use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Categories {
    pub worlds: Vec<String>,
    pub areas: Vec<String>,
    pub topics: Vec<String>,
    pub review: Vec<String>,
    pub changes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    patterns: Vec<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    title_only: bool,
    #[serde(default)]
    uses: Vec<String>,
    #[serde(default)]
    merged_into: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Registry {
    #[serde(default)]
    version: u32,
    worlds: Vec<Entry>,
    areas: Vec<Entry>,
}

fn remember_alias(entry: &mut Entry, raw: &str, today: &str, changes: &mut Vec<String>) -> bool {
    let raw_key = key(raw);
    if key(&entry.name) == raw_key || entry.aliases.iter().any(|alias| key(alias) == raw_key) {
        return false;
    }
    entry.aliases.push(raw.trim().to_owned());
    entry.updated = today.into();
    changes.push(format!("alias {} → {}", raw.trim(), entry.name));
    true
}
fn promote(entry: &mut Entry, today: &str, changes: &mut Vec<String>) -> bool {
    if entry.status != "proposed" {
        return false;
    }
    entry.status = "auto".into();
    entry.updated = today.into();
    changes.push(format!("promoted area {}", entry.name));
    true
}

fn seed() -> Result<Registry> {
    serde_json::from_slice(include_bytes!("../../../brain/registry-seed.json"))
        .map_err(|e| format!("invalid embedded Brain registry seed: {e}").into())
}
fn key(s: &str) -> String {
    s.nfkc()
        .case_fold()
        .filter(|c| c.is_alphanumeric())
        .collect()
}
fn slug(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}
fn edit_distance(a: &str, b: &str) -> usize {
    let mut row: Vec<usize> = (0..=b.chars().count()).collect();
    for (i, ac) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, bc) in b.chars().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(prev + usize::from(ac != bc));
            prev = old;
        }
    }
    row[b.chars().count()]
}
fn tokens(s: &str) -> BTreeSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|p| p.len() > 1)
        .map(str::to_lowercase)
        .collect()
}
fn similarity(a: &str, b: &str) -> bool {
    let (ta, tb) = (tokens(a), tokens(b));
    let (a, b) = (key(a), key(b));
    if a == b || slug(&a) == slug(&b) {
        return true;
    }
    if a.chars().count() >= 6 && edit_distance(&a, &b) <= 2 {
        return true;
    }
    !ta.is_empty() && !tb.is_empty() && (ta.is_subset(&tb) || tb.is_subset(&ta))
}
pub fn valid_name(name: &str) -> bool {
    let s = name.trim();
    let words: Vec<_> = s.split_whitespace().collect();
    if !s.chars().any(char::is_alphanumeric)
        || s.chars().count() > 40
        || words.is_empty()
        || words.len() > 4
        || s.ends_with(':')
        || s.contains('/')
        || s.contains('\\')
        || s.contains("://")
        || s.contains('#')
    {
        return false;
    }
    if Regex::new(r"(?i)^(topics|worlds|areas|project|review|source|recorded|unsorted|low[- ]confidence|needs[- ]grouping):?$")
        .unwrap()
        .is_match(s)
    {
        return false;
    }
    if Regex::new(r"(?i)(\b\d{4}[-/]\d{1,2}[-/]\d{1,2}\b|\bv?\d+\.\d+(?:\.\d+)?\b|[0-9a-f]{16,})")
        .unwrap()
        .is_match(s)
    {
        return false;
    }
    if Regex::new(r"(?i)^(?:[0-9a-f]{7,}|v?\d+(?:\.\d+){0,2})$")
        .unwrap()
        .is_match(s)
    {
        return false;
    }
    if s.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ')
        || Regex::new(r"(?i)\b(?:(?:19|20)\d{2}|[vr] *\d+|(?:version|release) +\d+)\b")
            .unwrap()
            .is_match(s)
        || Regex::new(r"(?i)^(?:\d{1,2}(?:st|nd|rd|th)?[ -]+)?(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)(?:[ -]+\d{1,4}(?:st|nd|rd|th)?){0,2}$")
            .unwrap()
            .is_match(s)
    {
        return false;
    }
    if s.chars()
        .any(|c| !(c.is_alphanumeric() || c == ' ' || c == '&' || c == '-'))
    {
        return false;
    }
    let first = words[0].to_ascii_lowercase();
    if [
        "this", "these", "those", "there", "when", "because", "how", "why", "what", "we", "i",
        "you", "it", "and", "or",
    ]
    .contains(&first.as_str())
    {
        return false;
    }
    if words.len() >= 3
        && words.iter().any(|w| {
            [
                "is", "are", "was", "were", "has", "have", "will", "can", "should", "would",
            ]
            .contains(&w.to_ascii_lowercase().as_str())
        })
    {
        return false;
    }
    // Fixed protocol/product spellings are retained; ordinary names are title-cased on creation.
    canonical_new(s).chars().count() <= 40
}
fn canonical_new(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            if ["dev-1", "prod", "nacre-niri", "HK416"]
                .iter()
                .any(|f| f.eq_ignore_ascii_case(w))
            {
                return (*["dev-1", "prod", "nacre-niri", "HK416"]
                    .iter()
                    .find(|f| f.eq_ignore_ascii_case(w))
                    .unwrap())
                .to_owned();
            }
            w.split('-')
                .map(|part| {
                    let normalized = part.to_lowercase();
                    let mut c = normalized.chars();
                    c.next()
                        .map(|h| h.to_uppercase().collect::<String>() + c.as_str())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("-")
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn resolve<'a>(r: &'a Registry, label: &str) -> Option<(bool, &'a str, Option<&'a str>)> {
    // Known aliases can contain version identifiers (for example Red v1).
    // They resolve to canonical names, but cannot become new names or aliases.
    if label
        .trim()
        .chars()
        .any(|c| !(c.is_alphanumeric() || c == ' ' || c == '&' || c == '-'))
    {
        return None;
    }
    let exact = |a: &str, b: &str| key(a) == key(b) || slug(a) == slug(b);
    for w in &r.worlds {
        if std::iter::once(&w.name)
            .chain(w.aliases.iter())
            .any(|x| exact(x, label))
        {
            return Some((true, &w.name, None));
        }
    }
    for a in &r.areas {
        if std::iter::once(&a.name)
            .chain(a.aliases.iter())
            .any(|x| exact(x, label))
        {
            return a
                .parent
                .as_deref()
                .map(|w| (false, w, Some(a.name.as_str())));
        }
    }
    if !valid_name(label) {
        return None;
    }
    // Prefer close spelling over broad token containment, across both levels.
    for token_match in [false, true] {
        let matches = |name: &str| {
            if token_match {
                similarity(name, label)
            } else {
                let (a, b) = (key(name), key(label));
                a.chars().count() >= 6 && edit_distance(&a, &b) <= 2
            }
        };
        for w in &r.worlds {
            if std::iter::once(&w.name)
                .chain(w.aliases.iter())
                .any(|x| matches(x))
            {
                return Some((true, &w.name, None));
            }
        }
        for a in &r.areas {
            if std::iter::once(&a.name)
                .chain(a.aliases.iter())
                .any(|x| matches(x))
            {
                return a
                    .parent
                    .as_deref()
                    .map(|w| (false, w, Some(a.name.as_str())));
            }
        }
    }
    None
}
fn score(patterns: &[String], title: &str, body: &str) -> (i32, bool) {
    let mut n = 0;
    let mut title_hit = false;
    for p in patterns {
        let pattern = p.replace("\\\\", "\\");
        if let Ok(rx) = FancyRegex::new(&format!("(?i){pattern}")) {
            if rx.is_match(title).unwrap_or(false) {
                n += 3;
                title_hit = true
            } else if rx.is_match(body).unwrap_or(false) {
                n += 1
            }
        }
    }
    (n, title_hit)
}
fn evidence(entry: &Entry, title: &str, body: &str) -> (i32, bool) {
    let (mut n, mut title_hit) = score(&entry.patterns, title, body);
    let mut seen = BTreeSet::new();
    for label in std::iter::once(&entry.name).chain(entry.aliases.iter()) {
        if !seen.insert(key(label)) {
            continue;
        }
        let escaped = regex::escape(label);
        let flags = if label.chars().count() <= 3 {
            ""
        } else {
            "(?i)"
        };
        let Ok(rx) = Regex::new(&format!(
            "{flags}(?:^|[^[:alnum:]]){escaped}(?:$|[^[:alnum:]])"
        )) else {
            continue;
        };
        if rx.is_match(title) {
            n += 3;
            title_hit = true
        } else if rx.is_match(body) {
            n += 1
        }
    }
    (n, title_hit)
}
fn safe_registry_path(root: &Path) -> Result<PathBuf> {
    let path = root.join("registry.json");
    if fs::symlink_metadata(&path)
        .is_ok_and(|m| m.file_type().is_symlink() || !m.is_file() || m.nlink() != 1)
    {
        return Err("Unsafe Brain registry file".into());
    }
    Ok(path)
}
use std::os::unix::fs::MetadataExt;
fn save(root: &Path, path: &Path, r: &Registry) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(r)?;
    let tmp = root.join(format!(".registry-{}.tmp", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(&tmp)?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Resolve categories and persist taxonomy changes. The caller must hold the vault lock.
pub fn resolve_locked(
    root: &Path,
    title: &str,
    body: &str,
    worlds: &[String],
    areas: &[String],
    topics: &[String],
) -> Result<Categories> {
    resolve_locked_at(
        root,
        title,
        body,
        worlds,
        areas,
        topics,
        &chrono::Local::now().date_naive().to_string(),
    )
}
/// Deterministic-clock form used by writer tests and promotion.
pub fn resolve_locked_at(
    root: &Path,
    title: &str,
    body: &str,
    worlds: &[String],
    areas: &[String],
    topics: &[String],
    today: &str,
) -> Result<Categories> {
    let classifier_body = body.chars().take(4000).collect::<String>();
    let body = classifier_body.as_str();
    let path = safe_registry_path(root)?;
    let (mut r, mut changed) = if path.exists() {
        (
            serde_json::from_slice(&fs::read(&path)?).map_err(|_| "Corrupt Brain registry JSON")?,
            false,
        )
    } else {
        (seed()?, true)
    };
    let mut out = Categories::default();
    let mut explicit_worlds = Vec::new();
    let mut explicit_areas = Vec::new();
    for raw in worlds {
        if let Some((is_world, w, a)) =
            resolve(&r, raw).map(|(kind, w, a)| (kind, w.to_owned(), a.map(str::to_owned)))
        {
            if !explicit_worlds.contains(&w) {
                explicit_worlds.push(w.clone())
            }
            if is_world {
                if let Some(entry) = r.worlds.iter_mut().find(|e| e.name == w) {
                    changed |= remember_alias(entry, raw, today, &mut out.changes);
                }
            } else if let Some(a) = a {
                explicit_areas.push(a.clone());
                if let Some(entry) = r.areas.iter_mut().find(|e| e.name == a) {
                    changed |= remember_alias(entry, raw, today, &mut out.changes);
                    changed |= promote(entry, today, &mut out.changes);
                }
            }
        } else if valid_name(raw) {
            let n = canonical_new(raw);
            r.worlds.push(Entry {
                name: n.clone(),
                aliases: vec![],
                patterns: vec![],
                status: "auto".into(),
                created: today.into(),
                updated: today.into(),
                parent: None,
                title_only: false,
                uses: vec![],
                merged_into: None,
            });
            explicit_worlds.push(n.clone());
            out.changes.push(format!("new world {n}"));
            changed = true;
        } else {
            out.review.push("unknown-world".into());
        }
    }
    for raw in areas {
        if let Some((_, w, a)) =
            resolve(&r, raw).map(|(kind, w, a)| (kind, w.to_owned(), a.map(str::to_owned)))
        {
            if !explicit_worlds.contains(&w) {
                explicit_worlds.push(w)
            }
            if let Some(a) = a {
                explicit_areas.push(a.clone());
                if let Some(entry) = r.areas.iter_mut().find(|e| e.name == a) {
                    changed |= remember_alias(entry, raw, today, &mut out.changes);
                    changed |= promote(entry, today, &mut out.changes);
                }
            }
        } else if valid_name(raw) {
            let n = canonical_new(raw);
            let parent = explicit_worlds
                .first()
                .cloned()
                .or_else(|| best_world(&r, title, body));
            if let Some(parent) = parent {
                r.areas.push(Entry {
                    name: n.clone(),
                    aliases: vec![],
                    patterns: vec![],
                    status: "auto".into(),
                    created: today.into(),
                    updated: today.into(),
                    parent: Some(parent.clone()),
                    title_only: false,
                    uses: vec![today.into()],
                    merged_into: None,
                });
                explicit_areas.push(n.clone());
                explicit_worlds.push(parent);
                out.changes.push(format!("new area {n}"));
                changed = true;
            } else {
                out.review.push("unsorted".into());
            }
        } else {
            out.review.push("unknown-area".into());
        }
    }
    let mut wscore = BTreeMap::<String, i32>::new();
    let mut ascore = BTreeMap::<String, i32>::new();
    for w in &r.worlds {
        let own = evidence(w, title, &body.chars().take(4000).collect::<String>()).0;
        let mut best = 0;
        for a in r
            .areas
            .iter()
            .filter(|a| a.status != "merged-into" && a.parent.as_deref() == Some(&w.name))
        {
            let (s, th) = evidence(a, title, body);
            ascore.insert(a.name.clone(), s);
            if th || !a.title_only {
                best = best.max(s)
            }
        }
        wscore.insert(w.name.clone(), own + best);
    }
    let best = wscore.values().copied().max().unwrap_or(0);
    let mut ranked: Vec<_> = wscore.iter().map(|(w, s)| (w.clone(), *s)).collect();
    ranked.sort_by_key(|(w, s)| (std::cmp::Reverse(*s), w.clone()));
    for (w, s) in ranked {
        let own = r
            .worlds
            .iter()
            .find(|x| x.name == w)
            .map(|x| evidence(x, title, body).0)
            .unwrap_or_default();
        if explicit_worlds.len() >= 3 {
            break;
        }
        if !explicit_worlds.contains(&w)
            && s >= 4
            && s * 2 >= best
            && (explicit_worlds.is_empty() || own >= 2)
        {
            explicit_worlds.push(w);
        }
    }
    let mut seen = BTreeSet::new();
    explicit_worlds.retain(|w| seen.insert(w.clone()));
    if explicit_worlds.is_empty() {
        if let Some(w) = best_world(&r, title, body) {
            explicit_worlds.push(w);
            out.review.push("low-confidence".into())
        } else {
            out.review.push("unsorted".into())
        }
    }
    for w in &explicit_worlds {
        let mut candidates: Vec<_> = r
            .areas
            .iter()
            .filter(|a| {
                a.status != "merged-into"
                    && a.parent.as_deref() == Some(w.as_str())
                    && (!a.title_only || evidence(a, title, body).1)
            })
            .map(|a| {
                (
                    a.name.clone(),
                    ascore
                        .get(&a.name)
                        .copied()
                        .unwrap_or_else(|| evidence(a, title, body).0),
                    evidence(a, title, body).1,
                )
            })
            .collect();
        candidates.sort_by_key(|(n, s, _)| (std::cmp::Reverse(*s), n.clone()));
        let top = candidates.first().map(|x| x.1).unwrap_or(0);
        let secondary = !explicit_areas.is_empty();
        let threshold = if secondary { 6 } else { 3 };
        for (a, _, _) in candidates
            .into_iter()
            .filter(|(_, s, th)| *s >= threshold && *s * 2 >= top && (!secondary || *th))
            .take(1)
        {
            if !explicit_areas.contains(&a) {
                explicit_areas.push(a)
            }
        }
    }
    for t in topics {
        if let Some((_, w, a)) = resolve(&r, t) {
            if explicit_worlds.is_empty() || explicit_worlds.contains(&w.to_owned()) {
                if !explicit_worlds.contains(&w.to_owned()) {
                    explicit_worlds.push(w.to_owned());
                }
                if let Some(a) = a {
                    if !explicit_areas.contains(&a.to_owned()) {
                        explicit_areas.push(a.to_owned())
                    }
                }
                continue;
            }
        }
        let topic = normalize_topic(t);
        if !topic.is_empty() && !out.topics.contains(&topic) {
            out.topics.push(topic);
        }
    }
    // Unmatched classifier subjects are proposals. Only title subjects are considered; body labels
    // and sentence-like text cannot create taxonomy entries.
    if let Some((subject, _)) = title.split_once(':') {
        let candidate = subject.trim();
        if valid_name(candidate) && !resolve(&r, candidate).is_some_and(|(world, _, _)| world) {
            let existing = resolve(&r, candidate)
                .and_then(|(_, _, a)| a)
                .and_then(|name| r.areas.iter().position(|a| a.name == name));
            if let Some(i) = existing {
                let a = &mut r.areas[i];
                changed |= remember_alias(a, candidate, today, &mut out.changes);
                if let Some(parent) = &a.parent {
                    if !out.areas.contains(&a.name) {
                        out.areas.push(a.name.clone());
                    }
                    if !out.worlds.contains(parent) {
                        out.worlds.push(parent.clone());
                    }
                }
            } else if let Some(parent) = explicit_worlds.first().cloned() {
                let n = canonical_new(candidate);
                r.areas.push(Entry {
                    name: n.clone(),
                    aliases: vec![],
                    patterns: vec![],
                    status: "proposed".into(),
                    created: today.into(),
                    updated: today.into(),
                    parent: Some(parent.clone()),
                    title_only: false,
                    uses: vec![],
                    merged_into: None,
                });
                out.areas.push(n.clone());
                out.changes.push(format!("proposed area {n}"));
                changed = true;
            }
        }
    }
    for a in out.areas.drain(..) {
        if !explicit_areas.contains(&a) {
            explicit_areas.push(a)
        }
    }
    let mut seen = BTreeSet::new();
    explicit_areas.retain(|a| seen.insert(a.clone()));
    for name in &explicit_areas {
        if let Some(parent) = r
            .areas
            .iter()
            .find(|a| &a.name == name)
            .and_then(|a| a.parent.as_ref())
        {
            if !explicit_worlds.contains(parent) {
                explicit_worlds.push(parent.clone());
            }
        }
    }
    if explicit_worlds.len() > 3 {
        explicit_worlds.truncate(3);
        out.review.push("too-many-worlds".into());
    }
    if explicit_areas.len() > 4 {
        explicit_areas.truncate(4);
        out.review.push("too-many-areas".into());
    }
    explicit_areas.retain(|name| {
        r.areas
            .iter()
            .find(|a| &a.name == name)
            .and_then(|a| a.parent.as_ref())
            .is_some_and(|w| explicit_worlds.contains(w))
    });
    if !explicit_worlds.is_empty() {
        out.review.retain(|flag| flag != "unsorted");
    }
    out.worlds = explicit_worlds;
    out.areas = explicit_areas;
    // Every selected proposed area counts once per saved capture, whether selected
    // through a title subject, an existing alias, patterns or a topic.
    for area in &mut r.areas {
        if area.status == "proposed" && out.areas.contains(&area.name) {
            area.uses.push(today.into());
            area.updated = today.into();
            changed = true;
            let days: BTreeSet<_> = area.uses.iter().collect();
            if days.len() >= 2 && area.uses.len() >= 3 {
                promote(area, today, &mut out.changes);
            } else {
                let message = format!("proposed area {}", area.name);
                if !out.changes.contains(&message) {
                    out.changes.push(message);
                }
                out.review.push("low-confidence".into());
            }
        }
    }
    out.review.sort();
    out.review.dedup();
    if changed {
        save(root, &path, &r)?;
        brain_git::enqueue_locked(root, &path, "taxonomy")?;
    }
    Ok(out)
}
fn best_world(r: &Registry, title: &str, body: &str) -> Option<String> {
    r.worlds
        .iter()
        .map(|w| {
            let own = evidence(w, title, body).0;
            let area = r
                .areas
                .iter()
                .filter(|a| a.status != "merged-into" && a.parent.as_deref() == Some(&w.name))
                .map(|a| evidence(a, title, body).0)
                .max()
                .unwrap_or(0);
            (w.name.clone(), own.max(area))
        })
        .max_by_key(|(n, s)| (*s, std::cmp::Reverse(n.clone())))
        .filter(|(_, s)| *s >= 2)
        .map(|(n, _)| n)
}
fn normalize_topic(s: &str) -> String {
    let raw = s.trim();
    if raw.contains(':')
        || raw.contains('/')
        || raw.contains('\\')
        || raw.contains('#')
        || key(raw) == "topics"
    {
        return String::new();
    }
    let mut out = String::new();
    let mut dash = false;
    for c in raw.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            if out.len() < 40 {
                out.push(c)
            }
            dash = false;
        } else if !out.is_empty() && !dash && out.len() < 40 {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seed_aliases_and_area_world_resolution() {
        let r = seed().unwrap();
        for w in &r.worlds {
            for alias in std::iter::once(&w.name).chain(w.aliases.iter()) {
                assert!(resolve(&r, alias).is_some(), "world alias {alias}");
            }
        }
        for a in &r.areas {
            for alias in std::iter::once(&a.name).chain(a.aliases.iter()) {
                assert_eq!(
                    resolve(&r, alias).unwrap().2,
                    Some(a.name.as_str()),
                    "area alias {alias}"
                );
            }
        }
        assert_eq!(resolve(&r, "dev-1").unwrap().1, "Infrastructure");
        assert_eq!(resolve(&r, "desktop").unwrap().2, Some("Desktop"));
        assert_eq!(resolve(&r, "Bead green").unwrap().2, Some("Bead"));
        assert_eq!(
            resolve(&r, "Nacre compositrs").unwrap().2,
            Some("Compositors")
        );
        assert_eq!(resolve(&r, "Helpr").unwrap().2, Some("Helper"));
        assert_eq!(resolve(&r, "Red v1").unwrap().2, Some("Guns"));
        assert!(resolve(&r, "/Nacre").is_none());
    }
    #[test]
    fn reject_junk_and_accept_fixed_names() {
        for bad in [
            "Topics:",
            "Unsorted",
            "Needs grouping",
            "---",
            "&",
            "/home/user/project",
            "https://example.test",
            "2026-11-05",
            "2026-11",
            "05-11-2026",
            "Nov 5",
            "November 2026",
            "5th Nov",
            "Nov--11",
            "Aurora v 2",
            "Aurora v2",
            "Aurora release 3",
            "v1.2",
            "abcdef0123456789",
            "abcdef0",
            "42",
            "v2",
            "this is a sentence",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
        for good in [
            "dev-1",
            "prod",
            "nacre-niri",
            "HK416",
            "Home server",
            "Side projects",
        ] {
            assert!(valid_name(good), "{good}");
        }
        assert!(similarity("Bead", "Bead green"));
        assert_eq!(canonical_new("new field"), "New Field");
        assert_eq!(canonical_new("aurora-sensor NODE"), "Aurora-Sensor Node");
        assert_eq!(canonical_new("nacre-niri"), "nacre-niri");
        assert!(!valid_name(&"ß".repeat(40)));
    }
    #[test]
    fn generic_desktop_body_does_not_add_a_secondary_area() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let _lock = brain_git::acquire_vault_lock(root).unwrap();
        let categories = resolve_locked_at(
            root,
            "Renderer evidence",
            "Launcher sidebar wallpaper palette dropdown notifications control center lock screen",
            &["Nacre".into()],
            &["Bead".into()],
            &[],
            "2026-11-06",
        )
        .unwrap();
        assert_eq!(categories.areas, ["Bead"]);
    }

    #[test]
    fn near_duplicate_is_an_alias_and_seed_status_is_protected() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let _lock = brain_git::acquire_vault_lock(root).unwrap();
        let initial_count = seed().unwrap().areas.len();
        let categories = resolve_locked_at(
            root,
            "Evidence",
            "Details",
            &["Nacre".into()],
            &["Compositrs".into()],
            &[],
            "2026-11-06",
        )
        .unwrap();
        assert_eq!(categories.areas, ["Compositors"]);
        resolve_locked_at(
            root,
            "Composiors: finding",
            "Details",
            &["Nacre".into()],
            &[],
            &[],
            "2026-11-06",
        )
        .unwrap();
        let registry: Registry =
            serde_json::from_slice(&fs::read(root.join("registry.json")).unwrap()).unwrap();
        assert_eq!(registry.areas.len(), initial_count);
        let area = registry
            .areas
            .iter()
            .find(|a| a.name == "Compositors")
            .unwrap();
        assert!(area.aliases.iter().any(|a| a == "Compositrs"));
        assert!(area.aliases.iter().any(|a| a == "Composiors"));
        assert_eq!(area.status, "seed");
        assert_eq!(area.updated, "2026-11-06");
    }

    #[test]
    fn explicit_area_mapping_promotes_a_proposal_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let _lock = brain_git::acquire_vault_lock(root).unwrap();
        let proposed = resolve_locked_at(
            root,
            "Photon Garden: finding",
            "Details",
            &["Nacre".into()],
            &[],
            &[],
            "2026-11-06",
        )
        .unwrap();
        assert!(proposed.review.iter().any(|flag| flag == "low-confidence"));
        assert!(proposed
            .changes
            .iter()
            .any(|change| change == "proposed area Photon Garden"));
        let explicit = resolve_locked_at(
            root,
            "Followup",
            "Details",
            &["Photon Garden".into()],
            &[],
            &[],
            "2026-11-06",
        )
        .unwrap();
        assert_eq!(explicit.worlds, ["Nacre"]);
        assert_eq!(explicit.areas, ["Photon Garden"]);
        let registry: Registry =
            serde_json::from_slice(&fs::read(root.join("registry.json")).unwrap()).unwrap();
        assert_eq!(
            registry
                .areas
                .iter()
                .find(|area| area.name == "Photon Garden")
                .unwrap()
                .status,
            "auto"
        );
    }

    #[test]
    fn explicit_growth_and_proposal_promotion() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let _lock = brain_git::acquire_vault_lock(root).unwrap();
        let w = vec!["New Domain".to_owned()];
        let first =
            resolve_locked_at(root, "First note", "details", &w, &[], &[], "2026-11-01").unwrap();
        assert_eq!(first.worlds, ["New Domain"]);
        assert!(first.changes.iter().any(|x| x.contains("new world")));
        let a = vec!["New Area".to_owned()];
        let explicit =
            resolve_locked_at(root, "Note", "details", &w, &a, &[], "2026-11-01").unwrap();
        assert_eq!(explicit.areas, ["New Area"]);
        let empty: Vec<String> = vec![];
        for (title, day) in [
            ("Novel Field: finding", "2026-11-01"),
            ("Novel Field followup", "2026-11-01"),
            ("Novel Field review", "2026-11-02"),
        ] {
            resolve_locked_at(root, title, "details", &w, &empty, &empty, day).unwrap();
        }
        let registry: Registry =
            serde_json::from_slice(&fs::read(root.join("registry.json")).unwrap()).unwrap();
        assert_eq!(
            registry
                .areas
                .iter()
                .find(|e| e.name == "Novel Field")
                .unwrap()
                .status,
            "auto"
        );
    }
}
