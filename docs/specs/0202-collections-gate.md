# N0202 — Collections UI only on nacre-niri

Small fixes batch: hide desk-collection settings on compositors without
collections, and stop two per-render log lines and repeated font selection.

## Collections gate

N0301 found Settings > Wallpaper still showing the Collections section on stock
niri with `workspaceCollections=false`. Only one of its two rows is about desk
collections: "Per-desk wallpapers" (`wallpaper.collection.collectionWallpapers`)
was already gated by `visibleWhen: {capability: workspaceCollections}`. The other
row, "Collection folder" (`wallpaper.collection.folder`, the CLI `wallpaper.directory`
used for random/rotation picks), is the wallpaper collection and works on every
compositor, but it kept the section visible.

The folder row now sits in the Rotation section. Its id, store and search entry
are unchanged, so saved values and links keep working. The Collections section
now holds only the gated row and carries the same `visibleWhen` gate, so stock
niri and Hyprland no longer show the heading (N0301; N0316 B8). The folder stays
visible on every tier because rotation and random picks use it everywhere.
Moving it back into `wallpaper.collection` would hide it on those tiers too.
`workspaceCollections` comes from
the same compositor capability that drives the bar's `bar.collection.enabled`
and collection rows (`crates/nacre-shell/src/workspace_rows.rs`). No launcher,
command-catalog, keybind-call or settings-navigation entry for collections exists.
The bar control and the switcher popout were already gated.

Test: `settings_tests::capability_gates_collection_wallpapers_and_niri_workspace_preferences`
now checks that the section heading and its row are absent without the capability and
present with it, and that the folder row is present in both cases.

## Log spam and font selection

- `PROFILE stage=font_select` (`crates/nacre-shell/src/font_sources.rs`) now prints
  only when `bead::core::profile::enabled()` (`BEAD_PROFILE_INPUT`).
- `CLOCK_TICK value=HH:MM` comes from Bead, not Nacre:
  `crates/bead-services/src/runtime.rs:84` at the pinned `5c0e3136`, and `:91`
  on Bead main `def278e6`. It is an unconditional `eprintln!` in
  `Shared::change`. This is a handoff to the Bead lead; Bead is unchanged here.

The laptop journal for the last 4 h had 1062 `font_select` lines, 976 of them
the same `files=2 faces=2`. They had a median of 2.3 ms, 2.6 s CPU in total, and
came in bursts of up to 61 a minute during panel and app activity. Two kinds
of caller re-ran the selection on every panel open:

- On-demand panels such as the launcher (`retain_when_closed: false`) build a
  new Session and Runtime each time they open, which also gave them a new,
  empty per-runtime `Selector` cache.
- Each new Luau generation creates its own text meters (Bead
  `LuauFrontEnd::measured_width`). Their picker was the uncached
  `font_sources::select`, so they re-ran Fontconfig and re-read the fallback
  font files for a font set the process already knew. The chat meter used the
  same picker.

There is now one process-wide selection cache. It uses the same LRU and the same
superset lookup, a 512 KiB byte budget and at most 32 entries. The runtime
selector and the session and meter picker both use it. `UiEvent::Reload` clears
it, as it cleared the per-runtime cache before. A scene too large to cache is
no longer cached, and it no longer empties the cache either. Memory: one shared
budget replaces one 512 KiB budget per runtime, and an entry takes a few KiB.
Test: `font_sources::tests::fresh_selectors_and_text_meters_reuse_one_font_selection`.

Known limit: fonts installed while the shell runs are picked up after the next
reload or restart, instead of at the next panel open.

## Parity

No new surface. The Wallpaper page shows the folder row under Rotation on every
tier, and the QML shell shows it under Collections. This is a deliberate
difference, because collections exist only on nacre-niri. No paired nested
screenshots were taken.

## Checks

Run on dev-1 in `n0200-build` containers, finally on top of `737acfd81`.

- `cargo fmt --check`: passed after one rustfmt chain fix.
- `cargo clippy -p nacre-shell --all-targets --locked -- -D warnings`: passed.
- `cargo test -p nacre-shell --bin nacre-shell -- --test-threads=6`: 911 passed,
  0 failed, 4 ignored. On the first run one golden failed,
  `settings_pages_tests::current_scene_goldens`, because the folder row moved out of
  Collections. Both scales were regenerated and reviewed: Collections now opens
  with "Per-desk wallpapers". The fixture publishes no capabilities, so it shows
  the full page.
- `cargo test -p nacre-helper --lib`, which embeds the registry: 1067 passed on
  the earlier base `b8f613077`.
- `python tools/compositor_boundary.py`: passed.
- `python tools/i18n/check.py` fails on `737acfd81` itself, with or without this
  unit. N0213 left `panels.top-bar.modules.*` ids that are missing from the English
  catalog and context, and three `*.unavailable`/`lock-planned` ids that are
  now unused. It passed on `b8f613077` with this unit applied.
