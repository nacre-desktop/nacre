//! Pure link planning and verification; never opens a link target on disk.
use crate::Result;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

type Notes = BTreeMap<String, Vec<u8>>;

#[derive(Debug)]
struct Link {
    start: usize,
    end: usize,
    target: String,
    wiki: bool,
}

fn decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'%' && i + 2 < s.len() {
            if let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|s| u8::from_str_radix(s, 16).ok())
            {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(s.as_bytes()[i]);
        i += 1;
    }
    String::from_utf8(bytes).unwrap_or_else(|_| s.to_owned())
}

fn normal(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in Path::new(path).components() {
        match part {
            Component::Normal(s) => parts.push(s.to_str()?),
            Component::CurDir => (),
            Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

fn links(text: &str) -> Vec<Link> {
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"\[\[([^\]|#]+)(?:[^\]]*)\]\]|\]\((?:<([^>]+)>|([^\s)]+))(?:\s+[^)]*)?\)")
            .unwrap()
    });
    let mut found = Vec::new();
    let mut offset = 0;
    let mut fence: Option<char> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let starts = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        if let Some(kind) = starts {
            if fence == Some(kind) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(kind);
            }
        } else if fence.is_none() {
            for c in RE.captures_iter(line) {
                let wiki = c.get(1).is_some();
                let m = c.get(1).or_else(|| c.get(2)).or_else(|| c.get(3)).unwrap();
                let raw = m.as_str();
                let target = if wiki {
                    raw
                } else {
                    raw.split('#').next().unwrap_or("")
                };
                // A pure fragment, URI or absolute filesystem link is not a vault note.
                if target.is_empty() || target.starts_with('/') || target.contains(':') {
                    continue;
                }
                if !wiki
                    && Path::new(&decode(target))
                        .extension()
                        .is_some_and(|x| x != "md")
                {
                    continue;
                }
                found.push(Link {
                    start: offset + m.start(),
                    end: offset + m.start() + target.len(),
                    target: decode(target),
                    wiki,
                });
            }
        }
        offset += line.len();
    }
    found
}

fn resolve(link: &Link, source: &str, notes: &Notes) -> Option<String> {
    let with_ext = |s: String| {
        if s.ends_with(".md") {
            s
        } else {
            format!("{s}.md")
        }
    };
    if link.wiki {
        if let Some(root) = normal(&link.target).map(with_ext) {
            if notes.contains_key(&root) {
                return Some(root);
            }
        }
    }
    let parent = Path::new(source).parent()?.to_str()?;
    let joined = if parent.is_empty() {
        link.target.clone()
    } else {
        format!("{parent}/{}", link.target)
    };
    if let Some(path) = normal(&joined).map(with_ext) {
        if notes.contains_key(&path) {
            return Some(path);
        }
    }
    if link.wiki && !link.target.contains('/') {
        let root = with_ext(link.target.clone());
        let name = Path::new(&root).file_name()?;
        let mut matches = notes
            .keys()
            .filter(|p| Path::new(p).file_name() == Some(name));
        let first = matches.next()?.clone();
        return matches.next().is_none().then_some(first);
    }
    None
}

fn historical_target(
    link: &Link,
    source: &str,
    notes: &Notes,
    moves: &BTreeMap<String, String>,
) -> Option<String> {
    let mut candidates = Vec::new();
    if link.wiki {
        candidates.extend(normal(&link.target));
    }
    let parent = Path::new(source).parent()?.to_str()?;
    let joined = if parent.is_empty() {
        link.target.clone()
    } else {
        format!("{parent}/{}", link.target)
    };
    candidates.extend(normal(&joined));
    for target in candidates {
        let path = if target.ends_with(".md") {
            target
        } else {
            format!("{target}.md")
        };
        if let Some(new) = moves.get(&path) {
            if notes.contains_key(new) {
                return Some(path);
            }
        }
    }
    None
}

fn relative(source: &str, target: &str) -> String {
    let from: Vec<_> = source.split('/').collect();
    let from = &from[..from.len().saturating_sub(1)];
    let to: Vec<_> = target.split('/').collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    std::iter::repeat_n("..", from.len() - common)
        .chain(to[common..].iter().copied())
        .collect::<Vec<_>>()
        .join("/")
}

/// Rewrites only resolved link targets. Moving a source also rebases its outbound links.
pub(super) fn rewrite_links(
    text: &str,
    old: &str,
    new: &str,
    moves: &BTreeMap<String, String>,
    notes: &Notes,
) -> String {
    let mut out = text.to_owned();
    for link in links(text).into_iter().rev() {
        let Some(target) =
            resolve(&link, old, notes).or_else(|| historical_target(&link, old, notes, moves))
        else {
            continue;
        };
        let destination = moves.get(&target).unwrap_or(&target);
        if destination == &target && old == new {
            continue;
        }
        let replacement = if link.wiki {
            if destination == &target {
                continue;
            }
            if link.target.ends_with(".md") {
                destination.clone()
            } else {
                destination.trim_end_matches(".md").to_owned()
            }
        } else {
            relative(new, destination).replace(' ', "%20")
        };
        out.replace_range(link.start..link.end, &replacement);
    }
    out
}

fn generated_parts(body: &str) -> Result<Option<(&str, &str)>> {
    let Some(start) = body.find("<!-- brain:generated:start") else {
        return Ok(None);
    };
    let end = body[start..]
        .find("<!-- brain:generated:end -->")
        .ok_or("unterminated generated wiki block")?
        + start;
    let after = end + "<!-- brain:generated:end -->".len();
    Ok(Some((&body[..start], &body[after..])))
}

fn wiki_body_unchanged(old: &str, new: &str) -> Result<bool> {
    match (generated_parts(old)?, generated_parts(new)?) {
        (Some((a, b)), Some((c, d))) => Ok(a == c && b == d),
        (None, Some((prefix, suffix))) => Ok(prefix
            .strip_prefix(old)
            .is_some_and(|extra| extra == "\n" || extra == "\n\n")
            && suffix == "\n"),
        (None, None) => Ok(old == new),
        _ => Ok(false),
    }
}

/// Verify actual disk bytes against the lock-held snapshot and exact link plan.
pub(super) fn verify(
    before: &Notes,
    after: &Notes,
    moves: &BTreeMap<String, String>,
    step: &str,
    registry: &Value,
) -> Result<()> {
    let worlds: BTreeSet<_> = registry["worlds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w["name"].as_str())
        .collect();
    for (old, bytes) in before {
        let path = moves.get(old).unwrap_or(old);
        let actual = after
            .get(path)
            .ok_or_else(|| format!("verify: missing original {old}"))?;
        if old == "INDEX.md" && step == "index" {
            continue;
        }
        let old_text = std::str::from_utf8(bytes)?;
        let new_text = std::str::from_utf8(actual)?;
        let (title, _, body) = super::header_body(old_text);
        let (new_title, _, new_body) = super::header_body(new_text);
        if !title.is_empty() && title != new_title {
            return Err(format!("verify: title changed in {old}").into());
        }
        let expected = if matches!(step, "inbox" | "archive" | "links") {
            rewrite_links(old_text, old, path, moves, before)
        } else {
            old_text.to_owned()
        };
        let expected_header = super::header_body(&expected).1;
        let new_header = super::header_body(new_text).1;
        for line in expected_header.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            if ["Worlds", "Areas", "Topics", "Review"].contains(&key)
                || (step == "wiki"
                    && old.starts_with("wiki/")
                    && ["Entity", "Name", "Aliases", "Parent", "Redirect"].contains(&key))
            {
                continue;
            }
            let context = key == "Project"
                && new_header.lines().any(|line| {
                    line.strip_prefix("Context:")
                        .is_some_and(|v| v.trim() == value.trim())
                });
            if !context && !new_header.lines().any(|new| new == line) {
                return Err(format!("verify: non-classification header lost in {old}").into());
            }
        }
        let expected_body = super::header_body(&expected).2;
        let unchanged = if step == "wiki" && old.starts_with("wiki/") {
            wiki_body_unchanged(body, new_body)?
        } else {
            expected_body.as_bytes() == new_body.as_bytes()
        };
        if !unchanged {
            return Err(format!("verify: body checksum mismatch in {old}").into());
        }
        // Every previously resolved link must still resolve after the step. Pre-existing
        // broken links are retained as evidence, never silently fixed or discarded.
        let old_links = links(old_text);
        let new_links = links(new_text);
        if step != "wiki" {
            for (i, link) in old_links.iter().enumerate() {
                if let Some(target) = resolve(link, old, before) {
                    let wanted = moves.get(&target).unwrap_or(&target);
                    if new_links
                        .get(i)
                        .and_then(|l| resolve(l, path, after))
                        .as_ref()
                        != Some(wanted)
                    {
                        return Err(format!("verify: broken or redirected link in {old}").into());
                    }
                }
            }
        }
    }
    for (path, bytes) in after {
        let text = std::str::from_utf8(bytes)?;
        let header = super::header_body(text).1;
        let fields: BTreeMap<_, _> = header
            .lines()
            .filter_map(|line| line.split_once(':').map(|(k, v)| (k, v.trim())))
            .collect();
        let changed = before.get(path) != Some(bytes);
        let require = path.contains('/')
            && changed
            && (step == "meta" || step == "wiki" || !before.contains_key(path));
        if require {
            if fields.get("Worlds").is_some_and(|s| s.is_empty()) {
                return Err(format!("verify: empty Worlds in {path}").into());
            }
            let canonical = fields
                .get("Worlds")
                .is_some_and(|v| v.split(',').all(|w| worlds.contains(w.trim())));
            if !canonical && !fields.get("Review").is_some_and(|s| !s.is_empty()) {
                return Err(format!("verify: no canonical world or Review in {path}").into());
            }
        }
        if !before.contains_key(path) || (path == "INDEX.md" && step == "index") {
            for link in links(text) {
                if resolve(&link, path, after).is_none() {
                    return Err(format!("verify: new page {path} contains a broken link").into());
                }
            }
        }
    }
    Ok(())
}

/// A ledger is filtered to absent old paths and present destinations, so it
/// changes link targets without moving any file in the current snapshot.
pub(super) fn verify_links(
    before: &Notes,
    after: &Notes,
    ledger: &BTreeMap<String, String>,
    registry: &Value,
) -> Result<()> {
    if ledger
        .iter()
        .any(|(old, new)| before.contains_key(old) || !before.contains_key(new))
    {
        return Err("unsafe historical move ledger".into());
    }
    verify(before, after, ledger, "links", registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_source_rebases_unchanged_target_and_preserves_fences_and_fragments() {
        let notes = BTreeMap::from([
            ("inbox/a.md".into(), Vec::new()),
            ("projects/a space.md".into(), Vec::new()),
        ]);
        let moves = BTreeMap::from([(
            "inbox/a.md".into(),
            "archive/checkpoints/2020-01/a.md".into(),
        )]);
        let text =
            "[[a#Anchor|label]] [target](../projects/a%20space.md#frag)\n```md\n[[inbox/a]]\n```\n";
        assert_eq!(rewrite_links(text, "inbox/a.md", "archive/checkpoints/2020-01/a.md", &moves, &notes),
            "[[archive/checkpoints/2020-01/a#Anchor|label]] [target](../../../projects/a%20space.md#frag)\n```md\n[[inbox/a]]\n```\n");
    }

    #[test]
    fn exact_body_check_rejects_a_changed_link_even_when_its_basename_matches() {
        let before = BTreeMap::from([
            (
                "projects/a.md".into(),
                b"# A\nWorlds: Nacre\n\n[[one/target]]\n".to_vec(),
            ),
            ("one/target.md".into(), b"# Target\n\nBody.\n".to_vec()),
        ]);
        let mut after = before.clone();
        after.insert(
            "projects/a.md".into(),
            b"# A\nWorlds: Nacre\n\n[[two/target]]\n".to_vec(),
        );
        let registry = serde_json::json!({"worlds":[{"name":"Nacre"}]});
        assert!(verify(&before, &after, &BTreeMap::new(), "meta", &registry).is_err());
    }
}
