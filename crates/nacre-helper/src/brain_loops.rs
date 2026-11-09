//! Deterministic open-loop extraction and note timeline.
use chrono::{DateTime, Datelike, NaiveDate};
use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

static UNITS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:N\d{4}|B\d{4}|NN\d{4}|PR\s*#?\s*\d{2,}|[0-9a-f]{7,40})\b").unwrap()
});
static NEGATED_DONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:not|never|isn't|wasn't)\s+(?:done|merged|fixed|resolved|landed|completed)\b",
    )
    .unwrap()
});

fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect()
}

fn ids(text: &str) -> Vec<String> {
    UNITS
        .find_iter(text)
        .map(|m| m.as_str().to_ascii_uppercase().replace([' ', '#'], ""))
        .collect()
}

fn date(value: &Value) -> String {
    value["date"]
        .as_str()
        .and_then(|s| s.get(..10))
        .map(str::to_owned)
        .or_else(|| {
            value["meta"]["recorded"]
                .as_str()
                .and_then(|s| s.get(..10))
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

fn recorded(value: &Value) -> Option<DateTime<chrono::FixedOffset>> {
    value["recorded"]
        .as_str()
        .or_else(|| value["meta"]["recorded"].as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
}

fn later(candidate: &Value, origin: &Value) -> bool {
    if let (Some(newer), Some(older)) = (recorded(candidate), recorded(origin)) {
        return newer > older;
    }
    let a = date(candidate);
    let b = date(origin);
    !a.is_empty() && !b.is_empty() && a > b
}

fn overlap(a: &str, b: &str) -> bool {
    let x = words(a);
    let y = words(b);
    let shared = x.intersection(&y).count();
    shared >= 3 && shared * 100 / x.len().min(y.len()).max(1) >= 70
}

fn same_scope(a: &Value, b: &Value) -> bool {
    let areas_a = a["areas"].as_array().cloned().unwrap_or_default();
    let areas_b = b["areas"].as_array().cloned().unwrap_or_default();
    if !areas_a.is_empty() && !areas_b.is_empty() {
        return areas_a.iter().any(|name| areas_b.contains(name));
    }
    let worlds_a = a["worlds"].as_array().cloned().unwrap_or_default();
    let worlds_b = b["worlds"].as_array().cloned().unwrap_or_default();
    worlds_a.iter().any(|name| worlds_b.contains(name))
}

fn same_text_scope(a: &Value, b: &Value) -> bool {
    let aa = a["areas"].as_array().cloned().unwrap_or_default();
    let ba = b["areas"].as_array().cloned().unwrap_or_default();
    if !aa.is_empty() || !ba.is_empty() {
        return !aa.is_empty() && !ba.is_empty() && aa.iter().any(|x| ba.contains(x));
    }
    same_scope(a, b)
}

fn content_lines(note: &Value) -> Vec<String> {
    let source = note["text"].as_str().unwrap_or_default();
    let mut lines = Vec::new();
    let (mut fence, mut curated, mut questions, mut header, mut metadata, mut generated) =
        (false, false, false, true, false, false);
    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if index == 0 && line.starts_with("# ") {
            continue;
        }
        if header {
            if line.is_empty() {
                if metadata {
                    header = false;
                }
                continue;
            }
            let key = line.split_once(':').map(|(k, _)| k.trim()).unwrap_or("");
            if [
                "Recorded",
                "Worlds",
                "Areas",
                "Topics",
                "Confidence",
                "Source",
                "Status",
                "Reviewed",
                "Review",
                "Entity",
                "Name",
                "Parent",
                "Aliases",
                "Supersedes",
            ]
            .contains(&key)
            {
                metadata = true;
                continue;
            }
            header = false;
        }
        if line.starts_with("<!-- brain:generated:start") {
            generated = true;
            continue;
        }
        if line.starts_with("<!-- brain:generated:end") {
            generated = false;
            continue;
        }
        if generated {
            continue;
        }
        if line.starts_with("```") || line.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        if line.starts_with("<!-- brain:curated:start") {
            curated = true;
            questions = false;
            continue;
        }
        if line.starts_with("<!-- brain:curated:end") {
            curated = false;
            questions = false;
            lines.push("##".to_owned());
            continue;
        }
        if curated {
            if line.starts_with('#') {
                questions = line
                    .trim_start_matches('#')
                    .trim()
                    .eq_ignore_ascii_case("open questions");
                if questions {
                    lines.push("## Open questions".to_owned());
                }
                continue;
            }
            if questions && (line.starts_with('-') || line.starts_with('*')) {
                lines.push(line.to_owned());
            }
            continue;
        }
        lines.push(line.to_owned());
    }
    lines
}

fn digest_line(note: &Value) -> String {
    content_lines(note)
        .into_iter()
        .find(|line| {
            !line.is_empty() && !line.starts_with(['#', '-', '*']) && !line.starts_with("<!--")
        })
        .unwrap_or_default()
}

fn add_loop(out: &mut Vec<Value>, note: &Value, line: &str, kind: &str, line_no: usize) {
    let text = line.trim().trim_start_matches(['-', '*', ' ']).trim();
    if text.is_empty() {
        return;
    }
    let source = note["id"]
        .as_str()
        .or_else(|| note["source"].as_str())
        .unwrap_or("");
    let path = note["path"].as_str().unwrap_or("");
    let identity = format!("{source}:{line_no}:{text}");
    let id = format!("loop-{:016x}", fnv1a(identity.as_bytes()));
    out.push(json!({"id":id,"text":text,"source":source,"path":path,"date":date(note),
        "recorded":note["recorded"].as_str().or_else(|| note["meta"]["recorded"].as_str()),
        "unitIds":if ids(text).is_empty() { ids(note["label"].as_str().unwrap_or_default()) } else { ids(text) },
        "worlds":note["worlds"].as_array().cloned().unwrap_or_default(),
        "areas":note["areas"].as_array().cloned().unwrap_or_default(),"kind":kind,"status":"open"}));
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

fn prefix_kind(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("next:") || lower.starts_with("todo:") {
        Some("todo")
    } else if lower.starts_with("pending:") || lower.starts_with("waiting on:") {
        Some("waiting")
    } else if lower.starts_with("blocked:") {
        Some("blocked")
    } else if lower.starts_with("open question:") || lower.starts_with("open questions:") {
        Some("question")
    } else if lower.starts_with("review:") {
        Some("waiting")
    } else {
        None
    }
}

fn extract(note: &Value, out: &mut Vec<Value>) {
    let path = note["path"].as_str().unwrap_or_default();
    if path.starts_with("packs/") || path.starts_with("digests/") {
        return;
    }
    let wiki = path.starts_with("wiki/");
    if let Some(review) = note["meta"]["review"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
    {
        add_loop(out, note, review, "waiting", 0);
    }
    let mut open_questions = false;
    for (n, raw) in content_lines(note).into_iter().enumerate() {
        let line = raw.as_str();
        if line.starts_with('#') {
            open_questions = wiki
                && line
                    .trim_start_matches('#')
                    .trim()
                    .eq_ignore_ascii_case("open questions");
            continue;
        }
        if line
            .get(..5)
            .is_some_and(|s| s.eq_ignore_ascii_case("- [ ]"))
        {
            let item = &line[5..];
            add_loop(out, note, item, "todo", n + 1);
        } else if line
            .get(..5)
            .is_some_and(|s| s.eq_ignore_ascii_case("- [x]"))
        {
            continue;
        } else if open_questions && (line.starts_with('-') || line.starts_with('*')) {
            add_loop(out, note, line, "question", n + 1);
        } else {
            let kind = prefix_kind(line);
            if let Some(kind) = kind {
                add_loop(
                    out,
                    note,
                    line.split_once(':').map_or(line, |(_, v)| v),
                    kind,
                    n + 1,
                );
            }
        }
    }
}

fn close_loops(loops: &mut [Value], notes: &[&Value]) {
    for loop_item in loops.iter_mut() {
        let loop_text = loop_item["text"].as_str().unwrap_or_default();
        let mut loop_units = ids(loop_text);
        loop_units.extend(
            loop_item["unitIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
        for note in notes {
            let path = note["path"].as_str().unwrap_or_default();
            if path.starts_with("packs/") || path.starts_with("digests/") {
                continue;
            }
            let note_date = date(note);
            if !later(note, loop_item) || !same_scope(loop_item, note) {
                continue;
            }
            let mut evidence = None;
            let title = note["label"].as_str().unwrap_or_default();
            let completion_lines = std::iter::once(title.to_owned()).chain(content_lines(note));
            for raw in completion_lines {
                let line = raw.as_str();
                let lower = line.to_ascii_lowercase();
                if prefix_kind(line).is_some() || line.starts_with("- [ ]") {
                    continue;
                }
                let close_units = ids(line);
                let units_conflict = !loop_units.is_empty()
                    && !close_units.is_empty()
                    && !loop_units.iter().any(|u| close_units.contains(u));
                if units_conflict || NEGATED_DONE.is_match(line) {
                    continue;
                }
                let checked = line
                    .get(..5)
                    .is_some_and(|s| s.eq_ignore_ascii_case("- [x]"));
                if checked
                    && (overlap(loop_text, line)
                        || line.get(5..).is_some_and(|payload| {
                            payload.trim().eq_ignore_ascii_case(loop_text.trim())
                        }))
                {
                    evidence = Some(line.trim().to_owned());
                    break;
                }
                let done = ["done", "merged", "fixed", "resolved", "landed", "completed"]
                    .iter()
                    .any(|term| {
                        lower
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|word| word == *term)
                    });
                if !done {
                    continue;
                }
                if !loop_units.is_empty() && loop_units.iter().any(|u| close_units.contains(u)) {
                    evidence = Some(line.trim().to_owned());
                    break;
                }
                if close_units.is_empty()
                    && same_text_scope(loop_item, note)
                    && overlap(loop_text, line)
                {
                    evidence = Some(line.trim().to_owned());
                    break;
                }
            }
            let supersedes = note["meta"]["supersedes"].as_str().unwrap_or("");
            let target = loop_item["path"].as_str().unwrap_or("");
            let target_stem = target
                .trim_end_matches(".md")
                .rsplit('/')
                .next()
                .unwrap_or(target);
            let supersedes_target = supersedes.split([',', ';', '\n']).any(|entry| {
                let entry = entry
                    .trim()
                    .trim_matches(['[', ']'])
                    .split('|')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches("[[")
                    .trim_end_matches("]]")
                    .trim_end_matches(".md");
                entry == target.trim_end_matches(".md")
                    || entry.rsplit('/').next() == Some(target_stem)
            }) && !target_stem.is_empty();
            if evidence.is_none() && supersedes_target {
                evidence = Some(format!(
                    "Superseded by {}",
                    note["path"].as_str().unwrap_or("later note")
                ));
            }
            if let Some(evidence) = evidence {
                loop_item["status"] = json!("closed");
                loop_item["closingEvidence"] = json!({"source":note["id"],"path":note["path"],"date":note_date,"text":evidence});
                break;
            }
        }
    }
}

/// Derive loops and a day-grouped timeline from indexed note values and review items.
pub fn derive(notes: &[Value], review: &Value) -> Value {
    let mut loops = Vec::new();
    let mut ordered_notes: Vec<&Value> = notes.iter().collect();
    ordered_notes.sort_by_key(|n| {
        (
            date(n),
            recorded(n)
                .map(|d| d.timestamp_millis())
                .unwrap_or_default(),
            n["path"].as_str().unwrap_or_default().to_owned(),
        )
    });
    for note in &ordered_notes {
        extract(note, &mut loops);
    }
    for (i, item) in review["items"].as_array().into_iter().flatten().enumerate() {
        let text = item["text"]
            .as_str()
            .or_else(|| item["reason"].as_str())
            .or_else(|| item["message"].as_str())
            .or_else(|| item["kind"].as_str())
            .unwrap_or_default();
        if text.is_empty() {
            continue;
        }
        let item_path = item["path"]
            .as_str()
            .or_else(|| {
                item["paths"]
                    .as_array()
                    .and_then(|p| p.first())
                    .and_then(Value::as_str)
            })
            .unwrap_or("");
        let source_note = notes
            .iter()
            .find(|n| n["path"].as_str() == Some(item_path))
            .or_else(|| {
                let area = item["area"].as_str();
                let world = item["world"].as_str();
                notes
                    .iter()
                    .filter(|note| {
                        area.is_some_and(|a| {
                            note["areas"]
                                .as_array()
                                .is_some_and(|v| v.iter().any(|x| x.as_str() == Some(a)))
                        }) || (area.is_none()
                            && world.is_some_and(|w| {
                                note["worlds"]
                                    .as_array()
                                    .is_some_and(|v| v.iter().any(|x| x.as_str() == Some(w)))
                            }))
                    })
                    .max_by_key(|note| {
                        (
                            date(note),
                            note["path"].as_str().unwrap_or_default().to_owned(),
                        )
                    })
            });
        let Some(source_note) = source_note else {
            continue;
        };
        let mut summary = item["kind"].as_str().unwrap_or("review").to_owned();
        if let Some(area) = item["area"].as_str() {
            summary.push_str(&format!(" for {area}"));
        }
        if let Some(world) = item["world"].as_str() {
            summary.push_str(&format!(" in {world}"));
        }
        if let Some(reason) = item["reason"].as_str() {
            summary.push_str(&format!(": {reason}"));
        } else {
            summary.push_str(&format!(": {text}"));
        }
        add_loop(
            &mut loops,
            source_note,
            item["text"].as_str().unwrap_or(&summary),
            "waiting",
            i + 1,
        );
    }
    close_loops(&mut loops, &ordered_notes);
    loops.sort_by(|a, b| {
        (b["date"].as_str(), a["id"].as_str()).cmp(&(a["date"].as_str(), b["id"].as_str()))
    });
    let mut days: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for note in ordered_notes.iter().rev() {
        if note["path"]
            .as_str()
            .is_some_and(|p| p.starts_with("packs/"))
        {
            continue;
        }
        let day = date(note);
        if day.is_empty() {
            continue;
        }
        let title = note["label"].as_str().unwrap_or_default();
        let digest = digest_line(note);
        let body = note["text"].as_str().unwrap_or_default();
        days.entry(day.clone()).or_default().push(json!({"id":note["id"],"title":title,"digest":digest.chars().take(180).collect::<String>(),"source":note["id"],"path":note["path"],"worlds":note["worlds"],"areas":note["areas"],"unitIds":ids(&format!("{title} {body}"))}));
    }
    let timeline = days
        .into_iter()
        .rev()
        .map(|(day, events)| {
            let week = NaiveDate::parse_from_str(&day, "%Y-%m-%d")
                .ok()
                .map(|d| format!("{}-W{:02}", d.iso_week().year(), d.iso_week().week()))
                .unwrap_or_default();
            json!({"date":day,"week":week,"events":events})
        })
        .collect::<Vec<_>>();
    json!({"schema":1,"loops":loops,"timeline":timeline})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, date: &str, text: &str, areas: &[&str], worlds: &[&str]) -> Value {
        let label = text
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .unwrap_or(path);
        json!({"id":path,"path":path,"label":label,"date":date,"text":text,
            "areas":areas,"worlds":worlds,"meta":{}})
    }

    #[test]
    fn extracts_loop_kinds_and_curated_review_items() {
        let page = note("wiki/area-a.md", "2026-11-01", "# Area\n\n<!-- brain:curated:start (curated 2026-11-01) -->\nSummary.\n\n### Open questions\n\n- Why keep this?\n<!-- brain:curated:end -->", &["Area A"], &["World A"]);
        let mut body = note("inbox/item.md", "2026-11-02", "- [ ] Implement the migration\nNext: update migration tests\nTODO: confirm compiler behavior\nPending: release review\nBlocked: missing fixture\nWaiting on: owner response\nOpen question: which API?", &["Area A"], &["World A"]);
        body["meta"]["review"] = json!("low-confidence item");
        let review = json!({"items":[{"kind":"stale-fact","path":"inbox/item.md","text":"Recheck compiler fact"}]});
        let result = derive(&[page, body], &review);
        let loops = result["loops"].as_array().unwrap();
        for kind in ["todo", "question", "waiting", "blocked"] {
            assert!(
                loops.iter().any(|item| item["kind"] == kind),
                "missing {kind}"
            );
        }
        assert!(loops
            .iter()
            .any(|item| item["text"] == "Recheck compiler fact"));
        let scoped = derive(
            &[note(
                "wiki/area-a.md",
                "2026-11-03",
                "# Area A\n\nArea notes",
                &["Area A"],
                &["World A"],
            )],
            &json!({"items":[{"kind":"taxonomy-split","area":"Area A","world":"World A","reason":"crossing notes"}]}),
        );
        assert_eq!(scoped["loops"][0]["path"], "wiki/area-a.md");
        assert!(scoped["loops"][0]["text"]
            .as_str()
            .unwrap()
            .contains("crossing notes"));
    }

    #[test]
    fn closes_unit_ids_and_checked_items_but_vetoes_false_matches() {
        for id in ["N0123", "B0227", "NN0021", "PR 1543", "abcdef123456"] {
            let first = note(
                "notes/open.md",
                "2026-11-01",
                &format!("TODO: {id} implement parser fixture"),
                &["Area A"],
                &["World A"],
            );
            let later = note(
                "notes/done.md",
                "2026-11-02",
                &format!("{id} parser fixture completed"),
                &["Area A"],
                &["World A"],
            );
            let result = derive(&[first, later], &json!({}));
            assert_eq!(result["loops"][0]["status"], "closed", "{id}");
        }
        let mut first = note(
            "notes/open.md",
            "2026-11-01",
            "TODO: N0123 implement parser fixture",
            &["Area A"],
            &["World A"],
        );
        first["label"] = json!("N0456 related work");
        let different_id = note(
            "notes/done.md",
            "2026-11-02",
            "N0456 parser fixture completed",
            &["Area A"],
            &["World A"],
        );
        let other_area = note(
            "notes/other.md",
            "2026-11-03",
            "N0123 parser fixture completed",
            &["Area B"],
            &["World A"],
        );
        let negation = note(
            "notes/no.md",
            "2026-11-04",
            "N0123 parser fixture not done",
            &["Area A"],
            &["World A"],
        );
        let short_unrelated = note(
            "notes/short.md",
            "2026-11-05",
            "- [x] go",
            &["Area A"],
            &["World A"],
        );
        let other_task = note(
            "notes/other-task.md",
            "2026-11-06",
            "- [x] N0123 update documentation",
            &["Area A"],
            &["World A"],
        );
        let result = derive(
            &[
                first,
                different_id,
                other_area,
                negation,
                short_unrelated,
                other_task,
            ],
            &json!({}),
        );
        assert_eq!(result["loops"][0]["status"], "open");
    }

    #[test]
    fn derived_text_and_pending_tasks_do_not_report_completion() {
        let first = note(
            "notes/open.md",
            "2026-11-01",
            "TODO: N0123 verify parser",
            &["Area A"],
            &["World A"],
        );
        for (path, body) in [
            ("packs/pack-a.md", "N0123 completed"),
            ("digests/2026-11.md", "N0123 completed"),
            ("notes/next.md", "TODO: Mark N0123 completed after acceptance"),
            ("notes/code.md", "```\nN0123 completed\n```\n<!-- brain:generated:start -->\nN0123 completed\n<!-- brain:generated:end -->"),
        ] {
            let update = note(path, "2026-11-02", body, &["Area A"], &["World A"]);
            let data = derive(&[first.clone(), update], &json!({}));
            let original = data["loops"].as_array().unwrap().iter().find(|l| l["path"] == "notes/open.md").unwrap();
            assert_eq!(original["status"], "open", "{path}");
        }
    }

    #[test]
    fn closes_exact_checkbox_and_superseding_reference() {
        let first = note(
            "notes/open.md",
            "2026-11-01",
            "- [ ] N0123 build index",
            &["Area A"],
            &["World A"],
        );
        let later = note(
            "notes/done.md",
            "2026-11-02",
            "- [x] N0123 build index",
            &["Area A"],
            &["World A"],
        );
        assert_eq!(
            derive(&[first.clone(), later], &json!({}))["loops"][0]["status"],
            "closed"
        );
        let mut replacement = note(
            "notes/replacement.md",
            "2026-11-02",
            "Supersedes old note",
            &["Area A"],
            &["World A"],
        );
        replacement["meta"]["supersedes"] = json!("[[open|old note]]");
        assert_eq!(
            derive(&[first, replacement], &json!({}))["loops"][0]["status"],
            "closed"
        );
    }

    #[test]
    fn uses_unit_ids_from_note_titles() {
        let first = note(
            "notes/open.md",
            "2026-11-01",
            "# N0123 parser work\n\nRecorded: 2026-11-01T10:00:00Z\n\nNext: Run acceptance",
            &["Area A"],
            &["World A"],
        );
        let later = note(
            "notes/done.md",
            "2026-11-02",
            "# N0123 completed\n\nRecorded: 2026-11-02T10:00:00Z\n\nAcceptance passed",
            &["Area A"],
            &["World A"],
        );
        assert_eq!(
            derive(&[first, later], &json!({}))["loops"][0]["status"],
            "closed"
        );
    }

    #[test]
    fn closes_high_overlap_and_same_day_later_timestamp() {
        let mut first = note(
            "notes/open.md",
            "2026-11-01",
            "# Task\n\nRecorded: 2026-11-01T10:00:00Z\n\nTODO: parser fixture migration",
            &["Area A"],
            &["World A"],
        );
        first["meta"]["recorded"] = json!("2026-11-01T10:00:00Z");
        let mut later = note(
            "notes/done.md",
            "2026-11-01",
            "# Task update\n\nRecorded: 2026-11-01T12:00:00Z\n\nParser fixture migration is fixed",
            &["Area A"],
            &["World A"],
        );
        later["meta"]["recorded"] = json!("2026-11-01T12:00:00Z");
        assert_eq!(
            derive(&[first, later], &json!({}))["loops"][0]["status"],
            "closed"
        );
    }

    #[test]
    fn compares_recorded_offsets_before_calendar_dates() {
        let mut first = note(
            "notes/open.md",
            "2026-11-02",
            "TODO: N0123 verify parser",
            &["Area A"],
            &["World A"],
        );
        first["meta"]["recorded"] = json!("2026-11-02T00:30:00+02:00");
        let mut update = note(
            "notes/done.md",
            "2026-11-01",
            "N0123 completed",
            &["Area A"],
            &["World A"],
        );
        update["meta"]["recorded"] = json!("2026-11-01T23:30:00+00:00");
        assert_eq!(
            derive(&[first, update], &json!({}))["loops"][0]["status"],
            "closed"
        );
    }

    #[test]
    fn review_sources_are_independent_of_input_order() {
        let a = note(
            "notes/a.md",
            "2026-11-01",
            "First event",
            &["Area A"],
            &["World A"],
        );
        let b = note(
            "notes/b.md",
            "2026-11-01",
            "Second event",
            &["Area A"],
            &["World A"],
        );
        let review =
            json!({"items":[{"kind":"taxonomy-split","area":"Area A","reason":"mixed evidence"}]});
        assert_eq!(
            derive(&[a.clone(), b.clone()], &review),
            derive(&[b, a], &review)
        );
    }

    #[test]
    fn timeline_groups_days_and_iso_weeks() {
        let notes = [
            note(
                "a.md",
                "2021-01-01",
                "# First\n\nRecorded: 2021-01-01\n\nFirst event",
                &[],
                &[],
            ),
            note(
                "b.md",
                "2021-01-02",
                "# Second\n\nRecorded: 2021-01-02\n\nSecond event",
                &[],
                &[],
            ),
        ];
        let timeline = derive(&notes, &json!({}))["timeline"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(timeline[0]["date"], "2021-01-02");
        assert_eq!(timeline[1]["week"], "2020-W53");
        assert_eq!(timeline[1]["events"][0]["digest"], "First event");
    }
}
