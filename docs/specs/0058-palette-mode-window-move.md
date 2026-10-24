# N0058 — Palette query mode and guarded window move

Status: Implemented; guarded window move deferred. Consumer: Bead B0077/B0079. The
additive menu query uses the existing `MenuQueryResult` and opaque `menu.activate`
ticket.

## Palette route

Use `menu.query {mode:"palette",route:"palette",query}`. The route returns:

- Empty effective query: launcher-eligible `DesktopActions` in source order.
- Non-empty effective query: matching actions, matching windows in compositor order,
  then at most eight fuzzy application results.
- Matching removes one leading `>` plus following whitespace, lowercases and trims,
  then requires each whitespace-delimited word in `name + " " + description`.
- `brain:` after that normalization returns one brain-query row. Its value follows
  QML's `query.slice(6).trim()` behavior.
- Applications use the existing `app-search.js` ranking port. App candidates still
  receive normal engine app activations; actions/windows receive opaque UI tickets.

`MenuQuery.mode` remains an open string, so the generated schema has no new required
fields. The description and protocol transcript are regenerated from Rust.
Palette window IDs/workspace IDs are retained separately from the launcher's indexed
window fields so palette metadata does not invalidate the launcher query cache.
Bead calls `menu.query {mode:"palette",route:"palette",query}` for both the empty
palette and typed queries. Activation returns existing action/window UI rows or the
existing application launch result; no new activation method is introduced.

## Guarded window move — deferred

This half is spec-only. Correct admission must re-resolve both the window and target
workspace while holding the shared backend lock, including cross-output workspace
targets. The existing `Control::scoped_action` protects one entity and one output;
reusing it would leave a race between the two stable-ID checks. A proper two-entity
path needs separate niri and Hyprland fake-backend cases for missing/reused IDs,
cross-output guards and action dispatch, which exceeds this unit's roughly 200-line
product-code simplicity bound.

Follow-up request: `window.move {window_id,workspace_id,output?}`. Missing/stale window
or workspace IDs, or an `output` guard mismatch against the window's current output,
must fail closed before dispatch. The workspace may belong to another output. Do not
replay after a lost mutation reply. Tests must cover stale IDs and cross-output guards
on both backends.

## Temporary duplicates

None added.

## Validation

The generated protocol export is current. The remote Arch `desktop` job passed on
the task commit: Rust formatting, Clippy with warnings denied, workspace tests,
minimal consumer, release helper, Python/QML checks, 534 parity cases, and both
translation galleries. The Rust test harness used
`RUST_TEST_THREADS=1 RAYON_NUM_THREADS=2` to avoid an unrelated shared-host timing
failure in the existing Colors preview guard.
