# N0163 — dashboard wallpaper playback and transport

Owner request: 2026-11-10. Nacre shell only; no Bead pin change.
Status: implemented; full dev-1 desktop workflow passed; CPU cost measured.
Desktop TEST captures are
deferred because the owner is using that machine. No live install is authorized.

The Today hero previously decoded one still poster through
`nacre-wallpaper-decode`. Dynamic/pause flags changed its controls, but no video
frames reached its Image. The wallpaper transport also put 32-high buttons in a
38-high pill, limiting their hit/hover rectangles and centering glyphs above the
pill's center.

The transport now has a passive clipped container and three equal, full-height
buttons with centered glyphs. Its dimensions, icon font/size and radius derive
from kit tokens; the outer footprint remains 228 by 38 at default tokens.

The existing `bead-wallpaper-video` binary gains a bounded `--preview` mode using
the same libmpv binding and software renderer. One request on stdin describes a
local, descriptor-pinned source and dimensions; stdout carries length-prefixed
PNG frames. There is no new video decoder implementation, binary, daemon,
installed service, persistent cache or per-frame filesystem output. The preview
mode is a generic source/size/frame output, not a Rust implementation of the UI.

Today owns at most one child and one latest-frame slot, caps raster size at
1600 by 1000 and delivery at 30 fps, and rejects frame payloads above 8 MiB.
The child loops without audio. The Image's model key changes for each delivered
frame, triggering normal Luau scene preparation; cropping, overlays and layout
remain in the shell. The poster remains the loading image and failure fallback.
Source changes replace the child and discard its frames. A failed attempt is
retained until its request or permission changes, avoiding a respawn loop.

Playback requires the open Today page, valid motion metadata, enabled motion,
an unpaused wallpaper, allowed battery mode and an unlocked, awake session.
Closing, switching tabs or changing those policies stops and reaps the exact
owned child and joins its reader; the close grace retains only the poster.
Desktop window coverage continues to govern the desktop wallpaper rather than
pausing a visible preview above that window. Existing settings defaults apply.

## Evidence and limits

Deterministic checks cover full-height hit rectangles and glyph centering at
scales 1 and 1.5, motion/power/session decisions, bounded/truncated frame reads,
changing decoded video frames, and rejection of invalid source identities and
dimensions. Existing poster loading/disconnect/close tests remain in place.
The 75 focused dashboard tests passed. The reveal reference update changes only
the pill's three glyphs: 459 pixels at scale 1 and 867 at scale 1.5. Eight
dashboard PNGs changed (atlases and visible reveal samples); all plan/timing
metadata and other routes remain byte/pixel-identical. Deterministic crops were
reviewed for centering. These references belong to the transport commit.
The complete Nacre `desktop` workflow was replicated in the task's Arch
container on dev-1 (8 CPUs, 16 GiB). All required commands completed:

- Workspace formatting/Clippy with warnings denied, examples, workspace tests,
  minimal protocol consumer and optimized helper build passed. Shell tests:
  871 passed, 4 ignored; the new preview integration passed.
- Trace memory-policy tests, no-default/all-feature checks and required shell,
  video renderer and raster worker builds passed.
- `python tools/check_product_shell.py --analyzer /runner-temp/luau/luau-analyze`
  passed 189 Luau modules and native wallpaper renderer proofs. `cargo deny
  --manifest-path crates/nacre-shell/Cargo.toml --all-features --config
  crates/nacre-shell/deny.toml check` passed advisories,
  bans, licenses and sources. Translation coverage: 1,787 messages.
- `dbus-run-session -- xvfb-run -a /tmp/check-env/bin/python tools/check.py`
  passed tools/AI/Brain/shell-tools suites (404/138/54/376 tests), source guards,
  palette CLI and JavaScript checks. The AUR review gallery passed.
- `NACRE_MEMORY_BINARY=/work/target/release/nacre-helper cargo test -j2 --locked -p nacre-helper --test daemon
  memory:: -- --ignored --nocapture` passed its one isolated regression case.

After private runner setup repairs (priority, tool directory and temporary
fixture placement/ownership), the final combined tree completed the entire
workflow in one run with exit code 0. No product guard or threshold was weakened.

The simple path adds a separate libmpv instance while the preview plays and
uses software decode, PNG transport and normal image decode/upload. This avoids
a cross-process GPU texture/surface framework, at a CPU and memory cost that
must be reported. It does not animate every wallpaper-picker tile.
Headless measurements do not establish compositor smoothness
or the owner's live-session acceptance. No temporary duplicate path is retained.

Product delta: 383 added / 18 removed, net +365 lines. Tests: 335 added /
3 removed, net +332. The new code is the bounded frame transport, source
validation, child lifecycle and dashboard wiring: there was no existing
embedded preview path to enable. No general media framework or service was added.

## CPU measurement

Optimized `bead-wallpaper-video --preview`, software/headless backend, generated
1920x1080 testsrc2 MPEG-4 at 30 fps. Five alternating 10-second preview/poster
pairs at each raster size; each preview starts a fresh child, each poster is
decoded once and retained. CPU is process user+system time (all decoder threads)
plus Python/Pillow reader decode/hash time. Percentages are of **one core**.

| Preview raster | Producer CPU | Producer + reader CPU | Retained poster CPU | Paired CPU increase | Stream fps |
| --- | ---: | ---: | ---: | ---: | ---: |
| 940x256 | 11.0% | 18.76% | 0.026% | +18.73 points | 29.88 |
| 1410x384 | 17.9% | 35.23% | 0.054% | +35.18 points | 29.87 |

These are medians, not whole-shell CPU figures. Total preview CPU ranges were
16.66–20.12% and 31.16–36.73%. Shell projection, layout, rendering/upload and GPU
costs are excluded. Whole-group preview memory and real compositor pacing remain
unmeasured; the daemon-memory regression above does not substitute for them.
All sampled streams had the requested dimensions, a nonblack first synthetic
frame and advancing frames. Median first-frame latencies were 53 and 65 ms;
the first cold process took 2,293 ms (233 frames in its 10-second window).
The retained poster covers loading. This cost is paid only while an allowed
dashboard preview is open; deterministic teardown tests verify child retirement.
Raw pairs and the one-off harness are retained in private task evidence.
