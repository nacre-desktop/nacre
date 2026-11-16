// SPDX-License-Identifier: GPL-3.0-only
//! N0217 Brain app: a data adapter over the on-demand `brain-control` worker's private socket
//! (`$XDG_RUNTIME_DIR/nacre/brain.sock`, JSON lines). It holds no index: every `brain.*` store
//! is a projection of a worker reply, refreshed when the worker pushes a `revision` event, and
//! the view's own state (screen, query, open note, history) stays in Luau stores.
use bead::core::{Stores, Value};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read, Write},
    os::unix::{net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

pub type Wake = Arc<dyn Fn() + Send + Sync>;
pub const SCREENS: [&str; 5] = ["overview", "graph", "search", "note", "loops"];
const SEARCH_LIMIT: u32 = 100;
const RECENT: usize = 8;

#[derive(Default)]
struct State {
    status: String,
    error: String,
    brain: Json,
    loops: Json,
    ask: Json,
    page: Option<(String, Json)>,
    note: Option<(String, Json)>,
    near: Option<(String, Json)>,
    /// Graph screen replies by scope key: `all`, `world:<name>` or `area:<name>`.
    graphs: BTreeMap<String, GraphView>,
    search: Option<(String, Json)>,
    /// Request id -> `op:key`, answered by the reader thread.
    pending: BTreeMap<u64, String>,
    /// `op:key` requested since the last revision; cleared by a `revision` event.
    fetched: BTreeSet<String>,
    /// A search typed while one is in flight: sent when the reply arrives.
    search_wanted: Option<(String, Json)>,
    search_inflight: bool,
}

/// The whole-vault graph for the Graph screen, packed to fit the store limits (a list holds at
/// most 2,048 records and 1 MiB): nodes and edges travel in chunk records `{rows:[..]}` and
/// refer to each other by position, so `ui.graph` ids stay short.
#[derive(Default)]
struct GraphView {
    state: String,
    nodes: Vec<Json>,
    edges: Vec<Json>,
    /// Per node position: note id, path, whether it opens in the reader, and the scope it drills
    /// into (worlds and areas; empty otherwise).
    meta: Vec<(String, String, bool, String)>,
}

impl GraphView {
    fn from_reply(body: &Json, key: &str) -> Self {
        let mut view = Self {
            state: text(&body["state"]).into(),
            ..Self::default()
        };
        let (mut nodes, mut position) = (Vec::new(), BTreeMap::new());
        for n in body["nodes"].as_array().into_iter().flatten() {
            let id = text(&n["id"]);
            if position.contains_key(id) {
                continue;
            }
            let kind = text(&n["kind"]);
            let note = ["note", "wiki"].contains(&kind);
            // The node's world by name (a world is its own; areas without one take the scope's);
            // `project` turns it into the `group` index, which needs the world list.
            let world = match kind {
                "world" => text(&n["label"]),
                _ => n["worlds"]
                    .as_array()
                    .and_then(|w| w.first())
                    .map_or_else(|| key.strip_prefix("world:").unwrap_or_default(), text),
            };
            position.insert(id.to_owned(), position.len());
            nodes.push(json!({"id":position.len()-1,"label":text(&n["label"]),"world":world}));
            let scope = match kind {
                "world" | "area" => format!("{kind}:{}", text(&n["label"])),
                _ => String::new(),
            };
            view.meta
                .push((id.to_owned(), text(&n["path"]).to_owned(), note, scope));
        }
        let edges = body["links"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let (from, to) = edge_ends(e);
                let (from, to) = (position.get(&from)?, position.get(&to)?);
                (from != to).then(|| json!({"from":from,"to":to}))
            })
            .collect::<Vec<_>>();
        // Radius follows degree; worlds and hubs keep a bigger minimum, so their labels show first.
        let mut degree = vec![0u32; nodes.len()];
        for e in &edges {
            for end in ["from", "to"] {
                degree[e[end].as_u64().unwrap_or_default() as usize] += 1;
            }
        }
        for (i, node) in nodes.iter_mut().enumerate() {
            let floor = if view.meta[i].2 { 1 } else { 4 };
            node["weight"] = json!(degree[i].max(floor));
        }
        view.nodes = nodes.chunks(512).map(|c| json!({"rows":c})).collect();
        view.edges = edges.chunks(1024).map(|c| json!({"rows":c})).collect();
        view
    }
}

struct Link {
    writer: UnixStream,
    next: u64,
}

pub struct Brain {
    socket: Option<PathBuf>,
    helper: PathBuf,
    state: Arc<Mutex<State>>,
    link: Arc<Mutex<Option<Link>>>,
    wake: Arc<Mutex<Option<Wake>>>,
    started: Cell<bool>,
    width: Cell<u32>,
    seen_open: RefCell<String>,
    seen_query: RefCell<String>,
    seen_scope: RefCell<String>,
    /// Bumps when the graph scope changes, so the canvas re-frames each scope.
    fit: Cell<u32>,
}

fn text(value: &Json) -> &str {
    value.as_str().unwrap_or_default()
}

/// The reply key of a graph scope (`""` is the all-worlds map).
fn graph_key(scope: &str) -> &str {
    if scope.is_empty() { "all" } else { scope }
}

fn recency(note: &Json) -> &str {
    Some(text(&note["recorded"]))
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| text(&note["date"]))
}

fn edge_ends(edge: &Json) -> (String, String) {
    let end = |v: &Json| {
        v.as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| text(&v["id"]).to_owned())
    };
    (end(&edge["source"]), end(&edge["target"]))
}

/// Markdown-lite for the reader (the kit has no rich text): headings, bullets and paragraphs;
/// wiki links, emphasis and code marks are stripped to their text. Leading `Key: value` note
/// fields and generated blocks are skipped.
pub(crate) fn blocks(markdown: &str) -> Vec<Json> {
    let mut out = Vec::new();
    let mut paragraph = String::new();
    let mut head = true;
    let mut generated = false;
    let flush = |paragraph: &mut String, out: &mut Vec<Json>| {
        if !paragraph.trim().is_empty() {
            out.push(json!({"kind":"paragraph","text":inline(paragraph.trim())}));
        }
        paragraph.clear();
    };
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<!-- brain:generated:start") {
            generated = true;
        }
        if generated {
            generated = !trimmed.starts_with("<!-- brain:generated:end");
            continue;
        }
        if trimmed.starts_with("<!--") {
            continue;
        }
        if head {
            if trimmed.is_empty() {
                continue;
            }
            // The title line and the `Key: value` fields under it.
            let field = trimmed
                .split_once(':')
                .is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_alphanumeric()));
            if trimmed.starts_with("# ") || field {
                continue;
            }
            head = false;
        }
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut out);
        } else if let Some(heading) = trimmed.strip_prefix('#') {
            flush(&mut paragraph, &mut out);
            out.push(
                json!({"kind":"heading","text":inline(heading.trim_start_matches('#').trim())}),
            );
        } else if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush(&mut paragraph, &mut out);
            out.push(json!({"kind":"bullet","text":inline(item.trim())}));
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(trimmed);
        }
    }
    flush(&mut paragraph, &mut out);
    out
}

fn inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("]]") else {
            rest = &rest[start..];
            break;
        };
        let link = &rest[start + 2..start + end];
        out.push_str(link.rsplit_once('|').map_or(link, |(_, alias)| alias));
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    let mut out = out.replace("**", "").replace('`', "");
    // `[text](url)` keeps the text.
    while let Some(start) = out.find("](") {
        let Some(open) = out[..start].rfind('[') else {
            break;
        };
        let Some(close) = out[start..].find(')') else {
            break;
        };
        let label = out[open + 1..start].to_owned();
        out.replace_range(open..start + close + 1, &label);
    }
    out
}

/// Store changes that open a note in the reader, remembering where to go back to.
fn open_changes(stores: &Stores, id: &str, path: &str) -> BTreeMap<String, Value> {
    let get = |name: &str| match stores.get(name) {
        Some(Value::Text(t)) => t.clone(),
        _ => String::new(),
    };
    let mut history = match stores.get("brain.history") {
        Some(Value::List(items)) => items.to_vec(),
        _ => Vec::new(),
    };
    history.push(json!({"screen":get("brain.screen"),"note":get("brain.note"),"path":get("brain.note_path"),"scope":get("brain.graph_scope")}));
    history.truncate(64);
    BTreeMap::from([
        ("brain.history".into(), Value::list(history)),
        ("brain.screen".into(), Value::Text("note".into())),
        ("brain.note".into(), Value::Text(id.into())),
        ("brain.note_path".into(), Value::Text(path.into())),
    ])
}

impl Brain {
    pub fn new(socket: Option<PathBuf>, helper: PathBuf) -> Self {
        Self {
            socket,
            helper,
            state: Arc::new(Mutex::new(State {
                status: "connecting".into(),
                ..Default::default()
            })),
            link: Default::default(),
            wake: Default::default(),
            started: Cell::new(false),
            width: Cell::new(1280),
            seen_open: Default::default(),
            seen_query: Default::default(),
            seen_scope: Default::default(),
            fit: Cell::new(0),
        }
    }
    pub fn set_viewport(&self, width: u32) {
        self.width.set(width);
    }
    pub fn subscribe(&self, wake: Wake) {
        *self.wake.lock().unwrap_or_else(|e| e.into_inner()) = Some(wake);
    }
    fn wake(wake: &Mutex<Option<Wake>>) {
        if let Some(wake) = wake.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            wake();
        }
    }

    /// Connect (starting the worker when its socket is absent), read replies and pushed events,
    /// and reconnect with a bounded backoff whenever the worker goes away. The first call
    /// starts the one socket thread; a Retry press only clears the caches.
    fn start(&self) {
        if self.started.replace(true) {
            return;
        }
        let Some(socket) = self.socket.clone() else {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.status = "error".into();
            state.error = crate::i18n::tr("bead.brain.error-runtime", &[]);
            return;
        };
        let helper = self.helper.clone();
        let (state, link, wake) = (self.state.clone(), self.link.clone(), self.wake.clone());
        let _ = std::thread::Builder::new()
            .name("nacre-brain-socket".into())
            .spawn(move || {
                let mut failures = 0u32;
                // The window's `Brain` holds the other `state` handle: stop once it is dropped.
                while Arc::strong_count(&state) > 1 {
                    if failures > 0 {
                        std::thread::sleep(Duration::from_secs(1 << failures.min(4)));
                    }
                    let Some((stream, reader)) = Self::connect(&socket, &helper) else {
                        failures += 1;
                        let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                        st.status = "error".into();
                        st.error = crate::i18n::tr("bead.brain.error-start", &[]);
                        drop(st);
                        Self::wake(&wake);
                        continue;
                    };
                    failures = 0;
                    *link.lock().unwrap_or_else(|e| e.into_inner()) = Some(Link {
                        writer: stream,
                        next: 1,
                    });
                    {
                        // A new worker knows none of the old requests: refetch everything.
                        let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                        st.fetched.clear();
                        st.pending.clear();
                        st.search_inflight = false;
                        st.search_wanted = None;
                        st.status = "ready".into();
                        st.error.clear();
                    }
                    Self::wake(&wake);
                    let mut lines = BufReader::new(reader).take(8 << 20);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match lines.read_line(&mut line) {
                            Ok(n) if n > 0 && line.ends_with('\n') => lines.set_limit(8 << 20),
                            _ => break,
                        }
                        let Ok(message) = serde_json::from_str::<Json>(&line) else {
                            continue;
                        };
                        let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                        if message["event"] == "revision" {
                            st.fetched.clear();
                        } else if let Some(id) = message["id"].as_u64()
                            && let Some(key) = st.pending.remove(&id)
                        {
                            Self::store(&mut st, &key, &message);
                            if key.starts_with("search:") {
                                st.search_inflight = false;
                                if let Some((key, params)) = st.search_wanted.take() {
                                    let mut link = link.lock().unwrap_or_else(|e| e.into_inner());
                                    Self::send_locked(&mut link, &mut st, "search", params, &key);
                                }
                            }
                        }
                        drop(st);
                        Self::wake(&wake);
                    }
                    // Keep showing the last data while reconnecting; only a failed reconnect errors.
                    *link.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .pending
                        .clear();
                    Self::wake(&wake);
                }
            });
    }

    /// One connection attempt run: up to ten seconds, starting the on-demand worker (the same
    /// one `ensure_server` starts; it idles out by itself) when the first try finds no socket.
    fn connect(socket: &Path, helper: &Path) -> Option<(UnixStream, UnixStream)> {
        for attempt in 0..100 {
            match UnixStream::connect(socket) {
                Ok(s) => return s.try_clone().ok().map(|r| (s, r)),
                Err(_) if attempt == 0 => {
                    if let Ok(mut child) = bead::services::process::command(helper)
                        .args(["brain-control", "serve"])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .process_group(0)
                        .spawn()
                    {
                        std::thread::spawn(move || child.wait());
                    }
                }
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    fn store(st: &mut State, key: &str, message: &Json) {
        let (op, key) = key.split_once(':').unwrap_or((key, ""));
        let body = message["body"].clone();
        let ok = message["status"].as_u64().is_some_and(|s| s < 300);
        match op {
            "brain" => st.brain = body,
            "loops" => st.loops = body,
            "wiki.page" => st.page = Some((key.into(), body)),
            "note" => st.note = Some((key.into(), body)),
            "graph"
                if ["all", "world:", "area:"]
                    .iter()
                    .any(|k| key.starts_with(k)) =>
            {
                st.graphs
                    .insert(key.into(), GraphView::from_reply(&body, key));
            }
            "graph" => st.near = Some((key.into(), body)),
            "search" => st.search = Some((key.into(), body)),
            "ask.settings" => st.ask["enabled"] = body["enabled"].clone(),
            "ask" if key == "question" => {
                st.ask["busy"] = json!(false);
                if ok {
                    st.ask["answer"] = body["answer"].clone();
                    st.ask["citations"] = body["citations"].clone();
                    st.ask["error"] = json!("");
                } else {
                    st.ask["answer"] = json!("");
                    st.ask["citations"] = json!([]);
                    st.ask["error"] = json!(text(&body["error"]));
                }
            }
            "ask" => st.ask["enabled"] = body["enabled"].clone(),
            _ => {}
        }
    }

    fn send_locked(link: &mut Option<Link>, st: &mut State, op: &str, params: Json, key: &str) {
        let Some(link) = link.as_mut() else {
            return;
        };
        let id = link.next;
        link.next += 1;
        let call = json!({"id":id,"op":op,"params":params});
        if writeln!(link.writer, "{call}").is_ok() {
            let tag = format!("{op}:{key}");
            st.fetched.insert(tag.clone());
            st.pending.insert(id, tag);
            if op == "search" {
                st.search_inflight = true;
            }
        }
    }

    /// Request `op` unless it has been fetched (or asked for) since the last revision.
    fn fetch(&self, st: &mut State, op: &str, params: Json, key: &str) {
        if st.fetched.contains(&format!("{op}:{key}")) {
            return;
        }
        if op == "search" && st.search_inflight {
            st.search_wanted = Some((key.into(), params));
            return;
        }
        let mut link = self.link.lock().unwrap_or_else(|e| e.into_inner());
        Self::send_locked(&mut link, st, op, params, key);
    }

    #[cfg(test)]
    pub(crate) fn with_replies(self, replies: BTreeMap<&str, Json>) -> Self {
        {
            let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
            st.status = "ready".into();
            for (key, body) in replies {
                st.fetched.insert(key.into());
                Self::store(&mut st, key, &json!({"status":200,"body":body}));
            }
        }
        self.started.set(true);
        self
    }

    // ---- projection ------------------------------------------------------------------

    pub fn project(&self, stores: &Stores) -> BTreeMap<String, Value> {
        self.start();
        let get = |name: &str| match stores.get(name) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let mut changes = BTreeMap::new();
        let open = get("brain.open");
        let (mut screen, mut note, mut path) = (
            get("brain.screen"),
            get("brain.note"),
            get("brain.note_path"),
        );
        let pick = get("brain.pick");
        let mut scope = get("brain.graph_scope");
        if !pick.is_empty() {
            // A graph node was selected: open its note; a world or area drills into its scope.
            changes.insert("brain.pick".into(), Value::Text(String::new()));
            let target = pick.parse::<usize>().ok().and_then(|i| {
                let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
                st.graphs
                    .get(graph_key(&scope))
                    .and_then(|g| g.meta.get(i))
                    .cloned()
            });
            match target {
                Some((id, p, true, _)) => {
                    changes.extend(open_changes(stores, &id, &p));
                    changes.insert("brain.scroll".into(), Value::Number(0.));
                    (screen, note, path) = ("note".into(), id, p);
                }
                Some((.., drill)) if !drill.is_empty() => scope = drill,
                _ => {}
            }
        }
        if !open.is_empty() && open != *self.seen_open.borrow() {
            // A deep link (`nacrectl ui nacre brain <id-or-path>`) opens the reader.
            self.seen_open.replace(open.clone());
            if open.starts_with("note:") {
                (note, path) = (open, String::new());
            } else {
                (note, path) = (String::new(), open);
            }
            screen = "note".into();
            changes.insert("brain.screen".into(), Value::Text(screen.clone()));
            changes.insert("brain.note".into(), Value::Text(note.clone()));
            changes.insert("brain.note_path".into(), Value::Text(path.clone()));
        }
        if !SCREENS.contains(&screen.as_str()) {
            screen = "overview".into();
            changes.insert("brain.screen".into(), Value::Text(screen.clone()));
        }
        if scope != *self.seen_scope.borrow() {
            self.seen_scope.replace(scope.clone());
            self.fit.set(self.fit.get() + 1);
        }
        let (query, world) = (get("brain.query"), get("brain.world"));
        if query != *self.seen_query.borrow() {
            // Typing in the toolbar from any screen opens the results (`/`, Ctrl K parity).
            self.seen_query.replace(query.clone());
            if !query.trim().is_empty() && screen != "search" {
                screen = "search".into();
                changes.insert("brain.screen".into(), Value::Text(screen.clone()));
            }
        }
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.status == "ready" {
            self.fetch(&mut st, "brain", json!({}), "");
            self.fetch(&mut st, "loops", json!({}), "");
            self.fetch(&mut st, "ask", json!({}), "");
            if screen == "graph" {
                let key = graph_key(&scope);
                self.fetch(&mut st, "graph", json!({"scope":key}), key);
            }
            if screen == "note" {
                if !note.is_empty() {
                    self.fetch(&mut st, "wiki.page", json!({"id":note}), &note);
                    self.fetch(
                        &mut st,
                        "graph",
                        json!({"scope":format!("note:{note}")}),
                        &note,
                    );
                }
                if !path.is_empty() {
                    self.fetch(&mut st, "note", json!({"path":path}), &path);
                }
            }
            if screen == "search" && (!query.trim().is_empty() || !world.is_empty()) {
                let key = format!("{query}\u{1}{world}");
                let mut params = json!({"q":query,"limit":SEARCH_LIMIT});
                if !world.is_empty() {
                    params["world"] = json!(world);
                }
                self.fetch(&mut st, "search", params, &key);
            }
        }
        let worlds = st.brain["worlds"].as_array().cloned().unwrap_or_default();
        let world_of = |note: &Json| {
            note["worlds"]
                .as_array()
                .and_then(|w| w.first())
                .map(|w| text(w).to_owned())
                .unwrap_or_default()
        };
        let world_index = |name: &str| {
            worlds
                .iter()
                .position(|w| text(&w["label"]) == name)
                .map_or(0, |i| i + 1)
        };
        let summary = |n: &Json| {
            json!({"id":text(&n["id"]),"label":text(&n["label"]),"path":text(&n["path"]),"date":text(&n["date"]),
                "world":world_of(n),"world_index":world_index(&world_of(n))})
        };
        let mut notes = st.brain["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|n| ["note", "wiki"].contains(&text(&n["kind"])))
            .collect::<Vec<_>>();
        // Newest first, as the web app's "Recent knowledge" orders them.
        notes.sort_by(|a, b| {
            recency(b)
                .cmp(recency(a))
                .then_with(|| text(&a["label"]).cmp(text(&b["label"])))
        });
        let loops = st.loops["loops"].as_array().cloned().unwrap_or_default();
        let counts = json!({
            "notes": st.brain["noteCount"].as_u64().unwrap_or(0),
            "worlds": worlds.len(),
            "review": st.brain["reviewCount"].as_u64().unwrap_or(0),
            "loops": loops.len(),
        });
        // Each world's share of the notes filed in a world, so the Overview bars add up to 100%.
        let filed: u64 = worlds
            .iter()
            .map(|w| w["count"].as_u64().unwrap_or(0))
            .sum();
        let world_rows = worlds
            .iter()
            .enumerate()
            .map(|(i, w)| {
                json!({"id":text(&w["id"]),"label":text(&w["label"]),"count":w["count"].as_u64().unwrap_or(0),"index":i+1,
                    "share":w["count"].as_u64().unwrap_or(0) as f64 / filed.max(1) as f64,
                    "selected":world == text(&w["label"]),
                    "areas":w["areas"].as_array().into_iter().flatten()
                        .map(|a| json!({"id":text(&a["id"]),"label":text(&a["label"]),"count":a["count"].as_u64().unwrap_or(0)})).collect::<Vec<_>>()})
            })
            .collect::<Vec<_>>();
        let loop_rows = loops
            .iter()
            .map(|l| {
                let world = l["worlds"]
                    .as_array()
                    .and_then(|w| w.first())
                    .map(|w| text(w).to_owned())
                    .unwrap_or_default();
                let path = text(&l["path"]);
                // Without a helper title use the note's own (as Recent knowledge does), then the file stem.
                let known = notes
                    .iter()
                    .find(|n| text(&n["id"]) == text(&l["source"]) || (!path.is_empty() && text(&n["path"]) == path))
                    .map(|n| text(&n["label"]));
                let title = Some(text(&l["title"]))
                    .filter(|t| !t.is_empty())
                    .or(known.filter(|t| !t.is_empty()))
                    .unwrap_or_else(|| path.rsplit('/').next().unwrap_or_default().trim_end_matches(".md"));
                json!({"id":text(&l["id"]),"title":title,"text":text(&l["text"]),"date":text(&l["date"]),"source":text(&l["source"]),"path":text(&l["path"]),
                    "world":world,"world_index":world_index(&world)})
            })
            .collect::<Vec<_>>();
        let (results, result_count, did_you_mean) = match &st.search {
            Some((key, body)) if *key == format!("{query}\u{1}{world}") => (
                body["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| {
                        let w = world_of(r);
                        json!({"id":text(&r["id"]),"label":text(&r["title"]),"path":text(&r["path"]),"snippet":text(&r["snippet"]),
                            "date":text(&r["date"]),"world":w,"world_index":world_index(&w),"archived":r["archived"] == true})
                    })
                    .collect::<Vec<_>>(),
                body["count"].as_u64().unwrap_or(0),
                text(&body["didYouMean"]).to_owned(),
            ),
            _ => (Vec::new(), 0, String::new()),
        };
        let page = st
            .page
            .as_ref()
            .filter(|(id, _)| *id == note)
            .map(|(_, p)| p.clone())
            .unwrap_or(Json::Null);
        let raw = st
            .note
            .as_ref()
            .filter(|(p, _)| *p == path)
            .map(|(_, n)| text(&n["text"]).to_owned())
            .unwrap_or_default();
        let near = st
            .near
            .as_ref()
            .filter(|(id, _)| *id == note)
            .map(|(_, g)| g.clone())
            .unwrap_or(Json::Null);
        let near_nodes: BTreeMap<String, &Json> = near["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|n| (text(&n["id"]).to_owned(), n))
            .collect();
        let (mut backlinks, mut links_out) = (Vec::new(), Vec::new());
        for edge in near["links"].as_array().into_iter().flatten() {
            if ["membership", "group"].contains(&text(&edge["kind"])) {
                continue;
            }
            let (source, target) = edge_ends(edge);
            if target == note
                && let Some(n) = near_nodes.get(&source)
            {
                backlinks.push(summary(n));
            } else if source == note
                && let Some(n) = near_nodes.get(&target)
            {
                links_out.push(summary(n));
            }
        }
        for list in [&mut backlinks, &mut links_out] {
            list.sort_by(|a, b| text(&a["label"]).cmp(text(&b["label"])));
            list.dedup_by(|a, b| a["id"] == b["id"]);
        }
        let title = if page.is_null() {
            raw.lines()
                .find_map(|l| l.strip_prefix("# "))
                .unwrap_or(
                    path.rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .trim_end_matches(".md"),
                )
                .to_owned()
        } else {
            text(&page["title"]).to_owned()
        };
        let body_blocks =
            if text(&page["body"]).trim().is_empty() && text(&page["intro"]).trim().is_empty() {
                blocks(&raw)
            } else {
                let mut all = blocks(text(&page["intro"]));
                all.extend(blocks(text(&page["body"])));
                all
            };
        let world_name = text(&page["world"]["label"]).to_owned();
        let page_row = json!({
            "id": note, "title": title, "path": if path.is_empty() { text(&page["page"]["path"]).to_owned() } else { path.clone() },
            "world": world_name, "world_index": world_index(&world_name), "area": text(&page["area"]["label"]),
            "state": if page.is_null() && raw.is_empty() { "loading" } else { "ready" },
            "facts": page["facts"].as_array().into_iter().flatten()
                .map(|f| json!({"text":text(&f["text"]),"stale":!f["stale"].is_null(),"superseded":!f["superseded"].is_null()})).collect::<Vec<_>>(),
            "blocks": body_blocks,
            "backlinks": backlinks, "links_out": links_out,
            "near_state": text(&near["state"]),
        });
        let graph = st
            .graphs
            .get(graph_key(&scope))
            .filter(|_| screen == "graph");
        changes.extend(BTreeMap::from([
            ("brain.status".into(), Value::Text(st.status.clone())),
            ("brain.error".into(), Value::Text(st.error.clone())),
            ("brain.counts".into(), Value::list(vec![counts])),
            ("brain.worlds".into(), Value::list(world_rows)),
            (
                "brain.recent".into(),
                Value::list(notes.iter().take(RECENT).map(|n| summary(n)).collect()),
            ),
            ("brain.loops".into(), Value::list(loop_rows)),
            ("brain.results".into(), Value::list(results)),
            (
                "brain.result_count".into(),
                Value::Number(result_count as f64),
            ),
            ("brain.did_you_mean".into(), Value::Text(did_you_mean)),
            ("brain.page".into(), Value::list(vec![page_row])),
            (
                "brain.ask.enabled".into(),
                Value::Bool(st.ask["enabled"] == true),
            ),
            ("brain.ask.busy".into(), Value::Bool(st.ask["busy"] == true)),
            (
                "brain.ask.answer".into(),
                Value::Text(text(&st.ask["answer"]).into()),
            ),
            (
                "brain.ask.error".into(),
                Value::Text(text(&st.ask["error"]).into()),
            ),
            (
                "brain.ask.citations".into(),
                Value::list(
                    st.ask["citations"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                        .map(|(i, c)| json!({"id":format!("c{i}"),"text":text(c)}))
                        .collect(),
                ),
            ),
            // Only the Graph screen carries the (large) graph.
            ("brain.graph_scope".into(), Value::Text(scope.clone())),
            (
                "brain.graph_fit".into(),
                Value::Number(f64::from(self.fit.get())),
            ),
            (
                "brain.graph_state".into(),
                Value::Text(graph.map(|g| g.state.clone()).unwrap_or_default()),
            ),
            (
                "brain.graph_nodes".into(),
                Value::list(
                    graph
                        .map(|g| {
                            let mut chunks = g.nodes.clone();
                            for node in chunks
                                .iter_mut()
                                .flat_map(|c| c["rows"].as_array_mut())
                                .flatten()
                            {
                                let group = world_index(text(&node["world"]));
                                node["group"] = json!(group);
                                node.as_object_mut().map(|o| o.remove("world"));
                            }
                            chunks
                        })
                        .unwrap_or_default(),
                ),
            ),
            (
                "brain.graph_edges".into(),
                Value::list(graph.map(|g| g.edges.clone()).unwrap_or_default()),
            ),
            (
                "brain.win_w".into(),
                Value::Number(f64::from(self.width.get())),
            ),
        ]));
        changes
    }

    // ---- actions ---------------------------------------------------------------------

    /// One host action `brain.do`, argument `op[:payload]`. Returns store changes.
    pub fn act(&self, arg: &str, stores: &Stores) -> BTreeMap<String, Value> {
        let (op, payload) = arg.split_once(':').unwrap_or((arg, ""));
        let get = |name: &str| match stores.get(name) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        };
        let history = || match stores.get("brain.history") {
            Some(Value::List(items)) => items.to_vec(),
            _ => Vec::new(),
        };
        let mut changes = BTreeMap::new();
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match op {
            "screen" if SCREENS.contains(&payload) => {
                changes.insert("brain.screen".into(), Value::Text(payload.into()));
            }
            "scope" => {
                changes.insert("brain.graph_scope".into(), Value::Text(payload.into()));
            }
            "open" => {
                let (id, path) = payload.split_once('|').unwrap_or((payload, ""));
                changes.extend(open_changes(stores, id, path));
            }
            "back" => {
                let mut history = history();
                if let Some(last) = history.pop() {
                    changes.insert(
                        "brain.screen".into(),
                        Value::Text(text(&last["screen"]).into()),
                    );
                    changes.insert("brain.note".into(), Value::Text(text(&last["note"]).into()));
                    changes.insert(
                        "brain.note_path".into(),
                        Value::Text(text(&last["path"]).into()),
                    );
                    changes.insert(
                        "brain.graph_scope".into(),
                        Value::Text(text(&last["scope"]).into()),
                    );
                    changes.insert("brain.history".into(), Value::list(history));
                }
            }
            "world" => {
                // Activating the selected world keeps it; only "All worlds" (empty payload) clears.
                changes.insert("brain.world".into(), Value::Text(payload.into()));
                changes.insert("brain.screen".into(), Value::Text("search".into()));
            }
            "ask" => {
                let question = get("brain.ask.text");
                let question = question.trim();
                if !question.is_empty() && st.ask["busy"] != true {
                    st.ask["busy"] = json!(true);
                    st.fetched.remove("ask:question");
                    self.fetch(&mut st, "ask", json!({"question":question}), "question");
                }
            }
            "ask.toggle" => {
                let enabled = st.ask["enabled"] != true;
                st.fetched.remove("ask.settings:");
                self.fetch(&mut st, "ask.settings", json!({"enabled":enabled}), "");
            }
            "action" if ["capture", "new", "tasks", "resume", "open-note"].contains(&payload) => {
                let value = if payload == "open-note" {
                    get("brain.note_path")
                } else {
                    String::new()
                };
                st.fetched.remove(&format!("action:{payload}"));
                self.fetch(
                    &mut st,
                    "action",
                    json!({"name":payload,"value":value}),
                    payload,
                );
            }
            "refresh" => {
                // `summary?retry=1` reschedules the graph build; the next frame refetches.
                // A lost connection is retried by the socket thread; Retry just shows "connecting".
                st.fetched.clear();
                if self
                    .link
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .is_none()
                {
                    if self.socket.is_some() {
                        st.status = "connecting".into();
                        st.error.clear();
                    }
                } else {
                    self.fetch(&mut st, "summary", json!({"retry":1}), "");
                }
            }
            _ => {}
        }
        // A new screen or note starts at the top.
        if changes.contains_key("brain.screen") {
            changes.insert("brain.scroll".into(), Value::Number(0.));
        }
        changes
    }
}

// ---- window --------------------------------------------------------------------------------------

pub fn socket_path() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    dir.is_absolute().then(|| dir.join("nacre/brain.sock"))
}

pub fn window_spec(
    root: PathBuf,
    palette: PathBuf,
    colors: bead::render::Palette,
    trace: bool,
) -> Result<bead::wayland::WindowSpec, Box<dyn std::error::Error>> {
    let behavior = root.join("brain/behavior.luau");
    let schema = bead::ui::script::Script::load(&std::fs::read(&behavior)?, &behavior)?.schema()?;
    let number = |key: &str| -> Result<u32, Box<dyn std::error::Error>> {
        match schema.get(key) {
            Some(Value::Number(value)) if value.is_finite() && *value > 0. && *value <= 4096. => {
                Ok(*value as u32)
            }
            _ => Err(format!("invalid Brain window dimension: {key}").into()),
        }
    };
    let state_path = crate::state::shell_dir()
        .ok()
        .map(|state| state.join("brain-window.json"));
    let min_size = (number("window.min_width")?, number("window.min_height")?);
    let size = state_path
        .as_deref()
        .and_then(crate::widgets::restored_window_size)
        .map(|(w, h)| (w.max(min_size.0), h.max(min_size.1)))
        .unwrap_or((number("window.width")?, number("window.height")?));
    let mut fallback = crate::sound::fallback(colors.get("surface")?);
    fallback.motion = None;
    fallback.primitives.clear();
    fallback.offset = (0., 0.);
    fallback.width = size.0 as f32;
    fallback.bounds = bead::render::Rect::new(0., 0., size.0 as f32, size.1 as f32);
    let cold = fallback.clone();
    let mut spec = bead::wayland::WindowSpec::new(
        "nacre-brain",
        crate::i18n::tr("bead.brain.title", &[]),
        size,
        fallback,
        Arc::new(move |_| {
            let mut session = bead::ui::Session::new(
                root.clone(),
                "brain/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold.clone(),
            );
            session.set_viewport(size.0, size.1);
            crate::reload::Runtime::new(session)
                .with_trace(trace)
                .with_brain(Brain::new(socket_path(), crate::prompt::helper_binary()))
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    );
    spec.min_size = min_size;
    spec.on_resize =
        state_path.map(|path| crate::widgets::window_size_saver(path, "nacre-brain-size-save"));
    spec.model = Arc::new(|args| {
        args.first()
            .and_then(Json::as_str)
            .filter(|s| !s.is_empty())
            .map(|note| BTreeMap::from([("brain.open".into(), Value::Text(note.into()))]))
            .unwrap_or_default()
    });
    Ok(spec)
}

#[cfg(test)]
#[path = "brain_tests.rs"]
mod tests;
