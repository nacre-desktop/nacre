# Rust runtime migration

Installed Nacre must run no Python processes and require no Python/Pillow runtime.
Keep QML, public commands, protocols and existing user data; no UI/design work.

| Wave | Status | Numbers / decisions |
| --- | --- | --- |
| 0 Orient decoder | Complete | 49 inputs / 55 frames and 160 palettes exact, including JPEG; notices retained. |
| 1 Service / frequent helpers | Complete; CI green | 13 ports; busy helper RSS 26.95 → 10.21 MiB. Absolute XDG roots match QML. |
| 2 Compositor / pearls | Complete; CI green | Preview busy CPU 52.58 → 1.26 ms; RSS 21.54 → 7.30 MiB. Native IPC/session runtime. |
| 3 Wallpaper / colors | Active | Palette, watcher and media public; final CI and lock dependency pending. |
| 4 Remaining shell tools | Pending | Preserve session/privilege policy; bring palette dependencies forward only as needed. |
| 5 AI | Pending | Claude stream-json CLI; retain existing permissions/history. |
| 6 Brain | Pending | Same bge-small model; cosine ≥ 0.9999 or backed-up automatic reindex. |
| 7 Installer / proof | Pending | Disposable installs/nested sessions and zero Python launches; shared packaging belongs to install task. |

Historical baselines in performance.md: launcher ≈ 105 ms, dashboard ≈ 115 ms,
wallpaper pair ≈ 225 ms, palette pair ≈ 190 ms; fresh Python helpers 19–58 ms.
These are not new matched measurements. No new whole-desktop speed claim yet.
Busy-machine evidence: child user+system CPU, launches and helper RSS only.
Do not use whole-shell wall time or aggregate RSS/PSS while paging.

Quiet gate: measured processes SwapPss 0; host swap-in UNDER 1 MiB.
Record page counts/page size; discard runs above that or with measured-process swap.
At the next owner-announced desktop window, compare built baseline/native A/B/A/B.
No new window announced. Include it before Wave 3 final status.

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
16 immutable publication/transaction cases match bytes, modes, symlinks and stamps.
Six Colors workflows and 13 original shared engine-cache files match exactly.
12 CLI workflows and five additional original subcommand-help cases match exactly.
Four complete lock PNGs, 56 font masks/metrics and 36 geometry masks match.
Integration preserves automatic wallpaper mode and border-gradient controls from main.
Integrated: six gradient refs, 22 media tests, 221 Rust tests and 889 repo tests pass.
Busy cached preview CPU 65.82 → 9.98 ms; Colors RSS 34.02 → 16.85 MiB (2 → 1 processes).
Reference producers refuse replacement; existing frozen outputs are immutable.
State write_text preserves legacy bytes; existing write/update APIs are unchanged.

Intended differences: custom absolute XDG roots follow QML; dconf and compositor
socket operations replace equivalent processes; terminal reloads target the same HOME.
Main added paletteAutoMode=false: one explicit old picker-file default is compatible;
all other frozen file bytes remain checked, and original expectations are unchanged.
Orient 3efe8d1 supplies exact image APIs: all seven CI 38664017062 jobs green.
Shared Arch recipes/Core maps remain with the packaging owner; handoff coordinated.

Palette 8185a87f + harness 39323a43 public; CI 38678765683 fully green.
Watcher 85652512 public; fixture mask 26a64238 and write-close race b7a19a16 fixed.
Watcher CI 38690610081 fully green (desktop and fresh-install).
Watcher: 13 CLI / 6 module / 15 events, all files and SIGTERM status exact.
Busy watcher RSS 16.75 → 10.20 MiB; startup+change CPU 31.36 → 3.13 ms.

Media 330f250e public; exact fixture fixes e0297dc9 / dcfa8297 / b9523e2c.
CI 38713957659: media parity and fresh-install pass; unrelated AUR gallery fails.
AUR owner is fixing its private D-Bus environment; hold lock push until main green.
Local gates: 222 Rust / 892 repo / 526 parity and all immutable workflow cases pass.
Fully private busy CPU 86.21 → 11.58 ms; ready RSS 25.84 → 16.73 MiB.
After publication RSS 26.18 → 27.35 MiB; 30 s idle CPU 0/1 ticks (10 ms resolution).
No battery, whole-shell latency or quiet regression claim from busy helper samples.

Lock dependency unit validated on current main; native follower opt-out preserved.
Native labels 141 / artwork eight / CLI 11 / preparation three cases/files exact.
230 Rust / 320 tools / 107 AI / 45 Brain / 428 shell / 534 parity cases pass.
Native QML/install/generated callbacks replace Python; frozen workflows all pass.
Frozen preparation inputs explicitly include private XDG_DATA_HOME/public fonts.
An initial uncontrolled-font capture is preserved privately; no expected file rewritten.
Busy cached lock CPU 78.29 → 2.57 ms/call; 100 Python launches → zero.
Next: publish the validated lock unit once main CI is green, then check its CI.
Baseline/native nested shell frame + pixel smokes pass; no latency claim on busy host.
GTK/Qt/Kitty/actual-lock pixel attribution and quiet A/B/A/B remain incomplete.
No live deployment, tags, package publication or session takeover in this task.
