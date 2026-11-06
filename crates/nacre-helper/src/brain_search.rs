//! Ranked Brain search (N0142), shared by `nacre-brain search` and the viewer's
//! `/api/search`: BM25F over title, categories, topics and body; casefolded,
//! lightly stemmed tokens with compound split/join; registry alias and entity
//! expansion; trigram + Damerau-Levenshtein typo tolerance. Per-note analysis
//! persists in `.brain-state/index/search.json` and is reused while a note's
//! size and modification time are unchanged.
use crate::{brain_web, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        LazyLock, Mutex,
    },
};
use unicode_casefold::UnicodeCaseFold;

const SCHEMA: u32 = 1;
/// Field boosts: title, world/area/entity names, topics, body.
const BOOST: [f64; 4] = [3.0, 2.5, 2.0, 1.0];
const K1: f64 = 1.2;
const B: f64 = 0.75;
const ALIAS: f64 = 0.8;
const WEAK_ALIAS: f64 = 0.5;
const FUZZY: f64 = 0.5;
const ARCHIVED: f64 = 0.6;
const STOP: &[&str] = &[
    "a", "an", "the", "and", "or", "of", "to", "in", "on", "for", "with", "from", "by", "at", "as",
    "is", "are", "was", "be", "this", "that", "it",
];
const SPELLING: &[(&str, &str)] = &[
    ("colour", "color"),
    ("grey", "gray"),
    ("behaviour", "behavior"),
    ("centre", "center"),
    ("licence", "license"),
    ("organise", "organize"),
    ("favourite", "favorite"),
    ("catalogue", "catalog"),
    ("dialogue", "dialog"),
    ("artefact", "artifact"),
    ("cancelled", "canceled"),
];
const VOCABULARY: &[u8] = include_bytes!("brain-migrate-vocabulary.json");

static INDEX: LazyLock<Mutex<Option<Index>>> = LazyLock::new(|| Mutex::new(None));
static DIRTY: AtomicBool = AtomicBool::new(true);

/// Mark the in-memory index stale; the vault watcher calls this on changes.
pub fn invalidate() {
    DIRTY.store(true, Ordering::Release);
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub limit: usize,
    pub worlds: Vec<String>,
    pub areas: Vec<String>,
    pub kind: Option<String>,
    pub since: Option<NaiveDate>,
    pub include_archive: bool,
}

/// Registry world/area resolution shared with the viewer's note model.
pub(crate) struct Categories {
    worlds: HashMap<String, Option<String>>,
    areas: HashMap<String, Option<String>>,
    parents: HashMap<String, String>,
}
impl Categories {
    pub(crate) fn new(registry: &Value) -> Self {
        let table = |key: &str| {
            let entries = registry[key].as_array().cloned().unwrap_or_default();
            let merged: HashMap<&str, &str> = entries
                .iter()
                .map(|e| (str_of(&e["name"]), str_of(&e["merged_into"])))
                .collect();
            let mut map = HashMap::new();
            for entry in &entries {
                let mut name = str_of(&entry["name"]);
                for _ in 0..entries.len() {
                    match merged.get(name) {
                        Some(next) if !next.is_empty() => name = next,
                        _ => break,
                    }
                }
                // The first matching entry decides, even when its chain is broken.
                let valid = merged.get(name).is_some_and(|next| next.is_empty());
                let labels = entry["aliases"].as_array().into_iter().flatten();
                for label in std::iter::once(&entry["name"]).chain(labels) {
                    map.entry(casefold(str_of(label)))
                        .or_insert_with(|| valid.then(|| name.to_owned()));
                }
            }
            map
        };
        let parents = registry["areas"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                (
                    str_of(&a["name"]).to_owned(),
                    str_of(&a["parent"]).to_owned(),
                )
            })
            .collect();
        Self {
            worlds: table("worlds"),
            areas: table("areas"),
            parents,
        }
    }
    fn world(&self, raw: &str) -> Option<String> {
        self.worlds.get(&casefold(raw.trim())).cloned().flatten()
    }
    fn area(&self, raw: &str) -> Option<String> {
        self.areas.get(&casefold(raw.trim())).cloned().flatten()
    }
    /// Canonical (worlds, areas) of a note header, following `merged_into`.
    pub(crate) fn resolve(&self, meta: &Value) -> (BTreeSet<String>, BTreeSet<String>) {
        let labels = |key: &str| {
            str_of(&meta[key])
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let mut worlds: BTreeSet<String> = labels("worlds")
            .iter()
            .filter_map(|x| self.world(x))
            .collect();
        let mut areas: BTreeSet<String> = labels("areas")
            .iter()
            .filter_map(|x| self.area(x))
            .collect();
        areas.extend(
            labels("worlds")
                .iter()
                .filter(|raw| self.world(raw).is_none())
                .filter_map(|raw| self.area(raw)),
        );
        for area in &areas {
            if let Some(parent) = self.parents.get(area) {
                worlds.insert(parent.clone());
            }
        }
        let entity = str_of(&meta["entity"]);
        if worlds.is_empty() && entity.eq_ignore_ascii_case("world") {
            worlds.extend(self.world(str_of(&meta["name"])));
        }
        if areas.is_empty() && entity.eq_ignore_ascii_case("area") {
            if let Some(area) = self.area(str_of(&meta["name"])) {
                worlds.extend(self.parents.get(&area).cloned());
                areas.insert(area);
            }
        }
        (worlds, areas)
    }
}

/// Persisted per-note analysis; `terms` holds "term" or "term:count" per field.
#[derive(Serialize, Deserialize)]
struct Doc {
    path: String,
    size: u64,
    mtime: u64,
    title: String,
    meta: Value,
    terms: [String; 4],
    lens: [u32; 4],
}
#[derive(Deserialize)]
struct Stored {
    schema: u32,
    docs: Vec<Doc>,
}
#[derive(Serialize)]
struct StoredRef<'a> {
    schema: u32,
    docs: &'a [Doc],
}
struct Note {
    id: String,
    kind: &'static str,
    folder: String,
    date: Option<NaiveDate>,
    worlds: Vec<String>,
    areas: Vec<String>,
    review: bool,
}
struct Index {
    root: PathBuf,
    docs: Vec<Doc>,
    notes: Vec<Note>,
    dict: HashMap<String, u32>,
    postings: Vec<Vec<(u32, [u16; 4])>>,
    average: [f64; 4],
    grams: HashMap<u64, Vec<u32>>,
    /// Alias key (concatenated normalized words) -> canonical names.
    aliases: HashMap<String, Vec<usize>>,
    /// Canonical name terms: (word terms, joined compounds).
    canonical: Vec<(Vec<String>, Vec<String>)>,
    categories: Categories,
    registry: Value,
}

fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn casefold(s: &str) -> String {
    s.case_fold().collect()
}
fn spelled(word: &str) -> Option<String> {
    SPELLING
        .iter()
        .find(|(from, _)| *from == word)
        .map(|(_, to)| (*to).to_owned())
}
fn stem(w: &str) -> String {
    let n = w.chars().count();
    let cut = |suffix: &str, min: usize| {
        (n >= min && w.ends_with(suffix)).then(|| w[..w.len() - suffix.len()].to_owned())
    };
    let mut s = cut("ies", 6)
        .map(|s| s + "y")
        .or_else(|| cut("ing", 7))
        .or_else(|| cut("ed", 6))
        .or_else(|| {
            ["ches", "shes", "sses", "xes", "zes"]
                .iter()
                .find_map(|suffix| cut(suffix, 6).map(|_| w[..w.len() - 2].to_owned()))
        })
        .or_else(|| {
            (!["ss", "us", "is"].iter().any(|x| w.ends_with(x)))
                .then(|| cut("s", 5))
                .flatten()
        })
        .unwrap_or_else(|| w.to_owned());
    if s.chars().count() > 4 && s.ends_with('e') {
        s.pop();
    }
    s
}
/// Casefolded word -> index term: spelling variants, then light stemming.
fn normalize(word: &str) -> String {
    let w = spelled(word).unwrap_or_else(|| word.to_owned());
    if w.chars().count() < 4 || !w.chars().all(char::is_alphabetic) {
        return w;
    }
    let s = stem(&w);
    spelled(&s).unwrap_or(s)
}
/// Compound runs: alphanumeric parts joined by '-' or '_' (byte span, parts).
fn runs(text: &str) -> Vec<(usize, usize, Vec<&str>)> {
    let mut out = Vec::new();
    let mut parts = Vec::new();
    let (mut start, mut part) = (None::<usize>, None::<usize>);
    let mut end = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(at, c)) in chars.iter().enumerate() {
        if c.is_alphanumeric() {
            start.get_or_insert(at);
            part.get_or_insert(at);
            end = at + c.len_utf8();
            continue;
        }
        if let Some(p) = part.take() {
            parts.push(&text[p..at]);
        }
        let joiner = (c == '-' || c == '_')
            && start.is_some()
            && chars.get(i + 1).is_some_and(|(_, n)| n.is_alphanumeric());
        if !joiner {
            if let Some(s) = start.take() {
                out.push((s, end, std::mem::take(&mut parts)));
            }
        }
    }
    if let Some(p) = part {
        parts.push(&text[p..]);
    }
    if let Some(s) = start {
        out.push((s, end, parts));
    }
    out
}
/// Index terms of text: each word part plus the joined compound
/// (smash-board -> smash, board, smashboard). Stop words and single
/// characters are not standalone terms.
fn analyze(text: &str, out: &mut Vec<String>) {
    let folded = casefold(text);
    for (_, _, parts) in runs(&folded) {
        for part in &parts {
            if part.chars().count() > 1 && !STOP.contains(part) {
                out.push(normalize(part));
            }
        }
        if (2..=4).contains(&parts.len()) {
            out.push(normalize(&parts.concat()));
        }
    }
}
fn encode(terms: &[String]) -> String {
    let mut counts: Vec<(&str, u32)> = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for t in terms {
        match seen.get(t.as_str()) {
            Some(&i) => counts[i].1 += 1,
            None => {
                seen.insert(t.as_str(), counts.len());
                counts.push((t.as_str(), 1));
            }
        }
    }
    counts
        .iter()
        .map(|(t, n)| {
            if *n > 1 {
                format!("{t}:{n}")
            } else {
                (*t).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn decode(field: &str) -> impl Iterator<Item = (&str, u16)> {
    field
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(|t| match t.rsplit_once(':') {
            Some((term, n)) => (term, n.parse::<u16>().unwrap_or(u16::MAX)),
            None => (t, 1),
        })
}
fn grams(term: &str) -> Vec<u64> {
    let c: Vec<u64> = term.chars().map(|c| c as u64).collect();
    c.windows(3)
        .map(|w| (w[0] << 42) | (w[1] << 21) | w[2])
        .collect()
}
/// Optimal-string-alignment Damerau-Levenshtein distance.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

fn note_doc(root: &Path, path: &str, size: u64, mtime: u64) -> Result<Doc> {
    let body = brain_web::read(&root.join(path), brain_web::NOTE_LIMIT)?;
    let title = body
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(str::trim))
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("")
        })
        .to_owned();
    let meta = brain_web::fields(&body);
    let mut fields: [Vec<String>; 4] = Default::default();
    analyze(&title, &mut fields[0]);
    for key in ["worlds", "areas", "entity", "name", "aliases"] {
        analyze(str_of(&meta[key]), &mut fields[1]);
    }
    for key in ["topics", "tags"] {
        analyze(str_of(&meta[key]), &mut fields[2]);
    }
    let rest = body.split_once('\n').map_or("", |(_, rest)| rest);
    analyze(rest, &mut fields[3]);
    analyze(path.trim_end_matches(".md"), &mut fields[3]);
    Ok(Doc {
        path: path.to_owned(),
        size,
        mtime,
        title,
        meta,
        lens: fields.each_ref().map(|f| f.len() as u32),
        terms: fields.each_ref().map(|f| encode(f)),
    })
}

fn scan(root: &Path) -> Result<Vec<(String, u64, u64)>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|s| s == "md"))
    {
        let rel = entry
            .path()
            .strip_prefix(root)?
            .to_string_lossy()
            .into_owned();
        if brain_web::ROOT_FILES.contains(&rel.as_str()) {
            continue;
        }
        let meta = entry.metadata()?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as u64);
        files.push((rel, meta.len(), mtime));
    }
    files.sort();
    if files.len() > 10000
        || files.iter().map(|f| f.1 as usize).sum::<usize>() > brain_web::VAULT_LIMIT
    {
        return Err("Brain vault exceeds indexing limit".into());
    }
    Ok(files)
}

fn stored_path(root: &Path) -> PathBuf {
    root.join(".brain-state/index/search.json")
}
fn load(root: &Path) -> Vec<Doc> {
    brain_web::read(&stored_path(root), 256 * 1024 * 1024)
        .ok()
        .and_then(|text| serde_json::from_str::<Stored>(&text).ok())
        .filter(|s| s.schema == SCHEMA)
        .map(|s| s.docs)
        .unwrap_or_default()
}
fn save(root: &Path, docs: &[Doc]) -> Result<()> {
    let path = stored_path(root);
    let parent = path.parent().ok_or("Invalid index path")?;
    brain_web::private_dir(&root.join(".brain-state"))?;
    brain_web::private_dir(parent)?;
    brain_web::reject_links(&path)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    let mut writer = std::io::BufWriter::new(file.as_file_mut());
    serde_json::to_writer(
        &mut writer,
        &StoredRef {
            schema: SCHEMA,
            docs,
        },
    )?;
    writer.flush()?;
    drop(writer);
    file.persist(&path)?;
    Ok(())
}

impl Index {
    /// Re-stat the vault, re-analyze changed notes only, and rebuild the
    /// in-memory dictionary, postings, trigram and alias tables.
    fn refresh(previous: Option<Index>, root: &Path) -> Result<Self> {
        let registry = brain_web::registry(root)?;
        let mut previous = previous.filter(|index| index.root == root);
        let old = match previous.as_mut() {
            Some(index) => std::mem::take(&mut index.docs),
            None => load(root),
        };
        let mut by_path: HashMap<String, Doc> =
            old.into_iter().map(|d| (d.path.clone(), d)).collect();
        let mut changed = false;
        let mut docs = Vec::new();
        for (path, size, mtime) in scan(root)? {
            match by_path.remove(&path) {
                Some(doc) if doc.size == size && doc.mtime == mtime => docs.push(doc),
                _ => {
                    // Oversized, vanished or unreadable notes are skipped, not fatal.
                    changed = true;
                    docs.extend(note_doc(root, &path, size, mtime).ok());
                }
            }
        }
        changed |= !by_path.is_empty();
        if let Some(mut index) = previous.filter(|i| !changed && i.registry == registry) {
            index.docs = docs;
            return Ok(index);
        }
        if changed {
            save(root, &docs)?;
        }
        Ok(Self::build(root, docs, registry))
    }

    fn build(root: &Path, docs: Vec<Doc>, registry: Value) -> Self {
        let categories = Categories::new(&registry);
        let mut dict: HashMap<String, u32> = HashMap::new();
        let mut postings: Vec<Vec<(u32, [u16; 4])>> = Vec::new();
        let mut totals = [0f64; 4];
        let mut notes = Vec::with_capacity(docs.len());
        for (d, doc) in docs.iter().enumerate() {
            let mut local: HashMap<u32, [u16; 4]> = HashMap::new();
            for (f, field) in doc.terms.iter().enumerate() {
                totals[f] += f64::from(doc.lens[f]);
                for (term, n) in decode(field) {
                    let next = dict.len() as u32;
                    let id = *dict.entry(term.to_owned()).or_insert(next);
                    if id == next {
                        postings.push(Vec::new());
                    }
                    let tf = local.entry(id).or_default();
                    tf[f] = tf[f].saturating_add(n);
                }
            }
            for (id, tf) in local {
                postings[id as usize].push((d as u32, tf));
            }
            let (worlds, areas) = categories.resolve(&doc.meta);
            let file = Path::new(&doc.path)
                .file_name()
                .map(|f| f.as_encoded_bytes().to_vec())
                .unwrap_or_default();
            notes.push(Note {
                id: format!("note:{}", &brain_web::digest(file)[..16]),
                kind: if doc.path.starts_with("archive/") {
                    "archive"
                } else if doc.path.starts_with("digests/") {
                    "digest"
                } else {
                    "note"
                },
                folder: doc
                    .path
                    .split('/')
                    .next()
                    .filter(|_| doc.path.contains('/'))
                    .unwrap_or("")
                    .to_owned(),
                date: brain_web::date(&doc.meta),
                worlds: worlds.into_iter().collect(),
                areas: areas.into_iter().collect(),
                review: !str_of(&doc.meta["review"]).trim().is_empty(),
            });
        }
        let count = docs.len().max(1) as f64;
        let average = totals.map(|t| (t / count).max(1.0));
        let mut grams_index: HashMap<u64, Vec<u32>> = HashMap::new();
        for (term, &id) in &dict {
            for g in grams(term) {
                grams_index.entry(g).or_default().push(id);
            }
        }
        // Registry worlds/areas, migration entities and wiki entity pages.
        let mut names: Vec<(String, Vec<String>)> = Vec::new();
        for key in ["worlds", "areas"] {
            for entry in registry[key].as_array().into_iter().flatten() {
                let aliases = entry["aliases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| str_of(a).to_owned());
                let target = str_of(&entry["merged_into"]);
                let name = if target.is_empty() {
                    str_of(&entry["name"])
                } else {
                    target
                };
                names.push((
                    name.to_owned(),
                    std::iter::once(str_of(&entry["name"]).to_owned())
                        .chain(aliases)
                        .collect(),
                ));
            }
        }
        let vocabulary: Value = serde_json::from_slice(VOCABULARY).unwrap_or_default();
        for entity in vocabulary["entities"].as_array().into_iter().flatten() {
            let aliases = entity["aliases"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| str_of(a).to_owned());
            names.push((
                str_of(&entity["name"]).to_owned(),
                std::iter::once(str_of(&entity["name"]).to_owned())
                    .chain(aliases)
                    .collect(),
            ));
        }
        for doc in &docs {
            let name = str_of(&doc.meta["name"]);
            if !str_of(&doc.meta["entity"]).is_empty() && !name.is_empty() {
                let aliases = str_of(&doc.meta["aliases"])
                    .split(',')
                    .map(|a| a.trim().to_owned());
                names.push((
                    name.to_owned(),
                    std::iter::once(name.to_owned()).chain(aliases).collect(),
                ));
            }
        }
        let mut aliases: HashMap<String, Vec<usize>> = HashMap::new();
        let mut canonical = Vec::new();
        for (name, labels) in names {
            let (mut words, mut joins) = (Vec::new(), Vec::new());
            for (_, _, parts) in runs(&casefold(&name)) {
                analyze(&parts.join(" "), &mut words);
                if (2..=4).contains(&parts.len()) {
                    joins.push(normalize(&parts.concat()));
                }
            }
            if words.is_empty() && joins.is_empty() {
                continue;
            }
            canonical.push((words, joins));
            for label in labels {
                let key = alias_key(&label);
                if !key.is_empty() {
                    aliases.entry(key).or_default().push(canonical.len() - 1);
                }
            }
        }
        Self {
            root: root.to_owned(),
            docs,
            notes,
            dict,
            postings,
            average,
            grams: grams_index,
            aliases,
            canonical,
            categories,
            registry,
        }
    }

    fn df(&self, term: &str) -> usize {
        self.dict
            .get(term)
            .map_or(0, |&id| self.postings[id as usize].len())
    }
    /// Up to three dictionary terms within the typo budget, nearest first.
    fn corrections(&self, word: &str) -> Vec<String> {
        let target: Vec<char> = word.chars().collect();
        if target.len() < 5 {
            return Vec::new();
        }
        let budget = if target.len() >= 8 { 2 } else { 1 };
        let query = grams(word);
        let mut shared: HashMap<u32, usize> = HashMap::new();
        for g in &query {
            for &id in self.grams.get(g).into_iter().flatten() {
                *shared.entry(id).or_default() += 1;
            }
        }
        let need = query.len().saturating_sub(3 * budget).max(1);
        let mut found: Vec<(usize, usize, String)> = Vec::new();
        for (term, &id) in &self.dict {
            let Some(&n) = shared.get(&id) else { continue };
            let chars: Vec<char> = term.chars().collect();
            if n < need || chars.len().abs_diff(target.len()) > budget {
                continue;
            }
            let d = distance(&target, &chars);
            if d <= budget {
                found.push((
                    d,
                    usize::MAX - self.postings[id as usize].len(),
                    term.clone(),
                ));
            }
        }
        found.sort();
        found.into_iter().take(3).map(|(_, _, t)| t).collect()
    }
}

fn alias_key(label: &str) -> String {
    let folded = casefold(label);
    runs(&folded)
        .iter()
        .flat_map(|(_, _, parts)| parts.iter().map(|p| normalize(p)))
        .collect::<Vec<_>>()
        .concat()
}

/// One expanded query term. A compound join lists its parts: it scores with
/// the idf of its rarest known part, so a rare spelling ("white-motor") does
/// not outrank notes that write the words apart.
struct Term {
    term: String,
    weight: f64,
    source: &'static str,
    parts: Vec<String>,
}
/// Query words (bitmask) a note covers when it matches all `all` terms or any
/// `any` term (bit sets over the expansion's terms).
struct Rule {
    all: u128,
    any: u128,
    mask: u64,
}
struct Expansion {
    terms: Vec<Term>,
    rules: Vec<Rule>,
    words: usize,
    /// (raw word, corrected term) for unknown words fixed by typo tolerance.
    fixes: Vec<(String, String)>,
}
impl Expansion {
    fn add(&mut self, term: String, weight: f64, source: &'static str, parts: Vec<String>) -> u128 {
        if let Some(i) = self.terms.iter().position(|t| t.term == term) {
            let t = &mut self.terms[i];
            if weight > t.weight {
                (t.weight, t.source) = (weight, source);
            }
            return 1 << i;
        }
        if self.terms.len() == 128 {
            return 0;
        }
        self.terms.push(Term {
            term,
            weight,
            source,
            parts,
        });
        1 << (self.terms.len() - 1)
    }
    fn rule(&mut self, all: u128, any: u128, mask: u64) {
        self.rules.push(Rule { all, any, mask });
    }
}
fn expand(index: &Index, query: &str) -> Expansion {
    let folded = casefold(&query.chars().take(200).collect::<String>());
    let all = runs(&folded);
    let mut words: Vec<&str> = Vec::new();
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (_, _, parts) in &all {
        let start = words.len();
        words.extend(
            parts
                .iter()
                .filter(|p| !STOP.contains(*p))
                .take(63usize.saturating_sub(start)),
        );
        groups.push((start, words.len()));
    }
    let mut x = Expansion {
        terms: Vec::new(),
        rules: Vec::new(),
        words: words.len(),
        fixes: Vec::new(),
    };
    let span = |s: usize, e: usize| (s..e).fold(0u64, |m, i| m | (1 << i));
    let mut known = vec![false; words.len()];
    let normalized: Vec<String> = words.iter().map(|w| normalize(w)).collect();
    for (i, term) in normalized.iter().enumerate() {
        if index.df(term) > 0 {
            known[i] = true;
            let bit = x.add(term.clone(), 1.0, "query", Vec::new());
            x.rule(0, bit, span(i, i + 1));
        }
    }
    let mut joins: Vec<(usize, usize)> = (0..words.len().saturating_sub(1))
        .map(|i| (i, i + 2))
        .collect();
    joins.extend(
        groups
            .iter()
            .copied()
            .filter(|(s, e)| e - s > 2 && e - s <= 4),
    );
    for (s, e) in joins {
        let term = normalize(&words[s..e].concat());
        if index.df(&term) > 0 {
            known[s..e].fill(true);
            let bit = x.add(term, 1.0, "compound", normalized[s..e].to_vec());
            x.rule(0, bit, span(s, e));
        }
    }
    let mut matched_alias = vec![false; words.len()];
    for s in 0..words.len() {
        for e in s + 1..=(s + 4).min(words.len()) {
            let key = normalized[s..e].concat();
            let Some(targets) = index.aliases.get(&key) else {
                continue;
            };
            let weight = if e - s > 1 || index.df(&key) == 0 {
                ALIAS
            } else {
                WEAK_ALIAS
            };
            matched_alias[s..e].fill(true);
            for &target in targets {
                let (names, joined) = &index.canonical[target];
                let (mut all, mut any) = (0, 0);
                for term in names.iter().filter(|t| index.df(t) > 0) {
                    all |= x.add(term.clone(), weight, "alias", Vec::new());
                }
                for term in joined.iter().filter(|t| index.df(t) > 0) {
                    any |= x.add(term.clone(), weight, "alias", Vec::new());
                }
                x.rule(all, any, span(s, e));
            }
        }
    }
    for (i, word) in words.iter().enumerate() {
        if known[i] || matched_alias[i] {
            continue;
        }
        let corrections = index.corrections(&normalized[i]);
        if let Some(best) = corrections.first() {
            x.fixes.push(((*word).to_owned(), best.clone()));
        }
        for c in corrections {
            let bit = x.add(c, FUZZY, "fuzzy", Vec::new());
            x.rule(0, bit, span(i, i + 1));
        }
    }
    x
}

/// The best matching line of a note as (1-based line, snippet, highlights in
/// Unicode scalar offsets), plus surface words for corrected terms.
fn snippet(
    text: &str,
    matched: &HashSet<&str>,
    surfaces: &mut HashMap<String, String>,
) -> (usize, String, Vec<[usize; 2]>) {
    let mut best: Option<(f64, usize, &str)> = None;
    let mut header = true;
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if n > 0 && trimmed.is_empty() {
            header = false;
        }
        let folded = casefold(trimmed);
        let mut hits = HashSet::new();
        for (_, _, parts) in runs(&folded) {
            let mut forms: Vec<String> = parts.iter().map(|p| normalize(p)).collect();
            if parts.len() > 1 {
                forms.push(normalize(&parts.concat()));
            }
            for (form, raw) in forms.iter().zip(parts.iter().chain(std::iter::once(&""))) {
                if matched.contains(form.as_str()) {
                    hits.insert(form.clone());
                    if !raw.is_empty() {
                        surfaces
                            .entry(form.clone())
                            .or_insert_with(|| (*raw).to_owned());
                    }
                }
            }
        }
        let penalty = if n == 0 {
            10.0
        } else if header {
            0.25
        } else {
            0.0
        };
        let score = hits.len() as f64 - penalty;
        if !trimmed.is_empty() && best.is_none_or(|(s, _, _)| score > s) {
            best = Some((score, n, trimmed));
        }
    }
    let Some((_, line, chosen)) = best else {
        return (1, String::new(), Vec::new());
    };
    let chars: Vec<char> = chosen.chars().collect();
    let mut marks = Vec::new();
    for (start, end, parts) in runs(chosen) {
        let folded: Vec<String> = parts.iter().map(|p| casefold(p)).collect();
        let joined = normalize(&folded.concat());
        if matched.contains(joined.as_str())
            || folded
                .iter()
                .any(|p| matched.contains(normalize(p).as_str()))
        {
            marks.push([
                chosen[..start].chars().count(),
                chosen[..end].chars().count(),
            ]);
        }
    }
    let first = marks.first().map_or(0, |m| m[0]);
    let from = if chars.len() > 240 {
        first.saturating_sub(80)
    } else {
        0
    };
    let to = chars.len().min(from + 240);
    let mut out: String = chars[from..to].iter().collect();
    let shift = usize::from(from > 0);
    if from > 0 {
        out.insert(0, '…');
    }
    if to < chars.len() {
        out.push('…');
    }
    let marks = marks
        .into_iter()
        .filter(|m| m[0] >= from && m[1] <= to)
        .map(|m| [m[0] - from + shift, m[1] - from + shift])
        .collect();
    (line + 1, out, marks)
}

fn wanted(index: &Index, note: &Note, options: &Options) -> bool {
    let category =
        |values: &[String], wanted: &[String], resolve: &dyn Fn(&str) -> Option<String>| {
            wanted.is_empty()
                || wanted.iter().any(|w| {
                    let name = resolve(w).unwrap_or_else(|| w.clone());
                    values.iter().any(|v| v == &name)
                })
        };
    (options.include_archive || note.kind != "archive")
        && category(&note.worlds, &options.worlds, &|w| {
            index.categories.world(w)
        })
        && category(&note.areas, &options.areas, &|a| index.categories.area(a))
        && options.kind.as_ref().is_none_or(|k| {
            let k = casefold(k.trim());
            let k = k.trim_end_matches('s');
            note.kind.trim_end_matches('s') == k || note.folder.trim_end_matches('s') == k
        })
        && options
            .since
            .is_none_or(|since| note.date.is_some_and(|d| d >= since))
}

/// Rank notes for `query`. The result carries ranked `ids` (viewer
/// compatibility), `results` with snippets and chips, and `didYouMean`.
pub fn search(root: &Path, query: &str, options: &Options) -> Result<Value> {
    let mut guard = INDEX.lock().map_err(|_| "Search index lock failed")?;
    let stale =
        DIRTY.swap(false, Ordering::AcqRel) || guard.as_ref().is_none_or(|i| i.root != root);
    if stale {
        match Index::refresh(guard.take(), root) {
            Ok(index) => *guard = Some(index),
            Err(e) => {
                invalidate();
                return Err(e);
            }
        }
    }
    let index = guard.as_ref().ok_or("Search index missing")?;
    let expansion = expand(index, query);
    let mut scores: HashMap<u32, (f64, u128)> = HashMap::new();
    let count = index.docs.len() as f64;
    let idf = |df: usize| (1.0 + (count - df as f64 + 0.5) / (df as f64 + 0.5)).ln();
    for (i, t) in expansion.terms.iter().enumerate() {
        let Some(&id) = index.dict.get(&t.term) else {
            continue;
        };
        let posting = &index.postings[id as usize];
        let rarest = t
            .parts
            .iter()
            .map(|p| idf(index.df(p)))
            .fold(f64::NAN, f64::max);
        let idf = idf(posting.len()).min(if rarest.is_nan() { f64::MAX } else { rarest });
        for &(d, tf) in posting {
            let lens = index.docs[d as usize].lens;
            let weighted: f64 = (0..4)
                .filter(|&f| tf[f] > 0)
                .map(|f| {
                    BOOST[f] * f64::from(tf[f])
                        / (1.0 - B + B * f64::from(lens[f]) / index.average[f])
                })
                .sum();
            let entry = scores.entry(d).or_default();
            entry.0 += t.weight * idf * weighted * (K1 + 1.0) / (weighted + K1);
            entry.1 |= 1 << i;
        }
    }
    let phrase = casefold(query.trim())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut ranked: Vec<(f64, u32)> = scores
        .into_iter()
        .filter(|(d, _)| wanted(index, &index.notes[*d as usize], options))
        .map(|(d, (score, matched))| {
            let note = &index.notes[d as usize];
            let covered = expansion
                .rules
                .iter()
                .filter(|r| (r.all != 0 && matched & r.all == r.all) || matched & r.any != 0)
                .fold(0u64, |m, r| m | r.mask);
            let coverage = f64::from(covered.count_ones()) / expansion.words.max(1) as f64;
            let title = casefold(&index.docs[d as usize].title)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let exact = if !phrase.is_empty() && title == phrase {
                2.0
            } else {
                1.0
            };
            let archived = if note.kind == "archive" {
                ARCHIVED
            } else {
                1.0
            };
            (score * coverage.min(1.0) * exact * archived, d)
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| {
                index.notes[b.1 as usize]
                    .date
                    .cmp(&index.notes[a.1 as usize].date)
            })
            .then_with(|| {
                index.docs[a.1 as usize]
                    .path
                    .cmp(&index.docs[b.1 as usize].path)
            })
    });
    let limit = if options.limit == 0 {
        20
    } else {
        options.limit
    };
    let matched: HashSet<&str> = expansion.terms.iter().map(|t| t.term.as_str()).collect();
    let mut surfaces = HashMap::new();
    let mut results = Vec::new();
    for (rank, &(score, d)) in ranked.iter().take(limit).enumerate() {
        let (doc, note) = (&index.docs[d as usize], &index.notes[d as usize]);
        let text =
            brain_web::read(&root.join(&doc.path), brain_web::NOTE_LIMIT).unwrap_or_default();
        let (line, excerpt, highlights) = snippet(&text, &matched, &mut surfaces);
        let mut chips: Vec<String> = note
            .areas
            .iter()
            .map(|a| match index.categories.parents.get(a) {
                Some(w) if !w.is_empty() => format!("{w} › {a}"),
                _ => a.clone(),
            })
            .collect();
        for w in &note.worlds {
            if !chips.iter().any(|c| c.starts_with(&format!("{w} › "))) {
                chips.push(w.clone());
            }
        }
        results.push(json!({
            "rank": rank + 1, "id": note.id, "path": doc.path, "file": root.join(&doc.path),
            "line": line, "title": doc.title, "snippet": excerpt, "highlights": highlights,
            "score": (score * 1000.0).round() / 1000.0, "worlds": note.worlds, "areas": note.areas,
            "chips": chips, "kind": note.kind, "folder": note.folder,
            "date": note.date.map(|d| d.to_string()).unwrap_or_default(),
            "confidence": str_of(&doc.meta["confidence"]), "review": note.review,
            "archived": note.kind == "archive",
        }));
    }
    let did_you_mean = (!expansion.fixes.is_empty() && !ranked.is_empty()).then(|| {
        let mut out = casefold(query.trim());
        for (raw, term) in &expansion.fixes {
            out = out.replace(raw.as_str(), surfaces.get(term).unwrap_or(term));
        }
        out
    });
    Ok(json!({
        "query": query,
        "count": ranked.len(),
        "ids": ranked.iter().map(|&(_, d)| index.notes[d as usize].id.clone()).collect::<Vec<_>>(),
        "results": results,
        "didYouMean": did_you_mean,
        "expanded": expansion.terms.iter().filter(|t| t.source != "query")
            .map(|t| json!({"term": t.term, "weight": t.weight, "source": t.source})).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(root: &Path, path: &str, body: &str) {
        let p = root.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    fn titles(root: &Path, query: &str, options: &Options) -> Vec<String> {
        invalidate();
        let value = search(root, query, options).unwrap();
        value["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["title"].as_str().unwrap().to_owned())
            .collect()
    }
    #[test]
    fn analyzer_splits_joins_and_stems() {
        let mut out = Vec::new();
        analyze(
            "Smash-Board batteries, HK416 dev-1 updated colours",
            &mut out,
        );
        assert_eq!(
            out,
            [
                "smash",
                "board",
                "smashboard",
                "battery",
                "hk416",
                "dev",
                "dev1",
                "updat",
                "color"
            ]
        );
        assert_eq!(alias_key("Smash card"), alias_key("smash cards"));
        assert_eq!(distance(&['a', 'b', 'c'], &['a', 'c', 'b']), 1);
        assert_eq!(
            distance(
                &"tailscail".chars().collect::<Vec<_>>(),
                &"tailscal".chars().collect::<Vec<_>>()
            ),
            1
        );
    }
    #[test]
    fn ranks_aliases_compounds_typos_and_filters() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        note(root, "projects/board.md", "# Smash-Board firmware\nRecorded: 2026-11-01\nWorlds: Newbringer\nAreas: Smash-Board\n\nFlashing the board.\n");
        note(
            root,
            "inbox/card.md",
            "# SD card reader\nRecorded: 2026-11-02\n\nThe card reader on the laptop.\n",
        );
        note(
            root,
            "infrastructure/tailnet.md",
            "# Tailscale ACL\nRecorded: 2026-11-03\nAreas: Network\n\nTailscale serves dev-1.\n",
        );
        note(
            root,
            "devices/hk.md",
            "# HK416 trigger\nRecorded: 2026-10-01\n\nThe HK416 trigger sensor.\n",
        );
        note(
            root,
            "archive/checkpoints/2026-09/old.md",
            "# Smash-Board old checkpoint\nRecorded: 2026-09-01\n\nOld smash board notes.\n",
        );
        let all = Options::default();
        assert_eq!(titles(root, "smash card", &all)[0], "Smash-Board firmware");
        assert_eq!(titles(root, "smashboard", &all), ["Smash-Board firmware"]);
        assert_eq!(titles(root, "hk 416", &all), ["HK416 trigger"]);
        assert_eq!(titles(root, "dev1", &all), ["Tailscale ACL"]);
        let value = search(root, "tailscail", &all).unwrap();
        assert_eq!(value["results"][0]["title"], "Tailscale ACL");
        assert_eq!(value["didYouMean"], "tailscale");
        assert_eq!(
            value["results"][0]["chips"],
            json!(["Infrastructure › Network"])
        );
        assert!(value["results"][0]["highlights"]
            .as_array()
            .is_some_and(|h| !h.is_empty()));
        assert!(titles(root, "qzxv plorktunk", &all).is_empty());
        let archive = Options {
            include_archive: true,
            ..Options::default()
        };
        assert_eq!(
            titles(root, "smash board", &archive),
            ["Smash-Board firmware", "Smash-Board old checkpoint"]
        );
        let world = Options {
            worlds: vec!["newbringer".into()],
            ..Options::default()
        };
        assert_eq!(titles(root, "board", &world), ["Smash-Board firmware"]);
        let kind = Options {
            kind: Some("inbox".into()),
            ..Options::default()
        };
        assert_eq!(titles(root, "card", &kind), ["SD card reader"]);
        let since = Options {
            since: NaiveDate::from_ymd_opt(2026, 11, 2),
            ..Options::default()
        };
        assert_eq!(titles(root, "tailscale card", &since).len(), 2);
        assert!(stored_path(root).is_file());
    }
    #[test]
    fn reuses_unchanged_notes_and_sees_edits() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        note(root, "inbox/a.md", "# Alpha\n\nfirst words\n");
        assert_eq!(titles(root, "first", &Options::default()), ["Alpha"]);
        note(root, "inbox/a.md", "# Alpha\n\nsecond longer words\n");
        note(root, ".hidden/b.md", "# Hidden\n\nsecond\n");
        note(root, "AGENTS.md", "# Agents\n\nsecond\n");
        assert_eq!(titles(root, "second", &Options::default()), ["Alpha"]);
        assert!(titles(root, "first", &Options::default()).is_empty());
        fs::remove_file(root.join("inbox/a.md")).unwrap();
        assert!(titles(root, "second", &Options::default()).is_empty());
    }
}
