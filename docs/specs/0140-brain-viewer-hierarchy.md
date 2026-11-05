# N0140 — Brain viewer hierarchy

The Brain viewer presents the N0138 registry as worlds › areas, with Review as
a separate queue and discovery clusters available only as collapsed suggestions.
World and area pages expose their notes; note IDs remain stable when a file moves
to `archive/`. The graph uses registry world/area nodes and theme-token hues that
remain distinct and meet 3:1 contrast against both canvas backgrounds.

## Scope and behavior

Read the registry only. Seed and auto entries appear in the sidebar and overview;
proposed areas use a muted `New: …` label, and auto entries created within seven
days show a New badge. Review is a separate counted entry. Notes show world/area
chips; area pages include their wiki page. Archive and digest kinds are filterable,
with archives hidden by default. Discovered suggestions sample at most the 128
most recent unclassified notes and remain collapsed. New strings are translated.
The watcher uses the already-locked notify crate (inotify), excludes directory
symlinks and pauses after ten idle minutes without clients, then checks once on
the next request.

Filename collisions produce an explicit indexing error. New badges are refreshed
when serving cached data, so they expire even when no note changes. The obsolete
keyword color classifier, flat grouping projection and manual assignment UI are
removed; the registry remains read-only.
Legacy area aliases in `Worlds:` imply their registry parent for display. Moving
an open note preserves its selection and refreshes the reader's file path.

## Validation

Use only a synthetic vault with seven worlds, approximately thirty areas, two auto
areas (one created on the test date), one proposed area, fifteen Review notes and
2,000 notes. Headless Chromium verifies hierarchy expansion/navigation, Review,
collapsed suggestions, archive visibility, filename-stable IDs and no console
errors. Capture overview and graph in light and dark at 1600×1000 to
`.lead/evidence/`. Verify watcher idle behavior without periodic wakeups and
active update latency under five seconds. Run existing Brain Rust/Python/browser
checks, formatting, clippy, i18n, Prettier and the complete `desktop` job from
`.github/workflows/check.yml` in the task's bounded Arch container on dev-1.

Registry pages and counts always include the full vault. Classified notes skip
pairwise discovery. The existing navigation poll carries a cached graph revision
so visible clients refresh after mutations without another polling timer.
No temporary runtime duplicate is introduced. The existing imported Python
parity/rollback reference retains N0024's expiry: delete 7 days after trust in the
native worker, through the owner-approved cleanup list. Its CLI already delegates
to the native worker.
