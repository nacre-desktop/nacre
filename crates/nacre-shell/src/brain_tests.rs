// SPDX-License-Identifier: GPL-3.0-only
//! N0217 Brain app: view models per screen over a synthetic fixture (never the owner's vault),
//! the adapter over a fake worker socket, the reader's markdown-lite, the layout store and
//! the screen goldens.
use super::*;
use bead_test::{Config as HarnessConfig, Harness};
use std::path::Path;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
/// A fresh private directory under the system temp dir.
fn private_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("n0217-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---- synthetic fixture: 3 worlds, 5 areas, 10 notes, 3 loops -------------------------------------

const WORLDS: [(&str, &[&str]); 3] = [
    ("Cooking", &["Bread", "Fermentation"]),
    ("Health", &["Sleep"]),
    ("Tools", &["Rust", "Shell"]),
];
const TITLES: [&str; 10] = [
    "Sourdough starter hydration",
    "Autolyse timing for whole wheat",
    "Kimchi salt ratio",
    "Sleep debt and caffeine half-life",
    "Rust ownership rules",
    "Shell startup order",
    "Baking stone preheat",
    "Lacto-fermented hot sauce",
    "Melatonin timing",
    "Cargo workspace layout",
];

fn note_id(i: usize) -> String {
    format!("note:{i:016x}")
}
fn note_node(i: usize) -> Json {
    let (world, areas) = WORLDS[i % 3];
    json!({"id":note_id(i),"label":TITLES[i],"kind":"note","path":format!("{}/{}.md",world.to_lowercase(),TITLES[i].to_lowercase().replace(' ', "-")),
        "date":format!("2026-11-{:02}",12-i),"worlds":[world],"areas":[areas[i % areas.len()]],"new":i<3})
}
fn brain_summary() -> Json {
    let worlds = WORLDS
        .iter()
        .enumerate()
        .map(|(i, (world, areas))| {
            let count = [4, 3, 3][i];
            json!({"id":world.to_lowercase(),"label":world,"count":count,
                "areas":areas.iter().enumerate().map(|(j,a)| json!({"id":a.to_lowercase(),"label":a,"count":2-j%2})).collect::<Vec<_>>()})
        })
        .collect::<Vec<_>>();
    let mut nodes = WORLDS
        .iter()
        .map(|(world, _)| json!({"id":world.to_lowercase(),"label":world,"kind":"world"}))
        .collect::<Vec<_>>();
    nodes.extend((0..10).map(note_node));
    json!({"nodes":nodes,"links":[],"noteCount":10,"worlds":worlds,"reviewCount":2})
}
fn loops() -> Json {
    json!({"loops":[
        {"id":"loop-1","text":"Try a 78% hydration bake and compare the crumb","date":"2026-11-10","source":note_id(0),"path":note_node(0)["path"],"worlds":["Cooking"]},
        {"id":"loop-2","text":"Waiting for the sleep tracker export","date":"2026-11-08","source":note_id(3),"path":note_node(3)["path"],"worlds":["Health"]},
        {"id":"loop-3","text":"Which crate owns the shell startup order?","date":"2026-11-07","source":note_id(5),"path":note_node(5)["path"],"worlds":["Tools"]},
    ],"timeline":[]})
}
fn page() -> Json {
    json!({"id":note_id(0),"kind":"note","title":TITLES[0],"page":{"id":note_id(0),"path":note_node(0)["path"],"title":TITLES[0]},
        "world":{"id":"cooking","label":"Cooking"},"area":{"id":"bread","label":"Bread"},
        "intro":"A starter kept at 100% hydration doubles in about six hours at 24 °C.",
        "facts":[{"text":"Feed 1:1:1 twice a day at room temperature.","stale":null,"superseded":null},
                 {"text":"Peak rise comes at four hours in summer.","stale":{"days":40},"superseded":null},
                 {"text":"Use 80% hydration.","stale":null,"superseded":{"date":"2026-10-01","by":[]}}],
        "body":"## Notes\n\nSee [[kimchi-salt-ratio|Kimchi salt ratio]] for the brine maths and **never** skip the autolyse.\n\n- Whole wheat drinks more water.\n- Rye ferments faster.\n<!-- brain:generated:start (x) -->\n## Evidence index\n- [[a]]\n<!-- brain:generated:end -->\n",
        "links":{},"state":"ready"})
}
fn near() -> Json {
    json!({"state":"ready","nodes":[note_node(0),note_node(1),note_node(2),note_node(6)],
        "links":[{"source":note_id(1),"target":note_id(0),"kind":"wiki"},{"source":note_id(6),"target":note_id(0),"kind":"mention"},
                 {"source":note_id(0),"target":note_id(2),"kind":"wiki"},{"source":"cooking","target":note_id(0),"kind":"membership"}]})
}
fn search(query: &str) -> Json {
    let results = [0usize, 6, 1]
        .iter()
        .enumerate()
        .map(|(rank, &i)| {
            let mut r = note_node(i);
            r["title"] = r["label"].clone();
            r["rank"] = json!(rank + 1);
            r["snippet"] = json!(format!("…the {query} temperature matters more than the flour; keep the dough at 26 °C and the starter lively…"));
            r["archived"] = json!(false);
            r
        })
        .collect::<Vec<_>>();
    json!({"query":query,"count":3,"results":results,"didYouMean":null})
}
fn fixture() -> BTreeMap<&'static str, Json> {
    BTreeMap::from([
        ("brain:", brain_summary()),
        ("loops:", loops()),
        ("ask:", json!({"enabled":true})),
        ("wiki.page:note:0000000000000000", page()),
        ("graph:note:0000000000000000", near()),
        ("search:bread\u{1}", search("bread")),
    ])
}
fn brain() -> Brain {
    Brain::new(None, PathBuf::from("/bin/false")).with_replies(fixture())
}
fn stores(values: &[(&str, Value)]) -> Stores {
    let mut map: BTreeMap<String, Value> = [
        ("brain.screen", "overview"),
        ("brain.query", ""),
        ("brain.world", ""),
        ("brain.note", ""),
        ("brain.note_path", ""),
        ("brain.open", ""),
        ("brain.ask.text", ""),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), Value::Text(v.into())))
    .collect();
    map.insert("brain.history".into(), Value::list(vec![]));
    map.extend(values.iter().map(|(k, v)| (k.to_string(), v.clone())));
    Stores::default().reconcile(&map).unwrap()
}
fn list(model: &BTreeMap<String, Value>, key: &str) -> Vec<Json> {
    match &model[key] {
        Value::List(items) => items.to_vec(),
        other => panic!("{key}: {other:?}"),
    }
}
fn text(model: &BTreeMap<String, Value>, key: &str) -> String {
    match &model[key] {
        Value::Text(t) => t.clone(),
        other => panic!("{key}: {other:?}"),
    }
}
fn t(s: &str) -> Value {
    Value::Text(s.into())
}

// ---- view models -----------------------------------------------------------------------------

#[test]
fn overview_projects_counts_worlds_and_recent_notes() {
    let model = brain().project(&stores(&[]));
    assert_eq!(text(&model, "brain.status"), "ready");
    let counts = &list(&model, "brain.counts")[0];
    assert_eq!(counts, &json!({"notes":10,"worlds":3,"review":2,"loops":3}));
    let worlds = list(&model, "brain.worlds");
    assert_eq!(worlds.len(), 3);
    assert_eq!(worlds[0]["label"], "Cooking");
    assert_eq!(worlds[0]["index"], 1);
    assert_eq!(worlds[0]["areas"][1]["label"], "Fermentation");
    let recent = list(&model, "brain.recent");
    assert_eq!(recent.len(), 8, "capped at {RECENT}");
    assert_eq!(recent[1]["world"], "Health");
    assert_eq!(recent[1]["world_index"], 2);
    assert_eq!(list(&model, "brain.loops")[0]["world_index"], 1);
    assert_eq!(model["brain.graph_available"], Value::Bool(false));
}

#[test]
fn search_projects_results_for_the_typed_query_only() {
    let brain = brain();
    let hit = brain.project(&stores(&[
        ("brain.screen", t("search")),
        ("brain.query", t("bread")),
    ]));
    let results = list(&hit, "brain.results");
    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["label"], TITLES[0]);
    assert_eq!(results[1]["world"], "Cooking");
    assert_eq!(hit["brain.result_count"], Value::Number(3.));
    let other = brain.project(&stores(&[
        ("brain.screen", t("search")),
        ("brain.query", t("rye")),
    ]));
    assert!(
        list(&other, "brain.results").is_empty(),
        "a stale reply is never shown"
    );
}

#[test]
fn typing_a_query_opens_the_search_screen() {
    let brain = brain();
    let model = brain.project(&stores(&[("brain.query", t("bre"))]));
    assert_eq!(text(&model, "brain.screen"), "search");
    let again = brain.project(&stores(&[
        ("brain.screen", t("overview")),
        ("brain.query", t("bre")),
    ]));
    assert!(
        !again.contains_key("brain.screen"),
        "only a change of query switches"
    );
}

#[test]
fn reader_projects_facts_blocks_and_neighbour_links() {
    let model = brain().project(&stores(&[
        ("brain.screen", t("note")),
        ("brain.note", t(&note_id(0))),
    ]));
    let page = &list(&model, "brain.page")[0];
    assert_eq!(page["title"], TITLES[0]);
    assert_eq!(
        (page["world"].as_str(), page["area"].as_str()),
        (Some("Cooking"), Some("Bread"))
    );
    assert_eq!(page["state"], "ready");
    assert_eq!(page["facts"].as_array().unwrap().len(), 3);
    assert_eq!(page["facts"][1]["stale"], true);
    assert_eq!(page["facts"][2]["superseded"], true);
    let blocks = page["blocks"].as_array().unwrap();
    assert_eq!(blocks[0]["kind"], "paragraph");
    assert_eq!(blocks[1], json!({"kind":"heading","text":"Notes"}));
    assert_eq!(
        blocks[2]["text"],
        "See Kimchi salt ratio for the brine maths and never skip the autolyse."
    );
    assert_eq!(
        blocks[3],
        json!({"kind":"bullet","text":"Whole wheat drinks more water."})
    );
    assert_eq!(blocks.len(), 5, "the generated block is skipped");
    let backlinks = page["backlinks"].as_array().unwrap();
    assert_eq!(
        backlinks
            .iter()
            .map(|n| n["label"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [TITLES[1], TITLES[6]]
    );
    assert_eq!(page["links_out"][0]["label"], TITLES[2]);
    assert_eq!(page["path"], note_node(0)["path"]);
}

#[test]
fn a_plain_note_reads_its_raw_text() {
    let brain = Brain::new(None, PathBuf::from("/bin/false")).with_replies(BTreeMap::from([
        ("brain:", brain_summary()),
        ("note:tools/rust-ownership-rules.md", json!({"text":"# Rust ownership rules\nKind: note\n\nOne owner at a time.\n\n- Borrows are shared or unique.\n"})),
    ]));
    let model = brain.project(&stores(&[
        ("brain.screen", t("note")),
        ("brain.note_path", t("tools/rust-ownership-rules.md")),
    ]));
    let page = &list(&model, "brain.page")[0];
    assert_eq!(page["title"], "Rust ownership rules");
    assert_eq!(
        page["blocks"],
        json!([{"kind":"paragraph","text":"One owner at a time."},{"kind":"bullet","text":"Borrows are shared or unique."}])
    );
    assert_eq!(page["state"], "ready");
}

#[test]
fn markdown_lite_strips_marks_and_links() {
    assert_eq!(
        blocks(
            "# T\nEntity: x\n\nSee [[a|A]] and [[b]] plus [c](http://c) `d` **e**.\nSecond line.\n\n### Deep\n* item"
        ),
        vec![
            json!({"kind":"paragraph","text":"See A and b plus c d e. Second line."}),
            json!({"kind":"heading","text":"Deep"}),
            json!({"kind":"bullet","text":"item"}),
        ]
    );
    assert!(blocks("").is_empty());
}

#[test]
fn deep_links_open_the_reader_by_id_or_path() {
    let brain = brain();
    let by_id = brain.project(&stores(&[("brain.open", t(&note_id(0)))]));
    assert_eq!(text(&by_id, "brain.screen"), "note");
    assert_eq!(text(&by_id, "brain.note"), note_id(0));
    assert_eq!(text(&by_id, "brain.note_path"), "");
    let by_path = brain.project(&stores(&[("brain.open", t("cooking/x.md"))]));
    assert_eq!(text(&by_path, "brain.note_path"), "cooking/x.md");
    assert_eq!(text(&by_path, "brain.note"), "");
    let same = brain.project(&stores(&[
        ("brain.open", t("cooking/x.md")),
        ("brain.screen", t("overview")),
    ]));
    assert!(
        !same.contains_key("brain.note"),
        "the same link is not reopened every frame"
    );
}

// ---- actions ---------------------------------------------------------------------------------

#[test]
fn open_and_back_keep_a_history() {
    let brain = brain();
    let opened = brain.act(
        &format!("open:{}|cooking/a.md", note_id(1)),
        &stores(&[("brain.screen", t("search"))]),
    );
    assert_eq!(text(&opened, "brain.screen"), "note");
    assert_eq!(text(&opened, "brain.note"), note_id(1));
    assert_eq!(text(&opened, "brain.note_path"), "cooking/a.md");
    let history = list(&opened, "brain.history");
    assert_eq!(
        history,
        vec![json!({"screen":"search","note":"","path":""})]
    );
    let back = brain.act(
        "back",
        &stores(&[
            ("brain.screen", t("note")),
            ("brain.history", Value::list(history)),
        ]),
    );
    assert_eq!(text(&back, "brain.screen"), "search");
    assert!(list(&back, "brain.history").is_empty());
    assert!(
        brain.act("back", &stores(&[])).is_empty(),
        "nothing to go back to"
    );
}

#[test]
fn world_filter_toggles_and_screens_are_validated() {
    let brain = brain();
    let on = brain.act("world:Health", &stores(&[]));
    assert_eq!(text(&on, "brain.world"), "Health");
    assert_eq!(text(&on, "brain.screen"), "search");
    let off = brain.act("world:Health", &stores(&[("brain.world", t("Health"))]));
    assert_eq!(text(&off, "brain.world"), "");
    assert_eq!(
        text(&brain.act("screen:loops", &stores(&[])), "brain.screen"),
        "loops"
    );
    assert!(
        brain.act("screen:graph", &stores(&[])).is_empty(),
        "no graph screen yet"
    );
    assert!(
        brain.act("action:rm", &stores(&[])).is_empty(),
        "only allow-listed actions"
    );
    let model = brain.project(&stores(&[("brain.screen", t("bogus"))]));
    assert_eq!(text(&model, "brain.screen"), "overview");
}

// ---- the adapter over a fake worker socket ----------------------------------------------------

/// A worker stand-in: answers ops from the fixture, pushes one `revision` event after the first
/// search, and records every request line.
fn fake_worker(socket: &Path) -> std::sync::mpsc::Receiver<Json> {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let reader = std::io::BufReader::new(stream);
        let mut pushed = false;
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let call: Json = serde_json::from_str(&line).unwrap();
            let op = call["op"].as_str().unwrap();
            let body = match op {
                "brain" => brain_summary(),
                "loops" => loops(),
                "ask" if call["params"]["question"].is_string() => {
                    json!({"answer":"Keep it at 100% hydration.","citations":[note_id(0)],"confidence":"high","followups":[]})
                }
                "ask" => json!({"enabled":false}),
                "ask.settings" => json!({"enabled":call["params"]["enabled"]}),
                "search" => search(call["params"]["q"].as_str().unwrap()),
                "wiki.page" => page(),
                "note" => json!({"text":"# Sourdough starter hydration\n\nRaw text.\n"}),
                "graph" => near(),
                "action" => json!({"ok":true}),
                other => panic!("unexpected op {other}"),
            };
            use std::io::Write as _;
            writeln!(
                writer,
                "{}",
                json!({"id":call["id"],"status":200,"body":body})
            )
            .unwrap();
            if op == "search" && !pushed {
                pushed = true;
                writeln!(writer, "{}", json!({"event":"revision","revision":"r2"})).unwrap();
            }
            tx.send(call).unwrap();
        }
    });
    rx
}

#[test]
fn the_adapter_talks_json_lines_over_the_worker_socket() {
    let socket = private_dir("socket").join("brain.sock");
    let requests = fake_worker(&socket);
    let brain = Brain::new(Some(socket), PathBuf::from("/bin/false"));
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    brain.subscribe(Arc::new(move || {
        let _ = wake_tx.send(());
    }));
    let wait = |brain: &Brain, stores: &Stores, done: &dyn Fn(&BTreeMap<String, Value>) -> bool| {
        for _ in 0..200 {
            let model = brain.project(stores);
            if done(&model) {
                return model;
            }
            let _ = wake_rx.recv_timeout(Duration::from_millis(100));
        }
        panic!("the adapter never settled");
    };
    let overview = wait(&brain, &stores(&[]), &|m| {
        text(m, "brain.status") == "ready"
            && !list(m, "brain.worlds").is_empty()
            && !list(m, "brain.loops").is_empty()
    });
    assert_eq!(list(&overview, "brain.worlds").len(), 3);
    assert_eq!(overview["brain.ask.enabled"], Value::Bool(false));
    let first = requests.recv().unwrap();
    assert_eq!(
        (first["id"].as_u64(), first["op"].as_str()),
        (Some(1), Some("brain"))
    );
    assert_eq!(requests.recv().unwrap()["op"], "loops");
    assert_eq!(requests.recv().unwrap()["op"], "ask");
    // Search: typed text drives the request; a revision event refetches the summary.
    let searching = stores(&[("brain.screen", t("search")), ("brain.query", t("bread"))]);
    let hits = wait(&brain, &searching, &|m| {
        !list(m, "brain.results").is_empty()
    });
    assert_eq!(list(&hits, "brain.results")[0]["label"], TITLES[0]);
    let search_call = requests.recv().unwrap();
    assert_eq!(
        search_call["params"],
        json!({"q":"bread","limit":SEARCH_LIMIT})
    );
    let refetched = (0..50).any(|_| {
        brain.project(&searching);
        requests
            .recv_timeout(Duration::from_millis(100))
            .is_ok_and(|c| c["op"] == "brain")
    });
    assert!(refetched, "a pushed revision refetches the summary");
    // Ask: the opt-in switch and a question.
    brain.act("ask.toggle", &searching);
    let enabled = wait(&brain, &searching, &|m| {
        m["brain.ask.enabled"] == Value::Bool(true)
    });
    assert_eq!(enabled["brain.ask.enabled"], Value::Bool(true));
    let asking = stores(&[
        ("brain.screen", t("search")),
        ("brain.query", t("bread")),
        ("brain.ask.text", t("How wet?")),
    ]);
    brain.act("ask", &asking);
    let answered = wait(&brain, &asking, &|m| {
        !text(m, "brain.ask.answer").is_empty()
    });
    assert_eq!(
        text(&answered, "brain.ask.answer"),
        "Keep it at 100% hydration."
    );
    assert_eq!(
        list(&answered, "brain.ask.citations")[0]["text"],
        note_id(0)
    );
    assert_eq!(answered["brain.ask.busy"], Value::Bool(false));
    // The reader fetches the page and the neighbourhood; an action posts its name.
    let reading = stores(&[
        ("brain.screen", t("note")),
        ("brain.note", t(&note_id(0))),
        ("brain.note_path", t("cooking/x.md")),
    ]);
    let page = wait(&brain, &reading, &|m| {
        list(m, "brain.page")[0]["state"] == "ready"
            && !list(m, "brain.page")[0]["backlinks"]
                .as_array()
                .unwrap()
                .is_empty()
    });
    assert_eq!(list(&page, "brain.page")[0]["title"], TITLES[0]);
    brain.act("action:open-note", &reading);
    let mut seen = Vec::new();
    while let Ok(call) = requests.recv_timeout(Duration::from_secs(5)) {
        seen.push(call.clone());
        if call["op"] == "action" {
            assert_eq!(
                call["params"],
                json!({"name":"open-note","value":"cooking/x.md"})
            );
            break;
        }
    }
    assert!(
        seen.iter()
            .any(|c| c["op"] == "wiki.page" && c["params"]["id"] == note_id(0))
    );
    assert!(
        seen.iter()
            .any(|c| c["op"] == "graph" && c["params"]["scope"] == format!("note:{}", note_id(0)))
    );
}

#[test]
fn a_missing_worker_reports_an_error_instead_of_hanging() {
    let brain = Brain::new(
        Some(private_dir("absent").join("absent.sock")),
        PathBuf::from("/bin/false"),
    );
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    brain.subscribe(Arc::new(move || {
        let _ = wake_tx.send(());
    }));
    let first = brain.project(&stores(&[]));
    assert_eq!(text(&first, "brain.status"), "connecting");
    let _ = wake_rx.recv_timeout(Duration::from_secs(20));
    let model = brain.project(&stores(&[]));
    assert_eq!(text(&model, "brain.status"), "error");
    assert!(text(&model, "brain.error").contains("did not start"));
}

// ---- layout store ----------------------------------------------------------------------------

#[test]
fn the_layout_store_round_trips_and_discards_a_stale_node_set() {
    let dir = private_dir("layout");
    let path = dir.join("brain/graph-layout.json");
    let layout = json!({"nodes":{"a":{"x":1,"y":2},"b":{"x":3,"y":4},"c":{"x":5,"y":6},"d":{"x":7,"y":8}},"pins":["a"]});
    save_layout(&path, &layout).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let ids = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    assert_eq!(
        load_layout(&path, &ids(&["a", "b", "c", "d"])),
        Some(layout.clone())
    );
    assert_eq!(
        load_layout(&path, &ids(&["a", "b", "c", "e"])),
        Some(layout),
        "one of four differs"
    );
    assert_eq!(
        load_layout(&path, &ids(&["a", "b", "x", "y"])),
        None,
        "half differs: relaid"
    );
    assert_eq!(load_layout(&dir.join("none.json"), &ids(&["a"])), None);
}

// ---- goldens ---------------------------------------------------------------------------------

fn harness(model: BTreeMap<String, Value>, width: u32, height: u32, scale: f32) -> Harness {
    Harness::load_with_icons(
        &root(),
        Path::new("brain/main.luau"),
        &root().join("palette.json"),
        model,
        HarnessConfig {
            width,
            height,
            scale,
        },
        BTreeMap::new(),
    )
    .unwrap()
}
fn golden(name: &str, h: &mut Harness) {
    let frame = h.pose((0., 0.), 1.).unwrap();
    let path = root().join(format!("tests/goldens/{name}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path).unwrap();
    }
    frame
        .compare(
            &path,
            Default::default(),
            &std::env::temp_dir().join(format!("bead-{name}-diff")),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
}

#[test]
fn brain_window_goldens() {
    crate::i18n::init();
    let brain = brain();
    for (name, view) in [
        ("brain-overview-1", vec![("brain.screen", t("overview"))]),
        (
            "brain-search-1",
            vec![
                ("brain.screen", t("search")),
                ("brain.query", t("bread")),
                ("brain.ask.text", t("How wet should the starter be?")),
            ],
        ),
        (
            "brain-note-1",
            vec![("brain.screen", t("note")), ("brain.note", t(&note_id(0)))],
        ),
        ("brain-loops-1", vec![("brain.screen", t("loops"))]),
    ] {
        let stores = stores(&view);
        let mut model = brain.project(&stores);
        for (key, value) in &view {
            model
                .entry((*key).to_string())
                .or_insert_with(|| value.clone());
        }
        let mut h = harness(model, 1280, 800, 1.);
        golden(name, &mut h);
    }
    let down = Brain::new(None, PathBuf::from("/bin/false"));
    let mut model = down.project(&stores(&[]));
    model.insert("brain.status".into(), t("error"));
    model.insert(
        "brain.error".into(),
        t(&crate::i18n::tr("bead.brain.error-start", &[])),
    );
    golden("brain-unavailable-1", &mut harness(model, 1280, 800, 1.));
}
