# N0145 — Brain consolidation (U8a, deterministic)

Status: implemented; synthetic acceptance and the full desktop CI job passed
(dev-1). No live vault, unit or installation is touched.

`nacre-brain-maintain consolidate [--dry-run] [--report PATH]` keeps the Brain
tidy without model calls (DESIGN §7–9, §16). A run holds both vault locks
(`lock`, then `capture.lock`; note and memory writers wait), plans on one
lock-held snapshot and lands everything it changes in the vault as **one**
commit, `brain: consolidate — <counts>` with every taxonomy change listed in the
body and an `Agent:` trailer, then pushes like the migration (only when an
`origin` remote exists). A rerun with unchanged inputs plans nothing and makes
no commit. `--dry-run` takes no lock and writes nothing; `--report` writes a
private Markdown report outside the vault (moves, taxonomy changes, changed
files, review queue, pre-existing broken links). `NACRE_BRAIN_TODAY=YYYY-MM-DD`
injects the clock for tests.

## What a run does

1. **Retention (§9).** Inbox notes at least 3 days old are triaged by the
   migration's rules (handoff → checkpoints, preference → personal, decision →
   decisions, procedure → runbooks, device/host fact → devices/infrastructure,
   other with a world → projects). Checkpoints at least 14 days old (also ones
   just triaged there) move to `archive/checkpoints/YYYY-MM/`; unclear inbox
   items keep their `Review:` flag and move to `archive/inbox/YYYY-MM/` after
   30 days. File names never change. Links into and out of moved notes are
   repaired by the migration's shared resolver in their own style; pre-existing
   broken links stay byte-identical and are reported. Capture/dedup records are
   re-pointed and the migration move ledger is extended (same code as `migrate`).
2. **Generated pages.** The migration generators refresh monthly digests
   (archived checkpoints and inbox, per world/area; unresolved inbox in
   `review-unsorted`), wiki evidence blocks and `INDEX.md`. Proposed areas get
   no wiki page or INDEX entry until promoted, so retired proposals leave no
   stub pages behind.
3. **Taxonomy upkeep (§16).** In order: promote proposed areas used by ≥ 3
   notes on ≥ 2 days; mark a near-duplicate `auto`/`proposed` area
   `merged-into` the older or seed area in the same world (the registry's own
   similarity rule; the writer resolver now follows `merged_into`, as search and
   the viewer already did, so it acts as an alias); retire proposed areas with
   no notes whose last use is ≥ 30 days old; fold an `auto` world with < 5 notes
   that is ≥ 90 days old into the world its notes classify into, as an `auto`
   area of the same name and aliases (its areas move along; notes keep
   resolving because an area named in `Worlds:` implies its world); flag (never
   split) `auto` areas with ≥ 150 notes, or ≥ 30 notes of which ≥ 40 % also
   belong to another world. Seed worlds and areas are never modified.
4. **Freshness (§7).** Wiki fact lines (`- claim (… YYYY-MM-DD …)` outside the
   generated block) are classed: addresses/IPs/host state stale after 14 days,
   versions/revisions/deploy state after 30, decisions/preferences and other
   facts never; struck-through or `superseded` lines are history, not stale.
5. **Contradictions and reversals (§7).** Deterministic extractors key typed
   facts by (entity, attribute) on wiki fact lines and active notes (not
   archive, raw or digests): IPv4 and Tailscale (100.64/10) addresses, `rNNN`
   revisions, `v`/`version`/`release` versions, commit hashes next to
   deployed/installed/running/pinned, `runs on <host>`, enabled/disabled with
   the word it qualifies. The entity is the nearest registry area, vocabulary
   entity or (fallback) world name on the line, else the wiki page's `Name`.
   When the newest value for a key comes from a note and differs from an older
   value elsewhere, a review item links both; it clears once a wiki fact dated
   at least as late records the newest value. A decision that supersedes,
   reverts, disables, rolls back or undoes something is flagged as a reversal
   of the latest earlier decision naming the same entity in its title, or of
   its `Supersedes:` target.
6. **Context packs (§8).** `packs/pack-<world>[-<area>].md` for every world and
   every seed/auto area that has evidence notes or curated wiki facts:
   identity (first wiki paragraph, placement, aliases), current state, up to
   20 dated key facts (newest current wiki facts that are neither stale,
   superseded nor contradicted, then fresh newest typed values from the scope's
   notes), open questions (stale facts, contradictions, reversals, the page's
   open-question section and `- [ ]` items), where to look (wiki page, latest
   digests, a scoped search command, five newest notes) and do/don't lines.
   Hard cap 4,096 bytes: lines are trimmed least important first. Packs carry no
   run date, so they change only when their inputs change; packs whose scope
   disappears are removed in the same commit. `nacre-brain pack <world|area>
   [--json]` prints the area's pack or, failing that, its world's (aliases and
   merged areas resolve through the registry).

Derived results never enter page prose: `.brain-state/index/facts.json` (fact
lines with class/stale/age/superseded, current value per key, contradictions,
reversals) and `.brain-state/index/review.json` (items `stale-fact`,
`contradiction`, `reversal`, `taxonomy-split`, `tiny-world`, `orphan-page`,
`deferred`) are rewritten only when their content changes, private (0600).
The viewer can read them later; N0144 owns the viewer and is untouched here.

**Uncommitted work is never folded in.** Before applying, every existing path
the plan would move or rewrite is checked against HEAD (`git ls-files`/`git
diff`, shared with the migration baseline guard). Untracked or edited notes are
held: their own moves and the moves of notes they link to wait for a later run
(`deferred` review items), held generated pages are skipped, and an edited
`registry.json` defers taxonomy upkeep. The transaction then rechecks the same
baseline, so a later `git revert` of the run's commit restores exactly what it
changed.

## Reuse and shared changes

- `brain_migrate.rs`: the step transaction (CAS snapshots, writes, moves,
  fsync, verification, exact-path commit, rollback) is now `transact()`, used by
  every migration step and by consolidation; capture/ledger re-pointing is
  `state_updates()`; retention destinations are `retention_target()`; report
  path checks are `report_target()`/`write_report()`. Migration behavior is
  unchanged (its 8 synthetic integration tests pass). The plan also carries
  generated non-note files (packs) with expected hashes and removals.
- `brain_migrate_verify.rs`: a `consolidate` mode verifies moved/relinked notes
  byte for byte, wiki pages outside their generated block (after relinking),
  regenerated digests and INDEX only for links; new generated pages must
  classify; moved notes keep their categories as written. No step may add a
  broken link. `link_targets()` serves the deferral planner.
- `brain_migrate_generated.rs`: digests are now named
  `digests/YYYY-MM/YYYY-MM-<world>[-<area>].md`. The old `<world>-<area>.md`
  name repeated every month and matched wiki page names (`newbringer.md`,
  `nacre-compositors.md`), and the viewer rejects duplicate note file names
  (its note ids are file-name hashes), so the first multi-month migration or
  consolidation would have made the viewer refuse to index. The live vault has
  no digests yet. Pack names use the `pack-` prefix for the same reason.
  Digest worlds/areas now resolve through the shared registry resolution
  (aliases, `merged_into`, a folded world as its area); unresolvable notes go
  to `review-unsorted`. Before, a header written with an alias or a folded
  world would have produced a non-canonical digest that verification rejects.
- `brain_registry.rs`: `upkeep()` (above), `resolve_label()` for the pack CLI,
  and `merged_into` resolution in the shared resolver.
- `brain_git.rs`: `unclean_paths()` (the baseline check, per path),
  `commit_consolidation()` (same exact-path private-index commit as a migration
  step, custom message) and `has_origin()`.

## Timer

`ai/systemd/nacre-brain-consolidate.{service,timer}`: oneshot `nice 10`, idle
I/O, `%h/.local/bin/nacre-brain-maintain consolidate`, nightly 03:30 with a
20-minute random delay, `Persistent=true`. `ai/install.py`, `tools/configure.py`
and the packaging glob install the files; nothing enables the timer (the
packaged runtime's `timers.target.wants` links stay check-only). The owner
decides; to enable later: `systemctl --user daemon-reload && systemctl --user
enable --now nacre-brain-consolidate.timer`. On-demand runs use the CLI.

## Validation (dev-1 container `n0145`, synthetic data only)

`crates/nacre-helper/tests/brain_consolidate.rs` builds a git vault with a bare
remote and runs the real binary with `NACRE_BRAIN_TODAY=2026-06-15`:

- old checkpoint archived and the rest of the inbox triaged by every rule, the
  unclear old item archived with a `review-unsorted` digest, recent items kept;
  rooted wiki links and relative Markdown links rebased, a name link and a
  pre-existing broken link untouched (and reported);
- a stale Tailscale address and a stale revision flagged, a fresh LAN address,
  a decision and a superseded line not flagged, nothing written into the page;
- a newer note's different Tailscale address produces a contradiction linking
  the note and the wiki page; a "disable" decision is flagged as reversing the
  earlier "enable" decision;
- a proposed area promoted, an empty proposal retired, a near-duplicate auto
  area merged into its seed area, a tiny auto world folded (with its area moved
  and alias kept; its archived checkpoint's digest lands under the new world
  and area), seed entries unchanged; each change in the commit message;
- packs ≤ 4 KB with the fresh facts and the newest contradicting value as key
  facts, stale ones only under open questions; `nacre-brain pack` for an area,
  a merged alias and an area without a pack (world fallback);
- one commit, pushed; a rerun leaves the tree and HEAD unchanged;
- dry run read-only (no `.brain-state`), report refused inside the vault, an
  injected verification failure restores every file, an uncommitted edit defers
  the move it links to until it is committed;
- a `nacre-brain note` started while consolidation holds the lock finishes only
  after the lock is released and is not part of the run's commit.

Unit tests cover the extractors (entity choice, versions/subnets/loopback
excluded), fact classes, prose/generated-block separation and pack names.
Helper parity adds `nacre-brain-maintain/consolidate-help`,
`nacre-brain/pack-json` and `nacre-brain/pack-fallback`; the root help lists of
both commands gain `consolidate` and `pack` (intentional help-contract
extension, reviewed manually as in N0141).

Full `desktop` job of `.github/workflows/check.yml`, every run step replicated
in `ghcr.io/archlinux/archlinux:latest` (container `n0145`, 8 CPUs, 16 GiB,
nice 10, `cargo -j2`, no Chromium): `N0145 DESKTOP CI PASS` on 6bb4073 (fmt,
clippy `-D warnings`, workspace tests with 88 passing test binaries and none
failing, product shell/Luau/proof suites, cargo deny, i18n, `tools/check.py`
with 540 helper parity cases and 0 failures, galleries, daemon memory). No
timing flake needed a rerun. Log: `.lead/n0145-full-ci.log` in the worktree.

Size: product code about +2,640/−410 lines including in-file unit tests
(`brain_consolidate.rs` 1,750 lines after rustfmt, about 80 of them tests;
`brain_registry.rs` +236; `brain_migrate.rs` net +63 after extracting the
shared transaction). There is no Python original to compare with. The
simplicity pass reused the migration transaction, generators and resolver
instead of adding a second write path, and removed nothing else (no code is
replaced by this unit). Tests: 660 integration lines, 3 parity cases.

## Lead's first real run

1. `nacre-brain-maintain consolidate --dry-run --report ~/brain-consolidate-report.md`
   and read the report (moves, taxonomy changes, review queue).
2. `nacre-brain-maintain consolidate --report ~/brain-consolidate-report.md`;
   inspect `git -C ~/Documents/Nacre-Brain show --stat HEAD`; undo with
   `git revert HEAD` in the vault if needed.

Run the migration (N0141) first: consolidation assumes migrated headers.

Known limitation: wiki evidence blocks still match notes by the category names
as written, so notes of a merged area or folded world appear on the new page
only once their headers are re-filed (left to U8b, which rewrites pages and can
re-file). The viewer and search already resolve them through the registry.

No temporary duplicates are added. Out of scope: the agent pass that rewrites
"Current facts" (U8b), viewer UI (N0144), search internals.
