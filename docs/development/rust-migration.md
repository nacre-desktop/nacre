# Rust runtime migration

Installed runtime target: no Python processes or Python/Pillow dependencies.
QML UI, command names, protocols and existing user state remain compatible.
Developer-only Python tools and test harnesses remain permitted.

| Wave | Status | Numbers / decisions |
| --- | --- | --- |
| 0 Orient decoder | Complete; quiet timings deferred | Exact decoder/dev6; busy helper CPU/RSS baseline recorded. |
| 1 Service / frequent helpers | Pending | Freeze CLI stdout, exits and writes in disposable HOME. |
| 2 Compositor / pearls | Pending | Persistent native IPC; one neutral compositor contract. |
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

Existing tasks coordinate Wave 1: peer owns workspace/paths/service + menu-backend,
dashboard-data/media, network-state, launcher-preferences, notification-history.
This task owns dashboard-system, resource-state, window-chat-title, light-devices,
menu-cli; gate deferred with owner conditions, Rust modules not started yet.
Frozen helper corpus: `tests/parity/helpers` (separate parity task; do not regenerate).
API: execute(&[String], Option<Value>) -> Result<Option<Value>>; custom text as needed.

Follow-up: Rust Paths accepts absolute config/state/cache XDG roots, otherwise
HOME defaults, matching QML. Custom-XDG behavior is an intended parity difference;
current frozen cases use defaults and remain unchanged.
Main CI must be green before further wave 1 ports land. Parity harness CI failure
is under investigation; do not skip or weaken assertions without a tracked reason.

Next: validate/publish Paths and repair the failing CI harness, verify green main,
then use frozen current-Python contracts before each assigned helper port.
No live machine/QML/config changes by this task; no tags or package uploads.
