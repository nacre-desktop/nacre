//! The viewer's Wiki layer (N0144): world -> area -> entity pages read from
//! `wiki/`, each led by its "Current facts". Fact provenance, supersession and
//! render-time staleness are parsed here; the evidence index (latest notes,
//! digests, sub-pages, backlinks, related) comes from the connections of the
//! background graph index. Nothing here writes to the vault.
use crate::{
    brain_search::Categories,
    brain_web::{self, area_id, link_targets, slug_id, Resolver},
    Result,
};
use chrono::NaiveDate;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    sync::LazyLock,
};
use unicode_casefold::UnicodeCaseFold;

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn casefold(s: &str) -> String {
    s.case_fold().collect()
}
fn rx(s: &str) -> Regex {
    Regex::new(s).expect("constant wiki expression")
}

struct Page {
    id: String,
    path: String,
    title: String,
    text: String,
    meta: Value,
    entity: String,
    name: String,
    world: String,
    area: String,
    parent: String,
}

/// Wiki pages under `wiki/` (no symlinks, hidden files or redirects), placed
/// by their header: worlds/areas through the registry, then `Parent:`.
fn pages(root: &Path, categories: &Categories) -> Vec<Page> {
    let dir = root.join("wiki");
    if brain_web::reject_links(&dir).is_err() || !dir.is_dir() {
        return Vec::new();
    }
    let mut paths = walkdir::WalkDir::new(&dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "md"))
        .map(|e| e.into_path())
        .collect::<Vec<_>>();
    paths.sort();
    paths.truncate(2000);
    let mut out = Vec::new();
    for path in paths {
        let Ok(body) = brain_web::read(&path, brain_web::NOTE_LIMIT) else {
            continue;
        };
        let meta = brain_web::fields(&body);
        let entity = casefold(text(&meta["entity"]).trim());
        if entity == "alias" || !text(&meta["redirect"]).is_empty() {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
        let title = body
            .lines()
            .find_map(|l| l.strip_prefix("# "))
            .map(str::trim)
            .unwrap_or_else(|| path.file_stem().and_then(|v| v.to_str()).unwrap_or(""))
            .to_owned();
        let name = Some(text(&meta["name"]).trim())
            .filter(|v| !v.is_empty())
            .unwrap_or(title.as_str())
            .to_owned();
        let (mut worlds, mut areas) = categories.resolve(&meta);
        let parent = text(&meta["parent"]).trim().to_owned();
        if areas.is_empty() && worlds.is_empty() && !parent.is_empty() {
            (worlds, areas) = categories.resolve(&json!({ "worlds": parent }));
        }
        let mut parent_entity = String::new();
        if !parent.is_empty()
            && !worlds
                .iter()
                .chain(areas.iter())
                .any(|v| casefold(v) == casefold(&parent))
        {
            parent_entity = parent;
        }
        out.push(Page {
            id: brain_web::note_id(&path),
            path: rel.into_owned(),
            title,
            entity: if entity.is_empty() {
                "page".into()
            } else {
                entity
            },
            name,
            world: worlds.into_iter().next().unwrap_or_default(),
            area: areas.into_iter().next().unwrap_or_default(),
            parent: parent_entity,
            meta,
            text: body,
        });
    }
    // Pages under a parent entity inherit its placement.
    for _ in 0..4 {
        let placed: HashMap<String, (String, String)> = out
            .iter()
            .map(|p| (casefold(&p.name), (p.world.clone(), p.area.clone())))
            .collect();
        for p in out
            .iter_mut()
            .filter(|p| !p.parent.is_empty() && p.world.is_empty())
        {
            if let Some((world, area)) = placed.get(&casefold(&p.parent)) {
                (p.world, p.area) = (world.clone(), area.clone());
            }
        }
    }
    out
}
impl Page {
    /// A page that describes its registry world or area.
    fn hub(&self) -> bool {
        (self.entity == "world" && !self.world.is_empty())
            || (self.entity == "area" && !self.area.is_empty())
    }
}

/// A note body split into (intro, current-facts section, rest), without the
/// title, header or generated evidence block.
fn sections(body: &str) -> (String, Option<String>, String) {
    let prose = crate::brain_connections::prose(body);
    let prose = if body.starts_with("# ") {
        prose.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &prose
    };
    let (mut intro, mut facts, mut rest) = (String::new(), None::<String>, String::new());
    let mut stage = 0;
    for line in prose.lines() {
        let heading = line.trim_end();
        if let Some(name) = heading.strip_prefix("## ") {
            if stage == 0 && casefold(name.trim()) == "current facts" {
                stage = 1;
                facts = Some(String::new());
                continue;
            }
            if stage == 1 {
                stage = 2;
            }
        }
        let target = match stage {
            0 => &mut intro,
            1 => facts.as_mut().expect("facts section"),
            _ => &mut rest,
        };
        target.push_str(line);
        target.push('\n');
    }
    (
        intro.trim().to_owned(),
        facts.map(|f| f.trim_end().to_owned()),
        rest.trim().to_owned(),
    )
}

static DATE: LazyLock<Regex> = LazyLock::new(|| rx(r"\b(\d{4}-\d{2}-\d{2})\b"));
static ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)\b(?:\d{1,3}\.){3}\d{1,3}\b|\b(?:[0-9a-f]{1,4}:){3,7}[0-9a-f]{1,4}\b|\b(?:ip|ipv4|ipv6|address(?:es)?|hostname|port|mac|dns|online|offline|uptime|mounted|reachable|unreachable|is up|is down|free space|disk usage)\b",
    )
});
static VERSION: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)\br\d{3,}\b|\bv?\d+\.\d+\.\d+\b|\bv\d+(?:\.\d+)?\b|\b(?:version|revision|release|deployed|installed|pinned|firmware|kernel|commit|build)\b|\b[0-9a-f]*\d[0-9a-f]*[a-f][0-9a-f]*\b",
    )
});

/// Render-time staleness (DESIGN §7): addresses/IPs/host state after 14
/// days, versions/revisions/deploy state after 30; decisions and preferences
/// only through supersession. N0145's persisted flags take the same shape
/// with `source: "index"` and win over this rule.
pub(crate) fn staleness(fact: &str, date: &str, today: NaiveDate) -> Option<Value> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let (class, limit) = if ADDRESS.is_match(fact) {
        ("address", 14)
    } else if VERSION
        .find_iter(fact)
        .any(|m| !m.as_str().chars().all(|c| c.is_ascii_hexdigit()) || m.len() >= 7)
    {
        ("version", 30)
    } else {
        return None;
    };
    let days = (today - date).num_days();
    (days > limit).then(|| json!({"class":class,"days":days,"limit":limit,"source":"render"}))
}

/// Trailing parenthetical groups that contain a date, innermost-last.
fn trailing_groups(line: &str) -> (&str, Vec<&str>) {
    let mut rest = line.trim_end();
    let mut groups = Vec::new();
    while rest.ends_with(')') {
        let mut depth = 0;
        let Some(open) = rest.char_indices().rev().find_map(|(i, c)| {
            match c {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
            None
        }) else {
            break;
        };
        let inner = &rest[open + 1..rest.len() - 1];
        if !DATE.is_match(inner) {
            break;
        }
        groups.insert(0, inner);
        rest = rest[..open].trim_end();
    }
    (rest, groups)
}

/// Parse a "Current facts" section: bullets become facts with provenance
/// `(verb YYYY-MM-DD; [[source]])` and optional `(superseded YYYY-MM-DD by
/// [[note]])` (or `~~struck~~` text); other lines are returned as a note.
pub(crate) fn facts(section: &str, today: NaiveDate) -> (Vec<Value>, String) {
    static BULLET: LazyLock<Regex> = LazyLock::new(|| rx(r"^(\s*)(?:[-*+]|\d+\.)\s+(.*)$"));
    let (mut out, mut note) = (Vec::new(), String::new());
    for line in section.lines() {
        if let Some(heading) = line.strip_prefix("### ") {
            out.push(json!({"heading":heading.trim()}));
            continue;
        }
        let Some(bullet) = BULLET.captures(line) else {
            note.push_str(line);
            note.push('\n');
            continue;
        };
        let (mut body, groups) = trailing_groups(&bullet[2]);
        let mut struck = false;
        if let Some(inner) = body.strip_prefix("~~").and_then(|b| b.strip_suffix("~~")) {
            body = inner;
            struck = true;
        }
        let (mut provenance, mut superseded) = (Value::Null, Value::Null);
        for group in groups {
            let date = DATE
                .captures(group)
                .map(|c| c[1].to_owned())
                .unwrap_or_default();
            let sources = link_targets(group)
                .into_iter()
                .map(|(t, _)| t)
                .collect::<Vec<_>>();
            if casefold(group).contains("superseded") {
                superseded = json!({"date":date,"by":sources});
            } else {
                let verb = group
                    .split(|c: char| !c.is_alphabetic())
                    .find(|w| !w.is_empty())
                    .unwrap_or("")
                    .to_lowercase();
                provenance = json!({"verb":verb,"date":date,"sources":sources});
            }
        }
        if struck && superseded.is_null() {
            superseded = json!({"date":"","by":[]});
        }
        let stale = if superseded.is_null() {
            staleness(body, text(&provenance["date"]), today)
        } else {
            None
        };
        out.push(json!({
            "text": body.trim(),
            "indent": bullet[1].len() / 2,
            "provenance": provenance,
            "superseded": superseded,
            "stale": stale,
        }));
    }
    (out, note.trim().to_owned())
}

fn counts(list: &[Value]) -> Value {
    let facts = list.iter().filter(|f| f.get("text").is_some());
    let (mut current, mut superseded, mut stale) = (0, 0, 0);
    for f in facts {
        if f["superseded"].is_null() {
            current += 1;
        } else {
            superseded += 1;
        }
        stale += usize::from(!f["stale"].is_null());
    }
    json!({"current":current,"superseded":superseded,"stale":stale})
}

/// One-line plain summary: the intro's first paragraph, else the first fact.
fn plain(markdown: &str) -> String {
    static LINK: LazyLock<Regex> =
        LazyLock::new(|| rx(r"\[\[(?:[^]|\n]*\|)?([^]\n]+)\]\]|\[([^]]+)\]\([^)]*\)|[*_`]"));
    let text = LINK.replace_all(markdown, |c: &regex::Captures| {
        c.get(1)
            .or_else(|| c.get(2))
            .map_or(String::new(), |m| m.as_str().to_owned())
    });
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 220 {
        line.chars().take(219).collect::<String>() + "…"
    } else {
        line
    }
}

fn summary(page: &Page, today: NaiveDate) -> Value {
    let (intro, section, _) = sections(&page.text);
    let (list, _) = facts(section.as_deref().unwrap_or(""), today);
    let first = intro
        .split("\n\n")
        .find(|p| !p.trim().starts_with('#') && !p.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            list.iter()
                .find(|f| f["superseded"].is_null() && f.get("text").is_some())
                .map(|f| text(&f["text"]).to_owned())
        })
        .unwrap_or_default();
    json!({
        "id": page.id, "path": page.path, "title": page.title, "name": page.name,
        "entity": page.entity, "world": page.world, "area": page.area, "parent": page.parent,
        "status": page.meta["status"], "reviewed": page.meta["reviewed"],
        "summary": plain(&first), "facts": counts(&list),
    })
}

struct Model {
    registry: Value,
    pages: Vec<Page>,
}
impl Model {
    fn load(root: &Path) -> Result<Self> {
        let registry = brain_web::registry(root)?;
        let pages = pages(root, &Categories::new(&registry));
        Ok(Self { registry, pages })
    }
    fn worlds(&self) -> Vec<&str> {
        self.registry["worlds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|w| text(&w["merged_into"]).is_empty())
            .map(|w| text(&w["name"]))
            .collect()
    }
    fn areas(&self, world: &str) -> Vec<(&str, bool)> {
        self.registry["areas"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| text(&a["parent"]) == world && text(&a["merged_into"]).is_empty())
            .map(|a| (text(&a["name"]), text(&a["status"]) == "proposed"))
            .collect()
    }
    /// The page that describes a world or area (its `Entity:` names it).
    fn page_for(&self, kind: &str, name: &str) -> Option<&Page> {
        self.pages.iter().find(|p| {
            p.entity == kind && (if kind == "world" { &p.world } else { &p.area }) == name
        })
    }
    /// Entity pages placed directly in a world (no area), an area, or under
    /// a parent entity.
    fn children(&self, world: &str, area: &str, parent: &str) -> Vec<&Page> {
        self.pages
            .iter()
            .filter(|p| !p.hub())
            .filter(|p| {
                if !parent.is_empty() {
                    casefold(&p.parent) == casefold(parent)
                } else {
                    p.parent.is_empty()
                        && p.area == area
                        && (area.is_empty() && p.world == world || !area.is_empty())
                }
            })
            .collect()
    }
}

/// `/api/wiki`: registry worlds and areas with their pages and entity pages.
pub(crate) fn index(root: &Path) -> Result<Value> {
    let today = chrono::Utc::now().date_naive();
    let model = Model::load(root)?;
    let page = |p: Option<&Page>| p.map_or(Value::Null, |p| summary(p, today));
    let mut placed = BTreeSet::new();
    let mut worlds = Vec::new();
    for world in model.worlds() {
        let areas = model
            .areas(world)
            .into_iter()
            .map(|(area, proposed)| {
                let pages = model.children(world, area, "");
                placed.extend(pages.iter().map(|p| p.id.clone()));
                json!({"id":area_id(area),"label":area,"proposed":proposed,"page":page(model.page_for("area", area)),
                       "pages":pages.into_iter().map(|p| summary(p, today)).collect::<Vec<_>>()})
            })
            .collect::<Vec<_>>();
        let pages = model.children(world, "", "");
        placed.extend(pages.iter().map(|p| p.id.clone()));
        worlds.push(json!({"id":slug_id(world),"label":world,"page":page(model.page_for("world", world)),"areas":areas,
                           "pages":pages.into_iter().map(|p| summary(p, today)).collect::<Vec<_>>()}));
    }
    let other = model
        .pages
        .iter()
        .filter(|p| !p.hub() && p.parent.is_empty() && !placed.contains(&p.id))
        .map(|p| summary(p, today))
        .collect::<Vec<_>>();
    Ok(json!({"worlds":worlds,"other":other,"pageCount":model.pages.len()}))
}

fn node_summary(node: &Value) -> Value {
    let mut out = Map::new();
    for key in [
        "id",
        "label",
        "path",
        "date",
        "recorded",
        "kind",
        "confidence",
        "worlds",
        "areas",
    ] {
        out.insert(key.into(), node[key].clone());
    }
    Value::Object(out)
}

fn nodes(graph: &Value) -> HashMap<&str, &Value> {
    graph["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| (text(&n["id"]), n))
        .collect()
}

fn summaries(nodes: &HashMap<&str, &Value>, ids: &Value, limit: usize) -> Vec<Value> {
    ids.as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| nodes.get(text(id)))
        .take(limit)
        .map(|n| node_summary(n))
        .collect()
}

fn related(nodes: &HashMap<&str, &Value>, record: &Value) -> Vec<Value> {
    record["r"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let mut value = node_summary(nodes.get(text(&r["id"]))?);
            value["score"] = r["score"].clone();
            value["why"] = r["why"].clone();
            Some(value)
        })
        .collect()
}

/// `/api/connections?id=`: mentions, links, backlinks and related notes.
pub(crate) fn connections(graph: &Value, id: &str) -> Value {
    let nodes = nodes(graph);
    let record = &graph["connections"]["notes"][id];
    let targets: HashMap<&str, &Value> = graph["connections"]["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| (text(&t["id"]), t))
        .collect();
    let mentions = record["m"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| targets.get(text(t)).map(|v| (*v).clone()))
        .collect::<Vec<_>>();
    json!({
        "state": "ready",
        "mentions": mentions,
        "linksTo": summaries(&nodes, &record["l"], 40),
        "linkedFrom": summaries(&nodes, &record["b"], 40),
        "related": related(&nodes, record),
    })
}

fn newest(mut list: Vec<&Value>) -> Vec<&Value> {
    list.sort_by(|a, b| {
        text(&b["recorded"])
            .cmp(text(&a["recorded"]))
            .then(text(&b["date"]).cmp(text(&a["date"])))
    });
    list
}

fn evidence(graph: &Value, kind: &str, name: &str, target: &str, page: Option<&str>) -> Value {
    static NULL: Value = Value::Null;
    let nodes = nodes(graph);
    let all: &[Value] = graph["nodes"].as_array().map_or(&[], Vec::as_slice);
    let has = |n: &Value, key: &str| {
        n[key]
            .as_array()
            .is_some_and(|v| v.iter().any(|x| text(x) == name))
    };
    let member = |n: &Value| match kind {
        "world" => has(n, "worlds"),
        "area" => has(n, "areas"),
        _ => false,
    };
    let mentioned: BTreeSet<&str> = graph["connections"]["mentionedBy"][target]
        .as_array()
        .into_iter()
        .flatten()
        .map(text)
        .collect();
    let evidence = newest(
        all.iter()
            .filter(|n| {
                text(&n["kind"]) == "note"
                    && !text(&n["path"]).starts_with("wiki/")
                    && (member(n) || mentioned.contains(text(&n["id"])))
            })
            .collect(),
    );
    let digests = newest(
        all.iter()
            .filter(|n| {
                text(&n["kind"]) == "digest" && (member(n) || mentioned.contains(text(&n["id"])))
            })
            .collect(),
    );
    let record = page.map_or(&NULL, |id| &graph["connections"]["notes"][id]);
    json!({
        "count": evidence.len(),
        "latest": evidence.iter().take(10).map(|n| node_summary(n)).collect::<Vec<_>>(),
        "digests": digests.iter().take(8).map(|n| node_summary(n)).collect::<Vec<_>>(),
        "mentions": mentioned.len(),
        "linkedFrom": summaries(&nodes, &record["b"], 20),
        "related": related(&nodes, record),
    })
}

/// Link targets in curated wiki text, resolved against the graph index so the
/// viewer can open notes that its first-paint summary does not hold.
fn resolved_links(page_path: &str, markdown: &str, graph: &Value) -> Value {
    let nodes = nodes(graph);
    let resolver = Resolver::new(
        nodes
            .values()
            .filter(|n| !text(&n["path"]).is_empty())
            .map(|n| (text(&n["path"]).to_owned(), text(&n["id"]).to_owned())),
    );
    let mut out = Map::new();
    for (target, wiki) in link_targets(markdown).into_iter().take(64) {
        if let Some(node) = resolver
            .resolve(page_path, &target, wiki)
            .and_then(|id| nodes.get(id))
        {
            out.insert(target, node_summary(node));
        }
    }
    Value::Object(out)
}

/// `/api/wiki/page?id=`: a world (`world:`), area (`area:`) or wiki page
/// (`note:`) with its facts, curated text, sub-pages and evidence index.
pub(crate) fn page(
    root: &Path,
    id: &str,
    today: NaiveDate,
    graph: Option<&Value>,
) -> Result<Value> {
    let model = Model::load(root)?;
    let (world, area);
    let (kind, title, page) = if id.starts_with("world:") {
        let name = model
            .worlds()
            .into_iter()
            .find(|w| slug_id(w) == id)
            .ok_or("Unknown wiki world")?;
        (world, area) = (name.to_owned(), String::new());
        ("world", name.to_owned(), model.page_for("world", name))
    } else if id.starts_with("area:") {
        let (name, parent) = model
            .worlds()
            .into_iter()
            .flat_map(|w| model.areas(w).into_iter().map(move |(a, _)| (a, w)))
            .find(|(a, _)| area_id(a) == id)
            .ok_or("Unknown wiki area")?;
        (world, area) = (parent.to_owned(), name.to_owned());
        ("area", name.to_owned(), model.page_for("area", name))
    } else {
        let page = model
            .pages
            .iter()
            .find(|p| p.id == id)
            .ok_or("Unknown wiki page")?;
        (world, area) = (page.world.clone(), page.area.clone());
        ("entity", page.name.clone(), Some(page))
    };
    let (intro, section, body) = page.map(|p| sections(&p.text)).unwrap_or_default();
    let (list, note) = facts(section.as_deref().unwrap_or(""), today);
    let curated = format!("{intro}\n{}\n{body}", section.as_deref().unwrap_or(""));
    let subpages = match kind {
        "world" => model
            .areas(&world)
            .into_iter()
            .map(|(a, proposed)| {
                let mut value = model
                    .page_for("area", a)
                    .map_or_else(|| json!({}), |p| summary(p, today));
                value["areaId"] = json!(area_id(a));
                value["label"] = json!(a);
                value["proposed"] = json!(proposed);
                value["pages"] = json!(model.children(&world, a, "").len());
                value
            })
            .chain(
                model
                    .children(&world, "", "")
                    .into_iter()
                    .map(|p| summary(p, today)),
            )
            .collect::<Vec<_>>(),
        "area" => model
            .children(&world, &area, "")
            .into_iter()
            .map(|p| summary(p, today))
            .collect(),
        _ => model
            .children("", "", &title)
            .into_iter()
            .map(|p| summary(p, today))
            .collect(),
    };
    let target = match kind {
        "world" => slug_id(&world),
        "area" => area_id(&area),
        _ => id.to_owned(),
    };
    let parent = page
        .filter(|p| !p.parent.is_empty())
        .and_then(|p| {
            model
                .pages
                .iter()
                .find(|q| casefold(&q.name) == casefold(&p.parent))
        })
        .map(|p| json!({"id":p.id,"label":p.name}));
    let links = graph.map_or(json!({}), |g| {
        resolved_links(page.map_or("wiki/", |p| p.path.as_str()), &curated, g)
    });
    let evidence = graph.map(|g| evidence(g, kind, &title, &target, page.map(|p| p.id.as_str())));
    Ok(json!({
        "id": id, "kind": kind, "title": title,
        "page": page.map(|p| json!({"id":p.id,"path":p.path,"entity":p.entity,"title":p.title})),
        "world": (!world.is_empty()).then(|| json!({"id":slug_id(&world),"label":world})),
        "area": (!area.is_empty() && kind != "area").then(|| json!({"id":area_id(&area),"label":area})),
        "parent": parent,
        "meta": page.map_or(json!({}), |p| json!({
            "status": p.meta["status"], "reviewed": p.meta["reviewed"], "source": p.meta["source"],
            "aliases": text(&p.meta["aliases"]).split(',').map(str::trim).filter(|a| !a.is_empty()).collect::<Vec<_>>(),
        })),
        "intro": intro, "facts": list, "factsNote": note, "body": body,
        "counts": counts(&list),
        "subpages": subpages,
        "links": links,
        "state": if graph.is_some() { "ready" } else { "building" },
        "evidence": evidence,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 6).unwrap()
    }

    #[test]
    fn facts_parse_provenance_supersession_and_stale_classes() {
        let section = "- Red gun MAIN MCU is STM32H743. (confirmed 2026-11-05; [[devices/red-gun|red gun]])\n\
            - ~~Bead runs r3070.~~ (confirmed 2026-09-01; [[a]]) (superseded 2026-11-01 by [[decisions/b]])\n\
            - The laptop address is 100.64.0.7. (confirmed 2026-10-01; [[c]])\n\
            - Deployed nacre-niri 1.4.2 to the desktop. (confirmed 2026-10-20)\n\
            - The owner prefers dark mode. (decided 2026-01-01)\n\
            - Address noted recently. (seen 2026-11-01)\n\
            ### Hosts\n\
            - Plain fact without provenance.\n\
            Not a bullet.";
        let (list, note) = facts(section, day());
        assert_eq!(note, "Not a bullet.");
        assert_eq!(list[0]["text"], "Red gun MAIN MCU is STM32H743.");
        assert_eq!(list[0]["provenance"]["verb"], "confirmed");
        assert_eq!(list[0]["provenance"]["date"], "2026-11-05");
        assert_eq!(list[0]["provenance"]["sources"], json!(["devices/red-gun"]));
        assert!(list[0]["stale"].is_null());
        assert_eq!(list[1]["text"], "Bead runs r3070.");
        assert_eq!(list[1]["superseded"]["by"], json!(["decisions/b"]));
        assert!(
            list[1]["stale"].is_null(),
            "superseded facts are never stale"
        );
        assert_eq!(list[2]["stale"]["class"], "address");
        assert_eq!(list[2]["stale"]["limit"], 14);
        assert!(
            list[3]["stale"].is_null(),
            "17-day-old version fact is fresh"
        );
        assert!(list[4]["stale"].is_null(), "preferences do not go stale");
        assert!(list[5]["stale"].is_null());
        assert_eq!(list[6]["heading"], "Hosts");
        assert!(list[7]["provenance"].is_null());
        let old = staleness(
            "Deployed build 3cc38598 to the laptop.",
            "2026-10-01",
            day(),
        );
        assert_eq!(old.unwrap()["class"], "version");
        assert!(staleness("Use the shared lock for captures.", "2020-01-01", day()).is_none());
        assert_eq!(counts(&list), json!({"current":6,"superseded":1,"stale":1}));
    }

    #[test]
    fn sections_split_intro_facts_and_curated_rest_without_generated_block() {
        let body = "# Bead\nEntity: area\nName: Bead\nReviewed: 2026-11-05\n\nArea in Nacre.\n\n## Current facts\n\n- Fact one. (confirmed 2026-11-05)\n\n## Notes\n\nCurated.\n<!-- brain:generated:start (x) -->\n## Evidence index\n- [[a]]\n<!-- brain:generated:end -->\nAfter.\n";
        let (intro, section, rest) = sections(body);
        assert_eq!(intro, "Area in Nacre.");
        assert_eq!(
            section.unwrap().trim(),
            "- Fact one. (confirmed 2026-11-05)"
        );
        assert_eq!(rest, "## Notes\n\nCurated.\nAfter.");
    }

    #[test]
    fn index_places_world_area_and_entity_pages_and_page_reads_facts() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        std::fs::write(
            root.join("registry.json"),
            r#"{"worlds":[{"name":"Nacre"},{"name":"Home"}],"areas":[{"name":"Bead","parent":"Nacre"},{"name":"Laptop","parent":"Home","status":"proposed"}]}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("wiki")).unwrap();
        std::fs::create_dir_all(root.join("devices")).unwrap();
        let write = |p: &str, b: &str| std::fs::write(root.join(p), b).unwrap();
        write(
            "wiki/nacre.md",
            "# Nacre\nEntity: world\nName: Nacre\nWorlds: Nacre\n\nThe desktop project.\n",
        );
        write("wiki/bead.md", "# Bead\nEntity: area\nName: Bead\nParent: Nacre\nWorlds: Nacre\nAreas: Bead\nStatus: Curated\n\n## Current facts\n\n- Bead is a framework. (confirmed 2026-11-05; [[devices/x]])\n");
        write(
            "wiki/bead-kit.md",
            "# Bead kit\nEntity: feature\nName: Bead kit\nParent: Bead\n\nWidgets.\n",
        );
        write(
            "wiki/kit-button.md",
            "# Kit button\nEntity: feature\nName: Kit button\nParent: Bead kit\n\nA button.\n",
        );
        write("wiki/old.md", "# Old\nEntity: alias\nRedirect: Bead\n");
        write(
            "devices/x.md",
            "# Device X\nRecorded: 2026-11-05\n\nEvidence.\n",
        );
        let index = index(root).unwrap();
        let nacre = &index["worlds"][0];
        assert_eq!(nacre["page"]["path"], "wiki/nacre.md");
        assert_eq!(nacre["areas"][0]["page"]["path"], "wiki/bead.md");
        assert_eq!(nacre["areas"][0]["pages"][0]["name"], "Bead kit");
        assert_eq!(index["worlds"][1]["areas"][0]["proposed"], true);
        assert_eq!(index["pageCount"], 4, "redirect pages are not listed");
        let bead = page(root, "area:bead", day(), None).unwrap();
        assert_eq!(bead["facts"][0]["text"], "Bead is a framework.");
        assert_eq!(
            bead["links"],
            json!({}),
            "links resolve with the graph index"
        );
        let graph = json!({"nodes":[{"id":"note:x","label":"Device X","path":"devices/x.md","kind":"note"}]});
        let ready = page(root, "area:bead", day(), Some(&graph)).unwrap();
        assert_eq!(ready["links"]["devices/x"]["label"], "Device X");
        assert_eq!(ready["state"], "ready");
        assert_eq!(ready["evidence"]["count"], 0);
        assert_eq!(bead["meta"]["status"], "Curated");
        assert_eq!(bead["world"]["label"], "Nacre");
        assert_eq!(bead["subpages"][0]["name"], "Bead kit");
        assert_eq!(bead["state"], "building");
        let kit_id = index["worlds"][0]["areas"][0]["pages"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let kit = page(root, &kit_id, day(), None).unwrap();
        assert_eq!(kit["subpages"][0]["name"], "Kit button");
        assert_eq!(kit["area"]["label"], "Bead");
        let button = page(root, text(&kit["subpages"][0]["id"]), day(), None).unwrap();
        assert_eq!(button["parent"]["label"], "Bead kit");
        assert!(page(root, "world:missing", day(), None).is_err());
    }
}
