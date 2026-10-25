//! Universal menu matching, stable order and the existing frecency formula.
use crate::text;
use serde_json::{json, Value};

pub fn normalized(value: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for c in value
        .to_lowercase()
        .chars()
        .map(|c| if matches!(c, '-' | '_' | ':') { ' ' } else { c })
    {
        if text::whitespace(c) {
            space = !out.is_empty();
        } else {
            if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}
pub fn scope(value: &str) -> Value {
    let trimmed = value.trim_start_matches(text::whitespace);
    let (kind, prefix) = match trimmed.chars().next() {
        Some('>') => ("actions", ">"),
        Some('=') => ("calculator", "="),
        Some('#') => ("brain", "#"),
        Some('?') => ("ai", "?"),
        Some('/') => ("help", "/"),
        Some(':') => ("files", ":"),
        _ => ("all", ""),
    };
    json!({"kind":kind,"prefix":prefix,"text":text::trim(if prefix.is_empty(){trimmed}else{&trimmed[1..]})})
}
pub fn distance(a: &str, b: &str, limit: usize) -> usize {
    let a: Vec<_> = a.encode_utf16().collect();
    let b: Vec<_> = b.encode_utf16().collect();
    distance_units(&a, &b, limit)
}
fn distance_units(a: &[u16], b: &[u16], limit: usize) -> usize {
    if a.len().abs_diff(b.len()) > limit {
        return limit + 1;
    }
    let mut prior: Vec<_> = (0..=b.len()).collect();
    let mut next = vec![0; b.len() + 1];
    for (i, letter) in a.iter().enumerate() {
        next[0] = i + 1;
        let mut minimum = i + 1;
        for (j, other) in b.iter().enumerate() {
            let n = (next[j] + 1)
                .min(prior[j + 1] + 1)
                .min(prior[j] + usize::from(letter != other));
            minimum = minimum.min(n);
            next[j + 1] = n;
        }
        if minimum > limit {
            return limit + 1;
        }
        std::mem::swap(&mut prior, &mut next);
    }
    prior[b.len()]
}
pub fn stable(row: &Value) -> String {
    let id = text::field(row, "id");
    if row["kind"] == "app" {
        id
    } else {
        row["favoriteId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("nacre-menu:{id}"))
    }
}
pub fn favorite<'a>(
    id: &str,
    apps: &'a [Value],
    rows: &'a [Value],
    legacy: &'a [Value],
) -> Option<&'a Value> {
    if let Some(id) = id.strip_prefix("nacre-menu:") {
        rows.iter().find(|r| r["id"] == id)
    } else if id.starts_with("nacre-action:") {
        legacy.iter().find(|r| r["id"] == id)
    } else {
        apps.iter().find(|r| r["id"] == id)
    }
}
fn name(row: &Value) -> String {
    row["name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| row["label"].as_str().unwrap_or(""))
        .to_owned()
}
fn ascii_words(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
}
pub fn search_index(row: &Value) -> Value {
    let name = normalized(&name(row));
    let alternate: Vec<_> = array_aliases(row)
        .iter()
        .map(|v| normalized(&text::string(v)))
        .collect();
    let haystack = normalized(
        &[
            name.clone(),
            text::field(row, "description"),
            text::field(row, "breadcrumb"),
            text::field(row, "id"),
            text::field(row, "matchedQuery"),
            alternate.join(" "),
        ]
        .join(" "),
    );
    let initials: String = ascii_words(&name)
        .filter_map(|word| word.chars().next())
        .collect();
    let tokens: Vec<_> = ascii_words(&[name.clone(), alternate.join(" ")].join(" "))
        .map(str::to_owned)
        .collect();
    json!({"name":name,"alternate":alternate,"haystack":haystack,"initials":initials,"tokens":tokens})
}
fn array_aliases(row: &Value) -> &[Value] {
    row["aliases"].as_array().map(Vec::as_slice).unwrap_or(&[])
}
struct Word {
    value: String,
    units: Vec<u16>,
}
pub struct ScoringQuery {
    term: String,
    units: usize,
    words: Vec<Word>,
}
impl ScoringQuery {
    fn from_normalized(term: &str) -> Self {
        Self {
            term: term.into(),
            units: term.encode_utf16().count(),
            words: term
                .split(' ')
                .filter(|s| !s.is_empty())
                .map(|s| Word {
                    value: s.into(),
                    units: s.encode_utf16().collect(),
                })
                .collect(),
        }
    }
}
/// Search fields are desktop-state data; prepare them outside the per-key loop.
pub struct Prepared {
    name: String,
    alternate: Vec<String>,
    haystack: String,
    initials: String,
    tokens: Vec<Word>,
    clipboard: bool,
}
impl Prepared {
    pub fn new(row: &Value) -> Self {
        let owned;
        let fields = if row["searchIndex"].is_object() {
            &row["searchIndex"]
        } else {
            owned = search_index(row);
            &owned
        };
        Self {
            name: text::field(fields, "name"),
            alternate: fields["alternate"]
                .as_array()
                .map(|v| v.iter().map(text::string).collect())
                .unwrap_or_default(),
            haystack: text::field(fields, "haystack"),
            initials: text::field(fields, "initials"),
            tokens: fields["tokens"]
                .as_array()
                .map(|v| {
                    v.iter()
                        .map(|v| {
                            let value = text::string(v);
                            let units = value.encode_utf16().collect();
                            Word { value, units }
                        })
                        .collect()
                })
                .unwrap_or_default(),
            clipboard: row["clipboard"] == true,
        }
    }
    pub fn score(&self, query: &ScoringQuery) -> i32 {
        let term = &query.term;
        let words = &query.words;
        if self.clipboard {
            return if words.iter().all(|w| self.haystack.contains(&w.value)) {
                55
            } else {
                -1
            };
        }
        if self.name == *term {
            return 120;
        }
        if self.name.starts_with(term) {
            return 100;
        }
        if self.alternate.iter().any(|v| v == term) {
            return 89;
        }
        if words.iter().all(|w| self.name.contains(&w.value)) {
            return 85;
        }
        if query.units >= 2 && self.initials.starts_with(term) {
            return 80;
        }
        if words.iter().all(|w| self.haystack.contains(&w.value)) {
            return 55;
        }
        if words.iter().all(|word| {
            self.tokens.iter().any(|token| {
                token.value.contains(&word.value)
                    || (word.units.len() >= 4
                        && distance_units(
                            &word.units,
                            &token.units,
                            if word.units.len() >= 7 { 2 } else { 1 },
                        ) <= if word.units.len() >= 7 { 2 } else { 1 })
            })
        }) {
            48
        } else {
            -1
        }
    }
}
pub fn ranked<'a>(
    rows: &'a [Value],
    query: &str,
    usage: &Value,
    recent: &[Value],
    now: f64,
) -> Vec<&'a Value> {
    ranked_items(
        rows,
        query,
        usage,
        recent,
        now,
        |value| value,
        |value, query| Prepared::new(value).score(query),
    )
}
pub fn ranked_items<'a, T>(
    rows: &'a [T],
    query: &str,
    usage: &Value,
    recent: &[Value],
    now: f64,
    value: impl Fn(&T) -> &Value,
    score: impl Fn(&T, &ScoringQuery) -> i32,
) -> Vec<&'a T> {
    let term = normalized(query);
    if term.is_empty() {
        return rows.iter().collect();
    }
    let prepared = ScoringQuery::from_normalized(&term);
    let mut result = Vec::new();
    for (index, item) in rows.iter().enumerate() {
        let row = value(item);
        let score = score(item, &prepared);
        if score < 0 {
            continue;
        }
        let id = stable(row);
        let history = &usage[&id];
        let position = recent.iter().position(|v| v.as_str() == Some(&id));
        let count = history["count"]
            .as_f64()
            .filter(|n| n.is_finite())
            .unwrap_or(0.0)
            .max(0.0);
        let age = history["last"]
            .as_f64()
            .filter(|n| n.is_finite())
            .map_or(f64::INFINITY, |last| (now - last).max(0.0));
        let boost = (count + 1.0).log2().min(6.0)
            + position
                .map_or(0.0, |p| 4.0 * (1.0 - p as f64 / 12.0))
                .max(4.0 * (-age / (7.0 * 86400.0)).exp());
        result.push((
            item,
            index,
            f64::from(score) + boost + if row["kind"] == "app" { 3.0 } else { 0.0 },
        ));
    }
    result.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    result
        .into_iter()
        .take(100)
        .map(|(row, _, _)| row)
        .collect()
}

pub fn parent_of(id: &str) -> &str {
    id.rsplit_once('.').map_or("", |(parent, _)| parent)
}
pub fn children<'a>(route: &str, rows: &'a [Value]) -> Vec<&'a Value> {
    rows.iter()
        .filter(|row| parent_of(row["id"].as_str().unwrap_or("")) == route)
        .collect()
}
pub fn trail(id: &str, rows: &[Value]) -> Vec<Value> {
    let parts: Vec<_> = id.split('.').collect();
    let mut result = Vec::new();
    for n in 1..=parts.len() {
        let key = parts[..n].join(".");
        if let Some(row) = rows.iter().find(|row| row["id"] == key) {
            result.push(json!({"id":row["id"],"label":row["label"]}));
        }
    }
    result
}
fn route_alias(route: &str) -> Option<&'static str> {
    match route {
        "favorites" => Some("home"),
        "all" => Some("apps"),
        "actions" => Some("actions"),
        "settings" => Some("setup"),
        "system" => Some("power"),
        "clipboard" => Some("actions.clipboard"),
        "keys" => Some("learn.shortcuts"),
        "overview" => Some("actions.windows"),
        "palette" => Some("style"),
        "wallpaper" => Some("style.wallpapers"),
        _ => None,
    }
}
pub fn resolve(value: &str, rows: &[Value]) -> String {
    let route = text::trim(if value.is_empty() { "home" } else { value }).to_lowercase();
    if rows.iter().any(|row| row["id"] == route) {
        return route;
    }
    if let Some(alias) = route_alias(&route) {
        if rows.iter().any(|row| row["id"] == alias) {
            return alias.into();
        }
    }
    rows.iter()
        .find(|row| {
            row["aliases"]
                .as_array()
                .is_some_and(|v| v.iter().any(|a| text::string(a).to_lowercase() == route))
        })
        .map(|row| text::field(row, "id"))
        .unwrap_or_default()
}
pub fn entry(row: &Value, rows: &[Value]) -> Value {
    let mut result = row.clone();
    result["name"] = row["label"].clone();
    result["breadcrumb"] = json!(trail(parent_of(row["id"].as_str().unwrap_or("")), rows)
        .iter()
        .map(|r| text::field(r, "label"))
        .collect::<Vec<_>>()
        .join(" › "));
    result
}
pub fn application(app: &Value) -> Value {
    let id = text::field(app, "id");
    let brand = match id.strip_suffix(".desktop").unwrap_or(&id) {
        "nacre-ai" => "ai",
        "nacre-brain" => "brain",
        "nacre-colors" => "colors",
        "nacre-settings" => "settings",
        "nacre-welcome" => "welcome",
        _ => "",
    };
    let mut result = json!({"kind":"app","brand":brand,"description":app["comment"].as_str().filter(|s|!s.is_empty()).unwrap_or("Open this application"),"aliases":app.get("keywords").cloned().unwrap_or(json!([])),"app":app,"breadcrumb":"All apps","operation":"app","disabled":false});
    for key in ["id", "name", "icon"] {
        if let Some(v) = app.get(key) {
            result[key] = v.clone();
        }
    }
    if let Some(v) = app.get("name") {
        result["label"] = v.clone();
    }
    result
}
pub fn alphabetical<'a>(rows: &'a [Value], ordering: &text::OrderingRules) -> Vec<&'a Value> {
    let mut result: Vec<_> = rows.iter().collect();
    result.sort_by(|a, b| {
        ordering
            .compare(&name(a).to_lowercase(), &name(b).to_lowercase())
            .then_with(|| ordering.compare(&stable(a), &stable(b)))
    });
    result
}
pub fn rail<'a>(rows: &'a [Value], preferences: &Value) -> Vec<&'a Value> {
    let mut order = vec!["home".to_owned()];
    if let Some(configured) = preferences["order"].as_array() {
        for id in configured.iter().filter_map(Value::as_str) {
            if !order.iter().any(|v| v == id) {
                order.push(id.into());
            }
        }
    }
    for id in [
        "home", "apps", "actions", "style", "setup", "software", "health", "learn", "power",
    ] {
        if !order.iter().any(|v| v == id) {
            order.push(id.into());
        }
    }
    let hidden = preferences["hidden"].as_array();
    let mut result: Vec<_> = rows
        .iter()
        .filter(|r| {
            r["kind"] == "group"
                && parent_of(r["id"].as_str().unwrap_or("")).is_empty()
                && !(r["id"] != "home" && hidden.is_some_and(|v| v.contains(&r["id"])))
        })
        .collect();
    result.sort_by_key(|r| order.iter().position(|v| r["id"] == *v).unwrap_or(99));
    result
}
