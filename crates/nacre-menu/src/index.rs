//! Prepared candidate generations; query-specific providers retain their original order.
use crate::{
    context::{array, Context},
    model, text, Result,
};
use serde_json::{json, Value};
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
pub(crate) struct QueryCache {
    prefix: Vec<Candidate>,
    suffix: Vec<Candidate>,
}
impl Context {
    pub fn prepare_queries(&self) -> Result<()> {
        if self.queries.get().is_none() {
            let cache = QueryCache {
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
        let dynamic: Vec<_> = self
            .global_dynamic(&term)?
            .into_iter()
            .map(Candidate::new)
            .collect();
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
            .chain(dynamic.iter())
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
