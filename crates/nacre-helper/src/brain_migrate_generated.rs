//! Deterministic digest, wiki, and index generation for Brain migration.
use super::{field_lines as fields, header_body as parts, lists as list};
use crate::Result;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const START: &str =
    "<!-- brain:generated:start (rebuilt by the index job; edit above this line) -->";
const END: &str = "<!-- brain:generated:end -->";

#[derive(Clone)]
struct Evidence {
    path: String,
    title: String,
    body: String,
    date: String,
    worlds: Vec<String>,
    areas: Vec<String>,
    review: Option<String>,
}

fn slug(value: &str) -> String {
    value
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn field(fields: &[(String, String)], name: &str) -> Option<String> {
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

fn evidence(notes: &BTreeMap<String, Vec<u8>>) -> Result<Vec<Evidence>> {
    notes
        .iter()
        .filter(|(path, _)| {
            let Some((root, _)) = path.split_once('/') else {
                return false;
            };
            [
                "inbox",
                "checkpoints",
                "decisions",
                "runbooks",
                "projects",
                "devices",
                "personal",
                "infrastructure",
                "archive",
                "raw",
            ]
            .contains(&root)
                && path.ends_with(".md")
        })
        .map(|(path, bytes)| {
            let text = std::str::from_utf8(bytes)?;
            let (title, header, body) = parts(text);
            let f = fields(header);
            Ok(Evidence {
                path: path.clone(),
                title: title.trim_start_matches("# ").to_owned(),
                body: body.to_owned(),
                date: super::date_from(path, text)
                    .map(|date| date.to_string())
                    .unwrap_or_default(),
                worlds: field(&f, "Worlds").map(|v| list(&v)).unwrap_or_default(),
                areas: field(&f, "Areas").map(|v| list(&v)).unwrap_or_default(),
                review: field(&f, "Review"),
            })
        })
        .collect()
}

fn set_generated(text: &str, block: &str) -> Result<String> {
    if let Some(start) = text.find("<!-- brain:generated:start") {
        let line_end = text[start..]
            .find('\n')
            .map(|n| start + n + 1)
            .unwrap_or_else(|| text.len());
        let end = text[line_end..]
            .find(END)
            .map(|n| line_end + n)
            .ok_or("wiki generated block has no end marker")?;
        Ok(format!(
            "{}{}\n{}",
            &text[..line_end],
            block.trim_end(),
            &text[end..]
        ))
    } else {
        let separator = if text.ends_with('\n') { "\n" } else { "\n\n" };
        Ok(format!(
            "{text}{separator}{START}\n{}\n{END}\n",
            block.trim_end()
        ))
    }
}

fn rewrite_wiki_header(text: &str, values: &[(&str, String)]) -> String {
    let (title, header, body) = parts(text);
    let managed = [
        "Entity", "Name", "Aliases", "Parent", "Redirect", "Worlds", "Areas", "Review",
    ];
    let kept = header
        .lines()
        .filter(|line| {
            line.split_once(':')
                .is_none_or(|(key, _)| !managed.contains(&key.trim()))
        })
        .collect::<Vec<_>>();
    let mut out = title.to_owned();
    for (key, value) in values {
        out.push_str(&format!("\n{key}: {value}"));
    }
    if !kept.is_empty() {
        out.push('\n');
        out.push_str(&kept.join("\n"));
    }
    out.push_str("\n\n");
    out.push_str(body);
    out
}

type Role = (String, String, String, Vec<String>, Option<String>);

fn roles(vocab: &Value, registry: &Value) -> Vec<Role> {
    let mut out = Vec::new();
    for world in vocab["worlds"].as_array().into_iter().flatten() {
        let name = world["name"].as_str().unwrap_or_default();
        let page = world["page"].as_str().unwrap_or("wiki/world.md");
        out.push((
            page.to_owned(),
            "world".into(),
            name.into(),
            strings(&world["aliases"]),
            None,
        ));
        for area in world["areas"].as_array().into_iter().flatten() {
            let name = area["name"].as_str().unwrap_or_default();
            let page = area["page"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("wiki/area-{}.md", slug(name)));
            out.push((
                page,
                "area".into(),
                name.into(),
                strings(&area["aliases"]),
                Some(world["name"].as_str().unwrap_or_default().into()),
            ));
        }
    }
    for entity in vocab["entities"].as_array().into_iter().flatten() {
        out.push((
            entity["page"].as_str().unwrap_or_default().into(),
            entity["kind"].as_str().unwrap_or("entity").into(),
            entity["name"].as_str().unwrap_or_default().into(),
            strings(&entity["aliases"]),
            entity["area"].as_str().map(str::to_owned),
        ));
    }
    if let Some(redirects) = vocab["redirects"].as_object() {
        for (page, target) in redirects {
            out.push((
                page.clone(),
                "alias".into(),
                String::new(),
                Vec::new(),
                target.as_str().map(str::to_owned),
            ));
        }
    }
    for world in registry["worlds"].as_array().into_iter().flatten() {
        let name = world["name"].as_str().unwrap_or_default();
        if world["status"].as_str() == Some("merged-into") {
            continue;
        }
        if !out
            .iter()
            .any(|(_, kind, label, _, _)| kind == "world" && label == name)
        {
            out.push((
                format!("wiki/world-{}.md", slug(name)),
                "world".into(),
                name.into(),
                strings(&world["aliases"]),
                None,
            ));
        }
    }
    for area in registry["areas"].as_array().into_iter().flatten() {
        let name = area["name"].as_str().unwrap_or_default();
        if area["status"].as_str() == Some("merged-into") {
            continue;
        }
        if !out
            .iter()
            .any(|(_, kind, label, _, _)| kind == "area" && label == name)
        {
            out.push((
                format!("wiki/area-{}.md", slug(name)),
                "area".into(),
                name.into(),
                strings(&area["aliases"]),
                area["parent"].as_str().map(str::to_owned),
            ));
        }
    }
    out
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn make_digests(
    notes: &BTreeMap<String, Vec<u8>>,
    registry: &Value,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let ev = evidence(notes)?;
    let mut groups: BTreeMap<(String, String, String), Vec<&Evidence>> = BTreeMap::new();
    for item in &ev {
        let checkpoint = item.path.starts_with("archive/checkpoints/");
        let inbox = item.path.starts_with("archive/inbox/");
        if !checkpoint && !inbox {
            continue;
        }
        let month = item.date.get(..7).unwrap_or("");
        if month.len() != 7 {
            continue;
        }
        let unresolved_inbox = inbox && item.review.is_some();
        let world = if unresolved_inbox {
            String::new()
        } else {
            item.worlds.first().cloned().unwrap_or_default()
        };
        let area = if unresolved_inbox {
            String::new()
        } else {
            item.areas
                .iter()
                .find(|name| {
                    registry["areas"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|area| {
                            area["name"].as_str() == Some(name.as_str())
                                && area["parent"].as_str() == Some(world.as_str())
                        })
                })
                .cloned()
                .unwrap_or_default()
        };
        groups
            .entry((month.into(), world, area))
            .or_default()
            .push(item);
    }
    let mut out = BTreeMap::new();
    for ((month, world, area), mut items) in groups {
        items.sort_by(|a, b| (&a.date, &a.path).cmp(&(&b.date, &b.path)));
        let name = if world.is_empty() {
            "review-unsorted".into()
        } else {
            slug(&world)
        };
        let name = if area.is_empty() {
            name
        } else {
            format!("{name}-{}", slug(&area))
        };
        let page = format!("digests/{month}/{name}.md");
        let label = if world.is_empty() {
            "Unsorted (review queue)".to_owned()
        } else if area.is_empty() {
            world.clone()
        } else {
            format!("{world} › {area}")
        };
        let mut text = format!("# Digest {month}: {label}\n\nRecorded: {}\nConfidence: verified\nSource: generated from {} archived notes in archive/checkpoints/{month}/ and archive/inbox/{month}/ (mechanical digest; durable facts still to be rolled into the wiki)\n", items.last().map(|x|x.date.as_str()).unwrap_or(month.as_str()), items.len());
        if world.is_empty() {
            text.push_str("Review: unsorted\n");
        } else {
            text.push_str(&format!("Worlds: {world}\n"));
        }
        if !area.is_empty() {
            text.push_str(&format!("Areas: {area}\n"));
        }
        text.push_str(&format!(
            "Kind: digest\n\n{} archived notes from {} to {}. Each line links the original.\n\n",
            items.len(),
            items.first().unwrap().date,
            items.last().unwrap().date
        ));
        for item in items {
            let first = plain_excerpt(item.body.trim());
            let first = first.split(". ").next().unwrap_or(&first);
            text.push_str(&format!(
                "- {} [[{}|{}]]: {}\n",
                item.date,
                item.path.trim_end_matches(".md"),
                item.title.chars().take(100).collect::<String>(),
                first.chars().take(220).collect::<String>()
            ));
        }
        if notes.get(&page).is_none_or(|old| old != text.as_bytes()) {
            out.insert(page, text.into_bytes());
        }
    }
    Ok(out)
}

fn plain_excerpt(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("[[") {
            if let Some(end) = after.find("]]") {
                let label = &after[..end];
                out.push_str(label.rsplit_once('|').map_or(label, |(_, label)| label));
                rest = &after[end + 2..];
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('[') {
            if let Some(label_end) = after.find(']') {
                if let Some(link) = after[label_end + 1..].strip_prefix('(') {
                    if let Some(link_end) = link.find(')') {
                        out.push_str(&after[..label_end]);
                        rest = &link[link_end + 1..];
                        continue;
                    }
                }
            }
        }
        out.push('[');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn generate_wiki(
    notes: &BTreeMap<String, Vec<u8>>,
    registry: &Value,
    vocab: &Value,
    today: &str,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let ev = evidence(notes)?;
    let role_list = roles(vocab, registry);
    let mut out = BTreeMap::new();
    for (path, kind, name, aliases, parent) in &role_list {
        if path.is_empty() {
            continue;
        }
        let old = notes
            .get(path)
            .map(|b| std::str::from_utf8(b))
            .transpose()?;
        if kind == "alias" {
            if let Some(old) = old {
                let existing_name = field(&fields(parts(old).1), "Name").unwrap_or_default();
                let mut vals = vec![
                    ("Entity", "alias".into()),
                    ("Name", existing_name),
                    ("Redirect", parent.clone().unwrap_or_default()),
                ];
                add_taxonomy(&mut vals, parent.as_deref().unwrap_or(""), registry);
                let updated = rewrite_wiki_header(old, &vals);
                if updated != old {
                    out.insert(path.clone(), updated.into_bytes());
                }
            }
            continue;
        }
        let existing_fields = old.map(|text| fields(parts(text).1)).unwrap_or_default();
        let mut all_aliases = field(&existing_fields, "Aliases")
            .map(|v| list(&v))
            .unwrap_or_default();
        if let Some(old_name) = field(&existing_fields, "Name") {
            if old_name != *name {
                all_aliases.push(old_name);
            }
        }
        all_aliases.extend(aliases.iter().filter(|x| *x != name).cloned());
        let mut seen = BTreeSet::new();
        all_aliases.retain(|x| seen.insert(x.to_lowercase()));
        let mut vals = vec![("Entity", kind.clone()), ("Name", name.clone())];
        if !all_aliases.is_empty() {
            vals.push(("Aliases", all_aliases.join(", ")));
        }
        if let Some(parent) = parent {
            vals.push(("Parent", parent.clone()));
        }
        if kind == "area" {
            add_taxonomy(&mut vals, parent.as_deref().unwrap_or(""), registry);
            vals.push(("Areas", name.clone()));
        } else {
            add_taxonomy(
                &mut vals,
                if kind == "world" {
                    name
                } else {
                    parent.as_deref().unwrap_or("")
                },
                registry,
            );
        }
        let mut text = if let Some(old) = old {
            old.to_owned()
        } else {
            let label = if parent.is_some() { "Area" } else { "World" };
            let description = vocab["worlds"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|world| world["name"].as_str() == Some(name.as_str()))
                .and_then(|world| world["description"].as_str())
                .unwrap_or("");
            let description = if description.is_empty() {
                String::new()
            } else {
                format!(" {description}")
            };
            format!("# {name}\n\nReviewed: {today}\nStatus: Generated stub; current facts not yet curated\nSource: brain-overhaul vocabulary.json (migration {today})\n\n{label}{}.{description}\n\n## Current facts\n\nNot yet curated. The consolidation job fills this from the evidence below.\n", parent.as_ref().map(|p|format!(" in {p}")).unwrap_or_default())
        };
        text = rewrite_wiki_header(&text, &vals);
        let items: Vec<&Evidence> = ev
            .iter()
            .filter(|item| {
                if kind == "world" {
                    item.worlds.iter().any(|x| x == name)
                } else if kind == "area" {
                    item.areas.iter().any(|x| x == name)
                } else {
                    let hay = format!(
                        "{} {}",
                        item.title,
                        item.body
                            .chars()
                            .take(3000)
                            .collect::<String>()
                            .to_lowercase()
                    );
                    std::iter::once(name).chain(aliases.iter()).any(|needle| {
                        !needle.is_empty() && hay.to_lowercase().contains(&needle.to_lowercase())
                    })
                }
            })
            .collect();
        let mut sorted = items;
        sorted.sort_by(|a, b| (&b.date, &b.path).cmp(&(&a.date, &a.path)));
        let mut block = format!(
            "## Evidence index (generated {})\n\n{} evidence notes.\n\n### Latest evidence\n",
            evidence_day(&ev),
            sorted.len()
        );
        if kind == "world" || kind == "area" {
            let children = if kind == "world" {
                role_list
                    .iter()
                    .filter(|(_, child_kind, _, _, child_parent)| {
                        child_kind == "area" && child_parent.as_deref() == Some(name.as_str())
                    })
                    .collect::<Vec<_>>()
            } else {
                role_list
                    .iter()
                    .filter(|(_, child_kind, _, _, child_parent)| {
                        child_kind != "world"
                            && child_kind != "area"
                            && child_parent.as_deref() == Some(name.as_str())
                    })
                    .collect::<Vec<_>>()
            };
            if !children.is_empty() {
                block.push_str("### Sub-pages\n\n");
                for (child_page, _, child_name, _, _) in children {
                    let count = if kind == "world" {
                        ev.iter().filter(|x| x.areas.contains(child_name)).count()
                    } else {
                        ev.iter()
                            .filter(|x| {
                                format!("{} {}", x.title, x.body)
                                    .to_lowercase()
                                    .contains(&child_name.to_lowercase())
                            })
                            .count()
                    };
                    block.push_str(&format!(
                        "- [[{}|{}]] · {count}\n",
                        child_page.trim_end_matches(".md"),
                        child_name
                    ));
                }
                block.push('\n');
            }
            let digest_paths = notes
                .iter()
                .filter(|(digest_path, bytes)| {
                    digest_path.starts_with("digests/")
                        && std::str::from_utf8(bytes).is_ok_and(|text| {
                            let f = fields(parts(text).1);
                            field(&f, "Worlds").is_some_and(|v| list(&v).contains(name))
                                || field(&f, "Areas").is_some_and(|v| list(&v).contains(name))
                        })
                })
                .map(|(p, _)| p)
                .collect::<Vec<_>>();
            if !digest_paths.is_empty() {
                block.push_str("### Monthly digests\n\n");
                for digest in digest_paths {
                    block.push_str(&format!("- [[{}]]\n", digest.trim_end_matches(".md")));
                }
                block.push('\n');
            }
        }
        for item in sorted.iter().take(15) {
            block.push_str(&format!(
                "\n- {} [[{}|{}]]",
                if item.date.is_empty() {
                    "?"
                } else {
                    &item.date
                },
                item.path.trim_end_matches(".md"),
                item.title.chars().take(90).collect::<String>()
            ));
        }
        text = set_generated(&text, &block)?;
        if old.is_none_or(|before| before != text) {
            out.insert(path.clone(), text.into_bytes());
        }
    }
    Ok(out)
}

fn add_taxonomy(values: &mut Vec<(&'static str, String)>, label: &str, registry: &Value) {
    if registry["worlds"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|entry| entry["name"].as_str() == Some(label))
    {
        values.push(("Worlds", label.to_owned()));
        return;
    }
    if let Some(area) = registry["areas"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["name"].as_str() == Some(label))
    {
        let world = area["parent"].as_str().unwrap_or("");
        if !world.is_empty() {
            values.push(("Worlds", world.to_owned()));
        }
        values.push(("Areas", label.to_owned()));
        return;
    }
    values.push(("Review", "unknown-world".into()));
}

fn evidence_day(items: &[Evidence]) -> &str {
    items
        .iter()
        .map(|x| x.date.as_str())
        .filter(|d| !d.is_empty())
        .max()
        .unwrap_or("unknown")
}

fn make_index(
    notes: &BTreeMap<String, Vec<u8>>,
    registry: &Value,
    vocab: &Value,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let ev = evidence(notes)?;
    let review_count = ev.iter().filter(|item| item.review.is_some()).count();
    let mut text = String::from("# Nacre Brain\n\nStart with current knowledge, then follow the evidence. Generated from the controlled vocabulary; edit wiki pages or the vocabulary, not this list.\n\nRead order for agents: this index -> the world/area page -> its context pack -> the linked evidence (dated notes, digests, archived checkpoints).\n\n");
    let all_roles = roles(vocab, registry);
    for world in registry["worlds"].as_array().into_iter().flatten() {
        if world["status"].as_str() == Some("merged-into") {
            continue;
        }
        let name = world["name"].as_str().unwrap_or_default();
        let Some((page, _, _, _, _)) = all_roles
            .iter()
            .find(|(_, kind, label, _, _)| kind == "world" && label == name)
        else {
            continue;
        };
        let count = ev
            .iter()
            .filter(|item| item.worlds.iter().any(|w| w == name))
            .count();
        text.push_str(&format!("## [{name}]({page}) · {count} notes\n\n"));
        if let Some(desc) = vocab["worlds"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|item| item["name"].as_str() == Some(name))
            .and_then(|item| item["description"].as_str())
        {
            text.push_str(desc);
            text.push_str("\n\n");
        }
        for area in registry["areas"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|area| {
                area["parent"].as_str() == Some(name)
                    && area["status"].as_str() != Some("merged-into")
            })
        {
            let area_name = area["name"].as_str().unwrap_or_default();
            let Some((area_page, _, _, _, _)) = all_roles
                .iter()
                .find(|(_, kind, label, _, _)| kind == "area" && label == area_name)
            else {
                continue;
            };
            let area_count = ev
                .iter()
                .filter(|item| item.areas.iter().any(|a| a == area_name))
                .count();
            let entities = all_roles
                .iter()
                .filter(|(_, kind, _, _, parent)| {
                    kind != "world"
                        && kind != "area"
                        && kind != "alias"
                        && parent.as_deref() == Some(area_name)
                })
                .map(|(entity_page, _, entity_name, _, _)| {
                    format!("[{entity_name}]({entity_page})")
                })
                .collect::<Vec<_>>();
            let tail = if entities.is_empty() {
                String::new()
            } else {
                format!(" · {}", entities.join(", "))
            };
            text.push_str(&format!(
                "- [{area_name}]({area_page}) · {area_count}{tail}\n"
            ));
        }
        text.push('\n');
    }
    text.push_str(&format!("\n## Review queue\n\n{review_count} notes carry a Review flag.\n\n## Layers\n\n- wiki/: current truth per world, area and entity.\n- digests/YYYY-MM/: monthly summaries of archived checkpoints.\n- archive/checkpoints/YYYY-MM/: original task handoffs.\n- checkpoints/, inbox/: recent handoffs and untriaged facts.\n- decisions/, runbooks/, projects/, devices/, infrastructure/, personal/: dated evidence.\n\nDated notes preserve observations. Current pages synthesize them; they do not prove a device is online or authorize an action. raw/ holds attributed source extracts.\n"));
    if notes
        .get("INDEX.md")
        .is_some_and(|old| old.as_slice() == text.as_bytes())
    {
        return Ok(BTreeMap::new());
    }
    Ok(BTreeMap::from([("INDEX.md".into(), text.into_bytes())]))
}

/// Return generated file contents that differ from the virtual vault snapshot.
pub(super) fn generate(
    step: &str,
    notes: &BTreeMap<String, Vec<u8>>,
    registry: &Value,
    vocabulary_json: &Value,
    today: &str,
) -> Result<BTreeMap<String, Vec<u8>>> {
    match step {
        "digests" => make_digests(notes, registry),
        "wiki" => generate_wiki(notes, registry, vocabulary_json, today),
        "index" => make_index(notes, registry, vocabulary_json),
        _ => Ok(BTreeMap::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry() -> Value {
        json!({
            "version": 1,
            "worlds": [{"name":"Nacre", "status":"seed", "aliases":[]}],
            "areas": [
                {"name":"Helper", "parent":"Nacre", "status":"seed", "aliases":[]},
                {"name":"New Area", "parent":"Nacre", "status":"proposed", "aliases":[]}
            ]
        })
    }

    fn vocabulary() -> Value {
        json!({"worlds":[{"name":"Nacre","page":"wiki/nacre.md","description":"Synthetic world.","aliases":[],"areas":[{"name":"Helper","page":"wiki/helper.md","aliases":[]}]}],"entities":[],"redirects":{}})
    }

    fn note(path: &str, text: &str) -> (String, Vec<u8>) {
        (path.to_owned(), text.as_bytes().to_vec())
    }

    #[test]
    fn legacy_header_parser_keeps_metadata_after_title_blank() {
        let text = "# Legacy synthetic note\n\nRecorded: 2020-01-01\nWorlds: Nacre\nTopics:\n\nBody starts here.\n";
        let (title, header, body) = parts(text);
        assert_eq!(title, "# Legacy synthetic note");
        assert!(header.contains("Worlds: Nacre"));
        assert_eq!(body, "Body starts here.\n");
    }

    #[test]
    fn generated_wiki_preserves_curated_body_around_generic_markers() {
        let body = "Curated introduction.  \n\n<!-- brain:generated:start -->\nold generated content\n<!-- brain:generated:end -->\n\nCurated ending with spaces.  \n";
        let old = format!("# Helper\n\nEntity: area\nName: Helper\nReviewed: 2020-01-01\nCustom: keep raw spacing  \n\n{body}");
        let mut notes = BTreeMap::new();
        notes.insert("wiki/helper.md".into(), old.as_bytes().to_vec());
        notes.insert("projects/evidence.md".into(), note("projects/evidence.md", "# Helper evidence\n\nRecorded: 2020-01-02\nWorlds: Nacre\nAreas: Helper\n\nEvidence body.\n").1);
        let updated = generate("wiki", &notes, &registry(), &vocabulary(), "2020-01-02").unwrap();
        let text = std::str::from_utf8(&updated["wiki/helper.md"]).unwrap();
        assert!(text.contains("Custom: keep raw spacing  \n"));
        let (_, _, new_body) = parts(text);
        let start = "<!-- brain:generated:start -->";
        let old_start = body.find(start).unwrap();
        let new_start = new_body.find(start).unwrap();
        let old_end = body.find(END).unwrap();
        let new_end = new_body.find(END).unwrap();
        assert_eq!(&body[..old_start], &new_body[..new_start]);
        assert_eq!(
            &body[old_end + END.len()..],
            &new_body[new_end + END.len()..]
        );
        assert!(new_body.contains("Curated ending with spaces.  \n"));
    }

    #[test]
    fn missing_generated_end_marker_is_an_error() {
        assert!(set_generated(
            "# Page\n\n<!-- brain:generated:start -->\ntruncated",
            "replacement"
        )
        .is_err());
    }

    #[test]
    fn new_wiki_pages_have_registry_taxonomy_fields() {
        let mut notes = BTreeMap::new();
        notes.insert("projects/evidence.md".into(), note("projects/evidence.md", "# Helper evidence\n\nRecorded: 2020-01-02\nWorlds: Nacre\nAreas: Helper\n\nEvidence body.\n").1);
        let generated = generate("wiki", &notes, &registry(), &vocabulary(), "2020-01-02").unwrap();
        let area = std::str::from_utf8(&generated["wiki/helper.md"]).unwrap();
        assert!(area.contains("Worlds: Nacre"));
        assert!(area.contains("Areas: Helper"));
    }

    #[test]
    fn malformed_recorded_date_cannot_become_a_digest_path() {
        let notes = BTreeMap::from([note(
            "archive/checkpoints/2020-01/bad.md",
            "# Bad date\nRecorded: ../x-/../path\nWorlds: Nacre\n\nUntouched original.\n",
        )]);
        assert!(make_digests(&notes, &registry()).unwrap().is_empty());
    }

    #[test]
    fn digest_excerpt_is_deterministic_and_drops_link_targets() {
        let mut notes = BTreeMap::new();
        notes.insert("archive/checkpoints/2020-01/one.md".into(), note("archive/checkpoints/2020-01/one.md", "# Synthetic checkpoint\n\nRecorded: 2020-01-02\nWorlds: Nacre\nAreas: Helper\n\nFirst sentence\n[[devices/private-target|Device label]] and [readable](private/path.md). Second sentence.\n").1);
        notes.insert("archive/inbox/2020-01/review.md".into(), note("archive/inbox/2020-01/review.md", "# Unresolved archived inbox item\n\nRecorded: 2020-01-03\nReview: unsorted\n\nUnresolved synthetic item.\n").1);
        let generated = make_digests(&notes, &registry()).unwrap();
        let page = std::str::from_utf8(&generated["digests/2020-01/nacre-helper.md"]).unwrap();
        assert!(parts(page).1.contains("Worlds: Nacre"));
        assert!(page.contains("First sentence Device label and readable"));
        assert!(!page.contains("private-target"));
        assert!(!page.contains("private/path.md"));
        let review = std::str::from_utf8(&generated["digests/2020-01/review-unsorted.md"]).unwrap();
        assert!(review.contains("Unresolved archived inbox item"));
        assert!(review.contains("Review: unsorted"));
        assert_eq!(generated, make_digests(&notes, &registry()).unwrap());
    }

    #[test]
    fn proposed_registry_area_is_rendered_in_the_index() {
        let mut notes = BTreeMap::new();
        notes.insert("projects/proposed.md".into(), note("projects/proposed.md", "# Proposed area evidence\n\nRecorded: 2020-01-02\nWorlds: Nacre\nAreas: New Area\n\nSynthetic evidence.\n").1);
        let generated =
            generate("index", &notes, &registry(), &vocabulary(), "2020-01-02").unwrap();
        let index = std::str::from_utf8(&generated["INDEX.md"]).unwrap();
        assert!(index.contains("[New Area](wiki/area-new-area.md)"));
        let world = index.find("## [Nacre](wiki/nacre.md)").unwrap();
        let area = index.find("- [New Area](wiki/area-new-area.md)").unwrap();
        assert!(world < area, "area must appear under its world");
        let mut after_first = notes;
        after_first.insert("INDEX.md".into(), generated["INDEX.md"].clone());
        assert!(generate(
            "index",
            &after_first,
            &registry(),
            &vocabulary(),
            "2020-01-02"
        )
        .unwrap()
        .is_empty());
    }
}
