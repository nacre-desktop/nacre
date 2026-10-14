# Rust runtime migration

Installed runtime target: no Python processes or Python/Pillow dependencies.
QML UI, command names, protocols and existing user state remain compatible.
Developer-only Python tools and test harnesses remain permitted.
| Wave | Status | Numbers / decisions |
| --- | --- | --- |
| 0 Orient decoder | Complete; quiet timings deferred | Exact decoder/dev6; busy helper CPU/RSS baseline recorded. |
| 1 Service / frequent helpers | Complete; quiet timings deferred | 13 ports; RSS 26.95 -> 10.21 MiB (busy); Source/install CI green. |
| 2 Compositor / pearls | Compilers + pearl-state public; backends local | Pearl CPU load/save 88.63/92.91 -> 2.82/3.85 ms (busy). |
| 3 Wallpaper / colors | Pending | Direct Orient crates; preserve paired publication. |
| 4 Remaining shell tools | Pending | Preserve privilege and session policy. |
| 5 AI | Pending | Claude CLI stream-json; retain existing permissions/history. |
| 6 Brain | Pending | bge-small vectors cosine >= 0.9999 or backed-up reindex. |
| 7 Installer / proof | Pending | Disposable installs and nested sessions; zero Python launches. |

Baseline references: `performance.md` (~105 ms launcher, ~115 ms dashboard,
~225 ms wallpaper pair, ~190 ms palette pair; fresh Python helpers 19–58 ms,
~33 MiB helper RSS). These historical figures are not new matched measurements.

2026-10-13: Orient decoder/dev6 `0f2200f` and RPC/packaging `2ebc2ca` are public;
all CI green. Nacre final pin/adapter/package follow-up is public through `9d97b1d`.
49 frozen inputs / 55 frames and 160 PNG/GIF/WebP/JPEG palette cases exact.
Rust/Clippy/fmt, 67 Python tests, both goldens and sdist/wheel smoke pass.
Combined Nacre gate: 789 tests; syntax/lint/QML/JS/config and CI pass.
Shared Rust workspace foundation `e4ca136` is public (no caller cutover yet).

Matched lab held QML/tools at `0ecb3bb7`, applied C8 pin only, used public 1080p
PNGs, private HOME/XDG/D-Bus and nested Hyprland/headless labwc, 10 actions each.
Initial shell captures are discarded as performance evidence: cache warming and
paging were active, and the focused repeat failed in the unchanged baseline.
No shell wall-clock, RSS/PSS or idle CPU improvement is claimed in this window.
Owner authorized continued ports; all correctness gates stay mandatory.
Busy-machine evidence: helper user+sys CPU, launches, helper RSS only.
Quiet window is not announced: require A/B/A/B, zero vmstat si/so and SwapPss;
discard paged runs. Stop rule only applies to a quiet-window reproduced regression.
STOP before wave 3 lands if no announced quiet-window comparison is available.

Wave 1 service, six hosted and five frequent helpers plus diagnostics are public.
Caller swaps, source retirement and historical assertion mappings are retained.
Per-helper publication completed at 3ca5d316; Source and Niri install CI green.
Frozen helper corpus: `tests/parity/helpers` (separate parity task; do not regenerate).

Follow-up: Rust Paths accepts absolute config/state/cache XDG roots, otherwise
HOME defaults, matching QML. Custom-XDG behavior is an intended parity difference;
current frozen cases use defaults and remain unchanged.
Main CI gate retained; 2753707f fixed isolation without changing goldens/skips.
Wave 1 source/install CI passed; hardware checks remain manual.

Next: final integrated gate (526 parity pass), rebase latest green main, publish compositor + pearl-window.
Wave 1 main 3e155f5c CI 38528354524: source + fresh Niri install green; no skips.
Wave 2: compilers public at 314a2603; CI 38539710564 Source + Niri install green.
Three compiler sources retired; 84 integrated Rust tests / 770 repo tests / 526 cases pass.
Temporary native-only compiler client is removed as its Python callers are ported.
Compositor busy fresh CPU 46.07 -> 1.35 ms; persistent RSS 7.90 MiB, one connection. Niri exact: 79 snapshots / 78 actions / 12 frames / 45 emissions / 8 handshakes / 110 config/query / 7 bindings / 21 display / 13 file / 18 night-light / 11 metadata; Hyprland 195 cases; CLI 34 dispatch cases.
Busy fresh CPU: 55.67/38.50/35.55/16.88 -> 1.95/1.16/1.11/0.89 ms; Python 404 -> 0.
Capture: 11 frozen cases match; 0600/0700 security difference tracked; IPC/events pass.
Disposable release install/empty-PATH commands and atomic replacement tests pass.
QML kill/malformed/timeout recovery passes with a native fixture; service Python retired.
Media API: 64 frozen artwork cases pass (63 PNG files plus Pillow I;16 error);
128px frames / 160 palettes unchanged; Orient `40c1a75` public, all 7 CI jobs green.
Intended differences: unexpected telemetry/cache failures reach stderr; NVML
replaces nvidia-smi; inventory CLI bridges the existing diagnostic module call.
d920 fingerprint: native-image compares two declared IDs against lock/pin/build;
17 controls and all 349 cases pass; frozen outputs/core/colors unchanged.
Hyprland lab backend cannot start (headless/nested); native socket/capture fixtures pass.
No live machine/config changes; no tags or package uploads.
Port security follow-ups: [security review](security-review.md#fix-during-port).
Keep existing AI access/consent/retention policies; add explicit security cases
and intended differences when implementing the reviewed port hardening.

## Soak lifecycle acceptance

Retain per-PID/start-time resident-plus-swapped memory measurements when porting
persistent helpers. The completed three-hour Niri/Hyprland default-allocator
stress captures showed flat long-lived helper memory; they do not demonstrate a
Python helper leak. Test cancellation, bounded request/model caches, child reaping
and wallpaper-watch/service restart counts during repeated wallpaper/video,
palette and pearl operations. Short helper lifetimes and isolated zombie samples
alone do not prove a leak. See [the measured comparison](soak.md#results-and-fixes).
