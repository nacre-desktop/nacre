//! Standalone Brain graph, routes and bounded optional semantic worker.
use crate::{
    brain_connections, brain_data, brain_discovery, brain_http, brain_launch, brain_search,
    brain_wiki, Result,
};
use chrono::{DateTime, NaiveDate, Utc};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use unicode_casefold::UnicodeCaseFold;
fn casefold(s: &str) -> String {
    s.case_fold().collect()
}
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{mpsc, LazyLock, Mutex},
    time::Duration,
};

pub(crate) const NOTE_LIMIT: u64 = 1_048_576;
pub(crate) const VAULT_LIMIT: usize = 64 * 1024 * 1024;
pub(crate) const ROOT_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "README.md", "INDEX.md"];
const GRAPH_SCHEMA: u32 = 3;
// The independent API credential stays in storage scoped to scheme/host/port.
// Cookies are deliberately insufficient: every localhost port receives them.
const BROWSER_AUTH_SCRIPT: &str = r#"(()=>{const key='nacre-brain-authorization';const params=new URLSearchParams(location.hash.slice(1));const token=params.get('nacre-bootstrap');const original=window.fetch.bind(window);let ready=Promise.resolve();if(token&&/^[0-9a-f]{64}$/.test(token)){history.replaceState(null,'',location.pathname+location.search);ready=original('/api/bootstrap',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token})}).then(response=>response.ok?response.json():null).then(value=>{if(value&&/^[0-9a-f]{64}$/.test(value.authorization))localStorage.setItem(key,value.authorization);}).catch(()=>{});}window.__nacreBrainAuthReady=ready;window.fetch=async(input,init={})=>{await ready;const url=new URL(typeof input==='object'&&input!==null&&typeof input.url==='string'?input.url:String(input),location.href);if(url.origin!==location.origin)return original(input,init);const headers=new Headers(init.headers||(input instanceof Request?input.headers:undefined));const credential=localStorage.getItem(key);if(credential)headers.set('X-Nacre-Brain-Authorization',credential);return original(input,{...init,headers});};})();
"#;
const ACTIVITY_MODEL: &str = "Meaningful saved evidence, 21-day half-life, deduplicated captures and diminishing daily returns. Raw message counting is not enabled.";
pub(crate) fn digest(s: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(s.as_ref()))
}
/// Viewer note ids follow the file name, so moving a note keeps its id.
pub(crate) fn note_id(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|f| f.as_encoded_bytes())
        .unwrap_or_default();
    format!("note:{}", &digest(name)[..16])
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn regex(s: &str) -> Regex {
    brain_discovery::expression(s).expect("constant Brain expression")
}
pub fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?))
}
pub fn state() -> Result<PathBuf> {
    Ok(home()?.join(".local/state/nacre/brain"))
}
pub fn root() -> PathBuf {
    let installed = home().unwrap_or_default().join(".local/share/nacre/brain");
    if installed.join("web/index.html").is_file() {
        installed
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../brain")
    }
}
pub fn port() -> Result<u16> {
    let value = env::var("NACRE_BRAIN_PORT")
        .unwrap_or_else(|_| "17843".into())
        .parse::<u16>()?;
    if value != 0 && value < 1024 {
        return Err("Invalid Brain port".into());
    }
    Ok(value)
}
pub fn vault() -> Result<PathBuf> {
    if let Some(value) = env::var_os("NACRE_BRAIN_DIR").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    brain_data::configured_root_path()
}
pub(crate) fn reject_links(p: &Path) -> Result<()> {
    for a in p.ancestors() {
        if fs::symlink_metadata(a).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Refusing linked Brain state".into());
        }
    }
    Ok(())
}
pub(crate) fn private_dir(p: &Path) -> Result<()> {
    reject_links(p)?;
    fs::create_dir_all(p)?;
    fs::set_permissions(p, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
pub(crate) fn read(p: &Path, limit: u64) -> Result<String> {
    reject_links(p)?;
    let mut bytes = Vec::new();
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(p)?;
    if !file.metadata()?.is_file() {
        return Err("Brain file is not regular".into());
    }
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("Brain file exceeds limit".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
fn read_json(p: &Path) -> Value {
    read(p, 16 * 1024 * 1024)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}))
}
fn write_json(p: &Path, v: &Value) -> Result<()> {
    reject_links(p)?;
    let parent = p.parent().ok_or("Invalid state path")?;
    private_dir(parent)?;
    let mut f = tempfile::NamedTempFile::new_in(parent)?;
    f.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer(&mut f, v)?;
    f.flush()?;
    f.persist(p)?;
    Ok(())
}
pub fn note_path(relative: &str) -> Result<PathBuf> {
    // Web browsing covers every non-hidden Markdown page, including root and
    // user-defined folders; the CLI/sync data layer intentionally has a narrower
    // managed-kind allowlist. Both share the read-only configured vault root.
    note_path_at(&vault()?, relative)
}

fn note_path_at(root: &Path, relative: &str) -> Result<PathBuf> {
    let failure = || -> crate::Error { "Note is outside the managed Markdown vault".into() };
    let path = Path::new(relative);
    if path.is_absolute() || path.components().any(|c|!matches!(c,std::path::Component::Normal(v) if !v.to_string_lossy().starts_with('.'))) || path.extension().is_none_or(|x|x!="md"){return Err(failure())}
    // The shared data-layer validator also refuses links and hidden path components.
    let p = root.join(path);
    reject_links(&p).map_err(|_| failure())?;
    if !p.is_file() {
        return Err(failure());
    }
    Ok(p)
}
pub fn fields(body: &str) -> Value {
    brain_discovery::header_fields(body)
}
pub(crate) fn date(meta: &Value) -> Option<NaiveDate> {
    ["recorded", "reviewed"].into_iter().find_map(|key| {
        meta[key]
            .as_str()
            .and_then(|value| value.get(..10))
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
    })
}
fn timestamp(meta: &Value) -> String {
    let Some(recorded) = meta["recorded"].as_str() else {
        return String::new();
    };
    if let Ok(d) = DateTime::parse_from_rfc3339(recorded) {
        return d.with_timezone(&Utc).to_rfc3339();
    }
    if let Ok(d) = chrono::NaiveDateTime::parse_from_str(recorded, "%Y-%m-%dT%H:%M:%S%.f") {
        return d.and_utc().to_rfc3339();
    }
    if let Ok(d) = NaiveDate::parse_from_str(recorded, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0).unwrap().and_utc().to_rfc3339();
    }
    String::new()
}
pub(crate) fn registry(root: &Path) -> Result<Value> {
    let path = root.join("registry.json");
    if fs::symlink_metadata(&path).is_ok() {
        return Ok(serde_json::from_str(&read(&path, 2 * 1024 * 1024)?)?);
    }
    Ok(serde_json::from_slice(include_bytes!(
        "../../../brain/registry-seed.json"
    ))?)
}
/// Graph node id of a registry area.
pub(crate) fn area_id(name: &str) -> String {
    format!("area:{}", slug_id(name).trim_start_matches("world:"))
}
pub(crate) fn slug_id(name: &str) -> String {
    let slug = regex(r"[^\p{L}\p{N}]+")
        .replace_all(&casefold(name), "-")
        .trim_matches('-')
        .to_owned();
    format!(
        "{}:{}",
        if name.starts_with("area:") {
            "area"
        } else {
            "world"
        },
        slug
    )
}
fn new_category(entry: &Value, today: NaiveDate) -> bool {
    text(&entry["status"]) == "auto"
        && NaiveDate::parse_from_str(text(&entry["created"]), "%Y-%m-%d")
            .is_ok_and(|created| created <= today && (today - created).num_days() < 7)
}
fn hierarchy(root: &Path, all: &[Value], today: NaiveDate) -> Result<(Vec<Value>, Vec<Value>)> {
    let reg = registry(root)?;
    let worlds = reg["worlds"].as_array().cloned().unwrap_or_default();
    let areas = reg["areas"].as_array().cloned().unwrap_or_default();
    let mut world_values = Vec::new();
    let mut hierarchy_nodes = Vec::new();
    let mut membership = Vec::new();
    for w in worlds.iter().filter(|w| text(&w["merged_into"]).is_empty()) {
        let name = text(&w["name"]);
        let id = slug_id(name);
        let mut area_values = Vec::new();
        let mut world_members = BTreeSet::new();
        for a in areas
            .iter()
            .filter(|a| text(&a["parent"]) == name && text(&a["merged_into"]).is_empty())
        {
            let area_name = text(&a["name"]);
            let area_id = area_id(area_name);
            let area_notes = all
                .iter()
                .filter(|n| {
                    n["areas"]
                        .as_array()
                        .is_some_and(|v| v.iter().any(|x| text(x) == area_name))
                })
                .count();
            for n in all.iter().filter(|n| {
                n["areas"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|x| text(x) == area_name))
            }) {
                world_members.insert(text(&n["id"]).to_owned());
            }
            let created = text(&a["created"]);
            let new = new_category(a, today);
            area_values.push(json!({"id":area_id,"label":area_name,"status":a["status"],"created":created,"new":new,"proposed":a["status"]=="proposed","count":area_notes}));
            hierarchy_nodes.push(json!({"id":area_id,"label":area_name,"kind":"area","area":area_name,"world":name,"status":a["status"],"created":created,"new":new,"count":area_notes}));
            membership.push(json!({"source":id,"target":area_id,"kind":"membership"}));
        }
        let created = text(&w["created"]);
        let new = new_category(w, today);
        for n in all.iter().filter(|n| {
            n["worlds"]
                .as_array()
                .is_some_and(|v| v.iter().any(|x| text(x) == name))
        }) {
            world_members.insert(text(&n["id"]).to_owned());
        }
        hierarchy_nodes.push(json!({"id":id,"label":name,"kind":"world","world":name,"status":w["status"],"created":created,"new":new,"count":world_members.len()}));
        world_values.push(json!({"id":id,"label":name,"status":w["status"],"created":created,"new":new,"count":world_members.len(),"areas":area_values}));
    }
    for n in all {
        for world in n["worlds"].as_array().into_iter().flatten() {
            let has_area = n["areas"].as_array().is_some_and(|values| {
                values.iter().any(|area| {
                    areas.iter().any(|a| {
                        text(&a["name"]) == text(area) && text(&a["parent"]) == text(world)
                    })
                })
            });
            if !has_area {
                membership.push(
                    json!({"source":slug_id(text(world)),"target":n["id"],"kind":"membership"}),
                );
            }
        }
        for area in n["areas"].as_array().into_iter().flatten() {
            if areas
                .iter()
                .any(|a| text(&a["name"]) == text(area) && text(&a["merged_into"]).is_empty())
            {
                membership.push(
                    json!({"source":area_id(text(area)),"target":n["id"],"kind":"membership"}),
                );
            }
        }
    }
    Ok((world_values, {
        hierarchy_nodes.extend(membership);
        hierarchy_nodes
    }))
}
fn notes(root: &Path, today: NaiveDate) -> Result<Vec<Value>> {
    let categories = brain_search::Categories::new(&registry(root)?);
    let mut paths = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|s| s == "md"))
        .map(|e| e.into_path())
        .collect::<Vec<_>>();
    paths.sort();
    let mut notes = Vec::new();
    let mut note_ids = BTreeSet::new();
    let mut total = 0;
    for p in paths {
        let rel = p.strip_prefix(root)?.to_string_lossy().into_owned();
        if ROOT_FILES.contains(&rel.as_str()) {
            continue;
        }
        let body = read(&p, NOTE_LIMIT)?;
        total += body.len();
        if total > VAULT_LIMIT || notes.len() >= 10000 {
            return Err("Brain vault exceeds indexing limit".into());
        }
        let title = body
            .lines()
            .find_map(|l| l.strip_prefix("# ").map(str::trim))
            .unwrap_or_else(|| p.file_stem().and_then(|v| v.to_str()).unwrap_or(""))
            .to_owned();
        let meta = fields(&body);
        let day = date(&meta);
        let age = day.map(|d| (today - d).num_days().max(0));
        let activity = age
            .map(|a| (-std::f64::consts::LN_2 * a as f64 / 21.0).exp())
            .unwrap_or(0.0);
        let confidence = meta["confidence"].as_str().unwrap_or("source page");
        let filename = p.file_name().ok_or("Brain note has no filename")?;
        let note_id = note_id(&p);
        if !note_ids.insert(note_id.clone()) {
            return Err(format!(
                "Duplicate Brain note filename: {}",
                filename.to_string_lossy()
            )
            .into());
        }
        let (worlds, areas) = categories.resolve(&meta);
        let kind = if rel.starts_with("archive/") {
            "archive"
        } else if rel.starts_with("digests/") {
            "digest"
        } else {
            "note"
        };
        let review = !text(&meta["review"]).trim().is_empty();
        notes.push(json!({"id":note_id,"label":title,"kind":kind,"path":rel,"date":day.map(|d|d.to_string()).unwrap_or_default(),"recorded":timestamp(&meta),"confidence":confidence,"text":body.chars().take(60000).collect::<String>(),"activity":if day.is_some(){json!(brain_discovery::rounded(activity,4))}else{json!(0)},"revision":&digest(&body)[..16],"meta":meta,"worlds":worlds,"areas":areas,"review":review}));
    }
    Ok(notes)
}
pub fn decode(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(a), Some(c)) = (
                (b[i + 1] as char).to_digit(16),
                (b[i + 2] as char).to_digit(16),
            ) {
                out.push((a * 16 + c) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn normalize_path(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            std::path::Component::CurDir => {}
            _ => out.push(c),
        }
    }
    Some(out)
}
/// Link targets of a note body: Markdown `.md` links and `[[wikilinks]]`
/// (target text without `|alias`/`#fragment`; `true` marks a wikilink).
pub(crate) fn link_targets(body: &str) -> Vec<(String, bool)> {
    static MD: LazyLock<Regex> = LazyLock::new(|| regex(r"\]\(([^)]+\.md)(?:#[^)]*)?\)"));
    static WIKI: LazyLock<Regex> = LazyLock::new(|| regex(r"\[\[([^]\n]+)\]\]"));
    MD.captures_iter(body)
        .map(|c| (c[1].to_owned(), false))
        .chain(WIKI.captures_iter(body).map(|c| (c[1].to_owned(), true)))
        .filter_map(|(value, wiki)| {
            let target = decode(value.split(['|', '#']).next().unwrap_or(""))
                .trim()
                .to_owned();
            (!target.is_empty() && !target.contains("://")).then_some((target, wiki))
        })
        .collect()
}
/// Resolves note links to note ids: vault-rooted or note-relative paths, then
/// a unique wikilink file name.
pub(crate) struct Resolver {
    lookup: BTreeMap<String, String>,
    stems: BTreeMap<String, Vec<String>>,
}
impl Resolver {
    pub(crate) fn new(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        let lookup = pairs.into_iter().collect::<BTreeMap<_, _>>();
        let mut stems: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (p, id) in &lookup {
            stems
                .entry(casefold(
                    Path::new(p)
                        .file_stem()
                        .and_then(|v| v.to_str())
                        .unwrap_or(""),
                ))
                .or_default()
                .push(id.clone());
        }
        Self { lookup, stems }
    }
    pub(crate) fn resolve(&self, source: &str, target: &str, wiki: bool) -> Option<&str> {
        let mut target = target.to_owned();
        if wiki && !target.ends_with(".md") {
            target.push_str(".md")
        }
        let parent = Path::new(source).parent().unwrap_or(Path::new(""));
        let mut candidates = vec![];
        if wiki {
            candidates.push(PathBuf::from(&target))
        }
        candidates.push(parent.join(&target));
        let found = candidates
            .iter()
            .filter_map(|p| normalize_path(p))
            .find_map(|p| self.lookup.get(p.to_str().unwrap_or("")));
        if found.is_some() || !wiki || target.contains('/') {
            return found.map(String::as_str);
        }
        let stem = Path::new(&target).file_stem().and_then(|v| v.to_str());
        match self.stems.get(&casefold(stem.unwrap_or(""))) {
            Some(matches) if matches.len() == 1 => Some(matches[0].as_str()),
            _ => None,
        }
    }
}
fn source_links(notes: &mut [Value]) -> Vec<Value> {
    let resolver = Resolver::new(
        notes
            .iter()
            .map(|n| (text(&n["path"]).to_owned(), text(&n["id"]).to_owned())),
    );
    let mut pairs = BTreeSet::new();
    for note in notes.iter() {
        for (target, wiki) in link_targets(text(&note["text"])) {
            if let Some(id) = resolver.resolve(text(&note["path"]), &target, wiki) {
                if id != text(&note["id"]) {
                    pairs.insert((text(&note["id"]).to_owned(), id.to_owned()));
                }
            }
        }
    }
    let mut references: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (a, b) in &pairs {
        references.entry(a).or_default().push(b);
    }
    for note in notes.iter_mut() {
        let refs = references
            .get(text(&note["id"]))
            .cloned()
            .unwrap_or_default();
        note["references"] = json!(refs);
    }
    pairs
        .into_iter()
        .map(|(a, b)| json!({"source":a,"target":b,"kind":"source"}))
        .collect()
}
static GRAPH: LazyLock<Mutex<Option<(String, Value)>>> = LazyLock::new(|| Mutex::new(None));
static GRAPH_BUILDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static GRAPH_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static GRAPH_WATCHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static GRAPH_BUILD_REVISION: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static GRAPH_ERROR: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static SUMMARY_CACHE: LazyLock<Mutex<Option<(String, Value)>>> = LazyLock::new(|| Mutex::new(None));
static WATCH_ACTIVITY: LazyLock<Mutex<std::time::Instant>> =
    LazyLock::new(|| Mutex::new(std::time::Instant::now()));
static WATCH_PAUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WATCH_SIGNAL: LazyLock<(Mutex<()>, std::sync::Condvar)> =
    LazyLock::new(|| (Mutex::new(()), std::sync::Condvar::new()));
/// Bounded first paint data. This deliberately does not invoke discovery or
/// take the graph cache lock, so a cold graph build cannot hold up the viewer.
fn summary() -> Result<Value> {
    let root = vault()?;
    let current = vault_metadata_fingerprint(&root)?;
    if let Some((key, cached)) = SUMMARY_CACHE
        .lock()
        .map_err(|_| "Summary cache lock failed")?
        .as_ref()
    {
        if key == &current {
            let mut value = cached.clone();
            decorate(&mut value)?;
            decorate_connections(&mut value);
            return Ok(value);
        }
    }
    let mut all = notes(&root, Utc::now().date_naive())?;
    all.sort_by(|a, b| text(&b["recorded"]).cmp(text(&a["recorded"])));
    let (nodes, links, worlds) = summary_projection(&root, &all, Utc::now().date_naive())?;
    let review_count = all.iter().filter(|n| n["review"] == true).count();
    let mut value = json!({"nodes":nodes,"links":links,"noteCount":all.len(),"worlds":worlds,"reviewCount":review_count,"activityModel":ACTIVITY_MODEL,"discovery":{},"connections":{"state":"building"}});
    decorate(&mut value)?;
    decorate_connections(&mut value);
    *SUMMARY_CACHE
        .lock()
        .map_err(|_| "Summary cache lock failed")? = Some((current, value.clone()));
    Ok(value)
}

fn summary_projection(
    root: &Path,
    all: &[Value],
    today: NaiveDate,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let (worlds, projected) = hierarchy(root, all, today)?;
    let mut nodes = projected
        .iter()
        .filter(|n| n["kind"] != "membership")
        .cloned()
        .collect::<Vec<_>>();
    let mut links = projected
        .into_iter()
        .filter(|n| n["kind"] == "membership")
        .collect::<Vec<_>>();
    for n in all.iter().take(40) {
        let mut node = n.clone();
        if let Some(object) = node.as_object_mut() {
            for field in ["text", "meta", "references"] {
                object.remove(field);
            }
        }
        nodes.push(node);
    }
    let ids = nodes
        .iter()
        .map(|n| text(&n["id"]).to_owned())
        .collect::<BTreeSet<_>>();
    links.retain(|link| ids.contains(text(&link["source"])) && ids.contains(text(&link["target"])));
    Ok((nodes, links, worlds))
}

fn decorate_connections(value: &mut Value) {
    let error = GRAPH_ERROR.lock().ok().and_then(|e| e.clone());
    let state = if GRAPH_BUILDING.load(std::sync::atomic::Ordering::Acquire) {
        "building"
    } else if error.is_some() {
        "error"
    } else if GRAPH_READY.load(std::sync::atomic::Ordering::Acquire) {
        "ready"
    } else {
        "building"
    };
    value["connections"] = json!({"state":state});
    if let Some(error) = error {
        value["connections"]["error"] = json!(error);
    }
}

/// Project the already-built background graph for the graph viewer. This path
/// intentionally reads only the completed cache; it never starts discovery.
fn graph_projection(graph: &Value, scope: &str) -> Value {
    let nodes = graph["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut node| {
            if text(&node["kind"]) == "note" && text(&node["path"]).starts_with("wiki/") {
                node["kind"] = json!("wiki");
            }
            node
        })
        .collect::<Vec<_>>();
    // Derived mention/related edges join explicit links and membership; the
    // canvas kind filters hide them with their endpoints.
    let links = [&graph["links"], &graph["connections"]["links"]]
        .into_iter()
        .flat_map(|v| v.as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let ids = nodes
        .iter()
        .map(|node| text(&node["id"]).to_owned())
        .collect::<BTreeSet<_>>();
    let edge_ends = |edge: &Value| {
        let source = endpoint(&edge["source"]);
        let target = endpoint(&edge["target"]);
        (source, target)
    };
    let mut included = BTreeSet::new();
    if scope == "all" {
        included.extend(
            nodes
                .iter()
                .filter(|node| text(&node["kind"]) != "note")
                .map(|n| text(&n["id"]).to_owned()),
        );
    } else if let Some((kind, value)) = scope.split_once(':') {
        let matches = nodes
            .iter()
            .filter(|node| {
                let value = casefold(value);
                let explicit = [kind, if kind == "world" { "worlds" } else { "area" }]
                    .iter()
                    .any(|field| {
                        let field_value = text(&node[field]);
                        field_value
                            .split(',')
                            .any(|part| casefold(part.trim()) == value)
                    });
                let allowed_kind = match kind {
                    "world" => ["world", "hub", "area", "entity"].contains(&text(&node["kind"])),
                    "area" => ["area", "hub", "topic", "entity"].contains(&text(&node["kind"])),
                    _ => false,
                };
                allowed_kind
                    && (explicit
                        || ["id", "label"]
                            .iter()
                            .any(|field| casefold(text(&node[field])) == value))
            })
            .map(|node| text(&node["id"]).to_owned())
            .collect::<BTreeSet<_>>();
        if kind == "note" {
            included.insert(value.to_owned());
            let mut frontier = included.clone();
            for _ in 0..2 {
                let next = links
                    .iter()
                    .filter_map(|edge| {
                        let (source, target) = edge_ends(edge);
                        if frontier.contains(&source) {
                            Some(target)
                        } else if frontier.contains(&target) {
                            Some(source)
                        } else {
                            None
                        }
                    })
                    .filter(|id| ids.contains(id) && !included.contains(id))
                    .collect::<BTreeSet<_>>();
                included.extend(next.iter().cloned());
                frontier = next;
            }
        } else {
            included.extend(matches.iter().cloned());
            // Keep the hierarchy below a matching world/area and its directly
            // attached notes, without allowing cross-links to flood the view.
            let mut frontier = matches;
            while !frontier.is_empty() {
                let next = links
                    .iter()
                    .filter_map(|edge| {
                        if !["group", "membership"].contains(&text(&edge["kind"])) {
                            return None;
                        }
                        let (source, target) = edge_ends(edge);
                        if frontier.contains(&source) {
                            Some(target)
                        } else {
                            None
                        }
                    })
                    .filter(|id| ids.contains(id) && !included.contains(id))
                    .collect::<BTreeSet<_>>();
                included.extend(next.iter().cloned());
                frontier = next;
            }
            let categories = included.clone();
            let note_ids = nodes
                .iter()
                .filter(|node| ["note", "wiki"].contains(&text(&node["kind"])))
                .map(|node| text(&node["id"]).to_owned())
                .collect::<BTreeSet<_>>();
            // Derived edges only connect nodes already in scope.
            for edge in links
                .iter()
                .filter(|e| !["mention", "related"].contains(&text(&e["kind"])))
            {
                let (source, target) = edge_ends(edge);
                if categories.contains(&source) && note_ids.contains(&target) {
                    included.insert(target);
                }
            }
        }
    }
    let selected = nodes
        .into_iter()
        .filter(|node| included.contains(text(&node["id"])))
        .collect::<Vec<_>>();
    let selected_ids = selected
        .iter()
        .map(|node| text(&node["id"]).to_owned())
        .collect::<BTreeSet<_>>();
    let edges = links
        .into_iter()
        .filter(|edge| {
            let (source, target) = edge_ends(edge);
            selected_ids.contains(&source) && selected_ids.contains(&target)
        })
        .collect::<Vec<_>>();
    let mut value =
        json!({"state":"ready","nodes":selected,"links":edges,"worlds":graph["worlds"]});
    refresh_new_badges(&mut value, Utc::now().date_naive());
    value
}

fn endpoint(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| text(&value["id"]).to_owned())
}

fn schedule_graph_build(retry: bool) {
    if GRAPH_READY.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    if !retry && GRAPH_ERROR.lock().ok().and_then(|e| e.clone()).is_some() {
        return;
    }
    if GRAPH_BUILDING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    *GRAPH_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let revision = vault()
        .and_then(|root| vault_metadata_fingerprint(&root))
        .ok();
    *GRAPH_BUILD_REVISION
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = revision;
    std::thread::spawn(|| {
        let result = std::panic::catch_unwind(graph)
            .unwrap_or_else(|_| Err("Brain discovery worker failed".into()));
        match result {
            Ok(_) => {
                *GRAPH_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = None;
                let stale = vault()
                    .and_then(|root| vault_metadata_fingerprint(&root))
                    .ok()
                    .zip(GRAPH_BUILD_REVISION.lock().ok().and_then(|v| v.clone()))
                    .is_some_and(|(now, built)| now != built);
                GRAPH_READY.store(!stale, std::sync::atomic::Ordering::Release);
                GRAPH_BUILDING.store(false, std::sync::atomic::Ordering::Release);
                if stale {
                    schedule_graph_build(true);
                }
                return;
            }
            Err(e) => *GRAPH_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string()),
        }
        GRAPH_BUILDING.store(false, std::sync::atomic::Ordering::Release);
    });
}

fn vault_metadata_fingerprint(root: &Path) -> Result<String> {
    let mut stamp = Sha256::new();
    stamp.update(root.to_string_lossy().as_bytes());
    stamp.update(Utc::now().date_naive().to_string().as_bytes());
    let mut files = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "md"))
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.path().cmp(b.path()));
    for entry in files {
        let meta = entry.metadata()?;
        stamp.update(
            entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .as_bytes(),
        );
        stamp.update(meta.len().to_le_bytes());
        if let Some(time) = meta
            .modified()
            .ok()
            .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
        {
            stamp.update(time.as_nanos().to_le_bytes());
        }
    }
    let registry_path = root.join("registry.json");
    if let Ok(meta) = fs::metadata(&registry_path) {
        stamp.update(b"registry.json");
        stamp.update(meta.len().to_le_bytes());
        if let Some(time) = meta
            .modified()
            .ok()
            .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
        {
            stamp.update(time.as_nanos().to_le_bytes());
        }
    }
    let override_path = root.join(".brain-state/discovery/overrides.json");
    reject_links(&override_path)?;
    if let Ok(meta) = fs::metadata(override_path) {
        stamp.update(meta.len().to_le_bytes());
        if let Some(time) = meta
            .modified()
            .ok()
            .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
        {
            stamp.update(time.as_nanos().to_le_bytes());
        }
    }
    Ok(format!("{:x}", stamp.finalize()))
}

fn create_vault_watcher(
    root: &Path,
) -> notify::Result<(RecommendedWatcher, mpsc::Receiver<notify::Result<Event>>)> {
    let (sender, receiver) = mpsc::channel();
    let watched_root = root.to_path_buf();
    let mut watcher = RecommendedWatcher::new(
        move |event: notify::Result<Event>| {
            if event.as_ref().is_err()
                || event
                    .as_ref()
                    .is_ok_and(|event| visible_vault_event(event, &watched_root))
            {
                let _ = sender.send(event);
            }
        },
        Config::default().with_follow_symlinks(false),
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok((watcher, receiver))
}
fn visible_vault_event(event: &Event, root: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    event.paths.iter().any(|path| {
        path.strip_prefix(root).ok().is_some_and(|relative| {
            !relative
                .components()
                .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
        })
    })
}
fn start_vault_watch() {
    GRAPH_WATCHING.store(true, std::sync::atomic::Ordering::Release);
    std::thread::spawn(|| {
        let mut watched = vault()
            .ok()
            .and_then(|root| create_vault_watcher(&root).ok());
        while GRAPH_WATCHING.load(std::sync::atomic::Ordering::Acquire) {
            let (signal, cv) = &*WATCH_SIGNAL;
            let guard = signal.lock().unwrap_or_else(|e| e.into_inner());
            let elapsed = WATCH_ACTIVITY
                .lock()
                .map(|v| v.elapsed())
                .unwrap_or(Duration::from_secs(600));
            let Some(wait) = watcher_wait(elapsed) else {
                watched = None;
                WATCH_PAUSED.store(true, std::sync::atomic::Ordering::Release);
                let guard = cv
                    .wait_while(guard, |_| {
                        WATCH_ACTIVITY
                            .lock()
                            .map(|activity| watcher_wait(activity.elapsed()).is_none())
                            .unwrap_or(true)
                            && GRAPH_WATCHING.load(std::sync::atomic::Ordering::Acquire)
                    })
                    .unwrap_or_else(|e| e.into_inner());
                drop(guard);
                if !GRAPH_WATCHING.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                if let Ok(root) = vault() {
                    watched = create_vault_watcher(&root).ok();
                    if let Ok(current) = vault_metadata_fingerprint(&root) {
                        let built = GRAPH_BUILD_REVISION.lock().ok().and_then(|v| v.clone());
                        if built.as_ref().is_some_and(|old| old != &current) {
                            GRAPH_READY.store(false, std::sync::atomic::Ordering::Release);
                            if let Ok(mut cache) = SUMMARY_CACHE.lock() {
                                *cache = None;
                            }
                            brain_search::invalidate();
                            schedule_graph_build(true);
                        }
                    }
                }
                let _guard = signal.lock().unwrap_or_else(|e| e.into_inner());
                WATCH_PAUSED.store(false, std::sync::atomic::Ordering::Release);
                cv.notify_all();
                continue;
            };
            drop(guard);
            let received = watched
                .as_ref()
                .map(|(_, receiver)| receiver.recv_timeout(wait));
            let event = match received {
                Some(Ok(Ok(event))) => event,
                Some(Ok(Err(_))) => continue,
                Some(Err(mpsc::RecvTimeoutError::Timeout)) => continue,
                Some(Err(mpsc::RecvTimeoutError::Disconnected)) => {
                    watched = vault()
                        .ok()
                        .and_then(|root| create_vault_watcher(&root).ok());
                    continue;
                }
                None => {
                    std::thread::sleep(wait);
                    continue;
                }
            };
            let Ok(root) = vault() else { continue };
            if !visible_vault_event(&event, &root) {
                continue;
            }
            let mut quiet_until = std::time::Instant::now() + Duration::from_secs(2);
            loop {
                let remaining = quiet_until.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let Some((_, receiver)) = watched.as_ref() else {
                    break;
                };
                match receiver.recv_timeout(remaining) {
                    Ok(Ok(next)) if visible_vault_event(&next, &root) => {
                        quiet_until = std::time::Instant::now() + Duration::from_secs(2)
                    }
                    Ok(_) => continue,
                    Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                        break
                    }
                }
            }
            let Ok(current) = vault_metadata_fingerprint(&root) else {
                continue;
            };
            let built = GRAPH_BUILD_REVISION.lock().ok().and_then(|v| v.clone());
            if built.as_ref().is_some_and(|old| old != &current) {
                GRAPH_READY.store(false, std::sync::atomic::Ordering::Release);
                if let Ok(mut cache) = SUMMARY_CACHE.lock() {
                    *cache = None;
                }
                brain_search::invalidate();
                schedule_graph_build(true);
            }
        }
    });
}
fn watcher_wait(inactive: Duration) -> Option<Duration> {
    (inactive < Duration::from_secs(600)).then(|| Duration::from_secs(600).saturating_sub(inactive))
}
pub fn graph() -> Result<Value> {
    graph_at(&vault()?, Utc::now().date_naive())
}
fn graph_at(root: &Path, today: NaiveDate) -> Result<Value> {
    let mut notes = notes(root, today)?;
    let mut links = source_links(&mut notes);
    let derived = root.join(".brain-state/discovery");
    reject_links(&derived)?;
    let overrides = read_json(&derived.join("overrides.json"));
    let fingerprint = digest(serde_json::to_vec(&json!([
        GRAPH_SCHEMA,
        root,
        notes
            .iter()
            .map(|n| json!([n["id"], n["revision"], n["path"], n["kind"]]))
            .collect::<Vec<_>>(),
        registry(root)?,
        today.to_string(),
        overrides
    ]))?);
    let persisted = root.join(".brain-state/index/graph.json");
    reject_links(&persisted)?;
    let disk = read_json(&persisted);
    if disk["schema"] == GRAPH_SCHEMA
        && disk["fingerprint"] == fingerprint
        && disk["graph"].is_object()
    {
        let mut value = disk["graph"].clone();
        decorate(&mut value)?;
        *GRAPH.lock().map_err(|_| "Graph lock failed")? = Some((fingerprint, value.clone()));
        return Ok(value);
    }
    {
        let cache = GRAPH.lock().map_err(|_| "Graph lock failed")?;
        if let Some((key, value)) = &*cache {
            if key == &fingerprint {
                let mut v = value.clone();
                decorate(&mut v)?;
                return Ok(v);
            }
        }
    }
    let mut cached_suggestions = disk["graph"]["discovery"]["suggestions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if cached_suggestions.is_empty() {
        cached_suggestions = disk["graph"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|n| ["hub", "topic"].contains(&text(&n["kind"])))
            .cloned()
            .collect();
    }
    let prior_report = read_json(&derived.join("subjects.json"));
    let mut candidates = notes
        .iter()
        .filter(|n| {
            n["worlds"].as_array().is_none_or(Vec::is_empty)
                && n["areas"].as_array().is_none_or(Vec::is_empty)
        })
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        text(&b["recorded"])
            .cmp(text(&a["recorded"]))
            .then_with(|| text(&a["id"]).cmp(text(&b["id"])))
    });
    candidates.truncate(128);
    let has_candidates = !candidates.is_empty();
    let (suggestions, mut report) = if !has_candidates {
        (
            cached_suggestions,
            json!({"discovered":[],"promoted":[],"review":[],"method":"registry classified","embedding":"not needed"}),
        )
    } else {
        for note in &mut candidates {
            note["featureText"] = json!(brain_discovery::features(note).1);
        }
        let (dense, health) = semantic_vectors(&candidates, &derived.join("vectors.json"))?;
        let mut input = json!(candidates);
        let result = brain_discovery::organize_mut(&mut input, &dense, &prior_report, &overrides)?;
        let mut discovered = result[0].as_array().cloned().unwrap_or_default();
        if discovered.is_empty() {
            discovered = cached_suggestions;
        }
        write_json(&derived.join("subjects.json"), &result[1])?;
        let mut report = result[1].clone();
        report["embedding"] = json!(health);
        (discovered, report)
    };
    report["suggestions"] = json!(suggestions);
    report["suggestionSampleLimit"] = json!(128);
    if report.get("embedding").is_none() {
        report["embedding"] = json!("not needed");
    }
    let mut nodes = Vec::new();
    let (worlds, hierarchy_nodes) = hierarchy(root, &notes, today)?;
    let similar = brain_search::similar(root, 10).unwrap_or_default();
    let mut connections = brain_connections::build(&notes, &links, &registry(root)?, &similar);
    let node_ids = hierarchy_nodes
        .iter()
        .chain(notes.iter())
        .map(|n| text(&n["id"]).to_owned())
        .collect::<BTreeSet<_>>();
    if let Some(edges) = connections["links"].as_array_mut() {
        edges.retain(|e| {
            node_ids.contains(text(&e["source"])) && node_ids.contains(text(&e["target"]))
        });
    }
    let review_count = notes.iter().filter(|n| n["review"] == true).count();
    let note_count = notes.len();
    for node in hierarchy_nodes {
        if node["kind"] == "world" || node["kind"] == "area" {
            nodes.push(node);
        } else {
            links.push(node);
        }
    }
    for mut n in notes {
        if let Some(o) = n.as_object_mut() {
            for k in ["text", "meta", "references"] {
                o.remove(k);
            }
        }
        nodes.push(n)
    }
    let mut value = json!({"nodes":nodes,"links":links,"noteCount":note_count,"worlds":worlds,"reviewCount":review_count,"activityModel":ACTIVITY_MODEL,"discovery":report,"connections":connections});
    value["graphRevision"] = json!(fingerprint);
    decorate(&mut value)?;
    write_json(
        &persisted,
        &json!({"schema":GRAPH_SCHEMA,"fingerprint":fingerprint,"graph":value}),
    )?;
    *GRAPH.lock().map_err(|_| "Graph lock failed")? = Some((fingerprint.clone(), value.clone()));
    Ok(value)
}
fn refresh_new_badges(value: &mut Value, today: NaiveDate) {
    for node in value["nodes"].as_array_mut().into_iter().flatten() {
        if matches!(text(&node["kind"]), "world" | "area") {
            node["new"] = json!(new_category(node, today));
        }
    }
    for world in value["worlds"].as_array_mut().into_iter().flatten() {
        world["new"] = json!(new_category(world, today));
        for area in world["areas"].as_array_mut().into_iter().flatten() {
            area["new"] = json!(new_category(area, today));
        }
    }
}
fn decorate(v: &mut Value) -> Result<()> {
    refresh_new_badges(v, Utc::now().date_naive());
    v["theme"] = desktop_theme()?;
    v["accent"] = json!(palette()?);
    v["generated"] = json!(chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string());
    Ok(())
}
pub fn desktop_theme() -> Result<Value> {
    Ok(read_json(&home()?.join(".local/state/nacre/scheme.json"))
        .get("colours")
        .cloned()
        .unwrap_or_else(|| json!({})))
}
pub fn palette() -> Result<String> {
    let s = fs::read_to_string(home()?.join(".config/nacre/colors/primary")).unwrap_or_default();
    let s = s.trim();
    Ok(if regex(r"^#[0-9a-fA-F]{6}$").is_match(s) {
        s.into()
    } else {
        "#808080".into()
    })
}
pub fn theme_css() -> Result<String> {
    let value = desktop_theme()?;
    let Some(colours) = value.as_object() else {
        return Ok(String::new());
    };
    let valid = colours
        .iter()
        .filter_map(|(k, v)| {
            v.as_str()
                .filter(|s| regex(r"^#?[0-9a-fA-F]{6}$").is_match(s))
                .map(|s| (k.as_str(), format!("#{}", s.trim_start_matches('#'))))
        })
        .collect::<BTreeMap<_, _>>();
    let roles = [
        ("bg", "surface"),
        ("panel", "surfaceContainer"),
        ("raised", "surfaceContainerHigh"),
        ("text", "onSurface"),
        ("muted", "onSurfaceVariant"),
        ("line", "outlineVariant"),
        ("accent", "primary"),
        ("secondary", "secondary"),
        ("tertiary", "tertiary"),
        ("on-accent", "onPrimary"),
    ];
    let mut declarations = String::new();
    for (k, r) in roles {
        if let Some(v) = valid.get(r) {
            declarations += &format!("--{k}:{v};")
        }
    }
    let surface = valid
        .get("surface")
        .map(String::as_str)
        .unwrap_or("#151310");
    let light = [1, 3, 5]
        .iter()
        .map(|i| u32::from_str_radix(&surface[*i..*i + 2], 16).unwrap())
        .sum::<u32>()
        > 420;
    declarations += &format!(
        "color-scheme:{};--logo-highlight:{};",
        if light { "light" } else { "dark" },
        valid
            .get(if light { "primary" } else { "primaryFixed" })
            .map(String::as_str)
            .unwrap_or("#eee9f4")
    );
    Ok(format!("\n:root{{{declarations}}}\n"))
}
/// Ranked note ids for the module API (every kind, archive included).
pub fn search_notes(query: &str) -> Result<Value> {
    if !GRAPH_WATCHING.load(std::sync::atomic::Ordering::Acquire) {
        brain_search::invalidate();
    }
    let options = brain_search::Options {
        limit: 10000,
        include_archive: true,
        ..Default::default()
    };
    Ok(brain_search::search(&vault()?, query, &options)?["ids"].take())
}
pub fn assign_note(value: &str) -> Result<()> {
    let data: Value = serde_json::from_str(value)?;
    note_path(data["path"].as_str().ok_or("Invalid note path")?)?;
    assign_note_at(&vault()?, value)
}
fn assign_note_at(root: &Path, value: &str) -> Result<()> {
    let data: Value = serde_json::from_str(value)?;
    let relative = data["path"].as_str().ok_or("Invalid note path")?;
    note_path_at(root, relative)?;
    if ROOT_FILES.contains(&relative) {
        return Err("Not an indexed note".into());
    }
    let labels = data.get("subjects").cloned().unwrap_or_else(|| json!([]));
    let labels = labels.as_array().ok_or("Invalid subject labels")?;
    if labels.len() > 8
        || labels.iter().any(|l| {
            l.as_str().is_none_or(|s| {
                !(2..=80).contains(&s.trim().chars().count()) || s.chars().any(|c| (c as u32) < 32)
            })
        })
    {
        return Err("Invalid subject labels".into());
    }
    let directory = root.join(".brain-state/discovery");
    private_dir(&directory)?;
    let lock = directory.join("override.lock");
    reject_links(&lock)?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(lock)?;
    rustix::fs::flock(&guard, rustix::fs::FlockOperation::LockExclusive)?;
    let path = directory.join("overrides.json");
    let mut overrides = read_json(&path);
    let object = overrides.as_object_mut().ok_or("Invalid overrides")?;
    Path::new(relative)
        .file_name()
        .ok_or("Invalid note filename")?;
    let id = note_id(Path::new(relative));
    if data["reset"] == true {
        object.remove(&id);
    } else if !labels.is_empty() {
        let mut values = Vec::new();
        for l in labels {
            let l = json!(text(l).trim());
            if !values.contains(&l) {
                values.push(l)
            }
        }
        object.insert(id, json!(values));
    } else {
        return Err("Choose at least one subject".into());
    }
    write_json(&path, &overrides)
}

struct Semantic {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    output: Option<std::io::BufReader<std::process::ChildStdout>>,
}
impl Drop for Semantic {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Semantic {
    fn open() -> Result<Self> {
        use std::process::{Command, Stdio};
        let python = home()?.join(".local/share/nacre/brain-model/bin/python");
        let mut command = Command::new(python);
        command
            .arg(root().join("semantic.py"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::process::restore_child_signals(&mut command);
        let mut child = command.spawn()?;
        let input = child.stdin.take();
        let output = child.stdout.take().map(std::io::BufReader::new);
        Ok(Self {
            child,
            input,
            output,
        })
    }
    fn vectors(&mut self, texts: Vec<String>) -> Result<Value> {
        use std::io::BufRead;
        let mut input = self.input.take().ok_or("Missing semantic input")?;
        let mut output = self.output.take().ok_or("Missing semantic output")?;
        let request = serde_json::to_string(&json!({"texts":texts}))? + "\n";
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let result = (|| -> Result<Value> {
                input.write_all(request.as_bytes())?;
                input.flush()?;
                let mut line = String::new();
                output.by_ref().take(1024 * 1024 + 1).read_line(&mut line)?;
                if line.len() > 1024 * 1024 {
                    return Err("Semantic response exceeds limit".into());
                }
                Ok(serde_json::from_str(&line)?)
            })();
            let _ = tx.send((input, output, result));
        });
        match rx.recv_timeout(Duration::from_secs(25)) {
            Ok((input, output, result)) => {
                self.input = Some(input);
                self.output = Some(output);
                handle.join().map_err(|_| "Semantic worker failed")?;
                result
            }
            Err(_) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
                let _ = handle.join();
                Err("Embedding timeout".into())
            }
        }
    }
}
static SEMANTIC: LazyLock<Mutex<Option<Semantic>>> = LazyLock::new(|| Mutex::new(None));
fn semantic_vectors(notes: &[Value], path: &Path) -> Result<(Value, &'static str)> {
    if env::var("NACRE_BRAIN_SEMANTICS").as_deref() == Ok("0")
        || !home()?
            .join(".local/share/nacre/brain-model/bin/python")
            .is_file()
    {
        return Ok((json!({}), "lexical fallback"));
    }
    reject_links(path)?;
    let mut cache = read_json(path);
    if !cache.is_object() {
        cache = json!({})
    }
    let mut worker = SEMANTIC.lock().map_err(|_| "Semantic lock failed")?;
    let missing = notes
        .iter()
        .filter(|n| cache[text(&n["id"])]["revision"] != n["revision"])
        .collect::<Vec<_>>();
    let result = (|| -> Result<Value> {
        for batch in missing.chunks(12) {
            if worker.is_none() {
                *worker = Some(Semantic::open()?)
            }
            let reply = worker.as_mut().unwrap().vectors(
                batch
                    .iter()
                    .map(|n| text(&n["featureText"]).to_owned())
                    .collect(),
            )?;
            let vectors = reply["vectors"]
                .as_array()
                .filter(|v| v.len() == batch.len())
                .ok_or("Incomplete embeddings")?;
            for (n, vector) in batch.iter().zip(vectors) {
                let v = vector
                    .as_array()
                    .filter(|v| {
                        !v.is_empty()
                            && v.len() <= 4096
                            && v.iter().all(|x| x.as_f64().is_some_and(f64::is_finite))
                    })
                    .ok_or("Invalid embeddings")?;
                cache[text(&n["id"])] = json!({"revision":n["revision"],"vector":v});
            }
        }
        let retained = notes
            .iter()
            .filter_map(|n| {
                cache
                    .get(text(&n["id"]))
                    .map(|v| (text(&n["id"]).to_owned(), v.clone()))
            })
            .collect::<Map<_, _>>();
        cache = Value::Object(retained);
        if !missing.is_empty() {
            write_json(path, &cache)?
        }
        Ok(Value::Object(
            notes
                .iter()
                .filter_map(|n| {
                    cache
                        .get(text(&n["id"]))
                        .map(|v| (text(&n["id"]).to_owned(), v["vector"].clone()))
                })
                .collect(),
        ))
    })();
    match result {
        Ok(v) => Ok((v, "local semantic model")),
        Err(_) => {
            *worker = None;
            Ok((json!({}), "lexical fallback"))
        }
    }
}
fn clear_semantic() {
    if let Ok(mut worker) = SEMANTIC.lock() {
        *worker = None;
    }
}

fn query(query: &str, key: &str) -> String {
    query
        .split('&')
        .find_map(|s| {
            let (k, v) = s.split_once('=')?;
            if decode(k) == key {
                Some(decode(&v.replace('+', " ")))
            } else {
                None
            }
        })
        .unwrap_or_default()
}
fn response(status: u16, mime: &str, body: impl Into<Vec<u8>>) -> brain_http::Response {
    brain_http::Response {
        status,
        mime: mime.into(),
        headers: vec![],
        body: body.into(),
    }
}
fn json_response(value: Value) -> Result<brain_http::Response> {
    Ok(response(
        200,
        "application/json",
        serde_json::to_vec(&value)?,
    ))
}
fn route(request: brain_http::Request) -> brain_http::Response {
    let status = if request.method == "POST" { 400 } else { 404 };
    route_checked(request).unwrap_or_else(|_| response(status, "application/json", b"{}".to_vec()))
}
fn route_checked(request: brain_http::Request) -> Result<brain_http::Response> {
    if request.method == "GET" && request.path == "/api/health" {
        return json_response(json!({"ready":true,"authVersion":1}));
    }
    {
        let (signal, cv) = &*WATCH_SIGNAL;
        let guard = signal.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(mut activity) = WATCH_ACTIVITY.lock() {
            *activity = std::time::Instant::now();
        }
        if WATCH_PAUSED.load(std::sync::atomic::Ordering::Acquire) {
            cv.notify_all();
            let _guard = cv
                .wait_while(guard, |_| {
                    WATCH_PAUSED.load(std::sync::atomic::Ordering::Acquire)
                        && GRAPH_WATCHING.load(std::sync::atomic::Ordering::Acquire)
                })
                .unwrap_or_else(|e| e.into_inner());
        }
    }
    if request.method == "POST" {
        if request.path != "/api/action" {
            return Ok(response(404, "application/json", b"{}".to_vec()));
        }
        let data: Value = serde_json::from_slice(&request.body)?;
        let name = data["name"].as_str().ok_or("Invalid action")?;
        if ![
            "open-note",
            "capture",
            "tasks",
            "new",
            "resume",
            "notes",
            "assign-note",
        ]
        .contains(&name)
        {
            return Err("Invalid action".into());
        }
        if name == "assign-note" && data.get("value").is_some_and(|value| !value.is_string()) {
            return Err("Invalid action value".into());
        }
        let value = data
            .get("value")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_default();
        if name == "assign-note" {
            assign_note(&value)?
        } else {
            let exe = env::current_exe()?;
            let exe = exe.to_str().ok_or("Invalid helper path")?;
            brain_launch::launch_app(
                &[exe, "brain-control", "action", name, &value].map(str::to_owned),
            )?;
        }
        return json_response(json!({"ok":true}));
    }
    if request.method != "GET" {
        return Ok(response(405, "application/json", b"{}".to_vec()));
    }
    match request.path.as_str() {
        "/api/summary" => {
            schedule_graph_build(query(&request.query, "retry") == "1");
            return json_response(summary()?);
        }
        "/api/graph" => {
            let scope = query(&request.query, "scope");
            let scope = if scope.is_empty() {
                "all"
            } else {
                scope.as_str()
            };
            if scope != "all"
                && !["world:", "area:", "note:"]
                    .iter()
                    .any(|prefix| scope.starts_with(prefix) && scope.len() > prefix.len())
            {
                return json_response(json!({"state":"error","error":"Invalid graph scope"}));
            }
            schedule_graph_build(query(&request.query, "retry") == "1");
            if !GRAPH_READY.load(std::sync::atomic::Ordering::Acquire) {
                if let Some(error) = GRAPH_ERROR.lock().ok().and_then(|error| error.clone()) {
                    return json_response(json!({"state":"error","error":error}));
                }
                return json_response(json!({"state":"building"}));
            }
            let cached = GRAPH
                .lock()
                .map_err(|_| "Graph lock failed")?
                .as_ref()
                .map(|(_, value)| value.clone());
            let Some(graph) = cached else {
                return json_response(json!({"state":"building"}));
            };
            return json_response(graph_projection(&graph, scope));
        }
        "/api/brain" => {
            let retry = query(&request.query, "retry") == "1";
            schedule_graph_build(retry);
            let cached = GRAPH
                .lock()
                .map_err(|_| "Graph lock failed")?
                .as_ref()
                .map(|(_, value)| value.clone());
            if let Some(mut value) = cached {
                // Per-note connections are served on demand by /api/connections.
                if let Some(object) = value.as_object_mut() {
                    object.remove("connections");
                }
                decorate(&mut value)?;
                decorate_connections(&mut value);
                return json_response(value);
            }
            return json_response(summary()?);
        }
        "/api/wiki" => return json_response(brain_wiki::index(&vault()?)?),
        "/api/wiki/page" | "/api/connections" => {
            let ready = GRAPH_READY.load(std::sync::atomic::Ordering::Acquire);
            let cached = GRAPH
                .lock()
                .map_err(|_| "Graph lock failed")?
                .as_ref()
                .filter(|_| ready)
                .map(|(_, value)| value.clone());
            let id = query(&request.query, "id");
            return json_response(if request.path == "/api/connections" {
                cached.map_or_else(
                    || json!({"state":"building"}),
                    |graph| brain_wiki::connections(&graph, &id),
                )
            } else {
                brain_wiki::page(&vault()?, &id, Utc::now().date_naive(), cached.as_ref())?
            });
        }
        "/api/search" => {
            if !GRAPH_WATCHING.load(std::sync::atomic::Ordering::Acquire) {
                brain_search::invalidate();
            }
            let q = &request.query;
            let list = |key: &str| {
                Some(query(q, key))
                    .filter(|v| !v.trim().is_empty())
                    .into_iter()
                    .collect::<Vec<_>>()
            };
            let options = brain_search::Options {
                limit: query(q, "limit")
                    .parse::<usize>()
                    .unwrap_or(200)
                    .clamp(1, 500),
                worlds: list("world"),
                areas: list("area"),
                kind: list("kind").pop(),
                since: NaiveDate::parse_from_str(&query(q, "since"), "%Y-%m-%d").ok(),
                include_archive: query(q, "archive") == "1",
            };
            return json_response(brain_search::search(&vault()?, &query(q, "q"), &options)?);
        }
        "/api/note" => {
            return json_response(
                json!({"text":read(&note_path(&query(&request.query,"path"))?,NOTE_LIMIT)?}),
            )
        }
        "/api/navigation" => {
            let mut v =
                read_json(&home()?.join(".local/state/nacre/shell/extras/brain-focus.json"));
            if !v.is_object()
                || Utc::now().timestamp() as f64 - v["created"].as_f64().unwrap_or(0.0) > 60.0
            {
                v = json!({})
            }
            if GRAPH_READY.load(std::sync::atomic::Ordering::Acquire) {
                if let Some((revision, _)) = GRAPH.lock().map_err(|_| "Graph lock failed")?.as_ref()
                {
                    v["graphRevision"] = json!(revision);
                }
            }
            return json_response(v);
        }
        "/auth-complete" => {
            return Ok(response(
                200,
                "text/html",
                b"<!doctype html><script src=\"/auth-close.js\"></script>".to_vec(),
            ))
        }
        "/auth-close.js" => {
            return Ok(response(
                200,
                "text/javascript",
                (BROWSER_AUTH_SCRIPT.to_owned()
                    + "window.__nacreBrainAuthReady.then(()=>window.close());")
                    .into_bytes(),
            ))
        }
        _ => {}
    }
    let (file, mime) = match request.path.as_str() {
        "/" => ("index.html", "text/html; charset=utf-8"),
        "/app.js" => ("app.js", "text/javascript"),
        "/style.css" => ("style.css", "text/css"),
        "/i18n.js" => ("i18n.js", "text/javascript"),
        "/i18n-data.js" => ("i18n-data.js", "text/javascript"),
        "/translation-engine.js" => ("translation-engine.js", "text/javascript"),
        "/graph.js" => ("graph.js", "text/javascript"),
        "/vendor/d3-force.min.js" => ("vendor/d3-force.min.js", "text/javascript"),
        _ => return Ok(response(404, "application/json", b"{}".to_vec())),
    };
    let mut content = read(&root().join("web").join(file), 4 * 1024 * 1024)?;
    if file == "style.css" {
        content += &theme_css()?
    }
    if file == "app.js" {
        content = BROWSER_AUTH_SCRIPT.to_owned() + &content;
    }
    if file == "index.html" {
        let language = env::var("NACRE_LANGUAGE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                text(
                    &read_json(
                        &home()
                            .unwrap_or_default()
                            .join(".config/nacre/desktop.json"),
                    )["language"],
                )
                .to_owned()
            });
        let language =
            if regex(r"^(?:system|pseudo|[a-z]{2,3}(?:[-_][A-Za-z0-9]+)*)$").is_match(&language) {
                language.as_str()
            } else {
                "system"
            };
        content = content.replace(
            "<html lang=\"en\"",
            &format!("<html lang=\"en\" data-nacre-language=\"{language}\""),
        );
    }
    Ok(response(200, mime, content.into_bytes()))
}
pub fn run(tool: &str, args: &[String]) -> Result<i32> {
    if tool == "brain-launch" {
        let action = args.first().map(String::as_str).unwrap_or("brain");
        if !["brain", "capture", "notes"].contains(&action) {
            eprintln!("Usage: nacre-brain-ui [brain|capture|notes]");
            return Ok(2);
        }
        brain_launch::action(action, args.get(1).map(String::as_str).unwrap_or(""))?;
        return Ok(0);
    }
    let op = args.first().map(String::as_str).unwrap_or("");
    if tool == "brain-discovery"
        || tool == "brain-auth"
        || args.get(1).is_some_and(|s| s.starts_with('{'))
    {
        let request: Value = serde_json::from_str(args.get(1).map(String::as_str).unwrap_or("{}"))?;
        let result = match tool {
            "brain-discovery" => brain_discovery::api(op, &request),
            "brain-auth" => auth_api(op, &request),
            _ => api(op, &request),
        };
        let (value, code) = match result {
            Ok(v) => (v, 0),
            Err(e) => (json!({"error":e.to_string(),"type":"ValueError"}), 1),
        };
        println!("{}", serde_json::to_string(&value)?);
        return Ok(code);
    }
    match op {
        "serve" => {
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port()?))?;
            schedule_graph_build(true);
            start_vault_watch();
            let idle = env::var("NACRE_BRAIN_IDLE_SECONDS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(300);
            if brain_launch::browser_running() {
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_secs(1));
                    let _ = brain_launch::migrate_existing_browser();
                });
            }
            let result = brain_http::serve(listener, &state()?, Duration::from_secs(idle), route);
            {
                let (signal, cv) = &*WATCH_SIGNAL;
                let _guard = signal.lock().unwrap_or_else(|e| e.into_inner());
                GRAPH_WATCHING.store(false, std::sync::atomic::Ordering::Release);
                cv.notify_all();
            }
            clear_semantic();
            result?;
        }
        "watch-brain" => brain_launch::watch_brain()?,
        "action" => brain_launch::action(
            args.get(1).map(String::as_str).unwrap_or(""),
            args.get(2).map(String::as_str).unwrap_or(""),
        )?,
        "graph" => {
            let result = graph();
            clear_semantic();
            println!("{}", serde_json::to_string(&result?)?);
        }
        _ => return Ok(2),
    }
    Ok(0)
}
fn api(operation: &str, request: &Value) -> Result<Value> {
    let args = request["args"].as_array().cloned().unwrap_or_default();
    let first = args.first().cloned().unwrap_or(Value::Null);
    match operation {
        "fields" => Ok(fields(text(&first))),
        "note_path" => Ok(json!(note_path(text(&first))?)),
        "search_notes" => search_notes(text(&first)),
        "graph" => graph(),
        "theme_css" => Ok(json!(theme_css()?)),
        _ => Err("Unknown Brain operation".into()),
    }
}
fn auth_api(operation: &str, request: &Value) -> Result<Value> {
    if operation != "private_write" {
        return Err("Unknown Brain authentication operation".into());
    }
    let args = request["args"]
        .as_array()
        .ok_or("Missing authentication arguments")?;
    let path = PathBuf::from(
        args.first()
            .and_then(|v| v["$path"].as_str())
            .ok_or("Missing authentication path")?,
    );
    let body = args
        .get(1)
        .and_then(Value::as_str)
        .ok_or("Missing authentication body")?;
    reject_links(&path)?;
    private_dir(path.parent().ok_or("Invalid authentication path")?)?;
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    tmp.write_all(body.as_bytes())?;
    tmp.persist(path)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, AccessMode};
    fn fixture() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn note(root: &Path, path: &str, body: &str) {
        let p = root.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    #[test]
    fn vault_metadata_fingerprint_detects_note_mutation_without_reading_bodies() {
        let t = fixture();
        note(t.path(), "a.md", "one");
        let before = vault_metadata_fingerprint(t.path()).unwrap();
        assert_eq!(before, vault_metadata_fingerprint(t.path()).unwrap());
        note(t.path(), "a.md", "two with changed size");
        assert_ne!(before, vault_metadata_fingerprint(t.path()).unwrap());
    }
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }
    #[test]
    fn watcher_wait_has_no_deadline_while_paused_and_resets_on_request() {
        assert_eq!(watcher_wait(Duration::ZERO), Some(Duration::from_secs(600)));
        assert_eq!(
            watcher_wait(Duration::from_secs(599)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(watcher_wait(Duration::from_secs(600)), None);
        assert_eq!(watcher_wait(Duration::from_secs(3600)), None);
        // A request replaces the injected inactivity duration with zero.
        assert_eq!(watcher_wait(Duration::ZERO), Some(Duration::from_secs(600)));
    }
    #[test]
    fn cached_new_badges_expire_without_a_vault_mutation() {
        let entry = json!({"kind":"world","status":"auto","created":"2026-11-05","new":true});
        let mut value = json!({"nodes":[entry.clone()],"worlds":[{"status":"auto","created":"2026-11-05","areas":[entry]}]});
        refresh_new_badges(&mut value, NaiveDate::from_ymd_opt(2026, 11, 11).unwrap());
        assert_eq!(value["nodes"][0]["new"], true);
        refresh_new_badges(&mut value, NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        assert_eq!(value["nodes"][0]["new"], false);
        assert_eq!(value["worlds"][0]["new"], false);
        assert_eq!(value["worlds"][0]["areas"][0]["new"], false);
    }
    #[test]
    fn watcher_filters_access_and_hidden_events_but_keeps_vault_mutations() {
        let root = Path::new("/vault");
        let open = Event::new(EventKind::Access(AccessKind::Open(AccessMode::Read)))
            .add_path(root.join("note.md"));
        let close_read = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Read)))
            .add_path(root.join("note.md"));
        let write_close = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Write)))
            .add_path(root.join("note.md"));
        let mutate = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join("note.md"));
        let hidden = Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(root.join(".brain-state/index/graph.json"));
        let rename = Event::new(EventKind::Modify(notify::event::ModifyKind::Name(
            notify::event::RenameMode::Both,
        )))
        .add_path(root.join(".registry.json.tmp"))
        .add_path(root.join("registry.json"));
        assert!(!visible_vault_event(&open, root));
        assert!(!visible_vault_event(&close_read, root));
        assert!(!visible_vault_event(&write_close, root));
        assert!(visible_vault_event(&mutate, root));
        assert!(!visible_vault_event(&hidden, root));
        assert!(visible_vault_event(&rename, root));
    }
    #[test]
    fn watcher_does_not_follow_vault_directory_symlinks() {
        let root = fixture();
        let outside = fixture();
        note(outside.path(), "outside.md", "# Outside");
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        let (_watcher, events) = create_vault_watcher(root.path()).unwrap();
        note(outside.path(), "outside.md", "# Changed outside");
        assert!(events.recv_timeout(Duration::from_millis(100)).is_err());
        note(root.path(), "inside.md", "# Inside");
        let event = events
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert!(event.paths.iter().any(|path| path.starts_with(root.path())));
    }
    #[test]
    fn registry_projection_emits_world_area_and_review_contract() {
        let t = fixture();
        fs::write(t.path().join("registry.json"), r#"{"worlds":[{"name":"World A","aliases":["Old World"],"status":"seed","created":"2026-11-05"}],"areas":[{"name":"Area A","aliases":["Old Area"],"parent":"World A","status":"auto","created":"2026-11-05"}]}"#).unwrap();
        note(
            t.path(),
            "area-a.md",
            "# One\nWorlds: Old Area\nReview: low-confidence",
        );
        note(
            t.path(),
            "world-alias.md",
            "# Two\nWorlds: Old World\nAreas: Old Area",
        );
        let indexed = notes(t.path(), NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()).unwrap();
        let (worlds, nodes) = hierarchy(
            t.path(),
            &indexed,
            NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        )
        .unwrap();
        assert_eq!(worlds[0]["count"], 2);
        assert_eq!(worlds[0]["areas"][0]["count"], 2);
        assert_eq!(indexed[0]["review"], true);
        assert!(nodes
            .iter()
            .any(|n| n["kind"] == "area" && n["new"] == true));
        assert!(nodes.iter().any(|n| n["kind"] == "membership"));
    }
    #[test]
    fn first_paint_summary_uses_registry_and_keeps_all_edges_valid() {
        let t = fixture();
        fs::write(t.path().join("registry.json"), r#"{"worlds":[{"name":"World A","status":"seed","created":"2026-11-05"}],"areas":[{"name":"Area A","parent":"World A","status":"seed","created":"2026-11-05"}]}"#).unwrap();
        let all=(0..45).map(|i|json!({"id":format!("note:{i}"),"label":format!("Note {i}"),"kind":"note","recorded":"2026-11-05T00:00:00Z","meta":{},"worlds":["World A"],"areas":["Area A"],"text":"body"})).collect::<Vec<_>>();
        let (nodes, links, worlds) = summary_projection(
            t.path(),
            &all,
            NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
        )
        .unwrap();
        let ids = nodes
            .iter()
            .map(|n| text(&n["id"]).to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(worlds[0]["id"], "world:world-a");
        assert_eq!(worlds[0]["count"], 45);
        assert_eq!(nodes.iter().filter(|n| n["kind"] == "note").count(), 40);
        assert!(links
            .iter()
            .all(|e| ids.contains(text(&e["source"])) && ids.contains(text(&e["target"]))));
        assert!(nodes.iter().all(|n| n["kind"] != "hub"));
    }
    #[test]
    fn brain_page_and_bootstrap_use_the_same_loopback_origin_as_api() {
        let page = route(brain_http::Request {
            method: "GET".into(),
            path: "/".into(),
            query: String::new(),
            headers: BTreeMap::new(),
            body: Vec::new(),
        });
        let html = String::from_utf8(page.body).unwrap();
        assert_eq!(page.status, 200);
        assert!(html.contains("/app.js"));
        assert!(BROWSER_AUTH_SCRIPT.contains("'/api/bootstrap'"));
        assert!(BROWSER_AUTH_SCRIPT.contains("location.href"));
        assert!(!html.contains("file://"));
    }
    #[test]
    fn fields_ignore_fences_sections_and_preserve_annotations() {
        assert_eq!(
            fields("# Public\nWorlds: Demo\nTopics: fixture\n- [ ] Read docs\n"),
            json!({"worlds":"Demo","topics":"fixture"})
        );
        assert_eq!(
            fields("# Schema\n```\nEntity: world\nName: Imaginary\n```\n## Later\nWorlds: bad"),
            json!({})
        );
    }
    #[test]
    fn viewer_subjects_use_header_only_world_and_project_metadata() {
        let t = fixture();
        note(
            t.path(),
            "inbox/header-only.md",
            "# Header metadata regression\n\nRecorded: 2026-10-20\nWorlds: \nTopics: Training\n\nProject: /home/fixture/repo. Main and managed checkout paths.\nRecorded: 1999-01-01\nConfidence: verified\n",
        );
        let indexed = notes(t.path(), day()).unwrap();
        assert!(indexed[0]["meta"].get("worlds").is_none());
        assert_eq!(indexed[0]["meta"]["topics"], "Training");
        assert!(indexed[0]["meta"].get("project").is_none());
        assert_eq!(indexed[0]["date"], "2026-10-20");
        assert_eq!(indexed[0]["recorded"], "2026-10-20T00:00:00+00:00");
        assert_eq!(indexed[0]["confidence"], "source page");

        let graph = project(t.path());
        let hubs = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["kind"] == "hub")
            .map(|node| text(&node["label"]).to_lowercase())
            .collect::<Vec<_>>();
        assert!(!hubs.iter().any(|label| label.starts_with("topics")));
        assert!(!hubs.iter().any(|label| label.contains("home/fixture")));
    }
    #[test]
    fn index_excludes_hidden_instructions_and_links() {
        let t = fixture();
        note(t.path(), ".brain-state/private.md", "# Hidden");
        note(t.path(), "AGENTS.md", "# Instructions");
        note(
            t.path(),
            "personal/one.md",
            "# Personal\nRecorded: 2026-10-02",
        );
        std::os::unix::fs::symlink("/etc/passwd", t.path().join("escape.md")).unwrap();
        let ns = notes(t.path(), day()).unwrap();
        assert_eq!(ns.len(), 1);
        assert_eq!(ns[0]["label"], "Personal");
    }
    #[test]
    fn wiki_links_deduplicate_and_filename_collisions_are_rejected() {
        let t = fixture();
        note(t.path(),"wiki/start.md","# Start\n[[wiki/target|Target page]]\n[[wiki/target#Details]]\n[Target](target.md)\n[[duplicate]]");
        note(t.path(), "wiki/target.md", "# Target");
        let mut ns = notes(t.path(), day()).unwrap();
        assert_eq!(source_links(&mut ns).len(), 1);
        note(t.path(), "projects/duplicate.md", "# A");
        note(t.path(), "personal/duplicate.md", "# B");
        assert!(notes(t.path(), day())
            .unwrap_err()
            .to_string()
            .contains("Duplicate Brain note filename"));
    }
    #[test]
    fn filename_ids_survive_non_utf8_archive_moves() {
        use std::os::unix::ffi::OsStrExt;
        let root = fixture();
        let filename = std::ffi::OsStr::from_bytes(b"raw-\xff.md");
        fs::write(root.path().join(filename), "# Raw filename").unwrap();
        let before = notes(root.path(), day()).unwrap()[0]["id"].clone();
        fs::create_dir(root.path().join("archive")).unwrap();
        fs::rename(
            root.path().join(filename),
            root.path().join("archive").join(filename),
        )
        .unwrap();
        let moved = notes(root.path(), day()).unwrap();
        assert_eq!(before, moved[0]["id"]);
        assert_eq!(moved[0]["kind"], "archive");
    }
    #[test]
    fn timestamps_revision_and_activity() {
        let t = fixture();
        note(
            t.path(),
            "one.md",
            "# AI inference\nRecorded: 2026-10-03T18:00:00+02:00\nConfidence: reported",
        );
        note(
            t.path(),
            "two.md",
            "# Circuit soldering\nRecorded: 2020-01-01\nConfidence: verified",
        );
        note(t.path(), "three.md", "# Undated personal note");
        let ns = notes(t.path(), day()).unwrap();
        assert_eq!(ns[0]["recorded"], "2026-10-03T16:00:00+00:00");
        assert_eq!(ns[1]["activity"], 0);
        assert!(ns[0]["activity"].as_f64().unwrap() > ns[2]["activity"].as_f64().unwrap());
        let old = ns[0].clone();
        note(
            t.path(),
            "one.md",
            "# AI inference\nRecorded: 2026-10-03T18:00:00+02:00\nExtra evidence",
        );
        let new = notes(t.path(), day()).unwrap();
        assert_ne!(old["revision"], new[0]["revision"]);
        assert_eq!(old["activity"], new[0]["activity"]);
    }
    #[test]
    fn state_ancestor_links_are_refused() {
        let t = fixture();
        let outside = fixture();
        std::os::unix::fs::symlink(outside.path(), t.path().join("alias")).unwrap();
        assert!(write_json(&t.path().join("alias/deep/subjects.json"), &json!({})).is_err());
        assert!(!outside.path().join("deep").exists());
    }
    #[test]
    fn response_routes_allowlist_assets_and_reject_unknown_action() {
        for path in ["/auth.py", "/control.py", "/web/../auth.py", "/allowed.md"] {
            let r = route(brain_http::Request {
                method: "GET".into(),
                path: path.into(),
                query: String::new(),
                headers: Default::default(),
                body: vec![],
            });
            assert_eq!(r.status, 404);
        }
        let r = route(brain_http::Request {
            method: "POST".into(),
            path: "/api/action".into(),
            query: String::new(),
            headers: Default::default(),
            body: br#"{"name":"exec"}"#.to_vec(),
        });
        assert_eq!(r.status, 400);
    }

    #[test]
    fn graph_projection_scopes_nodes_and_keeps_only_valid_edges() {
        let graph = json!({
            "nodes":[
                {"id":"world:work","label":"Work","kind":"hub","world":"Operations","count":4},
                {"id":"area:tools","label":"Tools","kind":"area","area":"Tools","count":2},
                {"id":"note:a","label":"A","kind":"note"},
                {"id":"note:b","label":"B","kind":"note"},
                {"id":"wiki:guide","label":"Guide","kind":"note","path":"wiki/guide.md"}
            ],
            "links":[
                {"source":"world:work","target":"area:tools","kind":"membership"},
                {"source":"area:tools","target":"note:a","kind":"membership"},
                {"source":"note:a","target":"note:b","kind":"source"},
                {"source":"note:b","target":"wiki:guide","kind":"source"}
            ]
        });
        let all = graph_projection(&graph, "all");
        assert_eq!(all["nodes"].as_array().unwrap().len(), 3);
        assert!(all["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|node| node["kind"] != "note"));
        assert!(all["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["kind"] == "wiki"));
        let note = graph_projection(&graph, "note:note:a");
        assert_eq!(note["nodes"].as_array().unwrap().len(), 5);
        let area = graph_projection(&graph, "area:Tools");
        assert!(area["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == "note:a"));
        let world = graph_projection(&graph, "world:Operations");
        assert!(world["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == "area:tools"));
        assert!(area["links"].as_array().unwrap().iter().all(|edge| {
            let source = endpoint(&edge["source"]);
            let target = endpoint(&edge["target"]);
            area["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| text(&node["id"]) == source)
                && area["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|node| text(&node["id"]) == target)
        }));
    }

    fn project(root: &Path) -> Value {
        let mut ns = notes(root, day()).unwrap();
        let mut links = source_links(&mut ns);
        let (_, projected) = hierarchy(root, &ns, day()).unwrap();
        let mut nodes = Vec::new();
        for item in projected {
            if item["kind"] == "membership" {
                links.push(item);
            } else {
                nodes.push(item);
            }
        }
        let mut note_nodes = Vec::new();
        for mut n in ns.drain(..) {
            if let Some(object) = n.as_object_mut() {
                for key in ["text", "meta", "references"] {
                    object.remove(key);
                }
            }
            note_nodes.push(n);
        }
        nodes.extend(note_nodes);
        json!({"nodes":nodes,"links":links})
    }
    #[test]
    fn note_paths_reject_traversal_hidden_absolute_and_links() {
        let t = fixture();
        let outside = fixture();
        note(t.path(), "allowed.md", "# Public fixture");
        note(outside.path(), "outside.md", "outside fixture");
        std::os::unix::fs::symlink(
            outside.path().join("outside.md"),
            t.path().join("linked.md"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), t.path().join("directory-link")).unwrap();
        note(t.path(), ".hidden.md", "hidden fixture");
        for p in [
            "../outside.md",
            "linked.md",
            "directory-link/outside.md",
            ".hidden.md",
            "missing.md",
        ] {
            assert!(note_path_at(t.path(), p).is_err(), "{p}");
        }
        assert!(note_path_at(
            t.path(),
            outside.path().join("outside.md").to_str().unwrap()
        )
        .is_err());
        assert!(note_path_at(t.path(), t.path().join("allowed.md").to_str().unwrap()).is_err());
        assert!(note_path_at(t.path(), "./allowed.md").is_err());
        assert_eq!(
            read(&note_path_at(t.path(), "allowed.md").unwrap(), NOTE_LIMIT).unwrap(),
            "# Public fixture"
        );
    }
    #[test]
    fn explicit_links_remain_in_registry_graph() {
        let t = fixture();
        note(
            t.path(),
            "wiki/photography.md",
            "# Photography\nEntity: world\nName: Photography",
        );
        note(
            t.path(),
            "projects/camera.md",
            "# Camera controller\n[Entity](../wiki/photography.md)",
        );
        let graph = project(t.path());
        assert_eq!(
            graph["links"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| l["kind"] == "source")
                .count(),
            1
        );
    }
    #[test]
    fn grouping_override_preserves_note_and_reset_restores_annotation() {
        let t = fixture();
        note(
            t.path(),
            "inbox/result.md",
            "# A result\nRecorded: 2026-10-03\nWorlds: Original\nGrip training",
        );
        let before = fs::read(t.path().join("inbox/result.md")).unwrap();
        assign_note_at(
            t.path(),
            r#"{"path":"inbox/result.md","subjects":["Climbing"]}"#,
        )
        .unwrap();
        let graph = project(t.path());
        assert!(graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "note"));
        assert_eq!(fs::read(t.path().join("inbox/result.md")).unwrap(), before);
        assert_eq!(
            fs::metadata(t.path().join(".brain-state/discovery/overrides.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(assign_note_at(t.path(), r#"{"path":"../outside.md","subjects":["X"]}"#).is_err());
        assign_note_at(t.path(), r#"{"path":"inbox/result.md","reset":true}"#).unwrap();
        let graph = project(t.path());
        assert!(graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "note"));
    }
    #[test]
    fn declarations_in_raw_imports_and_examples_do_not_create_worlds() {
        let t = fixture();
        note(
            t.path(),
            "wiki/example.md",
            "# Schema\n```\nEntity: world\nName: Imaginary\n```",
        );
        note(
            t.path(),
            "raw/import.md",
            "# Import\nEntity: world\nName: Untrusted",
        );
        let graph = project(t.path());
        let labels = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["kind"] == "world")
            .map(|n| text(&n["label"]))
            .collect::<BTreeSet<_>>();
        assert!(!labels.contains("Imaginary"));
        assert!(!labels.contains("Untrusted"));
    }
    #[test]
    fn assignment_requires_the_existing_string_value_contract() {
        for value in [
            json!({"path":"fixture.md","subjects":["Unsafe coercion"]}),
            json!([]),
            json!(true),
            json!(42),
        ] {
            let r = route(brain_http::Request {
                method: "POST".into(),
                path: "/api/action".into(),
                query: String::new(),
                headers: Default::default(),
                body: serde_json::to_vec(&json!({"name":"assign-note","value":value})).unwrap(),
            });
            assert_eq!(r.status, 400);
        }
    }

    #[test]
    fn percent_decoder_accepts_unicode_and_malformed_escapes_without_panics() {
        assert_eq!(decode("%E2%82%AC"), "€");
        assert_eq!(decode("%é"), "%é");
        assert_eq!(decode("a%20b"), "a b");
    }
}
