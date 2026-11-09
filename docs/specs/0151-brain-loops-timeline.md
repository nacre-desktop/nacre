# N0151 — Brain open loops and timeline (U11)

Status: implemented. Exact verification results and evidence are recorded in
`.lead/report.md`.

The deterministic index derives outstanding work and dated history from notes.
No extracted result is written back into note bodies. This extends the existing
consolidation, background watcher, Wiki viewer and capped context packs; it adds
no service, model call or registry write. Curate, Ask and search ranking remain
outside this unit.

## Derived contract

`.brain-state/index/loops.json` contains loops and a timeline. Each loop carries
its text, source note ID/path, recorded date, canonical worlds/areas, kind
(`todo`, `question`, `waiting`, `blocked`), status and closing evidence when
closed. Review flags use their explicit source path or a note in the flagged
area/world; an item with no source note remains in Review rather than gaining
a broken source link. Inputs include unchecked boxes, Next/TODO/Pending/Blocked/
Waiting on/Open question(s) prefixes, curated Open questions lists and Review items.
A strictly later note in the same area/entity can close an item with a matching
checked box, completion wording sharing a unit/PR/commit ID or substantial term
overlap, or explicit supersession. Different explicit IDs prevent overlap-only
closure. An explicit ID on the task takes precedence over an ID in its source
title; checked items match task text rather than closing every task in a unit.
Same-day closures require a strictly later Recorded timestamp. Fenced examples,
generated evidence, pending task lines, packs and digests cannot close loops.
Historical loops and their evidence remain inspectable in the index. Closure is
a deterministic wording heuristic; date-only notes on the same day stay open.

Timeline events use Recorded dates, titles and short body digests, grouped by
calendar day and ISO week, newest first. Digests appear as events; generated
context packs do not. Source links open the existing reader.
The viewer adds a global Open loops entry and scoped Open and Timeline sections
on wiki pages. Kind filters and all labels use the existing translation system.

`nacre-brain loops [--area A] [--world W] [--kind K] [--json]` exposes outstanding
items. Packs include at most five scoped open loops while retaining their
4,096-byte cap. Freshness guidance stays under Open questions, and task state
under Open loops. Existing refresh paths rebuild the derived file.

## Validation and resource scope

Synthetic vaults only. Builds and every run step of the full `desktop` CI job
run on dev-1 in one Arch latest container named `n0151`, limited to 8 CPUs and
16 GiB, with nice 10 and Cargo -j2. Chromium is absent from CI. Browser suites
run separately under the desktop test host lock with task-private binaries,
profiles, environment and HOME; light/dark screenshots are 1600×1000.
Evidence and exact results are recorded in `.lead/report.md` after execution.

Always-on memory budget: zero outside the existing on-demand viewer/helper;
the derived arrays live only while those existing processes run. No temporary
duplicate implementation or obsolete alternate path is introduced.
