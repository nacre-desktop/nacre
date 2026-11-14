# N0201 — Widgets on every tier

v0.1 item "Widgets on every tier" ([checklist](../v0.1-checklist.md)). This spec says
what the item means, what is proven, and the one check that remains.

## Tiers

From the checklist and [compatibility.md](../compatibility.md):

- Tier 1: **nacre-niri**, Nacre's niri fork (pinned r3110, `f405f1a9`).
- Tier 2: **stock niri** and **Hyprland**. They run the core shell and hide the features
  that need the fork.

"Widgets on every tier" means that on all three compositors, a user can:

1. see the default bar widgets;
2. open the Widgets app (Super+Shift+W);
3. add a widget to the desktop or the bar, and have it render and update;
4. drag a widget on the desktop, from the bar to the desktop and back;
5. remove a widget;
6. find the change saved (`settings.get {"store":"widgets"}` rev goes up, and the layout
   is the same after the app closes).

## What works where

The widget system does not depend on the compositor. Widgets are Luau views (`shell/widgets/`)
drawn on Bead layer surfaces: the bar, the desktop canvas and the drag ghost canvases
(`drag_visuals_are_quiet_and_drag_surfaces_are_layer_canvases`). No widget reads the backend
or a compositor capability. The only backend reference is the Workspaces widget's
`backend.workspace.switch` action. Three things differ per tier:

| Piece | nacre-niri | Stock niri | Hyprland |
| --- | --- | --- | --- |
| Widget rendering, add/remove, drag, save | same code | same code | same code |
| Workspaces widget content | collections strand ([N0202](0202-collections-gate.md)) | no collections; window hints partly | no collections; some glyph hints missing |
| Widgets app window | niri rule `config/niri/window-rules.kdl:18` | same rule | `config/hypr/conf/windowrule.lua:4`: float, centred, 70 % × 75 % of the monitor (N0318 B5, `2d76abd0d`) |

## Evidence

On main:

- `0ebbef466` (N0233, [spec](0233-widgets-save.md)): app Add/Remove now save the layout
  store (B6), and a second Pill copy no longer breaks the Pill detail (B7).
- `2d76abd0d` (N0318): Hyprland opens the Widgets app at its natural size (B5).

Tests (nacre-shell binary, compositor-free; run for this unit, see the checks below):

- `reload::widget_drag_preview_tests::widgets_app_add_and_remove_commit_the_layout_store`
- `reload::widget_app_tests::pill_detail_builds_with_two_pill_copies_on_the_bar`
- `reload::widget_editor_tests::{desktop_drops_land_where_dropped_without_a_grid_and_never_overlap,
  desktop_motion_moves_only_the_ghost_surface_and_writes_only_on_drop,
  bar_drops_stay_exactly_where_released_and_the_title_stays_centred,
  drag_visuals_are_quiet_and_drag_surfaces_are_layer_canvases,
  niri_rules_keep_pearls_unblurred_and_the_widgets_app_an_ordinary_window}`

Nested session (Platform N0316 smoke, Nacre `fcd337c0c`, *before* N0233; evidence in the
lead's `platform/n0316-tier2-smoke/evidence.md`, row 22):

- Stock niri: the Widgets app opens. All 12 widgets show under "On your screen", but none
  was saved (B6, now fixed). A layout written straight to the store rendered the desktop
  clock on a background layer surface.
- Hyprland: the app opens. Add was not attempted, because the window was cramped (B5, now
  fixed).
- nacre-niri: not part of that smoke.

No add or drag run on any tier has happened since `0ebbef466`. That is the only open part
of this item.

## Remaining: tier smoke (Platform)

Run it on the desktop TEST host, in private nested sessions only, with each run under
`flock ~/nacre-lead/desktop-heavy.lock`. Use the N0316 harness
(`~/.local/share/nacre/ai/lead/platform/n0316-tier2-smoke/harness/`: `run.sh`, `n0316.py`,
`virtual-pointer.c`) on a Nacre build of origin/main (≥ `0ebbef466`). Components are
`configs,shell`, the output is 1920×1080, and there are three runs: `--compositor niri`
(stock), `hyprland` and `nacre-niri`.

The harness has `click`, `move` and `scroll` but no drag op. Add one beside `click`
(`n0316.py:379`) that calls `tools/virtual-pointer X Y W H drag X2 Y2 MS` on the nested
display. The tool already supports `drag` (left button held while it moves).

Read every coordinate from the screenshot taken just before the step. The N0316
coordinates for the Clock row and "Add to desktop" (`384,466`, `1547,378`) are a starting
point only.

| # | Step | Pass |
| --- | --- | --- |
| 1 | Start the session. Take `shot`. Run `call settings.get {"store":"widgets"}` | bar shows launcher, workspaces, title and status pill; record the store `rev` as R0 |
| 2 | Super+Shift+W | Widgets app opens at its natural size, not its minimum, on all three |
| 3 | Clock row → "Add to desktop"; `call settings.get {"store":"widgets"}` | rev = R0+1; the `canvas` host has one `clock` instance; the clock is drawn on the desktop and its minute changes within 60 s |
| 4 | With the app open, `drag` the desktop clock by (+300, +150) over 400 ms; `call settings.get` | the ghost follows during the drag; after release, rev = R0+2 and the instance origin has moved by the drag delta (no grid snap, no overlap) |
| 5 | `drag` the desktop clock onto the status pill on the bar, then drag it back out to the desktop | each drop raises rev by 1; the pill takes the clock as a child, then gives it back; the title stays centred |
| 6 | Clock row → Remove; `call settings.get` | rev goes up by 1; the clock is gone from the desktop |
| 7 | Clock → "Add to bar", then Super+Q to close the app; `call settings.get` | rev goes up by 1; the clock stays on the bar after the app closes |
| 8 | `logscan` | shell and daemon: 0 `UI_ERROR`, 0 "Reload failed", 0 RPC errors |

The item is **done** when all 8 steps pass on stock niri, Hyprland and nacre-niri. Record
the three runs (commit, compositor version, pass or fail per step, and the screenshot
names) in one short table in this spec. Raw logs stay on the host.
