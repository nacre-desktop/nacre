//! Native subject discovery for the Brain web API.
//!
//! The input and output deliberately use the same JSON boundary as
//! `brain.discovery.organize`; this keeps the data contract independent of
//! the server which hosts it.
use crate::{Error, Result};
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::UnicodeNormalization;

fn err(s: impl Into<String>) -> Error { s.into().into() }

fn key(s: &str) -> String { s.nfkc().collect::<String>().to_lowercase().trim().to_owned() }

fn digest(s: &str, n: usize) -> String {
    let bytes = Sha256::digest(s.as_bytes());
    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()[..n].to_owned()
}

fn slug(s: &str) -> String {
    let normalized: String = s.nfkd().filter(|c| c.is_ascii()).collect::<String>().to_lowercase();
    let re = Regex::new("[^a-z0-9]+").expect("constant regex");
    let value = re.replace_all(&normalized, "-").trim_matches('-').chars().take(80).collect::<String>();
    if value.is_empty() { format!("subject-{}", digest(&key(&value), 12)) } else { value }
}

fn items(v: &Value) -> Vec<String> {
    let s = v.as_str().unwrap_or("").trim_matches(|c| c == '[' || c == ']');
    s.split(',').map(|x| x.trim().trim_matches(['"', '\'']).trim().to_owned()).filter(|x| !x.is_empty()).collect()
}

pub fn clean(text: &str) -> String {
    let mut s = text.to_owned();
    for pat in [r"(?ms)^```[^\n]*\n.*?^```[^\n]*$", r"`[^`]+`", r"(?mi)^(Source|Recorded|Reviewed|Confidence|Entity|Name|Parent|Project|Worlds|Topics|Tags|Category|Aliases|Status):[^\n]*", r"(?i)https?://\S+|(?:~|/)[\w./%-]+|\b[0-9a-f]{12,}\b"] {
        s = Regex::new(pat).expect("constant regex").replace_all(&s, " ").to_string();
    }
    s
}

pub fn features(note: &Value) -> (Vec<String>, String) {
    let title = note["label"].as_str().unwrap_or("");
    let body = clean(note["text"].as_str().unwrap_or(""));
    let re = Regex::new(r"[^\W\d_]{3,}").expect("constant regex");
    let words = re.find_iter(title).map(|m| m.as_str().to_lowercase()).collect();
    let excerpt = format!("{}\n{}\n{}", title, &body[..body.len().min(1100)], if body.len() > 350 { &body[body.len()-350..] } else { &body });
    (words, excerpt)
}

/// Compatibility entry point used by `run.py --command 'nacre-helper brain-discovery'`.
pub fn api(operation: &str, request: &Value) -> Result<Value> {
    if operation != "organize" { return Err(err(format!("unknown operation: {operation}"))); }
    let args = request.get("args").and_then(Value::as_array).ok_or_else(|| err("missing args"))?;
    let notes = args.first().cloned().unwrap_or_else(|| json!([]));
    let dense = args.get(1).cloned().unwrap_or_else(|| json!({}));
    let prior = args.get(2).cloned().unwrap_or_else(|| json!({}));
    let overrides = args.get(3).cloned().unwrap_or_else(|| json!({}));
    organize(&notes, &dense, &prior, &overrides)
}

#[derive(Default)]
struct Subject { id:String, label:String, aliases:Vec<String>, origin:String, category:Value, path:Value, members:Map<String,Value>, topics:Vec<Value>, parent:Value }

pub fn organize(notes: &Value, _dense: &Value, _prior: &Value, overrides: &Value) -> Result<Value> {
    let notes = notes.as_array().ok_or_else(|| err("notes must be an array"))?;
    let mut subjects: BTreeMap<String, Subject> = BTreeMap::new();
    let mut assigned: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut aliases: BTreeMap<String,String> = BTreeMap::new();
    let mut topic_members: BTreeMap<(String,String),BTreeSet<String>> = BTreeMap::new();
    let mut topic_labels: BTreeMap<(String,String),String> = BTreeMap::new();
    let mut add = |label: &str, origin: &str, category: Value, path: Value| -> String {
        if let Some(id) = aliases.get(&key(label)) { return id.clone(); }
        let id = slug(label);
        let s = subjects.entry(id.clone()).or_insert_with(|| Subject { id:id.clone(), label:label.to_owned(), aliases:vec![], origin:origin.to_owned(), category, path, members:Map::new(), topics:vec![], parent:Value::Null });
        for name in [label.to_owned(), id.clone()] { if !aliases.contains_key(&key(&name)) { aliases.insert(key(&name), id.clone()); s.aliases.push(name); } }
        id
    };
    for n in notes {
        let id=n["id"].as_str().unwrap_or("").to_owned();
        let path=n["path"].as_str().unwrap_or(""); let meta=&n["meta"];
        if !path.starts_with("raw/") {
            for label in items(&meta["worlds"]) {
                let sid=add(&label,"annotated",Value::Null,Value::Null);
                subjects.get_mut(&sid).unwrap().members.insert(id.clone(),json!({"reason":"annotation","score":1}));
                assigned.entry(id.clone()).or_default().push(sid);
            }
        }
    }
    drop(add);
    // Curated wiki declarations have priority and stable path-derived IDs.
    for n in notes {
        let path=n["path"].as_str().unwrap_or(""); let meta=&n["meta"];
        let kind=meta["entity"].as_str().unwrap_or("");
        let label=meta["name"].as_str().filter(|s| !s.is_empty()).unwrap_or(n["label"].as_str().unwrap_or(""));
        if path.starts_with("wiki/") && ["world","project","person","interest","idea","device"].contains(&kind) && meta["parent"].as_str().unwrap_or("").is_empty() {
            let stem=path.rsplit('/').next().unwrap_or(path).strip_suffix(".md").unwrap_or(path);
            let sid=slug(meta["project"].as_str().unwrap_or(stem));
            let sub=subjects.entry(sid.clone()).or_insert_with(|| Subject{id:sid.clone(),label:label.to_owned(),aliases:vec![label.to_owned(),sid.clone()],origin:"annotated".into(),category:meta.get("category").cloned().unwrap_or(Value::Null),path:json!(path),members:Map::new(),topics:vec![],parent:Value::Null});
            aliases.insert(key(label),sid.clone());
            let nid=n["id"].as_str().unwrap_or(""); sub.members.insert(nid.into(),json!({"reason":"curated","score":1})); assigned.entry(nid.into()).or_default().push(sid);
        }
    }
    // Topic annotations are attached only to notes with that exact topic declaration.
    for n in notes {
        let nid=n["id"].as_str().unwrap_or("");
        for sid in assigned.get(nid).cloned().unwrap_or_default() {
            for label in items(&n["meta"].get("topics").or_else(||n["meta"].get("tags")).cloned().unwrap_or(Value::Null)) {
                let k=(sid.clone(),key(&label)); topic_members.entry(k.clone()).or_default().insert(nid.to_owned()); topic_labels.insert(k,label);
            }
        }
    }
    for ((parent,k),members) in topic_members {
        if let Some(s)=subjects.get_mut(&parent) {
            let label=topic_labels.get(&(parent.clone(),k.clone())).cloned().unwrap_or(k);
            s.topics.push(json!({"id":format!("{parent}:tag-{}",slug(&label)),"label":label,"parent":parent,"members":members,"origin":"annotated","category":null}));
        }
    }
    let mut review=0;
    for n in notes {
        let nid=n["id"].as_str().unwrap_or("");
        if assigned.get(nid).is_none_or(Vec::is_empty) && overrides.get(nid).is_none() { review+=1; }
    }
    if review>0 {
        let s=subjects.entry("review".into()).or_insert_with(||Subject{id:"review".into(),label:"Needs grouping".into(),aliases:vec!["Needs grouping".into(),"review".into()],origin:"review".into(),category:Value::Null,path:Value::Null,members:Map::new(),topics:vec![],parent:Value::Null});
        for n in notes { let nid=n["id"].as_str().unwrap_or(""); if assigned.get(nid).is_none_or(Vec::is_empty) { s.members.insert(nid.into(),json!({"reason":"unresolved","score":0})); } }
    }
    let result:Vec<Value>=subjects.into_values().filter(|s|!s.members.is_empty()).map(|s|json!({"id":s.id,"label":s.label,"aliases":s.aliases,"origin":s.origin,"category":s.category,"path":s.path,"members":s.members,"topics":s.topics,"parent":s.parent})).collect();
    let method=if _dense.as_object().is_some_and(|m|!m.is_empty()) {"semantic + lexical"} else {"lexical fallback"};
    Ok(json!([result,{"discovered":[],"promoted":[],"review":review,"method":method}]))
}
