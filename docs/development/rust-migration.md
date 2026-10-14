# Rust runtime migration

Installed runtime target: no Python processes or Python/Pillow dependencies.
QML UI, command names, protocols and existing user state remain compatible.
Developer-only Python tools and test harnesses remain permitted.

| Wave | Status | Numbers / decisions |
| --- | --- | --- |
| 0 Orient decoder | Complete; quiet timings deferred | Exact decoder/dev6; busy helper CPU/RSS baseline recorded. |
| 1 Service / frequent helpers | In progress; CI repaired | 13 native replacements validated; helper RSS 26.95 -> 10.21 MiB (busy). |
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

Wave 1 candidates combine the shared service and six hosted helpers with the
five frequent helpers. Runtime integration is consolidated; peer audits cover
historical assertions, cache files, private buses, translations and CLI grammar.
Caller swaps/Python deletions are ready to publish; 23 historical test methods map
explicitly to Rust. Diagnostics/mixed-dashboard tests pass (17). Native service
crash/concurrency/framing contracts pass. Per-helper publication is in progress.
Frozen helper corpus: `tests/parity/helpers` (separate parity task; do not regenerate).

Follow-up: Rust Paths accepts absolute config/state/cache XDG roots, otherwise
HOME defaults, matching QML. Custom-XDG behavior is an intended parity difference;
current frozen cases use defaults and remain unchanged.
Paths fix is public (`ae6a9f86`); Rust gates and default-XDG parity pass.
Main CI must be green before further wave 1 ports land. Repair complete:
`2753707f` uses Ubuntu 22.04 with the same Arch container/isolation; workspace
trust is scoped, test cache timestamps explicit, mock sockets UID1000/0600 and
reported interpreter version controlled at input. All original goldens remain.
Latest main `55c5a03b`: Source 38487998838 desktop and Niri fresh install pass;
834 source tests, 12 original controls, 349 frozen cases. No new skips.
Earlier `f3b42304` Source and performance both pass. CI workflow repair is retained; hardware checks are manual, correctness stays automatic.

Next: finish final private check; push 13 helper commits and shared integration.
526 native cases / 51 controls / 81 Rust tests pass; empty-PATH report passes.
Rewritten main e9e80a42 CI green (source + fresh install), no skips.
Busy fresh CPU: 55.67/38.50/35.55/16.88 -> 1.95/1.16/1.11/0.89 ms; Python 404 -> 0.
Capture: 11 frozen cases match; 0600/0700 security difference tracked; IPC/events pass.
CLI: 62 menu + 39 hosted cases pass; seven exact retired-frame differences tracked.
Disposable release install/empty-PATH commands and atomic replacement tests pass.
QML kill/malformed/timeout recovery passes with a native fixture; service Python retired.
Two retired nmcli error IDs retain exact catalog/context; finite compatibility checks pass.
Private integration Qt/D-Bus/software-GL gate: 783 tests / 51 controls / 526 cases.
Media API: 64 frozen artwork cases pass (63 PNG files plus Pillow I;16 error);
128px frames / 160 palettes unchanged; Orient `40c1a75` public, all 7 CI jobs green.
Intended differences: unexpected telemetry/cache failures reach stderr; NVML
replaces nvidia-smi; inventory CLI bridges the existing diagnostic module call.
d920 fingerprint: native-image compares two declared IDs against lock/pin/build;
17 controls and all 349 cases pass; frozen outputs/core/colors unchanged.
No live machine/QML/config changes by this task; no tags or package uploads.
Port security follow-ups: [security review](security-review.md#fix-during-port).
Keep existing AI access/consent/retention policies; add explicit security cases
and intended differences when implementing the reviewed port hardening.
