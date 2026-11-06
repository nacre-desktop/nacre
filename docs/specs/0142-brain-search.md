# N0142 — Brain search

Agents (`nacre-brain search`) and the owner (the viewer's search box through
`/api/search`) share one ranked, in-process engine:
`crates/nacre-helper/src/brain_search.rs`. It replaces the substring-AND viewer
match and the CLI's `rg -n -i -F` phrase grep. No new dependency; fully offline.

## Engine

- **Tokens.** Casefolded alphanumeric runs. A compound joined by `-`/`_` yields
  its parts and the joined word (`smash-board` → `smash`, `board`,
  `smashboard`; `dev-1` → `dev`, `dev1`); adjacent query words are joined too
  (`hk 416` → `hk416`). Light stemming (`-ies`, `-ing`, `-ed`, `-es` after
  sibilants, `-s`, final `-e`), a small British→American spelling map
  (`colour` → `color`), a 22-word stop list. Single characters are not
  standalone terms.
- **Ranking.** BM25F (k1 1.2, b 0.75) over four fields with boosts title 3,
  world/area/entity names and aliases 2.5, topics/tags 2, body (rest of the note
  plus its path) 1. The score is multiplied by query-word coverage: the share of
  query words covered by a matched word, typo correction or compound, or by an
  alias whose whole canonical name matched (all its words, or its joined form).
  So a note matching every concept outranks one repeating a single word, and
  "stack smashing" alone does not cover "smash card". A query-side compound join
  scores with the idf of its rarest part, so a rare spelling (`white-motor`)
  does not outrank notes that write the words apart. An exact title match
  doubles the score. Archived notes are excluded unless asked for, and ranked
  ×0.6 when included.
- **Aliases.** Registry worlds/areas (`registry.json`, else the seed), the
  migration's entities (`brain-migrate-vocabulary.json`) and vault wiki pages
  with `Entity:`/`Name:`/`Aliases:` map alias phrases to canonical names. A query
  span (≤ 4 words, compared as concatenated normalized words) that equals an
  alias adds the canonical name's terms at weight 0.8, or 0.5 for a one-word
  alias that is itself a vault term (so "tailscale" still prefers notes saying
  tailscale over everything in Network). "smash card" → Smash-Board.
- **Typos.** For a word of 5+ characters that is neither a vault term, part of a
  vault compound nor an alias: a trigram index over the term dictionary yields
  candidates, kept within Damerau-Levenshtein (OSA) 1 (5–7 chars) or 2 (8+);
  up to three, weight 0.5. The best correction, written in the surface form found
  in the top results, becomes `didYouMean` ("tailscail" → "tailscale").
- **Snippets.** The note line with most distinct matched terms (body lines before
  header lines, the title only as a last resort), windowed to 240 characters,
  with highlight ranges in Unicode scalar offsets.
- **Filters.** `world`/`area` (canonical names or aliases), `kind` (a folder such
  as `decisions`/`checkpoints`, or `note`/`archive`/`digest`; a trailing `s` is
  ignored), `since` (recorded/reviewed date ≥ the day), include archive.
- **Index.** Per-note analysis (path, size, mtime, title, header fields, term
  counts per field) persists in `.brain-state/index/search.json` (0600, private
  directories) next to `graph.json`. Each refresh re-stats the vault and
  re-reads only notes whose size or mtime changed, then rebuilds the in-memory
  dictionary, postings, trigram and alias tables. The viewer refreshes only after
  the existing inotify watcher (N0140) invalidates, or when the watcher is not
  running; each CLI run refreshes once. Oversized or unreadable notes are
  skipped; the 10,000-note / 64 MiB vault limits match the viewer.
- **Shared resolution.** `brain_search::Categories` resolves header worlds/areas
  through registry aliases and `merged_into` with hash tables; the viewer's note
  model (`brain_web::notes`) now uses it instead of its own linear closure.
- **Second ranker: deferred.** Since U3 the viewer embeds only unclassified
  discovery candidates (≤ 128 notes), so there is no full-vault vector set to
  fuse with; Reciprocal Rank Fusion with a query embedding waits for a
  full-vault binary vector file (separate unit). Lexical + alias + fuzzy work
  fully without the model.

## Interfaces

- `nacre-brain search QUERY… [--json] [--limit N] [--world W]… [--area A]…
  [--kind K] [--since YYYY-MM-DD] [--include-archive]`. Text output keeps the
  former `PATH:LINE:TEXT` shape, now one ranked line per note (default 20);
  `Did you mean: …` goes to stderr. Exit 1 when nothing matches (as `rg`).
  `--json` prints the object below.
- `GET /api/search?q=…[&archive=1][&world=][&area=][&kind=][&since=][&limit=]`
  keeps `ids` (now ranked) and adds `query`, `count`, `results` (rank, id, path,
  file, line, title, snippet, highlights, score, worlds, areas, chips
  `World › Area`, kind, folder, date, confidence, review, archived),
  `didYouMean` and `expanded` (alias/compound/fuzzy terms with weights).
- Viewer: ranked rows keep the note-row look, add the highlighted snippet under
  the title (`mark` on an accent tint), the existing world › area chips and a
  "Did you mean *x*" line whose suggestion reruns the search. The archive toggle
  re-queries; confidence/kind filters apply to the ranked list. New string:
  `brain.ui.label.did-you-mean`.

## Evaluation (synthetic, no owner data)

`crates/nacre-helper/tests/brain_search_eval.rs` writes 2,000 notes: 325 in 14
owner-like families (Smash-Board, HK416, nacre-niri, Tailscale, White motor,
dev-1, Orient, Bead, Temporium, MusikKI, laptop, home server, red gun, phone;
every eighth archived) and 1,675 distractor checkpoints/inbox notes with
confusable phrases (SD card, graphics card, whitelist, stepper motor, standing
desk, devtools, stack smashing, battery saver …). 30 queries have known answers
(recall@5 capped by the relevant count; MRR; for the empty-answer query both are
1 only when nothing is returned). "Before" replays the old engines on the same
vault: viewer substring AND (newest first) and CLI literal phrase (path order).

| Engine | recall@5 | MRR |
| --- | --- | --- |
| Old viewer (substring AND, newest first) | 0.640 | 0.654 |
| Old CLI (`rg -i -F` phrase, path order) | 0.673 | 0.700 |
| **N0142 ranked engine** | **1.000** | **1.000** |

First relevant rank per query (old viewer / old CLI / new; `-` = not found):
`smash card` -/-/1 (top: "Magazine detection on the smash-board"), `smash board`
30/1/1, `tailscail` -/-/1 (did you mean "tailscale"), `hk 416` 1/-/1, `white motor
batery` -/-/1, `dev1` -/-/1, `colour palette` -/-/1, `hk416 provisoning` -/-/1,
`calibraton` -/-/1, `networking` -/-/1, `laptop battery` 4/1/1, `white motor`
3/1/1. The other 18 queries (including the exact title and `qzxv plorktunk`,
which returns nothing) rank a relevant note first in all three engines; the old
engines lose there on recall@5 only. The table prints in full with
`cargo test -p nacre-helper --test brain_search_eval -- --nocapture`.

One regression was found and fixed during the unit, not tuned away: with
title-slug file names "white motor batery" first ranked 6, because the query
join `whitemotor` (only in hyphenated "white-motor" notes) carried a high idf
and alias terms claimed the whole span for coverage. Capping join idf at the
rarest part and the all-words alias rule above fixed it (rank 1) without
changing any other query. The fixture is synthetic and favourable to clean
headers; the live audit on the owner's vault is the real gate.

## Validation

All on dev-1 in the task's Arch container (`n0142`, 8 CPUs, 16 GiB, nice 10,
`cargo -j2`):

- Latency (release, `cargo test --release -p nacre-helper --test
  brain_search_eval -- --ignored --nocapture`, 2,000 notes, 30 queries × 20):
  warm p50 0.49 ms / p95 0.89 ms at 20 results (CLI default), p50 5.2 ms /
  p95 6.9 ms at 200 results with snippets (viewer request); cold build from
  files 60 ms; an unchanged re-stat 3.8 ms. CLI wall time per call
  (process start, persisted-index load, query, JSON print): 19–21 ms over 10
  runs; 66 ms when `search.json` is absent.
- Memory: the in-memory index for 2,000 notes adds about 8 MiB RSS to the
  on-demand viewer process (`brain-control serve`, idle exit 5 min) and to each
  CLI run; it is not in the shell. The persisted `search.json` is 1.4 MiB.
- Browser (headless Chromium, CDP): new `brain/tests/browser-search.mjs`
  (299 notes: "smash card" ranks the 12 Smash-Board notes first with a `mark`
  on "Smash-Board" and the `Newbringer › Smash-Board` chip; the archived note
  is hidden, then ranks 13th after "Show archive"; "tailscail" shows "Did you
  mean tailscale", the suggestion reruns the search, a ranked row opens the
  reader; a nonsense query shows the empty state; 0 console errors) plus the
  existing `browser-load.mjs`, `browser-graph.mjs` and `browser-hierarchy.mjs`.
  Evidence: `.lead/evidence/n0142-smash-card.png`, `n0142-did-you-mean.png`.
- Helper parity: `nacre-brain/search` now expects the ranked
  `PATH:LINE:TEXT` line and the persisted index; new `nacre-brain/search-json`
  covers `--json`, typo correction, chips and archive exclusion (fixture mtimes
  are fixed so the index bytes are stable).
- Full `desktop` job of `.github/workflows/check.yml` replicated step by step in
  `ghcr.io/archlinux/archlinux:latest`: `N0142 DESKTOP CI PASS` on 65ff793
  (fmt, clippy `-D warnings`, workspace tests, product shell suites, cargo
  deny, i18n, `tools/check.py` with 537 helper parity cases and 0 failures,
  galleries, daemon memory). Earlier runs failed: U4's `tests/brain_migrate.rs`
  tripped the public-names guard on base fdda3a9 (fixed on main; this branch is
  rebased onto 3e82cbc4); the evaluation fixture then tripped the same guard with
  an owner-named seed world (replaced by a neutral world, results unchanged); and
  Chromium installed for the browser tests made the wallpaper pipeline emit a
  Chromium theme. The passing run had no Chromium on PATH, like CI.

## Size and cleanup

Product code: +1,365 / −142 lines (new `brain_search.rs` 1,092
lines after rustfmt including docs; viewer +101/−15 JS and +24 CSS). Removed:
the viewer's substring-AND `search_notes`, the CLI `rg` call and the viewer's
linear category-resolution closure (replaced by the shared `Categories`).
Tests: 130 unit lines, the 509-line evaluation, the 314-line browser test and
two parity cases. No new dependency, no temporary duplicate. The imported
Python `brain/control.py` keeps its old `search_notes` as the existing
parity/rollback reference under N0024's expiry (delete 7 days after trust in the
native worker); it is not used at runtime.

Open: the left drawer's Brain section (`desktop_extras::brain_search`) and the
launcher `#` scope still use their own substring matching; switching them to
`brain_search::search` is a small follow-up. Agent docs (`nacre-brain` skill,
U9) should mention `--json`, `--world`/`--area` and `--since`.
