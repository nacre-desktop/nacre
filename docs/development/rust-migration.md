# Rust runtime migration

Installed Nacre must run no Python processes and require no Python/Pillow runtime.
Keep public commands, QML appearance, user data, privilege and session policy.

| Wave | Status | Numbers / decisions |
| --- | --- | --- |
| 0 Orient decoder | Complete | 49 inputs / 55 frames, 160 palettes exact, including JPEG; notices retained. |
| 1 Frequent helpers | Complete; CI green | 13 ports; busy service RSS 26.95 → 10.21 MiB; absolute XDG roots follow QML. |
| 2 Compositor / pearls | Complete; CI green | Preview CPU 52.58 → 1.26 ms; RSS 21.54 → 7.30 MiB; native IPC/session runtime. |
| 3 Wallpaper / colors | ports complete; desktop A/B/A/B captured, regression review open | Shell frame192.5→94ms Hypr/195.5→102ms niri; GTK endpoint missing; niri cold launcher+40/42ms repeats after control; Oct16 exact33a/84ce:439→460ms median, ranges423–454/440–472; noise by owner spread rule; historical gap tracked. |
| 4 Remaining shell tools | redirected: foundation F1–F4 first | Events/session published, CI green. Launcher local: 991 frozen/332 Rust pass; niri CPU0.820–1.020ms +worker0.235–0.250ms; PSS34.86/63.32MiB (320/2000 apps). 929 repo (3 skips)/534 helper parity pass; b44d1afb published; CI38874838563 green. Refresh fix: old-helper failure reproduced; 333 Rust/991 references/929 repo/534 parity pass; niri/Hypr rechecks valid; publication/CI next. |
| 5 AI | Pending | Claude stream-json; existing permissions/history; sidebar-chat, composer and AI controller belong here. |
| 6 Brain | Pending | Same bge-small model; cosine ≥ 0.9999 or backed-up automatic reindex. |
| 7 Installer / proof | Pending | install*.py/login-install.py deferred here; packaging owned separately. |

Frozen sources for the desktop-PC comparison (never substitute latest main):
- Python baseline: e910ffd7202a332c652ef227ddcf5970c39714bb.
- Baseline Orient: 820e199714d9d3a1233db80705e39838e86c49fc.
- Wave 3: 81d9e520d6f6d0a915018f93e3b6279b716ddc91.
- Wave 3 Orient: 3efe8d1e7fb50200d74a3e0193ad63699cb8ebe8.
- Wave3-only harness: Wave3 revision above; build before measuring.
- If Wave4 is included, use its pinned backward-compatible harness for both pairs.
If Wave 4 lands first, measure both waves in the same desktop session:
baseline/Wave3 A/B/A/B, then Wave3/Wave4 A/B/A/B; record the exact Wave4 endpoint.
Record any pre-Wave4 media fix separately; retain the frozen Wave3 source above.

Quiet gate: measured processes SwapPss 0; host swap-in UNDER 1 MiB.
Record counters/page size; discard invalid runs. Reconfirm connectivity and quiet.
Busy evidence is helper user+system CPU, launches and helper RSS only;
no whole-shell wall time or aggregate RSS/PSS evidence while paging.
Historical action baselines: launcher ≈105 ms, dashboard ≈115 ms,
wallpaper pair ≈225 ms, palette pair ≈190 ms; not new matched measurements.

Wave3 busy CPU ms/call: KDE 25.39→1.25; terminal 29.02→1.07;
login cache hit 50.74→0.95; demo verified 59.06→10.39 / fast 43.49→0.91.
Palette preview 65.82→9.98; Colors RSS 34.02→16.85 MiB (2→1 processes).
Watcher RSS 16.75→10.20 MiB; startup+change CPU 31.36→3.13 ms.
Media CPU 86.21→11.58 ms; ready RSS 25.84→16.73 MiB;
after publication RSS 26.18→27.35 MiB; idle 0/1 ticks over30s at100Hz.
Cached lock CPU 78.29→2.57 ms;100 Python launches→0. No battery claim.
Full receipts and immutable parity details remain in performance.md.

Both compositors: baseline/native shell frame/scene, Qt/Kitty mapped pixels and
actual lock new-color pixels verified. Developer lock uses immediate-render for
software Niri; production flags unchanged. GTK pixel endpoint unverified in both;
no finite whole-desktop completion time established. Harness14 tests pass.
Direct pinned Orient libraries own paired publication; no Orient process/RPC.
Custom absolute XDG paths and direct D-Bus/socket transports are intended differences.
Reference outputs are immutable; security additions use explicit new cases.

Foundation: F0 → F1 → F2 events/session → launcher engine → remaining F2 → F3 → F4; stop.
Oct16 order: schema3042f3f2 published first; immutable JS ranking/provider parity, indexed catalog next.
Long-running or frequently called work → daemon service; rare/heavy/risky → worker.
Settings' unpublished controller port is parked for F4, not a standalone landing.
Bead owns its ADR/F5 framework work; protocol changes coordinate across agents.
No live daemon units, notification ownership or keybindings without owner approval.
Nightly fuzz/soak, Bead hosting and remaining Wave4 ports deferred; menu.query now precedes remaining F2.
Apply docs/development/security-review.md cases where relevant, preserving privileges.
Never alter deliberate AI permissions or paru -Syu --skipreview.
Media response trim moved after cleanup: matched native RSS29.19→27.80MiB;
Python controls25.95/26.24MiB; no retained decoded frames or battery claim.
Media fix 16ea1b94: CI 38737660609 green; supervisor 253008f5: CI 38742419146 green.
Supervisor: 17 contracts + security canary + nested niri/Hyprland pass;
249 Rust / repo checks / 534 original parity pass; caller switch retires Python.
Supervisor busy CPU98.58→22.38ms; helper-groupRSS24.64→10.77MiB;2→1 processes.
Session-watch: 5 records/files, 3 readiness, 4 display policies, 12 event effects, 7 CLI
cases exact; private bus/socket, linked-state/XDG and both nested compositors pass.
259 Rust/fmt/clippy;917 full repo +15 incoming Niri checks/534 parity pass.
Python watch currently fails at removed event_socket bridge; no valid idle baseline.
Session check CPU125.79→29.50ms;20Pythonstarts→0; nativeRSS18.09MiB/idle0ticks30s.
F0 67674e85 CI38764589681 green; repaired main5995d171 CI38768972757 green.
F1:8 protocol/23 core/6 native integration tests;31 wire objects; minimal consumer32B; Luau passes.
Decoder/merge-patch fuzz targets compile; nightly/soak remain pending; desktop idle baseline accepted.
Desktop-PC F1: p99 .018/.027ms; CPU.006/.008ms; PSS10.27/10.08MiB; SwapPss/hostswap-in0.
UI fallback only before dispatch; native nice10 notice worker; idle watch detects pipe closure by event.
Recovery: fmt/clippy/299 Rust/921 repo/534 frozen parity pass; examples/fuzz compile.
Supervisor's first8s timeout passed an unchanged17-case recheck; no fixture or timeout changes.
Next: finish/publish refresh coalescing fix and CI; F2 widths/serve/network/wallpaper → F3 (memory/i18n/cold-open) → F4.
No live desktop/config/network changes, tags, releases or package publication.
