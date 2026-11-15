# N0207 — Panel smoothness at 120 Hz

v0.1 item "Panel smoothness at 120 Hz (frame gaps of 12 ms or less)"
([checklist](../v0.1-checklist.md)). This spec sets the acceptance, lists the evidence on
main, and points to tonight's measurement.

## Acceptance

- **Panels:** dashboard (top), control centre (right, opened by hover and by click), AI
  sidebar (left), launcher (bottom), notifications, and the OSD and toasts.
- **Gate:** every open and every close has a maximum frame gap of **12 ms or less**. A
  120 Hz frame is 8.33 ms, so 12 ms allows one late frame and never a dropped one. The
  first frame shows the start pose: no pop-in, no blank or dark first frame, no re-open
  (standing rule "smoothness first").
- **Measurement:** a private nested session on the desktop TEST host, at a 120 Hz output.
  Run at least 10 opens and 10 closes per panel, and report the max, p95 and p50 gap. The
  numbers come from Bead's journal marker, one per finished motion:
  `PANEL_SLIDE name=<panel> dir=<open|close> first_frame_ms=… frames=… duration_ms=… max_gap_ms=…`
  (Bead `crates/bead-wayland/src/overlay.rs:102`; fields from `bead-core` `animation::Slide`;
  committed-frame clock since B0170).
- Nested numbers are supporting evidence. The lead's live audit on the owner's laptop
  (nacre-niri, 120 Hz) is the final gate (standing rule "live gate").

## Evidence on main

Bead pins, each with its tests green in Bead's hosted CI run `43605832816` on `841de925`
(the current pin):

| Nacre commit | Bead | What it fixes | Guarding Bead tests (all `ok` in that run) |
| --- | --- | --- | --- |
| `9792c40d1` | `a11b4491` (B0277, `dd61bc74`) | A stationary reveal now closes as its open reversed. It used to travel the whole surface and pop at about 78 % ([Bead spec 0263](https://github.com/nacre-desktop/bead/blob/main/docs/specs/0263-motion-tokens-and-retarget.md), "Reveal close") | `layer_motion::tests::stationary_reveal_close_is_the_reversed_open_with_no_plane_travel`, `…::stationary_reveal_close_sends_a_visible_contour_once`, `tests::reveal_close_mirrors_the_open_and_settles_into_every_anchor_edge` |
| `be72104b6`, `4d8713dba` | `bc76a09c`, `841de925` (B0278) | The edge lip holds its completed dwell until the panel is open; there is no rest-bar flash. A 1 s expiry covers an open that never arrives ([Bead spec 0278](https://github.com/nacre-desktop/bead/blob/main/docs/specs/0278-lip-open-continuity.md)) | `edges::tests::pending_open_holds_the_completed_dwell_instead_of_the_rest_bar`, `edges::tests::opening_expiry_rests_inactive_ignores_stale_and_just_clears_active` |

Frame-gap tests (simulated 120 Hz vblank, no compositor):

- Bead `protocols::closed_panel_unmap_tests::companion_slide_callbacks_at_120_hz_keep_committing_without_staging`:
  a 300 ms dashboard slide commits at least 34 frames with a max gap of 12 ms or less,
  stages nothing, and emits `PANEL_SLIDE name=dashboard … frames=37`.
- Bead `overlay::tests::osd_and_toast_motion_commit_every_refresh_at_120_hz`: the OSD and
  toast show and hide with a max gap of 9 ms or less, and each logs its marker.
- Bead `tests::revealing_panel_close_moves_every_part_as_one_scene_at_120_hz`.
- Nacre `reload::panel_animation_wakeup_tests::forty_slide_frames_do_not_wake_the_script_stage_for_scalar_animation`:
  the frames of a scalar slide do not wake the Luau script stage.
- Nacre `headless::service_views_tests::workspace_switch_ticks_every_refresh_at_120_hz_in_both_directions`.
- Nacre `layer_motion_wayland_tests::real_prompt_wire_sequence_and_stock_compositor_fallback`:
  the layer-motion wire protocol and the stock compositor fallback.
- Nacre `chat::tests::streaming_at_fifty_tokens_per_second_stays_inside_the_frame_budget`.

The Nacre tests were run for this unit (see the checklist row). Per-frame CPU cost
(`frame_cost_bench`, release build, scale 1.5, a 120 Hz slide of the real views; numbers in
[spec 0237](0237-design-review.md)):

- AI sidebar open: p95 0.28 ms, max 2.9 ms.
- Launcher open: p95 0.86 ms, max 3.1 ms.
- OSD and toast: every frame under 1 ms.

All of these fit well inside the 8.33 ms frame. They are render cost, not frame gaps.

## Measurement tonight and what still fails

[`docs/benchmarks/shell-motion-idle-20261114.md`](../benchmarks/shell-motion-idle-20261114.md)
(aba6a02c5) is a nested run with the swap gate waived, so it supports the verdict but is
not acceptance. The numbers live there. Against the gate:

- **Fails, Bead:** nearly every transition (230 of 236) misses the gate once, at its start.
  Bead commits the first 3-4 frames before the first frame callback, and then one vblank
  is skipped (a 12-20 ms gap). This is the only failure for the launcher, the control
  centre and the AI sidebar. Their mid-motion max is 8.4-10.6 ms. Fix: pace the first
  frames (benchmark top-3 item 1).
- **Fails, dashboard:** 5 mid-motion single-vblank misses (16 ms), and a late settle on a
  cold open. Fix: warm the panel views (item 2, N0231).
- **Passes:** the OSD and toasts (`PANEL_SLIDE` max 9.1-10.9 ms).
- **Measurement caveat:** the last frame of an open is committed only on the next unrelated
  wakeup, after up to 5 s. That puts a ~5 s `max_gap_ms` in every launcher and control
  `PANEL_SLIDE` marker, although nothing visible changes. Until Bead commits that repaint
  with the motion (item 3), judge the gate on the start and mid-motion gaps of the
  benchmark's frame trace, not on the marker's `max_gap_ms`.

Other gaps:

- No nested 120 Hz capture was taken for the D7 AI sidebar or the D11 launcher when they
  landed ([spec 0237](0237-design-review.md)). The benchmark covers both now.
- The N0316 tier-2 smoke logged one launcher open at `max_gap_ms=1013.6` ([spec 0237](0237-design-review.md),
  B4). It ran on a 60 Hz nested parent, so it is not a 120 Hz measurement.
- The item stays **active** until the three fixes above land and a re-run of the benchmark
  shows every panel at 12 ms or less, and the lead's live laptop audit agrees.
