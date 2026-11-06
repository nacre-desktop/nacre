//! Derived Brain connections (N0144), built with the background graph index
//! and never written into notes. Mentions match registry area names/aliases
//! and wiki entity names/aliases (longest first, on word boundaries; labels
//! of up to three characters are case-sensitive). Explicit links give
//! backlinks. Each note keeps up to five related notes scored from
//! IDF-weighted shared mentions, a shared area, link proximity and the search
//! index's wording similarity.
use crate::brain_web::area_id;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};
use unicode_casefold::UnicodeCaseFold;

const RELATED: usize = 5;
const MENTION_EDGES: usize = 3;
const THRESHOLD: f64 = 0.3;

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn casefold(s: &str) -> String {
    s.case_fold().collect()
}

struct Target {
    id: String,
    label: String,
    kind: &'static str,
    page: Option<String>,
}

/// Unmerged registry areas (with the aliases of areas merged into them) and
/// wiki pages whose `Entity:` is not world, area or alias.
fn targets(notes: &[Value], registry: &Value) -> (Vec<Target>, Vec<(String, usize)>) {
    let areas = registry["areas"].as_array().cloned().unwrap_or_default();
    let by_name: HashMap<&str, &Value> = areas.iter().map(|a| (text(&a["name"]), a)).collect();
    let wiki = notes
        .iter()
        .filter(|n| text(&n["path"]).starts_with("wiki/"))
        .collect::<Vec<_>>();
    let mut area_pages: HashMap<String, String> = HashMap::new();
    for n in &wiki {
        if casefold(text(&n["meta"]["entity"])) == "area" {
            for area in n["areas"].as_array().into_iter().flatten() {
                area_pages
                    .entry(text(area).to_owned())
                    .or_insert_with(|| text(&n["id"]).to_owned());
            }
        }
    }
    let (mut targets, mut labels) = (Vec::new(), Vec::new());
    let mut index: HashMap<String, usize> = HashMap::new();
    for entry in &areas {
        let mut name = text(&entry["name"]);
        for _ in 0..areas.len() {
            match by_name.get(name).map(|e| text(&e["merged_into"])) {
                Some(next) if !next.is_empty() => name = next,
                _ => break,
            }
        }
        if by_name
            .get(name)
            .is_none_or(|e| !text(&e["merged_into"]).is_empty())
        {
            continue;
        }
        let t = *index.entry(name.to_owned()).or_insert_with(|| {
            targets.push(Target {
                id: area_id(name),
                label: name.to_owned(),
                kind: "area",
                page: area_pages.get(name).cloned(),
            });
            targets.len() - 1
        });
        let aliases = entry["aliases"].as_array().into_iter().flatten();
        for label in std::iter::once(&entry["name"]).chain(aliases) {
            labels.push((text(label).trim().to_owned(), t));
        }
    }
    for n in wiki {
        let entity = casefold(text(&n["meta"]["entity"]));
        let name = text(&n["meta"]["name"]).trim();
        let registered = entity == "area" && n["areas"].as_array().is_some_and(|a| !a.is_empty());
        if entity.is_empty()
            || name.is_empty()
            || entity == "world"
            || entity == "alias"
            || registered
        {
            continue;
        }
        targets.push(Target {
            id: text(&n["id"]).to_owned(),
            label: name.to_owned(),
            kind: "entity",
            page: Some(text(&n["id"]).to_owned()),
        });
        for label in std::iter::once(name).chain(text(&n["meta"]["aliases"]).split(',')) {
            labels.push((label.trim().to_owned(), targets.len() - 1));
        }
    }
    let mut seen = BTreeSet::new();
    labels.retain(|(label, _)| {
        (2..=60).contains(&label.chars().count()) && seen.insert(casefold(label))
    });
    (targets, labels)
}

/// One alternation over every label, longest first; labels of up to three
/// characters match case-sensitively.
struct Matcher {
    regex: Option<Regex>,
    folded: HashMap<String, usize>,
    exact: HashMap<String, usize>,
}
impl Matcher {
    fn new(labels: &[(String, usize)]) -> Self {
        let mut sorted = labels.to_vec();
        sorted.sort_by(|a, b| {
            b.0.chars()
                .count()
                .cmp(&a.0.chars().count())
                .then(a.0.cmp(&b.0))
        });
        let (mut folded, mut exact, mut pieces) = (HashMap::new(), HashMap::new(), Vec::new());
        for (label, t) in sorted {
            let edge = |c: Option<char>| {
                if c.is_some_and(char::is_alphanumeric) {
                    r"\b"
                } else {
                    ""
                }
            };
            let body = format!(
                "{}{}{}",
                edge(label.chars().next()),
                regex::escape(&label),
                edge(label.chars().last())
            );
            if label.chars().count() <= 3 {
                pieces.push(body);
                exact.insert(label, t);
            } else {
                pieces.push(format!("(?i:{body})"));
                folded.insert(casefold(&label), t);
            }
        }
        let regex = (!pieces.is_empty())
            .then(|| Regex::new(&format!("(?:{})", pieces.join("|"))).ok())
            .flatten();
        Self {
            regex,
            folded,
            exact,
        }
    }
    fn find(&self, prose: &str) -> BTreeSet<usize> {
        let Some(regex) = &self.regex else {
            return BTreeSet::new();
        };
        regex
            .find_iter(prose)
            .filter_map(|m| {
                let s = m.as_str();
                if s.chars().count() <= 3 {
                    self.exact.get(s)
                } else {
                    self.folded.get(&casefold(s))
                }
            })
            .copied()
            .collect()
    }
}

/// Title and body without the header block or the generated evidence block.
pub(crate) fn prose(note: &str) -> String {
    let mut out = String::new();
    let mut lines = note.lines().peekable();
    if let Some(title) = lines.next_if(|l| l.starts_with("# ")) {
        out.push_str(title.strip_prefix("# ").unwrap_or(title));
        out.push('\n');
        while lines.next_if(|l| l.trim().is_empty()).is_some() {}
        while lines
            .next_if(|l| {
                l.split_once(':').is_some_and(|(key, _)| {
                    (1..=32).contains(&key.len())
                        && key
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ' ')
                })
            })
            .is_some()
        {}
    }
    let mut generated = false;
    for line in lines {
        if line.starts_with("<!-- brain:generated:start") {
            generated = true;
        } else if line.starts_with("<!-- brain:generated:end") {
            generated = false;
        } else if !generated {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn shared<T: Ord + Copy>(a: &[T], b: &[T]) -> Vec<T> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// Build the derived connection record stored with the graph index:
/// `targets`, per-note `notes` (`m` mentions, `l` links, `b` backlinks,
/// `r` related), `mentionedBy` (target id -> notes, newest first) and capped
/// `links` (mention and related edges) for the graph view.
pub(crate) fn build(
    notes: &[Value],
    references: &[Value],
    registry: &Value,
    similar: &HashMap<String, Vec<(String, f64)>>,
) -> Value {
    let (targets, labels) = targets(notes, registry);
    let matcher = Matcher::new(&labels);
    let n = notes.len();
    let position: HashMap<&str, usize> = notes
        .iter()
        .enumerate()
        .map(|(i, v)| (text(&v["id"]), i))
        .collect();
    let mentions: Vec<Vec<usize>> = notes
        .iter()
        .map(|note| {
            let mut found = matcher.find(&prose(text(&note["text"])));
            found.retain(|t| targets[*t].page.as_deref() != Some(text(&note["id"])));
            found.into_iter().collect()
        })
        .collect();
    let mut by_target = vec![Vec::new(); targets.len()];
    for (i, found) in mentions.iter().enumerate() {
        for &t in found {
            by_target[t].push(i);
        }
    }
    let areas: Vec<Vec<&str>> = notes
        .iter()
        .map(|v| {
            v["areas"]
                .as_array()
                .into_iter()
                .flatten()
                .map(text)
                .collect()
        })
        .collect();
    let mut by_area: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, list) in areas.iter().enumerate() {
        for a in list {
            by_area.entry(a).or_default().push(i);
        }
    }
    let (mut out, mut inbound) = (vec![BTreeSet::new(); n], vec![BTreeSet::new(); n]);
    for edge in references {
        if let (Some(&a), Some(&b)) = (
            position.get(text(&edge["source"])),
            position.get(text(&edge["target"])),
        ) {
            out[a].insert(b);
            inbound[b].insert(a);
        }
    }
    let wording: Vec<HashMap<usize, f64>> = notes
        .iter()
        .map(|v| {
            similar
                .get(text(&v["id"]))
                .into_iter()
                .flatten()
                .filter_map(|(id, s)| position.get(id.as_str()).map(|&j| (j, *s)))
                .collect()
        })
        .collect();
    let total = n as f64 + 1.0;
    let idf = |t: usize| ((total / (by_target[t].len() as f64 + 0.5)).ln() / total.ln()).max(0.0);
    let is_wiki = |i: usize| text(&notes[i]["path"]).starts_with("wiki/");
    let mut related: Vec<Vec<(usize, f64, Value)>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut candidates = BTreeSet::new();
        for &t in &mentions[i] {
            if by_target[t].len() <= n / 4 + 1 {
                candidates.extend(by_target[t].iter().copied());
            }
        }
        for a in &areas[i] {
            candidates.extend(by_area[a].iter().copied().take(400));
        }
        for &j in out[i].iter().chain(inbound[i].iter()) {
            candidates.insert(j);
        }
        for &t in &out[i] {
            if inbound[t].len() <= 50 {
                candidates.extend(inbound[t].iter().copied());
            }
        }
        for &s in &inbound[i] {
            if out[s].len() <= 50 {
                candidates.extend(out[s].iter().copied());
            }
        }
        candidates.extend(wording[i].keys().copied());
        candidates.remove(&i);
        let mut scored = Vec::new();
        for j in candidates {
            if is_wiki(j) && !is_wiki(i) {
                continue;
            }
            let mut common = shared(&mentions[i], &mentions[j]);
            common.sort_by(|a, b| idf(*b).total_cmp(&idf(*a)));
            let entities = common.iter().map(|&t| idf(t)).sum::<f64>().min(2.0);
            let area = areas[i].iter().find(|a| areas[j].contains(*a));
            let link = out[i].contains(&j) || inbound[i].contains(&j);
            let colinks =
                out[i].intersection(&out[j]).count() + inbound[i].intersection(&inbound[j]).count();
            let words = wording[i]
                .get(&j)
                .or_else(|| wording[j].get(&i))
                .copied()
                .unwrap_or(0.0);
            let mut score = entities
                + if area.is_some() { 0.25 } else { 0.0 }
                + if link { 1.0 } else { 0.0 }
                + (0.15 * colinks as f64).min(0.6)
                + 1.2 * words;
            if text(&notes[j]["kind"]) == "archive" {
                score *= 0.6;
            }
            if score < THRESHOLD {
                continue;
            }
            let mut why = Map::new();
            if !common.is_empty() {
                why.insert(
                    "e".into(),
                    json!(common
                        .iter()
                        .take(3)
                        .map(|&t| &targets[t].label)
                        .collect::<Vec<_>>()),
                );
            }
            if let Some(area) = area {
                why.insert("a".into(), json!(area));
            }
            if link {
                why.insert("l".into(), json!(true));
            }
            if words >= 0.15 {
                why.insert("w".into(), json!(round3(words)));
            }
            scored.push((j, score, Value::Object(why)));
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(RELATED);
        related.push(scored);
    }
    let recency = |i: &usize| std::cmp::Reverse(text(&notes[*i]["recorded"]).to_owned());
    let mut records = Map::new();
    let mut edges = Vec::new();
    let mut pairs = BTreeSet::new();
    let generic = (n / 5).max(20);
    for i in 0..n {
        let id = text(&notes[i]["id"]);
        let mut record = Map::new();
        let list = |set: &BTreeSet<usize>| {
            let mut v = set.iter().copied().collect::<Vec<_>>();
            v.sort_by_key(recency);
            json!(v.iter().map(|&j| text(&notes[j]["id"])).collect::<Vec<_>>())
        };
        if !mentions[i].is_empty() {
            record.insert(
                "m".into(),
                json!(mentions[i]
                    .iter()
                    .map(|&t| &targets[t].id)
                    .collect::<Vec<_>>()),
            );
        }
        if !out[i].is_empty() {
            record.insert("l".into(), list(&out[i]));
        }
        if !inbound[i].is_empty() {
            record.insert("b".into(), list(&inbound[i]));
        }
        if !related[i].is_empty() {
            record.insert(
                "r".into(),
                json!(related[i]
                    .iter()
                    .map(|(j, score, why)| json!({"id":text(&notes[*j]["id"]),"score":round3(*score),"why":why}))
                    .collect::<Vec<_>>()),
            );
        }
        if !record.is_empty() {
            records.insert(id.to_owned(), Value::Object(record));
        }
        let mut strongest = mentions[i]
            .iter()
            .copied()
            .filter(|&t| {
                by_target[t].len() <= generic
                    && !(targets[t].kind == "area" && areas[i].contains(&targets[t].label.as_str()))
            })
            .collect::<Vec<_>>();
        strongest.sort_by(|a, b| idf(*b).total_cmp(&idf(*a)).then(a.cmp(b)));
        for t in strongest.into_iter().take(MENTION_EDGES) {
            edges.push(json!({"source":id,"target":targets[t].id,"kind":"mention"}));
        }
        for (j, score, _) in &related[i] {
            if pairs.insert((i.min(*j), i.max(*j))) {
                edges.push(json!({"source":id,"target":text(&notes[*j]["id"]),"kind":"related","score":round3(*score)}));
            }
        }
    }
    let mut mentioned = Map::new();
    let mut target_values = Vec::new();
    for (t, target) in targets.iter().enumerate() {
        let mut list = by_target[t].clone();
        list.sort_by_key(recency);
        target_values.push(json!({"id":target.id,"label":target.label,"kind":target.kind,"page":target.page,"count":list.len()}));
        if !list.is_empty() {
            mentioned.insert(
                target.id.clone(),
                json!(list
                    .iter()
                    .map(|&j| text(&notes[j]["id"]))
                    .collect::<Vec<_>>()),
            );
        }
    }
    json!({"targets":target_values,"notes":records,"mentionedBy":mentioned,"links":edges})
}

fn round3(value: f64) -> f64 {
    crate::brain_discovery::rounded(value, 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, path: &str, body: &str, areas: &[&str]) -> Value {
        let meta = crate::brain_web::fields(body);
        json!({"id":id,"path":path,"text":body,"meta":meta,"areas":areas,"kind":"note","recorded":format!("2026-11-0{}", id.len() % 9)})
    }

    fn registry() -> Value {
        json!({"worlds":[{"name":"Nacre"},{"name":"Infrastructure"}],"areas":[
            {"name":"Bead","parent":"Nacre","aliases":["bead green"]},
            {"name":"Old Bead","parent":"Nacre","merged_into":"Bead","aliases":["beadling"]},
            {"name":"CI","parent":"Infrastructure"},
            {"name":"Network","parent":"Infrastructure"}]})
    }

    #[test]
    fn prose_skips_header_and_generated_block() {
        let body = "# Title Bead\nEntity: device\nName: Red gun\n\nBody text.\n<!-- brain:generated:start (x) -->\n- [[a|Network]]\n<!-- brain:generated:end -->\nAfter.\n";
        assert_eq!(prose(body), "Title Bead\n\nBody text.\nAfter.\n");
    }

    #[test]
    fn mentions_are_longest_first_bounded_and_short_aliases_case_sensitive() {
        let notes = vec![
            note("note:page", "wiki/red-gun.md", "# Red gun\nEntity: device\nName: Red gun\nAliases: RG\n\nThe Red gun page.\n", &[]),
            note("note:a", "notes/a.md", "# A\n\nThe red gun fires; Bead green and beadling ship. ci is lowercase, RG is not.\n", &[]),
            note("note:b", "notes/b.md", "# B\n\nBeads and rg and subnetwork and CIA do not count.\n", &[]),
        ];
        let value = build(&notes, &[], &registry(), &HashMap::new());
        let a = &value["notes"]["note:a"]["m"];
        assert_eq!(a, &json!(["area:bead", "note:page"]), "{value}");
        assert!(value["notes"]["note:b"].get("m").is_none(), "{value}");
        // The page does not mention itself.
        assert!(value["notes"]["note:page"].get("m").is_none());
        assert_eq!(value["mentionedBy"]["note:page"], json!(["note:a"]));
    }

    #[test]
    fn related_combines_entities_links_and_wording_and_is_capped() {
        let mut notes = vec![note(
            "note:page",
            "wiki/tailnet.md",
            "# Tailnet\nEntity: network\nName: Tailnet\n\nPage.\n",
            &[],
        )];
        for i in 0..9 {
            notes.push(note(
                &format!("note:{i}"),
                &format!("notes/{i}.md"),
                &format!("# Note {i}\n\nThe Tailnet changed on the Network.\n"),
                &["Network"],
            ));
        }
        notes.push(note("note:x", "notes/x.md", "# X\n\nUnrelated.\n", &["CI"]));
        let refs = vec![json!({"source":"note:0","target":"note:x","kind":"source"})];
        let similar = HashMap::from([("note:0".to_owned(), vec![("note:x".to_owned(), 0.5)])]);
        let value = build(&notes, &refs, &registry(), &similar);
        let related = value["notes"]["note:0"]["r"].as_array().unwrap();
        assert_eq!(related.len(), RELATED);
        assert_eq!(related[0]["id"], "note:x");
        assert_eq!(related[0]["why"]["l"], true);
        assert!(related.iter().all(|r| r["id"] != "note:page"));
        let why = &related[1]["why"];
        assert!(
            why["e"].as_array().unwrap().contains(&json!("Tailnet")),
            "{why}"
        );
        assert_eq!(why["a"], "Network");
        assert_eq!(value["notes"]["note:x"]["b"], json!(["note:0"]));
        let edges = value["links"].as_array().unwrap();
        assert!(edges.iter().filter(|e| e["kind"] == "related").count() <= notes.len() * RELATED);
        assert!(edges.iter().any(|e| e["kind"] == "mention"
            && e["source"] == "note:0"
            && e["target"] == "note:page"));
        // Membership already connects notes to their own area.
        assert!(!edges
            .iter()
            .any(|e| e["kind"] == "mention" && e["target"] == "area:network"));
    }
}
