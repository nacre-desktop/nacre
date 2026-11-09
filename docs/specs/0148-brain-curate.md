# N0148 — Brain wiki curation (U8b, agent pass) and contradiction tuning

Status: implemented; synthetic acceptance and the full desktop CI job are
recorded below. No live vault, unit or installation is touched; no real model
call is made by tests or CI.

DESIGN §7 ("who updates"): after the deterministic consolidation (N0145) an
agent pass rewrites each wiki page's "Current facts" from evidence, one git
commit per page. Packs are built from those pages, so fresher facts mean
fresher session-start context.

## Command

`nacre-brain-maintain curate (--page WIKI_PATH | --area NAME | --world NAME |
--all) [--stale-only] [--max-pages N] [--dry-run] [--model MODEL] [--report
PATH]`. Defaults: model `sonnet`, at most 10 pages per run. Pages are
sequential with a 3-second pause (`NACRE_BRAIN_CURATE_DELAY_MS`); never
curated pages go first, then the oldest curation. `--stale-only` keeps pages
never curated, pages whose newest evidence is newer than their curation date,
and curated pages with open review flags after 7 days. Exit 0 when the run
completes (rejections are reported, not fatal), 3 when the CLI reports an
auth/usage error (the run stops at once), 1 when a commit could not be pushed.

### 1. Bounded input (deterministic)

Per page, at most 84,000 bytes (about 24k tokens): the page text without its
generated evidence block (24 KB budget, Current facts first), review flags
from `.brain-state/index/facts.json` and `review.json` (stale lines of the
page; contradictions, supersessions and reversals touching the page or its
evidence; 6 KB), then evidence newest first: world pages take the world's
notes, area pages the area's, other pages the notes that mention their name or
aliases, plus notes the page links and related/backlinked notes from the
viewer's connections index (`graph.json`, when present). Each evidence entry
is `[[path]] | Recorded date | title`, its categories and the first 600
characters of the body; at most 80. Notes under `credentials/` are never read
(the shared walker prunes them); evidence that trips a commit-guard secret
rule is never sent, and a page whose own text or flags trip one is rejected
before the call. All vault text sits inside `<vault-data id="nonce">`,
declared untrusted in the request and in the prompt; a closing tag inside the
data is defused.

### 2. Model call

`claude -p --model M --output-format json --tools "" --no-session-persistence
--disable-slash-commands --strict-mcp-config --safe-mode --system-prompt
<prompt>` with the input on stdin, in a private empty working directory,
`ANTHROPIC_API_KEY` removed (the owner's subscription), 300-second timeout
(`NACRE_BRAIN_CURATE_TIMEOUT_SECS`). The prompt is
`ai/skills/nacre-brain-curator/SKILL.md` without its front matter, embedded in
the helper at build time (one source; the skill directory is installed like
the others and says it is not for interactive use). A CLI error whose text
looks like auth, login, quota, rate limit or overload stops the run with a
clear message; other failures (timeout, non-JSON) reject the page.

### 3. Output contract and validation (Rust, before any write)

The reply is one JSON object (a code fence around it is tolerated):
`summary` (one paragraph), `facts` (current lines
`- <fact>. (confirmed YYYY-MM-DD; [[path]])` and superseded lines
`- ~~<old fact>~~ [(confirmed …; [[…]])] (superseded YYYY-MM-DD by [[path]])`)
and `open_questions`. A page is rejected, reported and skipped when:

- a line is not in the formats above, spans lines, contains `<!--`/`-->`, or a
  summary/question starts a heading, list or quote;
- a link does not resolve through the shared migration resolver
  (`brain_migrate_verify::LinkResolver`, Obsidian rules, relative to the
  page), cites a note that was not in the input (evidence shown or links
  already on the page), uses a Markdown link or code span, or names a
  `credentials` path;
- a date is invalid, in the future, or later than the newest cited note's
  Recorded date plus one day;
- the rendered block trips a commit-guard secret rule (`brain_git::secret_rule`,
  the same nine rules as the pre-commit guard) or mentions `credentials/`;
- there are 0 or more than 25 current facts, more than 25 superseded lines,
  more than 10 questions, or the block exceeds 16 KB (fact 400, summary 1,200,
  question 300 characters).

The prompt asks for 5–25 current facts "when the evidence supports that
many"; validation accepts 1–25 so thin pages are not padded with invented
facts.

### 4. Page format and byte-identity

```
## Current facts

<!-- brain:curated:start (curated 2026-11-09 from 14 notes [agent: claude]; edit outside this block) -->
Summary paragraph.

- Fact. (confirmed 2026-11-05; [[devices/…]])
- ~~Old fact.~~ (superseded 2026-11-05 by [[checkpoints/…]])

### Open questions

- Question?
<!-- brain:curated:end -->
```

Curation owns exactly one region: the existing curated block; else the body
of the page's `## Current facts` section (up to the next `## ` heading or the
generated block — the first curation replaces stub text such as "Not yet
curated"); else an empty point before the first `## ` heading (or the
generated block, or the end), where a `## Current facts` section is inserted.
An independent check then requires the text before and after that region to be
byte-identical to the old page, exactly one start and one end marker, and
nothing but blank lines and the section heading around the block; the header
(including `Status:`/`Reviewed:`) is never edited. More than one block or a
dangling marker refuses the page. The viewer's prose filter skips the marker
lines, so the wiki tab renders the summary and facts as before; the pack's
Identity now prefers the curated summary.

### 5. Write and commit

Under the vault lock: the page's SHA-256 must still equal the hash read before
the model call (else "page changed since it was read (compare-and-swap)") and
the page must be clean against HEAD (else "uncommitted edits"). The vault is
re-read, the new page substituted and consolidation's pack builder
(`consolidate::derive`, factored out of N0145's plan without behaviour
change) renders the page's area and world packs; changed packs that are clean
are written with it. Files are written privately, fsynced, re-read and
committed through `brain_git::commit_curation` (exact paths, private index,
guard over the staged blobs) as `wiki: <Page> curated from N notes [agent:
claude]` with `Model:` and `Agent:` lines; any failure restores every file.
Identical output (same reply, same inputs, same day) makes no commit. Commits
are pushed once at the end when `origin` exists.

`--dry-run` takes no lock and writes nothing in the vault; it still calls the
model and writes each proposed page (fenced), its evidence count, input size
and reported token usage to `--report` (default
`<vault parent>/brain-curate-report.md`; refused inside the vault).

## Timer (installed, disabled)

`ai/systemd/nacre-brain-curate.{service,timer}`: oneshot, `nice 10`, idle
I/O, `After=nacre-brain-consolidate.service`, PATH including `~/.local/bin`,
`nacre-brain-maintain curate --all --stale-only --max-pages 10`, nightly at
04:30 with a 10-minute random delay (consolidation runs at 03:30 + ≤ 20 min),
`Persistent=true`. `ai/install.py`, `tools/configure.py`, `tools/releases.py`
and the packaging glob ship the files; nothing enables them. To opt in:
`systemctl --user daemon-reload && systemctl --user enable --now
nacre-brain-curate.timer` (enable `nacre-brain-consolidate.timer` too).

## Contradiction tuning (N0145 extractors)

The live consolidation of 2026-11-09 flagged 49 contradictions and 32
reversals, mostly noise: commits, revisions and versions of different repos,
branches or hosts compared at area level; any hex token on a line that said
"deployed" somewhere; IPs attributed to the nearest name on the line; reversal
words anywhere in a decision body.

- **Qualified subjects.** Typed facts are keyed by (entity, attribute,
  qualifier). The qualifier is the nearest other registry/vocabulary name or
  identifier-like token (`helios-web`, `owner/repo`, `task/…`,
  `newbringer-v2-…`) between the previous value on the line (or the clause
  start, at most 64 bytes back) and the value, else the name part of the token
  the value is embedded in (`nacre-niri-26.04.r3063` → Compositors).
- **Commits.** A deployed commit is a 7–12 or 40 hex token with digits and
  letters, not glued to `-_.:/@`, not after serial/sha/digest/boot/uuid/id, and
  with deployed/installed/running/pinned within 48 bytes before or 24 after.
  Serials (16 hex), SHA-256 digests and UUID fragments no longer count.
- **Supersession vs contradiction.** For the newest note value of a key and
  the latest older different value from another file: a strictly later note
  with update wording (now, currently, updated, upgraded, changed, moved,
  migrated, switched, replaced, deployed, installed, new, latest, from X to …)
  supersedes; otherwise addresses (`ip`, `tailscale-ip`) contradict at any
  distance, and other attributes contradict only within 3 days. Anything else
  is ordinary progress, recorded as a supersession. Supersessions go to
  `facts.json` (`supersessions`, with `qualifier` on every observation); they
  are not review items. A wiki fact replaced by a newer note is left out of
  the pack's key facts and listed under Open questions as "Update …".
- **Reversals (decisions only).** A decision reverses another only through its
  title (entities from the title), its `Supersedes:` header, or a body line
  that both uses reversal wording and links the earlier decision.

The N0145 integration fixture's "dev-1 tailscale address is now 100.64.0.9"
case is now a supersession; a same-day pair of prod LAN addresses covers the
contradiction path.

## Validation

Synthetic data only, on dev-1 (container `n0148`, 8 CPUs, 16 GiB, nice 10,
`cargo -j2`). `crates/nacre-helper/tests/brain_curate.rs` builds a git vault
with a bare remote, runs consolidation, then the real binary with a stub
`claude` on PATH that records its input/arguments and returns canned
envelopes:

- `--world Infrastructure`: two good pages commit (one commit each, page and
  its packs, message format, pushed, tree clean), the world page's hallucinated
  link and two non-JSON replies are rejected with their pages unchanged;
  outside bytes identical; superseded line and open questions in the block;
  the regenerated dev-1 pack leads with the summary and lists the new facts;
  the input holds the evidence inside the untrusted block, the injection text
  only as data, no credential decoy and no secret-looking note, ≤ 84,000 bytes;
  the CLI arguments are as specified.
- A rerun with unchanged replies and inputs: no commit, identical tree.
  `--stale-only` skips both curated pages; a newer note makes one stale again.
- Rejections, each with an unchanged tree and HEAD: hallucinated link, future
  date, date later than its source, secret-looking string (built at run time),
  summary that closes the block and adds a heading, heading in a question,
  injected credentials citation echoed back, a real note that was not shown,
  unformatted fact, invalid JSON.
- An auth error (`Invalid API key · Please run /login`) stops the run after
  the first call with exit 3 and no change.
- `--dry-run` writes only the report (proposed page, usage); a report inside
  the vault is refused; a page edited during the model call is skipped by the
  compare-and-swap check and keeps the edit; a page with uncommitted edits is
  held.

Unit tests: splice/owned region (replace section, replace block, insert
section, append), outside-edit and extra-marker refusal, reply parsing and
line formats, prompt embedding; qualifier/commit extraction, the three
requested shapes (different repos in one area → no flag; "now" address change
→ supersession; same-day conflicting addresses → flag) plus window and
reversal-trigger cases.

Full `desktop` job of `.github/workflows/check.yml`, every run step replayed
in order in `ghcr.io/archlinux/archlinux:latest` from a clean clone of
6b540e6 (orient at its pin; no Chromium): `N0148 DESKTOP CI PASS` — fmt,
clippy `-D warnings`, workspace tests (88 suites, 2,242 passed, 0 failed), the
minimal protocol consumer, product shell Rust checks, Luau and proof suites,
cargo deny, helper sandbox integrity, i18n coverage, `tools/check.py` (54
Brain Python tests, 541 helper parity cases, 0 failures), English/pseudo
galleries (244 and 185 states), daemon raster memory. No timing flake needed
a rerun. Log: `.lead/n0148-full-ci.log` in the worktree.

The live effect of the tuning on the owner's 49 + 32 flags is not measured
here (no owner data in tests); the lead's next consolidation dry run shows it.

## Lead's first real run

1. `nacre-brain-maintain consolidate` (fresh flags), then
   `nacre-brain-maintain curate --page wiki/area-ci.md --dry-run --report
   ~/brain-curate-report.md` and read the proposed page and usage.
2. `nacre-brain-maintain curate --page wiki/area-ci.md`; inspect
   `git -C ~/Documents/Nacre-Brain show HEAD`; undo with `git revert HEAD`.
3. Then `--world <World> --max-pages 3`, and later `--all --stale-only`.

Estimated cost per page: input 15–24k tokens (bounded at ~24k), prompt ~1k,
output 1–3k tokens; about 20–28k tokens per page with Sonnet.

## Size and cleanup

Product code about +1,880 / −150 lines (after rustfmt): `brain_curate.rs`
1,413 product lines (+74 unit-test lines), `brain_consolidate.rs` about +370 /
−145 product lines (the pack loop moved into `derive()`; ~210 new unit-test
lines), small hooks in `brain_git.rs`, `brain_migrate_verify.rs`,
`brain_arguments.rs`, `brain_connections.rs`. Tests: 666 integration lines,
one parity case; prompt 61 lines. There is no Python original. No temporary
duplicates; nothing is replaced, so nothing is removed.
Out of scope: viewer UI, `ai/skills/nacre-brain/SKILL.md` and `ai/brain/*`,
taxonomy reshaping by the agent (§16 upkeep stays deterministic), re-filing
notes of merged areas (N0145 known limitation).
