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
or a compositor capability. The only backend references are the Workspaces widget's
`backend.workspace.switch` and `backend.window.focus` actions. Three things differ per tier:

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

### Tier smoke results (Platform, 2026-11-16)

Desktop TEST host, private nested sessions, 1920×1080, components `configs,shell`, Nacre `f3997131c`
(origin/main, release build, held pointer for every click and drag). Compositors: stock niri 26.04
(8ed0da4), Hyprland 0.56.2 (efb5099, no crash), nacre-niri `26.04.r3123.g95ea41bb`. R0 = 0 on all
three. Runs and shots: `niri1`/`niri2`, `hypr1`, `nn1` (`<run>-NN-name.png`). The Hyprland window was
moved off the clock with `hl.dsp.window.move` and the niri ones with `move-floating-window`, so the
desktop clock was visible and grabbable.

| # | Stock niri | Hyprland | nacre-niri |
| --- | --- | --- | --- |
| 1 | PASS, rev 0 (`niri1-01-bar-idle`) | PASS, rev 0 (`hypr1-01-widgets-open`) | PASS, rev 0 (`nn1-01-bar-idle`) |
| 2 | PASS 1330×765 (`niri1-03-widgets-app`) | PASS 1344×810 (`hypr1-01-widgets-open`) | PASS 1330×765 (`nn1-02-widgets-app`) |
| 3 | PASS rev 1; clock 00:19 → 00:23 (`niri1-05-…`, `-23-clock-minute`) | PASS rev 1; 00:27 (`hypr1-18-clock-t2`), but first draws as "00:2" | PASS rev 1; tens digit 2 → 3, last digit clipped (`nn1-04-…`, `-14-clock-t1`) |
| 4 | PASS rev 2; ghost shown; moved (+300, +152) (`niri1-08-mid-drag`, `niri2-04-after-drag`) | PASS rev 2; ghost shown; moved (+300, +150) (`hypr1-06-mid-drag`, `-07-after-drag`) | PASS rev 2; ghost shown; moved (+300, +152) (`nn1-05-mid-drag`, `-06-after-drag`) |
| 5 | **FAIL** (`niri1-12-settled-bar`, `niri2-05-after-drop-pill`) | **FAIL** (`hypr1-08-after-drop-pill`) | **FAIL** (`nn1-07-after-drop-pill`, `nn1-08-bar-crop`) |
| 6 | PASS rev +1, desktop clock gone (`niri2-08-after-remove`) | PASS rev +1 (`hypr1-12-after-remove`) | PASS rev +1 (`nn1-11-after-remove`) |
| 7 | PASS rev +1, clock stays in the pill after Super+Q (`niri2-09-after-close`) | PASS (`hypr1-13-after-close`) | PASS (`nn1-12-after-close`) |
| 8 | PASS: shell 0 `UI_ERROR`, 0 "Reload failed", 0 RPC errors | PASS | PASS |

**Step 5 fails on all three tiers.** Dropping the desktop clock on the pill (at 1520, 1620 and 1650,
25) raises rev by 1, but the clock becomes a separate `top_bar` instance (region right, start 0)
beside the pill, not a pill child. On the two niri tiers the bar then draws an oversized dark panel
over the top right (about 285×265 px, with a stale "00:2x"); on Hyprland the new bar clock is not
drawn at all, and I could not drag it back out. Dragging the pill's own clock out to the desktop
works on all three (rev +1, `canvas` clock at 220×92, drawn "00" clipped) and the title stays
centred. Other findings: the digital desktop clock clips its last digit at some times (seen
00:24 to 00:30, on all three tiers); a drop settles for over 1.4 s; the niri tiers move the dropped
y by 2 px.

The item stays **open** until step 5 is fixed and re-run on all three tiers. Raw logs and all shots:
`~/.local/share/nacre/ai/lead/platform/v01-tier-smoke-20261115/`.

#### Step 5 rerun on `4203fe91d` (Platform, 2026-11-16)

Same host, harness and tiers; Nacre `4203fe91d` (release build); hosted CI pending at test time (run
43782433668 contains `4203fe91d`). The desktop clock was dragged from its slot onto the pill
(`niri3`, `hypr3`, `nn3`), then out to the desktop.

| Tier | Drop on the pill | Drag back out | Clock text | Dark panel / undrawn clock |
| --- | --- | --- | --- | --- |
| Stock niri | PASS at x 1620 (rev 1 → 2): pill child `clock` (centre), no `top_bar` clock, canvas empty (`niri3-07-after-drop-1620`) | PASS (rev 3): `canvas` clock again (`niri3-09-after-dragout`) | full "02:06" / "02:07" | **Still shows**: oversized dark panel over the top right while the clock sits in the pill, still there 8 s later, gone after the drag out (`niri3-08-bar-after-drop-1620`, `niri3-12-redrop-t8`) |
| Hyprland 0.56.2 | PASS at x 1520 (rev 1 → 2): pill child `clock` (left), clock drawn in the pill "02:08" (`hypr3-02-after-drop-1520`) | PASS (rev 3) (`hypr3-03-after-dragout`) | full "02:07" / "02:08" | None: no panel, clock drawn |
| nacre-niri | PASS at x 1520 (rev 1 → 2): pill child `clock` (left) (`nn3-02-after-drop-1520`) | PASS (rev 3) (`nn3-03-after-dragout`) | full "02:08" / "02:09" | **Still shows**: same dark panel (`nn3-02-after-drop-1520`) |

Logscan: shell 0 `UI_ERROR`, 0 "Reload failed", 0 RPC errors on all three. The clipped last digit seen
at 00:24 to 00:30 did not appear at 02:06 to 02:09; not retested at 00:xx. Step 5 is fixed in the
store on all three tiers; the dark panel on both niri tiers is the open visual defect. Evidence:
`~/.local/share/nacre/ai/lead/platform/v01-tier-smoke-20261115/rerun-4203fe91d/`.

#### Step 5 rerun on `a84900b9a`, niri tiers (Platform, 2026-11-16)

Nacre `a84900b9a` (release build; hosted CI pending at test time, run 43786147276 is on `a84900b9a`),
stock niri 26.04 and nacre-niri r3123, same driver as the `4203fe91d` rerun. Desktop clock dragged
from (960, 540) onto the pill at (1620, 25), bar shot 1 s and 8 s after the drop, then dragged out.

| Tier | Drop | Bar at +1 s / +8 s | Drag out | Logscan |
| --- | --- | --- | --- | --- |
| Stock niri | rev 1 → 2: pill child `clock` (centre), no `top_bar` clock | **No dark panel**, "02:39" drawn in the pill (`x5niri-02-drop-p1s`, `x5niri-03-drop-p8s`) | rev 3, `canvas` clock again | 0 `UI_ERROR`, 0 "Reload failed" |
| nacre-niri | rev 1 → 2: pill child `clock` (centre) | **No dark panel**, "02:40" in the pill (`x5nn-02-drop-p1s`, `x5nn-03-drop-p8s`) | rev 3, `canvas` clock again | 0 `UI_ERROR`, 0 "Reload failed" |

The oversized panel is gone on both niri tiers. With Hyprland passing at `4203fe91d`, all 8 steps now
pass on all three tiers (steps 1-4 and 6-8 from the `f3997131c` run). Not retested: the clipped
last digit seen at 00:24 to 00:30 (the later runs were at 02:xx). Evidence:
`~/.local/share/nacre/ai/lead/platform/v01-tier-smoke-20261115/rerun-a84900b9a/`.
