//! Standalone Brain graph, routes and bounded optional semantic worker.
use crate::{brain_data, brain_discovery, brain_http, brain_launch, Result};
use chrono::{DateTime, NaiveDate, Utc};
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
    sync::{LazyLock, Mutex},
    time::Duration,
};

const NOTE_LIMIT: u64 = 1_048_576;
const VAULT_LIMIT: usize = 64 * 1024 * 1024;
const ROOT_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "README.md", "INDEX.md"];
// The independent API credential stays in storage scoped to scheme/host/port.
// Cookies are deliberately insufficient: every localhost port receives them.
const BROWSER_AUTH_SCRIPT: &str = r#"(()=>{const key='nacre-brain-authorization';const token=new URLSearchParams(location.hash.slice(1)).get('nacre-auth');const original=window.fetch.bind(window);let ready=Promise.resolve();if(token&&/^[0-9a-f]{64}$/.test(token)){history.replaceState(null,'',location.pathname+location.search);ready=original('/api/navigation',{headers:{'X-Nacre-Brain-Authorization':token}}).then(response=>{if(response.ok)localStorage.setItem(key,token);}).catch(()=>{});}window.__nacreBrainAuthReady=ready;window.fetch=async(input,init={})=>{await ready;const url=new URL(typeof input==='object'&&input!==null&&typeof input.url==='string'?input.url:String(input),location.href);if(url.origin!==location.origin)return original(input,init);const headers=new Headers(init.headers||(input instanceof Request?input.headers:undefined));const credential=localStorage.getItem(key);if(credential)headers.set('X-Nacre-Brain-Authorization',credential);return original(input,{...init,headers});};})();
"#;
const ACTIVITY_MODEL: &str = "Meaningful saved evidence, 21-day half-life, deduplicated captures and diminishing daily returns. Raw message counting is not enabled.";
const THEMES: &[(&str, &str, &str, &[&str])] = &[
    (
        "ai",
        "AI & learning",
        "#a9c9f2",
        &[
            "ai",
            "artificial intelligence",
            "machine learning",
            "inference",
            "tensorrt",
            "neural",
            "codex",
            "claude",
            "llm",
        ],
    ),
    (
        "electronics",
        "Electronics & hardware",
        "#efbd87",
        &[
            "electronics",
            "circuit",
            "solder",
            "sensor",
            "camera",
            "gun",
            "bosch",
            "firmware",
            "motor",
            "battery",
        ],
    ),
    (
        "software",
        "Software & games",
        "#c3b2ec",
        &[
            "software", "frontend", "backend", "api", "game", "parser", "code", "release",
            "package",
        ],
    ),
    (
        "infrastructure",
        "Systems & networks",
        "#87cbbb",
        &[
            "network",
            "server",
            "deployment",
            "sync",
            "ssh",
            "wifi",
            "latency",
        ],
    ),
    (
        "personal",
        "Personal life",
        "#e6c796",
        &[
            "personal",
            "bouldering",
            "climbing",
            "goals",
            "background",
            "education",
            "preference",
        ],
    ),
    (
        "music",
        "Music & sound",
        "#dda9c8",
        &["music", "musikki", "spotify", "audio", "sound"],
    ),
    (
        "research",
        "Research & science",
        "#a4d6dc",
        &[
            "research",
            "science",
            "experiment",
            "investigation",
            "comparison",
        ],
    ),
    ("general", "Mixed knowledge", "#a2b8c4", &[]),
];
fn digest(s: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(s.as_ref()))
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
fn reject_links(p: &Path) -> Result<()> {
    for a in p.ancestors() {
        if fs::symlink_metadata(a).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Refusing linked Brain state".into());
        }
    }
    Ok(())
}
fn private_dir(p: &Path) -> Result<()> {
    reject_links(p)?;
    fs::create_dir_all(p)?;
    fs::set_permissions(p, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
fn read(p: &Path, limit: u64) -> Result<String> {
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
    let clean = regex(r"(?ms)^```[^\n]*\n.*?^```[^\n]*$").replace_all(body, "");
    let clean = regex(r"(?m)^##\s")
        .split(&clean)
        .next()
        .unwrap_or("")
        .to_owned();
    let mut result = Map::new();
    for c in
        regex(r"(?mi)^(Entity|Name|Parent|Project|Worlds|Topics|Tags|Category|Aliases):\s*([^\n]+)")
            .captures_iter(&clean)
    {
        result.insert(
            c[1].to_lowercase(),
            json!(c[2].trim().trim_matches(['\"', '\''])),
        );
    }
    Value::Object(result)
}
fn category(label: &str, body: &str, declared: &str) -> (&'static str, &'static str) {
    if let Some(t) = THEMES.iter().find(|t| t.0 == declared) {
        return (t.0, "declared");
    }
    let clean=regex(r"(?mi)^(Source|Recorded|Reviewed|Confidence|Status|Entity|Name|Project|Parent|Category|Aliases|Topics|Tags):[^\n]*").replace_all(body,"");
    let body = clean.chars().take(1800).collect::<String>().to_lowercase();
    let label = label.to_lowercase();
    let mut best = ("general", 0);
    for t in THEMES {
        let score =
            t.3.iter()
                .map(|word| {
                    let r = regex(&format!(r"(?:^|[^\w]){}(?:$|[^\w])", regex::escape(word)));
                    4 * usize::from(r.is_match(&label)) + usize::from(r.is_match(&body))
                })
                .sum();
        if score > best.1 {
            best = (t.0, score)
        }
    }
    (best.0, "inferred")
}
fn theme(key: &str) -> (&'static str, &'static str) {
    let t = THEMES.iter().find(|t| t.0 == key).unwrap_or(&THEMES[7]);
    (t.1, t.2)
}
fn date(body: &str) -> Option<NaiveDate> {
    let r = regex(r"(?m)^(?:Recorded|Reviewed):\s*(\d{4}-\d{2}-\d{2})");
    r.captures(body)
        .and_then(|c| NaiveDate::parse_from_str(&c[1], "%Y-%m-%d").ok())
}
fn timestamp(body: &str) -> String {
    let r = regex(r"(?m)^Recorded:\s*(\S+)");
    let Some(c) = r.captures(body) else {
        return String::new();
    };
    if let Ok(d) = DateTime::parse_from_rfc3339(&c[1]) {
        return d.with_timezone(&Utc).to_rfc3339();
    }
    if let Ok(d) = chrono::NaiveDateTime::parse_from_str(&c[1], "%Y-%m-%dT%H:%M:%S%.f") {
        return d.and_utc().to_rfc3339();
    }
    if let Ok(d) = NaiveDate::parse_from_str(&c[1], "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0).unwrap().and_utc().to_rfc3339();
    }
    String::new()
}
fn notes(root: &Path, today: NaiveDate) -> Result<Vec<Value>> {
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
        let (cat, _) = category(&title, &body, text(&meta["category"]));
        let day = date(&body);
        let age = day.map(|d| (today - d).num_days().max(0));
        let activity = age
            .map(|a| (-std::f64::consts::LN_2 * a as f64 / 21.0).exp())
            .unwrap_or(0.0);
        let confidence = regex(r"(?m)^Confidence:\s*(.*)")
            .captures(&body)
            .map(|c| c[1].to_owned())
            .unwrap_or_else(|| "source page".into());
        notes.push(json!({"id":format!("note:{}",&digest(&rel)[..16]),"label":title,"kind":"note","path":rel,"date":day.map(|d|d.to_string()).unwrap_or_default(),"recorded":timestamp(&body),"confidence":confidence,"text":body.chars().take(60000).collect::<String>(),"activity":if day.is_some(){json!(brain_discovery::rounded(activity,4))}else{json!(0)},"revision":&digest(&body)[..16],"meta":meta,"category":cat,"color":theme(cat).1}));
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
fn source_links(notes: &mut [Value]) -> Vec<Value> {
    let lookup = notes
        .iter()
        .map(|n| (text(&n["path"]).to_owned(), text(&n["id"]).to_owned()))
        .collect::<BTreeMap<_, _>>();
    let mut stems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (p, id) in &lookup {
        stems
            .entry(casefold(
                Path::new(p).file_stem().unwrap().to_str().unwrap_or(""),
            ))
            .or_default()
            .push(id.clone());
    }
    let mut pairs = BTreeSet::new();
    let md = regex(r"\]\(([^)]+\.md)(?:#[^)]*)?\)");
    let wiki = regex(r"\[\[([^]\n]+)\]\]");
    for note in notes.iter() {
        let body = text(&note["text"]);
        let targets = md
            .captures_iter(body)
            .map(|c| (c[1].to_owned(), false))
            .chain(wiki.captures_iter(body).map(|c| (c[1].to_owned(), true)));
        for (value, is_wiki) in targets {
            let mut target = decode(value.split(['|', '#']).next().unwrap_or(""))
                .trim()
                .to_owned();
            if target.is_empty() || target.contains("://") {
                continue;
            }
            if is_wiki && !target.ends_with(".md") {
                target.push_str(".md")
            }
            let parent = Path::new(text(&note["path"]))
                .parent()
                .unwrap_or(Path::new(""));
            let mut candidates = vec![];
            if is_wiki {
                candidates.push(PathBuf::from(&target))
            }
            candidates.push(parent.join(&target));
            let mut ident = candidates
                .iter()
                .filter_map(|p| normalize_path(p))
                .find_map(|p| lookup.get(p.to_str().unwrap_or("")))
                .cloned();
            if ident.is_none() && is_wiki && !target.contains('/') {
                if let Some(matches) = stems.get(&casefold(
                    Path::new(&target)
                        .file_stem()
                        .unwrap()
                        .to_str()
                        .unwrap_or(""),
                )) {
                    if matches.len() == 1 {
                        ident = Some(matches[0].clone())
                    }
                }
            }
            if let Some(id) = ident {
                if id != text(&note["id"]) {
                    pairs.insert((text(&note["id"]).to_owned(), id));
                }
            }
        }
    }
    for note in notes {
        note["references"] = json!(pairs
            .iter()
            .filter(|(a, _)| a == text(&note["id"]))
            .map(|(_, b)| b)
            .collect::<Vec<_>>());
        note["featureText"] = json!(brain_discovery::features(note).1);
    }
    pairs
        .into_iter()
        .map(|(a, b)| json!({"source":a,"target":b,"kind":"source"}))
        .collect()
}
fn groups(subjects: &[Value], notes: &[Value], links: &mut Vec<Value>) -> Vec<Value> {
    let by = notes
        .iter()
        .map(|n| (text(&n["id"]), n))
        .collect::<BTreeMap<_, _>>();
    let mut nodes = vec![];
    for s in subjects {
        let Some(members) = s["members"].as_object() else {
            continue;
        };
        let labels = members
            .keys()
            .filter_map(|i| by.get(i.as_str()))
            .map(|n| text(&n["label"]))
            .collect::<Vec<_>>()
            .join(" ");
        let declared = text(&s["category"]);
        let cat = category(text(&s["label"]), &labels, declared).0;
        let mut days: BTreeMap<String, f64> = BTreeMap::new();
        let mut unique = BTreeSet::new();
        for id in members.keys() {
            if let Some(n) = by.get(id.as_str()) {
                let signature = digest(casefold(&brain_discovery::clean(text(&n["text"]))));
                if !text(&n["recorded"]).is_empty() && unique.insert(signature) {
                    *days.entry(text(&n["date"]).to_owned()).or_default() +=
                        n["activity"].as_f64().unwrap_or(0.0)
                }
            }
        }
        let activity = days.values().map(|x| x.ln_1p()).sum::<f64>();
        nodes.push(json!({"id":s["id"],"label":s["label"],"kind":"hub","count":members.len(),"activity":if days.is_empty(){json!(0)}else{json!(brain_discovery::rounded(activity,3))},"radius":20.0+8.0*activity.ln_1p(),"category":cat,"color":theme(cat).1,"themeLabel":theme(cat).0,"themeReason":if declared.is_empty(){"inferred"}else{"declared"},"origin":s["origin"],"parent":s["parent"],"summary":"Grouped from saved knowledge; automatic suggestions are not verified facts."}));
        let mut covered = BTreeSet::new();
        if let Some(topics) = s["topics"].as_array() {
            for t in topics {
                let ids = t["members"].as_array().cloned().unwrap_or_default();
                let labels = ids
                    .iter()
                    .filter_map(|i| by.get(text(i)))
                    .map(|n| text(&n["label"]))
                    .collect::<Vec<_>>()
                    .join(" ");
                let cat = category(text(&t["label"]), &labels, text(&t["category"])).0;
                nodes.push(json!({"id":t["id"],"label":t["label"],"kind":"topic","parent":s["id"],"count":ids.len(),"activity":0,"radius":12,"category":cat,"color":theme(cat).1,"groupReason":t["origin"],"summary":"Topics emerge from annotations and coherent note communities."}));
                links.push(
                    json!({"source":s["id"],"target":t["id"],"kind":"group","reason":t["origin"]}),
                );
                for id in ids {
                    covered.insert(text(&id).to_owned());
                    links.push(
                        json!({"source":t["id"],"target":id,"kind":"group","reason":t["origin"]}),
                    );
                }
            }
        }
        for (id, assignment) in members {
            if !covered.contains(id) {
                let mut l = json!({"source":s["id"],"target":id,"kind":"group"});
                if let Some(a) = assignment.as_object() {
                    for (k, v) in a {
                        l[k] = v.clone()
                    }
                }
                links.push(l)
            }
        }
        if !s["parent"].is_null() {
            links.push(json!({"source":s["parent"],"target":s["id"],"kind":"group","reason":"promoted topic"}));
        }
    }
    nodes
}
static GRAPH: LazyLock<Mutex<Option<(String, Value)>>> = LazyLock::new(|| Mutex::new(None));
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
        root,
        notes
            .iter()
            .map(|n| json!([n["id"], n["revision"]]))
            .collect::<Vec<_>>(),
        today.to_string(),
        overrides
    ]))?);
    let mut cache = GRAPH.lock().map_err(|_| "Graph lock failed")?;
    if let Some((key, value)) = &*cache {
        if key == &fingerprint {
            let mut v = value.clone();
            decorate(&mut v)?;
            return Ok(v);
        }
    }
    let prior = read_json(&derived.join("subjects.json"));
    let (dense, health) = semantic_vectors(&notes, &derived.join("vectors.json"))?;
    let mut input = json!(notes);
    let result = brain_discovery::organize_mut(&mut input, &dense, &prior, &overrides)?;
    notes = input.as_array().ok_or("Invalid discovery notes")?.clone();
    let subjects = result[0].as_array().ok_or("Invalid subjects")?;
    let mut report = result[1].clone();
    write_json(&derived.join("subjects.json"), &report)?;
    let mut nodes = groups(subjects, &notes, &mut links);
    for mut n in notes {
        if let Some(o) = n.as_object_mut() {
            for k in ["text", "meta", "references", "featureText"] {
                o.remove(k);
            }
        }
        nodes.push(n)
    }
    report["embedding"] = json!(health);
    let mut value = json!({"nodes":nodes,"links":links,"noteCount":input.as_array().unwrap().len(),"activityModel":ACTIVITY_MODEL,"discovery":report});
    decorate(&mut value)?;
    *cache = Some((fingerprint, value.clone()));
    Ok(value)
}
fn decorate(v: &mut Value) -> Result<()> {
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
pub fn search_notes(query: &str) -> Result<Value> {
    let words = casefold(&query.chars().take(200).collect::<String>());
    let words = words.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return Ok(json!([]));
    }
    let mut ids = vec![];
    for n in notes(&vault()?, Utc::now().date_naive())? {
        let content = casefold(&format!(
            "{}\n{}",
            text(&n["path"]),
            read(&vault()?.join(text(&n["path"])), NOTE_LIMIT)?
        ));
        if words.iter().all(|w| content.contains(w)) {
            ids.push(n["id"].clone())
        }
    }
    Ok(json!(ids))
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
    let id = format!("note:{}", &digest(relative)[..16]);
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
        "/api/health" => return json_response(json!({"ready":true,"authVersion":1})),
        "/api/brain" => return json_response(graph()?),
        "/api/search" => {
            return json_response(json!({"ids":search_notes(&query(&request.query,"q"))?}))
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
    fn fixture() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn note(root: &Path, path: &str, body: &str) {
        let p = root.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
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
    fn wiki_links_deduplicate_and_ambiguous_stems_do_not_connect() {
        let t = fixture();
        note(t.path(),"wiki/start.md","# Start\n[[wiki/target|Target page]]\n[[wiki/target#Details]]\n[Target](target.md)\n[[duplicate]]");
        note(t.path(), "wiki/target.md", "# Target");
        note(t.path(), "projects/duplicate.md", "# A");
        note(t.path(), "personal/duplicate.md", "# B");
        let mut ns = notes(t.path(), day()).unwrap();
        assert_eq!(source_links(&mut ns).len(), 1);
    }
    #[test]
    fn timestamps_revision_activity_and_categories() {
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
        assert_ne!(ns[0]["color"], ns[2]["color"]);
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

    fn project(root: &Path) -> Value {
        let mut ns = notes(root, day()).unwrap();
        let mut links = source_links(&mut ns);
        let mut input = json!(ns);
        let derived = root.join(".brain-state/discovery");
        let out = brain_discovery::organize_mut(
            &mut input,
            &json!({}),
            &json!({}),
            &read_json(&derived.join("overrides.json")),
        )
        .unwrap();
        ns = input.as_array().unwrap().clone();
        let mut nodes = groups(out[0].as_array().unwrap(), &ns, &mut links);
        nodes.extend(ns);
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
        assert_eq!(
            read(&note_path_at(t.path(), "allowed.md").unwrap(), NOTE_LIMIT).unwrap(),
            "# Public fixture"
        );
    }
    #[test]
    fn explicit_links_grouping_and_topic_entity_categories() {
        let t = fixture();
        note(
            t.path(),
            "wiki/photography.md",
            "# Photography\nEntity: world\nName: Photography\nCategory: research",
        );
        note(t.path(),"wiki/lighting.md","# Lighting\nEntity: topic\nParent: photography\nCategory: electronics\nLighting sensor circuit");
        note(
            t.path(),
            "projects/camera.md",
            "# Camera controller\nWorlds: Photography\n[Entity](../wiki/photography.md)",
        );
        let graph = project(t.path());
        let nodes = graph["nodes"].as_array().unwrap();
        let ids = nodes
            .iter()
            .map(|n| text(&n["id"]))
            .collect::<BTreeSet<_>>();
        assert!(graph["links"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| ids.contains(text(&l["source"])) && ids.contains(text(&l["target"]))));
        assert_eq!(
            graph["links"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| l["kind"] == "source")
                .count(),
            1
        );
        assert!(graph["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["kind"] == "group"));
        assert_eq!(
            nodes.iter().find(|n| n["id"] == "photography").unwrap()["themeReason"],
            "declared"
        );
        assert_eq!(
            nodes
                .iter()
                .find(|n| n["id"] == "photography:entity-lighting")
                .unwrap()["category"],
            "electronics"
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
        let n = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["kind"] == "note")
            .unwrap();
        assert_eq!(n["memberships"][0]["subject"], "climbing");
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
        assert_eq!(
            graph["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["kind"] == "note")
                .unwrap()["memberships"][0]["subject"],
            "original"
        );
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
            .filter(|n| n["kind"] == "hub")
            .map(|n| text(&n["label"]))
            .collect::<BTreeSet<_>>();
        assert!(!labels.contains("Imaginary"));
        assert!(!labels.contains("Untrusted"));
    }
    #[test]
    fn registered_project_and_shared_annotations_need_no_registry() {
        let t = fixture();
        note(t.path(),"projects/relay.md","# First circuit\nProject: relay\nTopics: Soldering\nRecorded: 2026-10-02\nSoldering a circuit");
        note(t.path(),"inbox/related.md","# A useful skill\nWorlds: Climbing, Electronics\nTopics: Learning\nRecorded: 2026-10-03\nNo project ID is needed.");
        let graph = project(t.path());
        let nodes = graph["nodes"].as_array().unwrap();
        assert_eq!(
            nodes.iter().find(|n| n["id"] == "relay").unwrap()["label"],
            "Relay"
        );
        assert_eq!(
            nodes
                .iter()
                .find(|n| n["id"] == "relay:tag-soldering")
                .unwrap()["category"],
            "electronics"
        );
        let topics = nodes
            .iter()
            .filter(|n| n["kind"] == "topic" && n["label"] == "Learning")
            .collect::<Vec<_>>();
        assert_eq!(topics.len(), 2);
        let id = &nodes
            .iter()
            .find(|n| n["label"] == "A useful skill")
            .unwrap()["id"];
        assert_eq!(
            graph["links"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| &l["target"] == id && topics.iter().any(|t| t["id"] == l["source"]))
                .count(),
            2
        );
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
