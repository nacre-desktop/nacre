# Rust runtime migration

Installed Nacre must run no Python processes and require no Python/Pillow runtime.
Keep QML, public commands, protocols and existing user data; no UI/design work.

| Wave | Status | Numbers / decisions |
| --- | --- | --- |
| 0 Orient decoder | Complete | 49 inputs / 55 frames and 160 palettes exact, including JPEG; notices retained. |
| 1 Service / frequent helpers | Complete; CI green | 13 ports; busy helper RSS 26.95 → 10.21 MiB. Absolute XDG roots match QML. |
| 2 Compositor / pearls | Complete; CI green | Preview busy CPU 52.58 → 1.26 ms; RSS 21.54 → 7.30 MiB. Native IPC/session runtime. |
| 3 Wallpaper / colors | Active | Four small helpers public; native palette pipeline validating as one unit. |
| 4 Remaining shell tools | Pending | Preserve session/privilege policy; bring palette dependencies forward only as needed. |
| 5 AI | Pending | Claude stream-json CLI; retain existing permissions/history. |
| 6 Brain | Pending | Same bge-small model; cosine ≥ 0.9999 or backed-up automatic reindex. |
| 7 Installer / proof | Pending | Disposable installs/nested sessions and zero Python launches; shared packaging belongs to install task. |

Historical baselines in performance.md: launcher ≈ 105 ms, dashboard ≈ 115 ms,
wallpaper pair ≈ 225 ms, palette pair ≈ 190 ms; fresh Python helpers 19–58 ms.
These are not new matched measurements. No new whole-desktop speed claim yet.
Busy-machine evidence: child user+system CPU, launches and helper RSS only.
Do not use whole-shell wall time or aggregate RSS/PSS while paging.
The original lab captures were discarded; the unchanged baseline also failed.

Quiet gate: measured processes SwapPss 0; host swap-in UNDER 1 MiB.
Record page counts/page size; discard runs above that or with measured-process swap.
At the next owner-announced desktop window, compare built baseline/native A/B/A/B.
No new window announced. Include it before Wave 3 final status.
Earlier 126/33-page captures are not a complete comparison and cannot be stitched in.
No reboot, swap, governor or live-session changes are needed for these ports.

Wave 3 public receipts (desktop + fresh-install Source CI green):
- KDE 10b1d328 / receipt 720525e6: CI 38631961373 + 38635667735; 14+3 cases.
- Terminal c4877dd5: CI 38644066952; 42 function + 2 real PTY references.
- Login 11f7705e: CI 38652117086; 20 cases / 13 exact preview PNGs.
- Demo c1417afe: CI 38654603463; 14 cases; 203 Rust / 884 repo / 526 parity.
Busy CPU ms/call: KDE 25.39 → 1.25; terminal 29.02 → 1.07;
login cache hit 50.74 → 0.95; demo verified 59.06 → 10.39, fast 43.49 → 0.91.
Each 100-call run reduced Python launches from 100 to zero.

Palette pipeline candidate: task/zero-python-palette-pipeline, rebased onto e910ffd7.
Direct pinned Orient core/decoder calls; paired publication holds one shared lock.
Required branding, file-icons and lock preparation code moves with this unit.
Native-only clients bridge remaining Python callers; retired implementations removed.
16 immutable publication/transaction cases match bytes, modes, symlinks and stamps.
Six Colors workflows and 13 original shared engine-cache files match exactly.
12 CLI workflows and five additional original subcommand-help cases match exactly.
Four complete lock PNGs, 56 font masks/metrics and 36 geometry masks match.
Before integration: 526 parity, Clippy, 216 Rust tests and 884 repo tests passed.
Integration preserves automatic wallpaper mode and border-gradient controls from main.
Integrated: six gradient refs, 22 media tests, 221 Rust tests and 889 repo tests pass.
Busy cached preview CPU 65.82 → 9.98 ms; Colors RSS 34.02 → 16.85 MiB (2 → 1 processes).
Reference producers refuse replacement; existing frozen outputs are immutable.
New help references were captured separately from unchanged c1417afe sources.
State write_text preserves legacy bytes; existing write/update APIs are unchanged.

Intended differences: custom absolute XDG roots follow QML; dconf and compositor
socket operations replace equivalent processes; terminal reloads target the same HOME.
Main added paletteAutoMode=false: one explicit old picker-file default is compatible;
all other frozen file bytes remain checked, and original expectations are unchanged.
The retired provisioner has developer-only historical tests, no runtime entry point.
Orient 3efe8d1 supplies exact image APIs: all seven CI 38664017062 jobs green.
Core colors are unchanged; only submodule/lock/SDK/bootstrap engine-label pins change.
Shared Arch recipes/Core maps remain with the packaging owner; handoff coordinated.

Next: finish integration gates and the exact candidate handoff to packaging;
push and check CI. Then wallpaper-watch,
wallpaper-media, their RSS/idle CPU, visible-consumer timings and quiet A/B/A/B.
Preserve concurrent Settings changes; rebase only own commits, owner identity.
No live deployment, tags, package publication or session takeover in this task.
