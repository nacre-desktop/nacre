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

The initial path added a separate libmpv instance while the preview played and
used software decode, PNG transport and normal image decode/upload. This avoids
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

## N0166 — preview CPU follow-up

Software measurements and full dev-1 desktop CI passed. Intel hardware
decoding and real-shell smoothness remain for the lead's live audit.

The preview now requests the existing renderer's `vaapi-copy` path, retaining
libmpv's software fallback. The live renderer already requests VA-API: its GL
path imports decoded surfaces, while its software path uses copy-back. See
the [mpv hardware decoding reference](https://mpv.io/manual/stable/#options-hwdec).
There is no new video decoder or GPU transport.

Only previews select libmpv's fast software conversion, disable zimg and request
bilinear scaling at the requested raster size. This uses the same resampling
choice as the existing GL renderer. The decoder still receives the source at
its original resolution. These are existing
[software scaler options](https://mpv.io/manual/stable/#software-scaler), applied
before loading the source; diagnostic logs confirm all three were accepted.
Fast conversion and bilinear resampling can differ from the prior software
defaults in chroma detail and sharpness; the lead's visual audit must judge that
tradeoff. The live renderer's configuration is unchanged.

PNG frames now pack opaque RGB into the existing render buffer's prefix,
instead of encoding an alpha channel that was always filled with 255. The next
mpv render overwrites the entire buffer. Fast compression and the Sub filter,
the framed-PNG protocol, dimension/payload bounds and 30 fps cap are retained.
Together, these changes reduce median frame sizes from 153,985 to 67,646 bytes at 940x256 and
327,521 to 127,450 bytes at 1410x384. There is no additional frame buffer.

The original N0163 harness was reused on dev-1: optimized binaries, identical
1920x1080 30 fps testsrc2 MPEG-4 input, five alternating 10-second preview/poster
pairs at each raster size for each phase (control phase, then candidate phase).
Each child is fresh; CPU includes all producer threads and Python/Pillow
decode/hash time. No task build or CI overlaps those windows. The table reports
medians as percentages of one core, rather than whole-shell CPU.

| Raster | Control producer | Candidate producer | Control + reader | Candidate + reader | Reduction | Control / candidate fps |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 940x256 | 12.19% | 9.29% | 20.96% | 17.12% | 18.3% | 29.87 / 29.87 |
| 1410x384 | 17.69% | 10.79% | 35.83% | 26.92% | 24.9% | 29.78 / 29.78 |

Per-run total CPU, in capture order:

| Raster | Control, five runs | Candidate, five runs |
| --- | --- | --- |
| 940x256 | 20.96, 21.35, 20.88, 21.38, 20.58% | 17.56, 17.29, 17.01, 17.12, 16.43% |
| 1410x384 | 35.63, 35.31, 35.86, 35.83, 35.88% | 27.65, 26.48, 27.28, 26.65, 26.92% |

Retained-poster CPU remains below 0.06% median. Candidate first-frame medians
are 51 and 56 ms (control: 56 and 69 ms). Every sampled stream has the requested
dimensions, a nonblack first frame and advancing frames. A separate 1600x1000
probe also delivered advancing, bounded frames. These are software results:
libmpv explicitly reports software decoding because MPEG-4 is outside its
hardware-codec whitelist. The container has no VA-API device. Intel VA-API
validation, compositor pacing, whole-group memory and real-shell UI_ERROR checks remain
unmeasured; the desktop TEST host was unreachable, so no visual capture ran.

Experiments retained only in private evidence explain the selected small change:
uncompressed PNG reached 16.11/26.79% but enlarged retained frames to roughly
0.96/2.17 MB; Fast/NoFilter regressed to 24.14/40.75%; Fast/Up measured
21.46/36.31%; fast software conversion with RGBA measured 18.57/28.71%.
The selected RGB path keeps compressed frames smaller while retaining 30 fps.
No experimental product paths or temporary duplicates remain.

Control binary SHA-256: `6e5efee91d571538e86dde422a1665330bddf17044a472c6bd39932a63f878eb`.
Measured candidate SHA-256: `1db9361be02cd2947883dd22afff575d27b438fbb1020006e199c6f00c3b2054`.
Control source: `5ae5309d1`; candidate: that base plus the N0166 product diff
(final comment edits do not change behavior). Raw JSON, logs, original harness
and the desktop CI recipe are retained in private task evidence.

The complete `desktop` job was rerun from the beginning and finished with exit
code 0 in the task's 8-CPU/16-GiB Arch container: workspace formatting/Clippy,
examples/tests, minimal consumer, release helper, trace-memory tests, all feature
checks and required binaries, Luau (189 modules), native wallpaper proofs,
cargo-deny, npm, translations (1,787 messages), `tools/check.py`
(404/138/54/376 tests), AUR gallery and the isolated daemon raster-memory case.
The shell suite passed 871 tests with 4 intentionally ignored; the real preview
stream integration passed. This validates the N0166 diff on `5ae5309d1`.
An independent N0160 lock-background change landed as `2a19b5467` during CI;
integration preserves it without changing the measured preview path.

The first CI attempt exposed the private Docker tmpfs's default `noexec` flag;
remounting that task-only temp directory with `exec` fixed fixture execution.
That attempt also emitted an LLVM SIGSEGV diagnostic during the release-helper
build; the same Cargo invocation launched another compiler and completed
successfully. No stack/profile override, test, guard or threshold was changed.
The final complete run passed after the mount repair. N0166 product delta:
13 added / 8 removed, net +5 lines; no new product files or dependencies.
