---
name: nacre-brain
description: Retrieve or save durable personal knowledge, host/device facts, project context, and operational lessons in the user's private local Markdown vault.
---

# Nacre Brain

The vault lives at `$HOME/Documents/Nacre-Brain` (`nacre-brain path` prints a
configured location). It is a git repository. Repo-local instructions and specs
stay authoritative for code and deployment; host addresses and service state are
dated observations, so verify before operating on a machine.

## Session start

1. Load the context pack for the area you work in: `nacre-brain pack WORLD|AREA`
   (names or aliases; an area without its own pack falls back to its world).
2. Search for specifics (below).
3. Open the evidence notes a pack or hit links to before relying on a claim.

`wiki/` pages (the viewer's Wiki tab) are the curated current truth. Notes and
checkpoints are the dated evidence layer behind them; prefer verified, recent
evidence and check its source when a fact controls an operation.

## Search

```sh
nacre-brain search QUERY [--json] [--limit N] [--world W] [--area A] \
  [--kind K] [--since YYYY-MM-DD] [--include-archive]
```

Ranked, typo-tolerant and alias-aware ("smash card" finds Smash-Board). One line
per note (`PATH:LINE:TEXT`); `--json` adds snippets and a `didYouMean`. Archive
is excluded unless `--include-archive`. Exit 1 means no match.

## Writing

Save decisions, outcomes, useful research and explicit preferences at verified
milestones and before finishing; do not wait for a reminder. Search first; skip
trivia, duplicates, raw transcripts and tool logs. Mark corrections as
superseding the older note. Never store secret values (credential notes hold OS
keyring references only); the guard blocks common patterns but review the text.

```sh
nacre-brain note --kind runbooks --title 'Short factual title' \
  --source 'Exact file, command evidence or URL' --confidence verified \
  --world Nacre --area Helper --topic some-topic   # Markdown on stdin
nacre-ai-memory --title T --source S --confidence reported --world W --area A
```

- Pass `--world` AND `--area` with canonical names from the table below, from
  `registry.json` in the vault, or from the pack. Aliases resolve automatically.
  Never invent near-duplicate names (check first: `nacre-brain search --area X`).
- A new explicit `--area` is created at once. A new subject inferred from a
  `Subject: title` becomes a proposed area and is promoted after repeated use.
- Read the one-line CLI feedback on stderr: resolved `world > area`, Review
  flags, and any new or proposed category. Fix wrong placement by rewriting
  the note with the right names, not by hand-moving files.
- Topics are lowercase-hyphen. Metadata lives only in the header block.
- Confidence: `verified` for tested outcomes, `reported` for user statements,
  `unverified` for ideas. Each call creates a unique file, so concurrent chats
  are safe.

## Seed worlds and areas

| World | Areas |
|---|---|
| personal world (first in `registry.json`) | Preferences, Background |
| Newbringer | HK416, Guns, White motor, Smash-Board, Stations, Game, Demo sites |
| Nacre | Desktop, Bead, Compositors, Orient, Packages, Performance, Helper, Nacre AI |
| AI & agents | Lead workflow, Brain, Inspiration |
| Home & devices | Desktop PC, Laptop, Home server, Phone, Peripherals |
| Infrastructure | dev-1, prod, Network, CI |
| Side projects | Temporium, MusikKI, Electronics bench |

Growth beyond this is automatic and registry-owned; never a free-form world.

## Git

Every write is auto-committed in batches (a quiet-period sweep) and pushed to a
private remote. Never commit `credentials/`; a pre-commit guard blocks secrets,
forbidden paths and oversized files. To undo a bad change use `git revert` in
the vault (`nacre-brain git status --json` shows pending and push state). Never
force-push or rewrite vault history.

## Maintenance

Read [maintenance](references/maintenance.md) at milestones, when notes conflict,
or when asked to organize. Agents add evidence through the tools above and do not
hand-move or rewrite other notes; the owner-approved `nacre-brain-maintain
consolidate` does retention, taxonomy upkeep, freshness flags and packs.

## Conversation-first context

New chats are general conversations, not assigned projects. Resolve repositories
only when work is requested, with `nacre-ai-context QUERY` (or MCP
`nacre_workflow.resolve_context`); registry build_host/build_path stay
authoritative. Codex also has `find_knowledge` and `remember` in `nacre_workflow`;
Claude uses the CLI. New repositories enter the project registry only when
actually created and verified. Ordinary web-chat memory is a separate system.
