//! Pure link planning and verification; never opens a link target on disk.
use crate::Result;
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

type Notes = BTreeMap<String, Vec<u8>>;

/// How a link spells its target. Resolution follows Obsidian: Markdown and
/// `[[./x]]`/`[[../x]]` are folder-relative, `[[folder/x]]` is vault-rooted and
/// `[[x]]` names a file anywhere in the vault.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Style {
    Markdown,
    Relative,
    Root,
    Name,
}

#[derive(Debug)]
struct Link {
    start: usize,
    end: usize,
    target: String,
    style: Style,
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

fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn with_ext(path: String) -> String {
    if path.ends_with(".md") {
        path
    } else {
        format!("{path}.md")
    }
}

/// Inline code spans (equal backtick runs) on one line; links inside stay text.
fn code_spans(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let run = |at: usize| bytes[at..].iter().take_while(|&&b| b == b'`').count();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let open = run(i);
        let mut j = i + open;
        i = j;
        while j < bytes.len() {
            if bytes[j] != b'`' {
                j += 1;
                continue;
            }
            let close = run(j);
            if close == open {
                spans.push((i - open, j + close));
                i = j + close;
                break;
            }
            j += close;
        }
    }
    spans
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
            let code = code_spans(line);
            for c in RE.captures_iter(line) {
                let whole = c.get(0).unwrap();
                if code
                    .iter()
                    .any(|&(a, b)| a < whole.end() && whole.start() < b)
                {
                    continue;
                }
                let wiki = c.get(1).is_some();
                let m = c.get(1).or_else(|| c.get(2)).or_else(|| c.get(3)).unwrap();
                let raw = m.as_str();
                let target = if wiki {
                    // `[[x\|alias]]` is the table-safe alias form.
                    raw.strip_suffix('\\').unwrap_or(raw)
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
                let style = if !wiki {
                    Style::Markdown
                } else if target.starts_with("./") || target.starts_with("../") {
                    Style::Relative
                } else if target.contains('/') {
                    Style::Root
                } else {
                    Style::Name
                };
                found.push(Link {
                    start: offset + m.start(),
                    end: offset + m.start() + target.len(),
                    target: decode(target),
                    style,
                });
            }
        }
        offset += line.len();
    }
    found
}

/// The note path a link spells, whether or not it exists; names spell no path.
fn spelled(link: &Link, source: &str) -> Option<String> {
    let path = match link.style {
        Style::Name => return None,
        Style::Root => link.target.clone(),
        Style::Markdown | Style::Relative => match folder(source) {
            "" => link.target.clone(),
            dir => format!("{dir}/{}", link.target),
        },
    };
    normal(&path).map(with_ext)
}

/// Note paths with a file-name index; resolution never opens a file.
struct Vault {
    paths: BTreeSet<String>,
    names: BTreeMap<String, Vec<String>>,
}

impl Vault {
    fn new(paths: impl IntoIterator<Item = String>) -> Self {
        let paths: BTreeSet<String> = paths.into_iter().collect();
        let mut names = BTreeMap::<String, Vec<String>>::new();
        for path in &paths {
            let name = path.rsplit('/').next().unwrap_or(path);
            names.entry(name.to_owned()).or_default().push(path.clone());
        }
        Self { paths, names }
    }

    fn resolve(&self, link: &Link, source: &str) -> Option<String> {
        if link.style != Style::Name {
            return spelled(link, source).filter(|path| self.paths.contains(path));
        }
        let name = with_ext(link.target.clone());
        if self.paths.contains(&name) {
            return Some(name);
        }
        match self.names.get(&name)?.as_slice() {
            [only] => Some(only.clone()),
            // An ambiguous name means the note beside the source, if any.
            many => many.iter().find(|p| folder(p) == folder(source)).cloned(),
        }
    }

    fn broken(&self, notes: &Notes) -> Vec<(String, String)> {
        let mut broken = Vec::new();
        for (path, bytes) in notes {
            for link in links(&String::from_utf8_lossy(bytes)) {
                if self.resolve(&link, path).is_none() {
                    broken.push((path.clone(), link.target));
                }
            }
        }
        broken
    }
}

/// Link edits for one step: `moves` applied to the step's input notes.
pub(super) struct Relinker<'a> {
    moves: &'a BTreeMap<String, String>,
    before: Vault,
    after: Vault,
}

impl<'a> Relinker<'a> {
    pub(super) fn new(notes: &Notes, moves: &'a BTreeMap<String, String>) -> Self {
        Self {
            moves,
            before: Vault::new(notes.keys().cloned()),
            after: Vault::new(notes.keys().map(|p| moves.get(p).unwrap_or(p).clone())),
        }
    }

    /// What a link meant before the step. A ledger move also repairs a link that
    /// still spells a path the migration moved away earlier.
    fn target(&self, link: &Link, source: &str) -> Option<String> {
        self.before.resolve(link, source).or_else(|| {
            let old = spelled(link, source)?;
            let new = self.moves.get(&old)?;
            self.after.paths.contains(new).then_some(old)
        })
    }

    /// Rewrites only links that would otherwise stop resolving to their target,
    /// keeping each link's style, `.md` suffix, fragment and alias.
    pub(super) fn rewrite(&self, text: &str, old: &str, new: &str) -> String {
        let mut out = text.to_owned();
        for link in links(text).into_iter().rev() {
            let Some(target) = self.target(&link, old) else {
                continue;
            };
            let destination = self.moves.get(&target).unwrap_or(&target);
            if self.after.resolve(&link, new).as_ref() == Some(destination) {
                continue;
            }
            let path = match link.style {
                Style::Markdown | Style::Relative => relative(new, destination),
                // A name that stopped resolving uniquely gets its vault path.
                Style::Root | Style::Name => destination.clone(),
            };
            let path = match path.strip_suffix(".md") {
                Some(bare) if !link.target.ends_with(".md") => bare.to_owned(),
                _ => path,
            };
            let replacement = match link.style {
                Style::Markdown => path.replace(' ', "%20"),
                Style::Relative if !path.starts_with("../") => format!("./{path}"),
                _ => path,
            };
            out.replace_range(link.start..link.end, &replacement);
        }
        out
    }
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

/// Links that resolve to no note; migration keeps them exactly as written.
pub(super) fn broken_links(notes: &Notes) -> Vec<(String, String)> {
    Vault::new(notes.keys().cloned()).broken(notes)
}

/// The shared Obsidian-style resolver over a fixed set of note paths.
pub(super) struct LinkResolver(Vault);

impl LinkResolver {
    pub(super) fn new(paths: impl IntoIterator<Item = String>) -> Self {
        Self(Vault::new(paths))
    }

    /// Every link in `text` (as written in `source`): its target and the note it
    /// resolves to, if any.
    pub(super) fn resolve_all(&self, source: &str, text: &str) -> Vec<(String, Option<String>)> {
        links(text)
            .into_iter()
            .map(|link| {
                let resolved = self.0.resolve(&link, source);
                (link.target, resolved)
            })
            .collect()
    }
}

/// Note paths the links of `path` resolve to.
pub(super) fn link_targets(notes: &Notes, path: &str) -> BTreeSet<String> {
    let vault = Vault::new(notes.keys().cloned());
    notes.get(path).map_or_else(BTreeSet::new, |bytes| {
        links(&String::from_utf8_lossy(bytes))
            .iter()
            .filter_map(|link| vault.resolve(link, path))
            .collect()
    })
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
    let relinker = Relinker::new(before, moves);
    let after_vault = Vault::new(after.keys().cloned());
    // Consolidation moves, relinks and regenerates digests, wiki evidence blocks
    // and the INDEX in one transaction; generated pages follow the wiki rules.
    let consolidate = step == "consolidate";
    let wiki_rules = step == "wiki" || consolidate;
    for (old, bytes) in before {
        let path = moves.get(old).unwrap_or(old);
        let actual = after
            .get(path)
            .ok_or_else(|| format!("verify: missing original {old}"))?;
        if old == "INDEX.md" && (step == "index" || consolidate) {
            continue;
        }
        if consolidate && old.starts_with("digests/") {
            continue;
        }
        let old_text = std::str::from_utf8(bytes)?;
        let new_text = std::str::from_utf8(actual)?;
        let (title, _, _) = super::header_body(old_text);
        let (new_title, _, new_body) = super::header_body(new_text);
        if !title.is_empty() && title != new_title {
            return Err(format!("verify: title changed in {old}").into());
        }
        let expected = if matches!(step, "inbox" | "archive" | "links" | "consolidate") {
            relinker.rewrite(old_text, old, path)
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
                || (wiki_rules
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
        let unchanged = if wiki_rules && old.starts_with("wiki/") {
            wiki_body_unchanged(expected_body, new_body)?
        } else {
            expected_body.as_bytes() == new_body.as_bytes()
        };
        if !unchanged {
            return Err(format!("verify: body checksum mismatch in {old}").into());
        }
        // Every previously resolved link must still resolve to the same (moved) note.
        // Pre-existing broken links stay as written: the byte check above keeps them
        // unchanged, and the run reports them instead of failing.
        let old_links = links(old_text);
        let new_links = links(new_text);
        if !(wiki_rules && old.starts_with("wiki/")) && step != "wiki" {
            for (i, link) in old_links.iter().enumerate() {
                let Some(target) = relinker.target(link, old) else {
                    continue;
                };
                let wanted = moves.get(&target).unwrap_or(&target);
                if new_links
                    .get(i)
                    .and_then(|l| after_vault.resolve(l, path))
                    .as_ref()
                    != Some(wanted)
                {
                    return Err(format!(
                        "verify: broken or redirected link in {old}: {}",
                        link.target
                    )
                    .into());
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
        // Consolidation moves notes as written; only new generated pages must classify.
        let moved = consolidate && moves.values().any(|target| target == path);
        let require = path.contains('/')
            && changed
            && !moved
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
    }
    // A step may keep pre-existing broken links but must add none, including in
    // generated pages and blocks. The INDEX is rebuilt whole: all of it resolves.
    let mut kept = BTreeMap::<(String, String), usize>::new();
    for (path, target) in relinker.before.broken(before) {
        let path = moves.get(&path).cloned().unwrap_or(path);
        *kept.entry((path, target)).or_default() += 1;
    }
    for (path, target) in after_vault.broken(after) {
        let index = step == "index" && path == "INDEX.md";
        match kept.get_mut(&(path.clone(), target.clone())) {
            Some(count) if *count > 0 && !index => *count -= 1,
            _ => return Err(format!("verify: new broken link in {path}: {target}").into()),
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

    fn notes(paths: &[&str]) -> Notes {
        paths
            .iter()
            .map(|p| ((*p).to_owned(), Vec::new()))
            .collect()
    }

    #[test]
    fn moved_source_rebases_unchanged_target_and_preserves_fences_and_fragments() {
        let notes = notes(&["inbox/a.md", "projects/a space.md"]);
        let moves = BTreeMap::from([(
            "inbox/a.md".into(),
            "archive/checkpoints/2020-01/a.md".into(),
        )]);
        let text = "[[a#Anchor|label]] [target](../projects/a%20space.md#frag) `[x](../projects/a%20space.md)`\n```md\n[[inbox/a]]\n```\n";
        assert_eq!(Relinker::new(&notes, &moves).rewrite(text, "inbox/a.md", "archive/checkpoints/2020-01/a.md"),
            "[[a#Anchor|label]] [target](../../../projects/a%20space.md#frag) `[x](../projects/a%20space.md)`\n```md\n[[inbox/a]]\n```\n");
    }

    #[test]
    fn every_link_style_keeps_its_spelling_and_resolves_after_moves() {
        let notes = notes(&[
            "checkpoints/s.md",
            "checkpoints/sib.md",
            "checkpoints/dup.md",
            "checkpoints/stay.md",
            "projects/dup.md",
            "runbooks/r.md",
            "inbox/i.md",
            "wiki/w.md",
            "INDEX.md",
        ]);
        let moves = BTreeMap::from([
            (
                "checkpoints/s.md".into(),
                "archive/checkpoints/2020-01/s.md".into(),
            ),
            (
                "checkpoints/sib.md".into(),
                "archive/checkpoints/2020-01/sib.md".into(),
            ),
            (
                "checkpoints/dup.md".into(),
                "archive/checkpoints/2020-01/dup.md".into(),
            ),
            ("inbox/i.md".into(), "projects/i.md".into()),
        ]);
        let relinker = Relinker::new(&notes, &moves);
        // A moved note linking out, in every style.
        let out = "[[sib]] [[../runbooks/r]] [[../runbooks/r#Steps|alias]] [[./sib.md]] \
                   [[runbooks/r.md|root]] [[r]] [[w|alias]] [[dup]] [[inbox/i]] [[../inbox/i]] \
                   [m](../runbooks/r.md#s) [m](sib.md) [m](../runbooks/r) | [[../runbooks/r\\|t]] | \
                   `[[../runbooks/r]]` [[../projects/missing]] [[nowhere]] [[#Local]]\n";
        assert_eq!(
            relinker.rewrite(out, "checkpoints/s.md", "archive/checkpoints/2020-01/s.md"),
            "[[sib]] [[../../../runbooks/r]] [[../../../runbooks/r#Steps|alias]] [[./sib.md]] \
             [[runbooks/r.md|root]] [[r]] [[w|alias]] [[dup]] [[projects/i]] [[../../../projects/i]] \
             [m](../../../runbooks/r.md#s) [m](sib.md) [m](../../../runbooks/r) | [[../../../runbooks/r\\|t]] | \
             `[[../runbooks/r]]` [[../projects/missing]] [[nowhere]] [[#Local]]\n"
        );
        // Unmoved notes linking into moved notes.
        let into =
            "[[../checkpoints/s]] [[checkpoints/s#H|a]] [[s]] [[./r]] [m](../checkpoints/s.md#f) \
                    [m](../checkpoints/sib) `[[checkpoints/s]]`\n";
        assert_eq!(
            relinker.rewrite(into, "runbooks/r.md", "runbooks/r.md"),
            "[[../archive/checkpoints/2020-01/s]] [[archive/checkpoints/2020-01/s#H|a]] [[s]] [[./r]] \
             [m](../archive/checkpoints/2020-01/s.md#f) [m](../archive/checkpoints/2020-01/sib) `[[checkpoints/s]]`\n"
        );
        let root = "[[./checkpoints/s]] [m](checkpoints/s.md) [[../inbox/i]]\n";
        assert_eq!(
            relinker.rewrite(root, "INDEX.md", "INDEX.md"),
            "[[./archive/checkpoints/2020-01/s]] [m](archive/checkpoints/2020-01/s.md) [[../inbox/i]]\n"
        );
        assert_eq!(
            relinker.rewrite("[[../inbox/i]] [[i]]\n", "wiki/w.md", "wiki/w.md"),
            "[[../projects/i]] [[i]]\n"
        );
        // `dup` meant the checkpoint beside it; once that moves away the name is
        // ambiguous, so the link takes the moved note's vault path.
        assert_eq!(
            relinker.rewrite(
                "[[dup]] [[dup.md|d]]\n",
                "checkpoints/stay.md",
                "checkpoints/stay.md"
            ),
            "[[archive/checkpoints/2020-01/dup]] [[archive/checkpoints/2020-01/dup.md|d]]\n"
        );
        // Rewritten text is stable: a rerun with no moves changes nothing.
        let after: Notes = notes
            .keys()
            .map(|p| (moves.get(p).unwrap_or(p).clone(), Vec::new()))
            .collect();
        let none = BTreeMap::new();
        let rewritten =
            relinker.rewrite(out, "checkpoints/s.md", "archive/checkpoints/2020-01/s.md");
        assert_eq!(
            Relinker::new(&after, &none).rewrite(
                &rewritten,
                "archive/checkpoints/2020-01/s.md",
                "archive/checkpoints/2020-01/s.md"
            ),
            rewritten
        );
    }

    #[test]
    fn verifier_rejects_new_breakage_but_keeps_preexisting_broken_links() {
        let source = b"# S\nWorlds: Nacre\n\n[[../runbooks/r]] [[nowhere]] [x](../gone.md)\n";
        let before = BTreeMap::from([
            ("checkpoints/s.md".to_owned(), source.to_vec()),
            ("runbooks/r.md".to_owned(), b"# R\n\nBody.\n".to_vec()),
        ]);
        let moves = BTreeMap::from([(
            "checkpoints/s.md".to_owned(),
            "archive/checkpoints/2020-01/s.md".to_owned(),
        )]);
        let registry = serde_json::json!({"worlds":[{"name":"Nacre"}]});
        let moved = |text: &[u8]| {
            let mut after = before.clone();
            after.remove("checkpoints/s.md");
            after.insert("archive/checkpoints/2020-01/s.md".into(), text.to_vec());
            after
        };
        let planned = Relinker::new(&before, &moves).rewrite(
            std::str::from_utf8(source).unwrap(),
            "checkpoints/s.md",
            "archive/checkpoints/2020-01/s.md",
        );
        assert!(planned.contains("[[../../../runbooks/r]] [[nowhere]] [x](../gone.md)"));
        verify(
            &before,
            &moved(planned.as_bytes()),
            &moves,
            "archive",
            &registry,
        )
        .unwrap();
        // The old bug moved a relative wikilink without rebasing it. The body check
        // rejects that text, and so does the independent resolution check when the
        // planned body agrees with the mistake (step "meta" expects no rewrite).
        assert!(verify(&before, &moved(source), &moves, "archive", &registry).is_err());
        let error = verify(&before, &moved(source), &moves, "meta", &registry).unwrap_err();
        assert!(
            error.to_string().contains("broken or redirected link"),
            "{error}"
        );
        // A generated page may not add a broken link of its own.
        let mut generated = before.clone();
        generated.insert(
            "digests/d.md".into(),
            b"# D\nWorlds: Nacre\n\n[[nowhere]]\n".to_vec(),
        );
        let error =
            verify(&before, &generated, &BTreeMap::new(), "digests", &registry).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("new broken link in digests/d.md"),
            "{error}"
        );
        assert_eq!(
            broken_links(&before),
            [
                ("checkpoints/s.md".to_owned(), "nowhere".to_owned()),
                ("checkpoints/s.md".to_owned(), "../gone.md".to_owned()),
            ]
        );
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
