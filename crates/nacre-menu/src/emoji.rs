//! Existing literal CLDR provider, indexed once per worker rather than per key.
use crate::{context::array, Result};
use serde_json::{json, Value};
use std::sync::OnceLock;
fn normalized(value: &str) -> String {
    value
        .to_lowercase()
        .replace(['-', '_', ':'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
struct Entry {
    data: Value,
    name: String,
    aliases: Vec<String>,
    haystack: String,
    initials: String,
}
pub struct Index(Vec<Entry>);
impl Index {
    pub fn new(catalog: &Value) -> Self {
        Self(
            array(&catalog["entries"])
                .iter()
                .map(|entry| {
                    let name = normalized(entry["name"].as_str().unwrap_or(""));
                    let mut aliases: Vec<_> = array(&entry["aliases"])
                        .iter()
                        .filter_map(Value::as_str)
                        .map(normalized)
                        .collect();
                    aliases.push(entry["value"].as_str().unwrap_or("").to_owned());
                    let haystack = format!(
                        "{name} {} actions emoji {}",
                        normalized(entry["category"].as_str().unwrap_or("")),
                        aliases.join(" ")
                    );
                    let initials = name
                        .split(|c: char| !c.is_ascii_alphanumeric())
                        .filter_map(|word| word.chars().next())
                        .collect();
                    Entry {
                        data: entry.clone(),
                        name,
                        aliases,
                        haystack,
                        initials,
                    }
                })
                .collect(),
        )
    }
    pub fn search(&self, query: &str) -> Result<Value> {
        if query.chars().count() > 160 || query.contains('\0') {
            return Err("Emoji search is too long".into());
        }
        let term = normalized(query);
        let words: Vec<_> = term.split_whitespace().collect();
        let mut matches = Vec::new();
        for (index, entry) in self.0.iter().enumerate() {
            let score = if term.is_empty() {
                1
            } else if entry.name == term {
                120
            } else if entry.name.starts_with(&term) {
                100
            } else if entry.aliases.contains(&term) {
                89
            } else if words.iter().all(|word| entry.name.contains(word)) {
                85
            } else if term.len() >= 2 && entry.initials.starts_with(&term) {
                80
            } else if words.iter().all(|word| entry.haystack.contains(word)) {
                55
            } else {
                continue;
            };
            matches.push((score, index, &entry.data));
        }
        matches.sort_by_key(|(score, index, _)| (std::cmp::Reverse(*score), *index));
        Ok(
            json!({"entries":matches.into_iter().take(100).map(|(_,_,entry)|entry).collect::<Vec<_>>()}),
        )
    }
}
pub fn bundled() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| {
        Index::new(
            &serde_json::from_str(include_str!(
                "../../../nacre/shell-tools/emoji-catalog.json"
            ))
            .expect("committed emoji catalog"),
        )
    })
}
