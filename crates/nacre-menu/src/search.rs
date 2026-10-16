//! Faithful app-search.js scoring, including UTF-16 offsets and the query limit.
use crate::text::{self, OrderingRules};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

pub fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .nfkd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect()
}
fn index(value: &[u16], query: &[u16], start: usize) -> Option<usize> {
    if query.is_empty() {
        return Some(start.min(value.len()));
    }
    value
        .get(start..)?
        .windows(query.len())
        .position(|part| part == query)
        .map(|offset| start + offset)
}
fn words(value: &[u16]) -> impl Iterator<Item = &[u16]> {
    value
        .split(|c| !matches!(c, 48..=57 | 97..=122 | 128..=65535))
        .filter(|word| !word.is_empty())
}
pub fn score_units(value: &[u16], query: &[u16]) -> i32 {
    if value.is_empty() || query.is_empty() {
        return 0;
    }
    if value == query {
        return 1000;
    }
    if value.starts_with(query) {
        return 900 - (value.len() - query.len()).min(100) as i32;
    }
    if words(value).any(|word| word.starts_with(query)) {
        return 750;
    }
    if query.len() > 1
        && words(value)
            .map(|word| word[0])
            .collect::<Vec<_>>()
            .starts_with(query)
    {
        return 680;
    }
    if let Some(position) = index(value, query, 0) {
        return 600 - position.min(100) as i32;
    }
    let (mut previous, mut first, mut gaps) = (None, None, 0);
    let mut at = 0;
    while at < query.len() {
        let count = if (0xd800..=0xdbff).contains(&query[at])
            && query
                .get(at + 1)
                .is_some_and(|c| (0xdc00..=0xdfff).contains(c))
        {
            2
        } else {
            1
        };
        let Some(position) = index(value, &query[at..at + count], previous.map_or(0, |p| p + 1))
        else {
            return 0;
        };
        if let Some(previous) = previous {
            gaps += position - previous - 1;
        } else {
            first = Some(position);
        }
        previous = Some(position);
        at += count;
    }
    (250 - first.unwrap_or(0) as i32 - gaps.min(200) as i32).max(1)
}
pub fn score(value: &str, query: &str) -> i32 {
    score_units(
        &value.encode_utf16().collect::<Vec<_>>(),
        &query.encode_utf16().collect::<Vec<_>>(),
    )
}
#[derive(Clone)]
pub struct Prepared {
    pub entry: Value,
    fields: Vec<Vec<u16>>,
}
impl Prepared {
    pub fn new(entry: Value) -> Self {
        let keywords = entry["keywords"]
            .as_array()
            .map(|v| v.iter().map(text::string).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let fields = [
            text::field(&entry, "name"),
            text::field(&entry, "genericName"),
            text::field(&entry, "comment"),
            keywords,
            text::field(&entry, "id"),
        ]
        .into_iter()
        .map(|s| normalize(&s).encode_utf16().collect())
        .collect();
        Self { entry, fields }
    }
}
pub fn query<'a>(records: &'a [Prepared], value: &str, ordering: &OrderingRules) -> Vec<&'a Value> {
    let normalized = normalize(value);
    let mut search: Vec<u16> = text::trim(&normalized).encode_utf16().collect();
    search.truncate(256);
    if search.is_empty() {
        return records.iter().map(|record| &record.entry).collect();
    }
    let terms: Vec<&[u16]> = search
        .split(|c| char::from_u32(u32::from(*c)).is_some_and(text::whitespace))
        .filter(|v| !v.is_empty())
        .collect();
    let mut ranked = Vec::new();
    for record in records {
        let mut total = 0.0;
        let mut valid = true;
        for term in &terms {
            let best = record
                .fields
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    f64::from(score_units(field, term)) * if i == 0 { 1.0 } else { 0.7 }
                })
                .fold(0.0, f64::max);
            if best == 0.0 {
                valid = false;
                break;
            }
            total += best;
        }
        if valid {
            ranked.push((
                &record.entry,
                total + f64::from(score_units(&record.fields[0], &search)),
            ));
        }
    }
    ranked.sort_by(|(a, sa), (b, sb)| {
        sb.total_cmp(sa)
            .then_with(|| ordering.compare(&text::field(a, "name"), &text::field(b, "name")))
            .then_with(|| ordering.compare(&text::field(a, "id"), &text::field(b, "id")))
    });
    ranked.into_iter().map(|(row, _)| row).collect()
}
