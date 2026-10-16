//! Prepared candidate generations; query-specific providers retain their original order.
use crate::{
    context::{array, Context},
    model, text, Result,
};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};
struct Candidate {
    row: Value,
    fields: model::Prepared,
}
impl Candidate {
    fn new(row: Value) -> Self {
        let fields = model::Prepared::new(&row);
        Self { row, fields }
    }
    fn original(&self) -> Value {
        self.row.clone()
    }
}
struct EmojiCandidate {
    key: Value,
    entry: Value,
    candidate: Arc<Candidate>,
}
pub(crate) struct QueryCache {
    prefix: Vec<Candidate>,
    suffix: Vec<Candidate>,
    // Reuse the same emoji rows while typing; never retain the full CLDR catalog.
    emoji: RefCell<VecDeque<EmojiCandidate>>,
}
impl Context {
    pub fn prepare_queries(&self) -> Result<()> {
        if self.queries.get().is_none() {
            let cache = QueryCache {
                emoji: RefCell::new(VecDeque::new()),
                prefix: self
                    .global_prefix()?
                    .into_iter()
                    .map(Candidate::new)
                    .collect(),
                suffix: self
                    .global_suffix()?
                    .into_iter()
                    .map(Candidate::new)
                    .collect(),
            };
            self.queries.get_or_init(|| cache);
        }
        Ok(())
    }
    /// Called on the existing provider-worker retirement event, before trimming.
    pub fn retire_provider_cache(&self) {
        if let Some(cache) = self.queries.get() {
            *cache.emoji.borrow_mut() = VecDeque::new();
        }
    }
    /// Same complete rows as content(), with state-derived candidates prepared once.
    pub fn indexed_content(
        &self,
        route: &str,
        query: &str,
        filter: &str,
        software_category: &str,
    ) -> Result<Vec<Value>> {
        let scope = model::scope(query);
        let term = text::field(&scope, "text");
        if scope["kind"] != "all" || term.is_empty() {
            return self.content(route, query, filter, software_category);
        }
        self.prepare_queries()?;
        let cache = self
            .queries
            .get()
            .ok_or("Launcher candidates are unavailable")?;
        let mut dynamic: Vec<_> = self
            .global_dynamic_without_emoji(&term)
            .into_iter()
            .map(|row| Arc::new(Candidate::new(row)))
            .collect();
        if self
            .node("actions.emoji")
            .is_some_and(|node| node["provider"] == "emoji")
        {
            let mut emoji = cache.emoji.borrow_mut();
            for entry in self.emoji_search_matches(&term) {
                let candidate = if let Some(index) =
                    emoji.iter().position(|cached| cached.key == entry["value"])
                {
                    // Preserve provider order while keeping recent rows at the back.
                    let mut cached = emoji.remove(index).expect("existing emoji cache entry");
                    if cached.entry != *entry {
                        cached.entry = entry.clone();
                        cached.candidate = Arc::new(Candidate::new(self.emoji_row(entry)));
                    }
                    cached
                } else {
                    EmojiCandidate {
                        key: entry["value"].clone(),
                        entry: entry.clone(),
                        candidate: Arc::new(Candidate::new(self.emoji_row(entry))),
                    }
                };
                dynamic.push(candidate.candidate.clone());
                if emoji.len() >= 256 {
                    emoji.pop_front();
                }
                emoji.push_back(candidate);
            }
        } else {
            dynamic.extend(
                self.provider("actions.emoji", &term)?
                    .into_iter()
                    .map(|row| Arc::new(Candidate::new(row))),
            );
        }
        let packages: Vec<_> = if route == "software.packages" {
            self.provider(route, &term)?
                .into_iter()
                .map(Candidate::new)
                .collect()
        } else {
            Vec::new()
        };
        let rows: Vec<&Candidate> = cache
            .prefix
            .iter()
            .chain(dynamic.iter().map(Arc::as_ref))
            .chain(cache.suffix.iter())
            .chain(packages.iter())
            .filter(|c| filter == "all" || crate::content::category(&c.row) == filter)
            .collect();
        let mut result: Vec<Value> = model::ranked_items(
            &rows,
            &term,
            &self.preferences["usage"],
            array(&self.preferences["recent"]),
            self.now,
            |candidate| &candidate.row,
            |candidate, query| candidate.fields.score(query),
        )
        .into_iter()
        .map(|candidate| candidate.original())
        .collect();
        if filter == "all" {
            result.extend(self.web_search_rows(&term));
            let short =
                String::from_utf16_lossy(&term.encode_utf16().take(100).collect::<Vec<_>>());
            result.push(self.value_row(
                "actions",
                "search-web",
                &self.tr(
                    "ui.menu-context.label.search-the-web-for-value0",
                    "Search the web for “{value0}”",
                    json!({"value0":short}),
                ),
                "search-web",
                json!(term),
                "public",
                &self.tr(
                    "ui.menu-context.description.opens-your-browser-only-when-selected",
                    "Opens your browser only when selected",
                    json!({}),
                ),
                json!({}),
            ));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_cache_preserves_changes_is_bounded_and_retires() -> Result<()> {
        let mut context = Context::new(
            crate::config::merge(
                include_str!("../../../nacre/shell-tools/launcher-menu.json"),
                "",
            )?,
            serde_json::from_str(include_str!("../data/defaults.json"))?,
            "en-US",
            1720000000.0,
        )?;
        let matches: Vec<_> = (0..300)
            .map(|n| json!({"value":format!("fixture-{n}"),"name":format!("Warm {n}"),"aliases":["warm"],"category":"Fixture"}))
            .collect();
        context.previews = json!({"emojiQuery":"warm","emojiMatches":&matches[..50]});
        for _ in 0..2 {
            assert_eq!(
                context.indexed_content("apps", "warm", "all", "all")?,
                context.content("apps", "warm", "all", "all")?
            );
            assert_eq!(context.queries.get().unwrap().emoji.borrow().len(), 50);
        }
        context.previews["emojiMatches"][0]["name"] = json!("Warm changed");
        assert_eq!(
            context.indexed_content("apps", "warm", "all", "all")?,
            context.content("apps", "warm", "all", "all")?
        );
        let changed = context.indexed_content("apps", "warm", "all", "all")?;
        assert!(changed.iter().any(|row| row["name"] == "Warm changed"));
        context.previews["emojiMatches"] = json!(matches);
        context.indexed_content("apps", "warm", "all", "all")?;
        assert_eq!(context.queries.get().unwrap().emoji.borrow().len(), 256);
        context.retire_provider_cache();
        assert!(context.queries.get().unwrap().emoji.borrow().is_empty());
        assert_eq!(
            context.indexed_content("apps", "warm", "all", "all")?,
            context.content("apps", "warm", "all", "all")?
        );
        Ok(())
    }
}
