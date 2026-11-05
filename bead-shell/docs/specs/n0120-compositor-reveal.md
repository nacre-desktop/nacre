# N0120 — Nacre compositor-driven panel reveals

Status: candidate; focused reference export passed against the current Bead WIP
planner, while compositor parity, final CPU measurements, exact-revision full CI,
and live battery acceptance remain pending.

## Scope and behavior

Dashboard, launcher, and control-centre openings advertise their existing joined
frame silhouette as a compositor reveal descriptor. Nacre supplies one prepared
content atlas plus the route's root-local body geometry, four radii, quadratic-cut
corner mode, optional rim tokens, fill, clips, and animation timing. Niri samples
the body size, corner radii, content clip, rim, and shadow with the same eased
progress as the content plane. Unsupported protocol/effect combinations retain the
existing stock Niri path and frame-renderer fallback.

At setup Bead also captures the full-open endpoint once. The compositor selects
that immutable snapshot when the opening completes. This preserves the legacy
renderer’s per-primitive 8-bit blending: flattening translucent content over
transparency first changes some control-centre endpoint pixels by two channel
values. The animated contour, rim and shadow remain compositor-owned throughout
the opening. Two setup paints replace refresh-driven paints; both snapshots share
Bead’s 64 MiB limit. The route test independently requires the endpoint snapshot
to match the legacy renderer byte-for-byte, and its fixture metadata refers to the
existing `t1.00.png` without changing any reference image or pixel tolerance.

Coordinates are layer-buffer local for content planes and output-root local for the
reveal descriptor. The layer-surface global origin is applied once by the protocol
presenter. Dashboard content stays at (0,0) for its opening; its body grows down
from y=32. Launcher content moves from y=590 to y=0 while its body grows upward
from y=622. Control-centre content moves from x=424 to x=0 while its body grows
leftward from x=456. The prepared content frame is made from the OPEN resting scene;
staged partial prepared frames are discarded before baking it. Per-frame CPU
references pose clones against that one open frame and apply motion opacity.

The route timelines are taken from the runtime plan: dashboard 360 ms EaseInOut,
launcher 300 ms EaseOut, control centre 280 ms EaseOut. Reference manifests sample
elapsed t = 0, .25, .5, .75, 1 and record the actual eased progress (dashboard
.0625 at t=.25; EaseOut routes .578125). Route references include scale 1 and 1.5.
PNG bytes are full-buffer premultiplied RGBA8, with origin (0,0).

## Evidence and validation

The route plans, atlases, frames, and manifests are in
[`crates/nacre-shell/tests/fixtures/bead-layer-motion/reveal-goldens/`](../../../crates/nacre-shell/tests/fixtures/bead-layer-motion/reveal-goldens/).
The exporter correction clears any scene-prepared partial snapshot before preparing
the OPEN worker frame. The focused route exporter passed on dev-1 in the
`n0120-gpu-reveal` Arch container (`--cpus=8 --memory=16g`):

```text
NACRE_REVEAL_REFERENCE_DIR=/work/nacre/crates/nacre-shell/tests/fixtures/bead-layer-motion/reveal-goldens \
nice -n 10 cargo test --locked -j2 \
  -p nacre-shell --bin nacre-shell \
  reload::shell_route_smoke_tests::real_shell_panel_routes_open_and_close_with_prepared_scenes \
  -- --nocapture
```

Result: 1 passed, 0 failed; exit 0. A second run without
`NACRE_REVEAL_REFERENCE_DIR` passed the same route test, verifying all exported
plans, atlases, and frame PNGs byte-for-byte (1 passed/0 failed; exit 0).
Both commands used an external Cargo path patch to Bead WIP `layer_motion.rs` (SHA-256
`af28cf108cfb7afb211463fdfb5f8ebe64a371145ae89c8b497495a127705638`) for this
export and verification. Because the external patch changes Cargo's source
resolution, the container temporarily resolved a patched lock and restored the
task's checked-in Cargo.lock on exit. The five Nacre dependency declarations,
eleven Cargo.lock Bead package sources, product revision guard, XML fixture, and
fixture provenance now point to reviewed Bead commit
`9b7ae08a588a4f27ba30e5e0aee20f15c8138d21`; exact-pin desktop CI remains pending.
The generated dashboard s1.0 plan has content
destination y=0→0; the shape still grows from the bar edge. The generated set
contains 30 frame PNGs, six content atlases, 36 JSON manifests/plans, and one
fixture README (73 files total).

The compositor's first comparison against the old dashboard reference exposed a
real planner issue: an absent reveal `slide_distance` means stationary content,
but the planner used a full-height offset. Bead WIP now preserves zero translation
for that case while explicit distances and full-scene close offsets remain intact.
The corrected references are available to the NN0016 GLES comparator. Full GLES
all-pixel parity and visual equality at both scales are not claimed yet.

Initial public-runtime synthetic measurements on Bead 446f3979 were two alternating
pairs of 64 refreshes at 120 Hz, scale 1: dashboard 3.44–3.54 ms client CPU per
refresh, launcher 3.52–3.64 ms, control centre 1.96–2.03 ms. These are preliminary,
not final: they predate the no-distance planner correction, and setup work is
separate. Final candidate CPU measurements must use the final Bead SHA and report
one-time setup separately. The 552 fixture log records 3.42–3.63 ms CPU per refresh
before vs 0.00075–0.00153 ms after for dashboard/launcher and 1.87–2.06 ms vs
0.00106–0.00134 ms for control; it has only two pairs and incomplete final source
and scheduling provenance, so it is not final acceptance evidence.

Pending: NN0016 all-pixel GLES parity and per-frame render cost; final CPU rerun
against the committed Bead planner; exact-pin Nacre desktop CI including
`tools/check.py` and >=100k property suites; lead live battery audit. No physical
battery result is claimed.
