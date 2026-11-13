# Noctalia vs Nacre shell benchmark — 2026-11-12

| Headline metric | Noctalia + stock niri | Nacre + stock niri | Nacre + Nacre niri |
| --- | ---: | ---: | ---: |
| Combined idle / interaction CPU (% of one core) | 0.117 [0.117, 0.117] / 1.233 [1.168, 1.250] | 0.418 [0.418, 0.418] / 23.482 [23.417, 23.498] | 0.434 [0.418, 0.434] / 23.614 [23.564, 23.647] |
| Launch → first mapped bar layer (s) | 0.219 [0.219, 0.220] | 0.113 [0.113, 0.114] | 0.113 [0.112, 0.113] |
| Combined PSS, 10 min idle (MiB) | 118.560 [116.058, 119.427] | 127.449 [126.746, 127.986] | 128.516 [128.440, 128.705] |
| Combined unreclaimable memory, 10 min idle (MiB) | 62.530 [60.319, 63.554] | 68.377 [67.232, 68.752] | 68.752 [68.725, 68.904] |
| Combined idle / interaction context switches/s | 7.099 [6.831, 7.117] / 184.789 [184.772, 185.054] | 15.283 [15.236, 15.352] / 202.763 [201.398, 205.064] | 15.286 [15.185, 15.520] / 205.279 [203.684, 209.916] |
| PPT above shared baseline, idle / interaction (W) | -0.045 [-0.098, -0.035] / 0.575 [0.456, 0.630] | -0.245 [-0.689, -0.056] / 0.452 [0.349, 0.721] | -0.320 [-0.484, -0.285] / 0.343 [0.212, 0.813] |
| Host GPU busy above baseline, idle / interaction (percentage points) | 0.000 [0.000, 0.000] / 9.937 [6.048, 11.270] | 0.000 [0.000, 0.000] / 2.873 [1.095, 3.063] | 0.000 [0.000, 0.000] / 2.238 [0.000, 2.413] |
| Host VRAM above baseline, idle / interaction (MiB) | 105.094 [105.094, 105.094] / 119.621 [119.218, 119.842] | 12.766 [12.766, 12.766] / 19.025 [18.915, 19.052] | 12.766 [12.766, 12.766] / 18.921 [18.783, 18.978] |

Values are **median [minimum, maximum] across three completed runs per arm**. CPU is process-tree CPU; 100% means one core. Combined shell-plus-daemon values are summed within each run before summarizing, so independently summarized shell and daemon medians need not add to the combined median. For Noctalia, services run in the UI tree and there is no separate daemon (D=0).

## Per-tree CPU and activity

Each cell gives shell / daemon / combined (S / D / C), in that order. Context switches are observed thread-counter increments per second and are a lower-bound activity proxy, not hardware wakeups.

| Metric | Noctalia + stock niri | Nacre + stock niri | Nacre + Nacre niri |
| --- | ---: | ---: | ---: |
| Idle CPU, S / D / C (%) | 0.117 [0.117, 0.117] / 0.000 [0.000, 0.000] / 0.117 [0.117, 0.117] | 0.017 [0.017, 0.017] / 0.401 [0.401, 0.401] / 0.418 [0.418, 0.418] | 0.017 [0.017, 0.033] / 0.401 [0.401, 0.418] / 0.434 [0.418, 0.434] |
| Interaction CPU, S / D / C (%) | 1.233 [1.168, 1.250] / 0.000 [0.000, 0.000] / 1.233 [1.168, 1.250] | 11.692 [11.692, 11.708] / 11.790 [11.725, 11.790] / 23.482 [23.417, 23.498] | 11.823 [11.823, 11.824] / 11.791 [11.741, 11.824] / 23.614 [23.564, 23.647] |
| Post-interaction through end CPU, S / D / C (%) | 0.126 [0.120, 0.128] / 0.000 [0.000, 0.000] / 0.126 [0.120, 0.128] | 0.114 [0.113, 0.115] / 0.482 [0.478, 0.484] / 0.596 [0.592, 0.599] | 0.116 [0.113, 0.116] / 0.481 [0.481, 0.503] / 0.597 [0.594, 0.619] |
| Idle context switches/s, S / D / C | 7.099 [6.831, 7.117] / 0.000 [0.000, 0.000] / 7.099 [6.831, 7.117] | 4.143 [4.142, 4.176] / 11.141 [11.059, 11.209] / 15.283 [15.236, 15.352] | 4.160 [4.160, 4.193] / 11.093 [11.026, 11.360] / 15.286 [15.185, 15.520] |
| Interaction context switches/s, S / D / C | 184.789 [184.772, 185.054] / 0.000 [0.000, 0.000] / 184.789 [184.772, 185.054] | 142.970 [141.260, 144.492] / 60.138 [59.793, 60.572] / 202.763 [201.398, 205.064] | 144.516 [144.220, 148.906] / 60.763 [59.464, 61.010] / 205.279 [203.684, 209.916] |
| Post-interaction through end context switches/s, S / D / C | 7.892 [6.764, 8.249] / 0.000 [0.000, 0.000] / 7.892 [6.764, 8.249] | 4.993 [4.975, 5.009] / 11.502 [11.393, 11.645] / 16.476 [16.402, 16.638] | 5.011 [4.982, 5.040] / 11.637 [11.356, 11.782] / 16.677 [16.367, 16.765] |

## Memory checkpoints

Each cell gives shell / daemon / combined (S / D / C), MiB. PSS is proportional set size from `smaps_rollup`. Unreclaimable memory is the kernel's `Pss_Anon + Pss_Shmem + SwapPss` sum. These are checkpoints, not peaks.

| PSS checkpoint | Noctalia + stock niri | Nacre + stock niri | Nacre + Nacre niri |
| --- | ---: | ---: | ---: |
| Startup | 155.240 [154.211, 158.494] / 0.000 / 155.240 [154.211, 158.494] | 62.420 [62.305, 62.628] / 127.470 [126.634, 127.577] / 189.882 [189.262, 189.890] | 62.796 [62.666, 63.298] / 128.130 [127.117, 128.192] / 190.796 [190.415, 190.988] |
| 10 min idle | 118.560 [116.058, 119.427] / 0.000 / 118.560 [116.058, 119.427] | 64.562 [64.439, 64.913] / 62.887 [62.307, 63.073] / 127.449 [126.746, 127.986] | 65.003 [64.784, 65.004] / 63.512 [63.438, 63.921] / 128.516 [128.440, 128.705] |
| Post-interaction | 122.301 [119.174, 122.700] / 0.000 / 122.301 [119.174, 122.700] | 89.979 [89.812, 90.137] / 86.396 [86.354, 86.665] / 176.533 [176.166, 176.645] | 90.174 [89.787, 90.198] / 87.312 [87.245, 87.354] / 177.419 [177.141, 177.510] |
| End, 30 min | 122.286 [119.959, 123.180] / 0.000 / 122.286 [119.959, 123.180] | 74.905 [74.699, 75.483] / 84.127 [83.929, 84.267] / 158.834 [158.826, 159.750] | 75.346 [75.128, 75.486] / 85.166 [84.749, 130.940] / 160.512 [159.877, 206.427] |

| Unreclaimable checkpoint | Noctalia + stock niri | Nacre + stock niri | Nacre + Nacre niri |
| --- | ---: | ---: | ---: |
| Startup | 93.214 [92.073, 96.140] / 0.000 / 93.214 [92.073, 96.140] | 35.059 [34.932, 35.369] / 95.832 [94.688, 96.059] / 130.891 [130.057, 130.990] | 35.426 [35.369, 35.855] / 96.109 [95.582, 96.500] / 131.479 [131.438, 131.926] |
| 10 min idle | 62.530 [60.319, 63.554] / 0.000 / 62.530 [60.319, 63.554] | 37.205 [37.201, 37.568] / 31.172 [30.031, 31.184] / 68.377 [67.232, 68.752] | 37.576 [37.561, 37.592] / 31.191 [31.148, 31.312] / 68.752 [68.725, 68.904] |
| Post-interaction | 64.973 [61.953, 65.203] / 0.000 / 64.973 [61.953, 65.203] | 61.736 [61.580, 61.795] / 49.402 [49.262, 49.441] / 111.057 [111.021, 111.139] | 61.959 [61.541, 61.975] / 49.766 [49.477, 49.887] / 111.740 [111.018, 111.846] |
| End, 30 min | 64.789 [62.738, 65.852] / 0.000 / 64.789 [62.738, 65.852] | 48.424 [48.107, 48.990] / 47.004 [46.992, 47.016] / 95.439 [95.100, 95.994] | 48.850 [48.654, 48.896] / 47.289 [47.203, 73.812] / 96.139 [95.857, 122.709] |

The Nacre + Nacre niri end checkpoint includes one elevated daemon observation: 130.940 MiB PSS and 73.812 MiB unreclaimable, versus 84.749–85.166 MiB PSS and 47.203–47.289 MiB unreclaimable in the other two runs. The cause is unknown; the range is retained, with no leak or worker-causality inference.

## Method and interpretation

The tested source is Nacre `382f36de6441ec4daa68792b0867ec67cbf2df82`, Bead UI `ed5d1b8fc814b5f0a2dda23dac4dda3ff4c62849`, and Bead services `ed03c8ee22e506a9e634e919b0b2da6ae30c054c`. Noctalia is `v5.4.1` (`815aebe1cfd79c9a18197441eea92f0b275035fc`). Stock niri is Arch `26.04-1.1` (`8ed0da4`); Nacre niri is `26.04.r3079.g1028c5b9`. These findings apply to the tested Nacre pin, not newer `origin/main`.

Noctalia is now a native C++23 shell built with Meson/Ninja, with integrated services in its UI process; short-lived `noctalia msg` clients are outside that measured tree. Nacre uses the Bead native shell plus the separate `nacre-helper daemon --notifications=daemon --behaviours=daemon` tree, including its live descendants. Both own `org.freedesktop.Notifications` on their respective private buses.

The desktop used a Ryzen 5 7600 (12 threads, 14 GiB) and AMD Raphael graphics. The nested labwc parent, niri, and Noctalia used the AMD renderer. Nacre retained its default per-view rendering: the bar, launcher, dashboard and notices use CPU paths, even though GL features are compiled in; no renderer override was applied. Thus this is a comparison of tested product defaults, not a CPU-versus-GPU renderer test. PPT is a host/package sensor measurement, not process energy; RAPL was unavailable to the ordinary user.

The output was 1920×1080, scale 1, 60 Hz. Readiness is launch to the first bar namespace listed by `niri msg layers`: `noctalia-bar-default` for A and `bead-dev` for B/C, sampled roughly every 100 ms; it is not first paint or fully interactive latency. No `--namespace` override was added. AMD device FDs and Noctalia’s radeonsi GLES 3.2 renderer log excluded llvmpipe fallback.

Each run had a separate 60 s common-fixture baseline with both measured trees absent, followed by a 1,800 s measured window: 600 s idle, 60 s interaction and 1,140 s rest. Runs followed **A1 B1 C1 / B2 C2 A2 / C3 A3 B3**. One A3 attempt stopped at about 900 s when the shared capture lock became contended; it was discarded and a fresh full A3 run passed. No measurements were stitched. An earlier C notification-ingress error did not recur: all nine accepted runs completed 24 notifications and all workspace acknowledgements.

The private fixture used each product's default bar, the same dark Nacre palette (including terminal/ANSI roles), pinned fonts and static 1920×1080 PNG, with private HOME/XDG/runtime and activation-free D-Bus. Nacre's selected IBM Plex and Material Symbols font files passed hash checks; Noctalia used bundled icons and its default sans-serif. Wallpaper was selected before startup with Noctalia’s own `wallpaper.default.path` configuration (verified by `noctalia msg wallpaper-get`) and with `nacre-helper wallpaper-media select` for B/C; the static image means animated wallpaper was not exercised. Default notification sounds remained enabled, but private PipeWire streams had no output link, so this does not measure audible playback. Volume OSD was omitted in all arms.

The six ten-second interaction cycles open the launcher for 0.8 s, close it, open Control Center/dashboard for 0.8 s, close it, focus workspaces 2→1→3→1 (acknowledged, 0.3 s each), then send four private-bus notifications (0.2 s spacing, 2.5 s expiry). For A the commands are `noctalia msg panel-toggle launcher` / `panel-close launcher`, and `panel-toggle control-center` / `panel-close control-center`. B/C use daemon `ui.dispatch`: target `nacre`, method `launcher` with an empty argument, then `close`; dashboard uses `drawers toggle dashboard`, `nacre tab 0`, then `nacre close`. Nacre starts with `nacre-shell --bar --core-views --ui-provider --notices` and private daemon/command sockets plus `--palette`. A starts with `noctalia`.

Noctalia defaults include launcher/wallpaper/workspaces, central clock, media/tray/notifications/clipboard/device/session controls; dock and desktop widgets are off. Nacre defaults show launcher, workspaces, title, update count, audio/network/Bluetooth/battery/notifications/calendar/clock/session controls. Stock niri hides the collection chip visible with the fork; stock does not advertise the fork-specific pin/hide/restore, collection, launch-workspace, monitor-workspace and overview-hover capabilities (separate fallback actions may exist). This measures those different defaults, not identical feature sets. Idle/launcher/Control Center-or-dashboard smoke screenshots were inspected for all arms: matching PNG, actual glyphs and Plex in B/C.

CPU uses cumulative process-tree ticks for the final idle minute, interaction minute and post-interaction remainder. Percent is relative to one core. Memory and SwapPss are summed from process-tree `smaps_rollup`; checkpoints are startup, 10-minute idle, post-interaction and 30-minute end. Compositors, bus, collector and watchdog are excluded from process-tree totals, but remain in host sensor readings. Context-switch rates sum sampled per-thread voluntary/involuntary counter increments over elapsed windows; vanished or unsampled threads may be missed, so they are lower-bound activity proxies rather than measured hardware wakeups.

All nine accepted runs used the same boot, input SHA-256 manifest and package manifest. Their UTC span was 2026-11-12 23:39:48 to 2026-11-13 04:44:04. All workloads completed; process SwapPss and host swap-in were zero, sampling gaps remained below 2 s, and capture scopes completed at 4 GiB maximum memory with swap disabled and zero OOM events. Host preflight required swap below 64 MiB and available memory above swap use plus 8 GiB; the watchdog abort rail was 3 GiB. Capture-lock holds stayed below 590 s (maximum 450.819 s). Each arm returned within 2°C of its baseline with three readings 10 s apart. No noisy or failed arms are included.

PPT comes from the readable AMD hwmon package-power input in microwatts; idle sensor means use seconds 540–600 and interaction means 600–660, subtracting each run’s no-shell baseline tail mean (seconds ≥30). Both measured trees are absent from that baseline; common compositor/device/audio fixtures remain. Baseline PPT medians were 20.676/20.731/20.780 W (A/B/C).

Idle PPT deltas are negative and retained rather than clipped; their magnitude is within baseline/sensor variation and does not indicate negative power. Interaction PPT ranges overlap, so these measurements support no power winner. GPU busy and VRAM are global host observations above baseline, not per-application attribution; VRAM is not comparable to process PSS. The benchmark does not establish battery life, smoothness, first-frame latency or a universal performance ranking.

## Dev-1 diagnostic appendix

| One observation per arm; no medians | A | B | C |
| --- | ---: | ---: | ---: |
| Readiness (s) | 0.217 | 0.112 | 0.111 |
| Combined idle CPU (%) | 0.117 | 0.401 | 0.401 |
| Combined interaction CPU (%) | 11.808 | 17.466 | Invalid: notification failure |
| Combined 10-minute PSS / unreclaimable (MiB) | 206.4 / 122.4 | 114.4 / 63.0 | 115.2 / 63.8 |
| Host swap-in (MiB; gate <1) | 607.219 | 854.445 | 231.480 |

This round used labwc pixman, niri/Noctalia llvmpipe and Bead CPU rendering. Software GL penalizes Noctalia CPU; its mapped GL libraries also affect host PSS. Power was N/A.

The earlier dev-1 appendix used Nacre `fe04e6535` and is diagnostic only: all arms failed the host swap-in gate and were contended. C1 failed its second notification after 10 minutes idle while load1 peaked at 127.96. It is not combined with these desktop results and supports no medians or hardware-causality claims.

## Build provenance

No product changes were made. Nacre was built on the build host in a fresh Arch container, Rust 1.99.1, release/locked, Cargo `-j2`, with the target inside the removed container. Desktop libraries resolved before execution. Runtime: kernel 7.3.0-5-cachyos, Mesa 26.2.5, labwc 0.20.2; CPU policy amd-pstate-epp/powersave/balance_performance, boost enabled, GPU policy auto, unchanged. Missing wlr-randr and libmpv dependencies were extracted into a private prefix from signed Arch packages; no system packages were installed. The static wallpaper decoder and motion helper were staged; motion remains off. Private library search paths were relocated for the motion helper/libmpv only. Original and relocated hashes, manifests, raw captures and the adapted N0154 harness stay in ignored task evidence.

| Measured executable | SHA-256 |
| --- | --- |
| Noctalia | `580e4fe21ef02cc1cd1b70360239963826d1eedbf67c80bcb26baa06e55bde2b` |
| Nacre shell | `8c7bcc27f1fcc393e499ddef24ac5f3b28ced351e72126cd4236836a1b220e58` |
| Nacre helper | `01107eb80eba8f127a7a2599e0aff6b976a37ab4dbcf699b239ffe78401a982d` |
| Wallpaper decode | `7eca2fcd59418fbe5c71b60b77868d3cfe073ea3776468bb6fb4b416d0da56f9` |
| Motion helper, private runtime path | `7f6ca5462cf56bd6baafa386d5bf4e100103896f6065d3b75082a5561506c570` |
| Stock niri | `389f6d1ee656d006d1759c7088e3ba2f6c0129747d8c3dfea4baaac3cd72e6a9` |
| Nacre niri | `83822c0f15363f5b0bc40d1616c220a9944306397d61bde2e8bbfc58e7df074e` |

See [N0154 benchmark method](bead-vs-qml-shell-benchmark-20261110.md) for the prior accounting and sensor methodology.
