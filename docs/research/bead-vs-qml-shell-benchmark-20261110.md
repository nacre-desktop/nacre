# Bead vs QML shell benchmark — N0154, 2026-11-10

| Metric | QML | Bead |
| --- | ---: | ---: |
| Shell PSS, 10 min idle (MiB) | 511.4 [509.3, 516.8] | 52.4 [52.1, 52.6] |
| Shell + daemon PSS, 10 min (MiB) | 552.2 [550.0, 556.1] | 105.8 [105.7, 106.5] |
| Shell idle CPU (% of one core) | 2.02 [1.98, 2.03] | 0.10 [0.08, 0.10] |
| Shell interaction CPU (% of one core) | 56.68 [56.52, 58.08] | 10.73 [10.72, 10.73] |
| Shell + daemon idle / interaction CPU (%) | 2.17 [2.12, 2.18] / 57.30 [57.12, 58.70] | 0.53 [0.53, 0.55] / 47.35 [47.35, 47.37] |
| Shell idle / interaction context switches per second | 39.4 [38.8, 40.6] / 491.4 [487.6, 494.6] | 4.9 [4.9, 5.0] / 137.6 [136.9, 141.2] |
| Idle AMD PPT power above no-shell baseline (W) | 1.47 [1.17, 1.66] | 0.65 [0.53, 0.69] |
| Idle Tctl / GPU edge temperature (°C) | 39.1 [39.0, 39.1] / 37.1 [37.1, 37.1] | 39.1 [39.0, 39.1] / 37.0 [37.0, 37.0] |
| Launch to IPC provider readiness (s) | 1.208 [1.201, 1.209] | 0.742 [0.736, 0.744] |
| Idle GPU busy / VRAM above baseline (percentage points / MiB) | 0.00 [0.00, 0.00] / 90.6 [90.6, 92.6] | 0.00 [0.00, 0.00] / 13.8 [13.8, 13.8] |
| Shell PSS change, post-interaction to 30 min (MiB) | -75.9 [-77.4, -75.5] | -12.6 [-13.0, -12.3] |

Values are **median [minimum, maximum] over three completed runs per shell**.
Combined values are summed within each run before summarizing; independently
summarized components need not add to the combined median.
CPU is process-tree CPU; 100% means one core, not the whole 12-thread machine.
This measures a controlled nested, private, default-settings fixture. It does
not establish physical-session battery life, visual smoothness or a universal
performance ranking. Energy, first-frame latency and fully interactive latency
were unavailable. Volume OSD is an explicit workload omission.

During interaction, Bead’s daemon uses 36.62% of one core (QML: 0.62%).
The combined shell-plus-daemon CPU figures, 47.35% versus 57.30%, capture that
work distribution and are the appropriate stack-level comparison.

## Method

Both products use Nacre commit
`2f33e1ffad5be0b517e720ab74bb7e74449e441d`, which was `origin/main` when
measurement started. Later documentation integration does not change the tested
source. QML runs the unmodified `nacre/shell/shell.qml` entry point with
`quickshell --path`; Bead runs the real `shell/` Luau tree using `nacre-shell
--bar --core-views --ui-provider --notices`, private daemon/command sockets and
the matched palette. There are no product changes in this unit.

The physical display was occupied by the owner's session, which was left running.
All measurements therefore use headless labwc hosting the same packaged
`nacre-niri`, with a verified 1280×720 output, scale 1 and 60 Hz. Niri animations
are enabled. Each arm has a fresh private HOME, configuration/cache/state/runtime
directories, an activation-free private D-Bus bus and a real private Nacre daemon.
The bus has no service activation directories. System discovery and host audio
are disconnected to avoid changing owner devices. This can exercise service
failure/retry paths differently in the two implementations.

Both arms use a private `desktop.json` containing `{}\n`, the same source dark
palette and a deterministic 1920×1080 static PNG, RGB (22, 31, 43). These are
matched default settings, rather than a copy of the owner's customized setup.
The private presentation records point to that PNG with animated/dynamic flags
false. Bead's native wallpaper consumer runs; no substitute `swaybg` is included.
Inputs, source trees, collector, executables and output geometry are verified
identical across accepted arms; hashes are in [provenance.csv](bead-vs-qml-shell-benchmark-20261110/provenance.csv).
The remaining wallpaper-helper and Orient executable hashes are listed below.

The daemon executable and commit match, but its modes follow the product session:
Bead uses `--notifications=daemon --behaviours=daemon`; QML uses
`NACRE_DAEMON_CLIENT=1`. Work can move between shell and daemon. Shell-only numbers
include all live descendants, including wallpaper decode/video helpers when
present; daemon and combined values are reported separately. Compositors, D-Bus,
collector and watchdog are excluded from process-tree totals, but remain part of
the host sensor readings and the no-shell baseline topology.

### Order, timing and workload

The six accepted runs are chronological **A B A B A B** (A=QML, B=Bead), each
with a 60-second no-shell baseline and 1,800 seconds after IPC readiness. They
are not six uninterrupted back-to-back launches: two incomplete Bead attempts
were discarded after shared desktop-lock contention, and the final Bead arm was
started separately after a swap preflight refusal. No data from those incomplete
attempts enter the medians. [run-ledger.csv](bead-vs-qml-shell-benchmark-20261110/run-ledger.csv)
records the source attempt, UTC start, last phase and acceptance for each file.

The same 60-second interaction starts after 600 seconds of idle. Six ten-second
cycles each request launcher open (0.8 s), close, dashboard drawer open (0.8 s),
close, workspace focus 2→1→3→1 (0.3 s each, with focus acknowledgement), then four
notifications (0.2 s apart, 2.5 s expiry), resting until the next cycle. Both
shells receive the same daemon `ui.dispatch` calls; workspace changes use private
compositor IPC and notifications use the private notification bus (24 total).
Volume OSD steps were skipped in both arms: the shared preview route was
unsupported by Bead. No host volume was changed. Thus the interaction result
covers launcher/dashboard/workspace/notification activity, not the requested
complete workload including OSD.

Between arms, temperature must return to within 2°C of that arm's no-shell
baseline median (last ten readings), three consecutive readings ten seconds
apart, with a 30-minute timeout. Sensors are sampled once per second. Reported
idle sensor means use seconds 540–600, compared with the baseline tail (samples at seconds ≥30) of the
60-second no-shell baseline. Interaction sensor means use seconds 600–660.
Baseline-subtracted quantities are calculated per arm before taking their median
and range; negative deltas are retained rather than clipped.

### Accounting and validity

RSS, PSS and SwapPss are summed from each live tree's `smaps_rollup` at startup
(first sample after IPC readiness), 10 minutes immediately before interaction,
after interaction and 30 minutes. These are checkpoints, not startup peak memory.
CPU uses `/proc/PID/stat` self plus waited-child user/system ticks for each live
descendant, retaining reaped helper CPU in its parent's child counters. Percent
is `100 × delta_ticks / (100 Hz × elapsed_monotonic_seconds)`. Idle CPU uses the
last pre-interaction minute; interaction CPU uses the following minute. CSV
elapsed times are rounded to 0.1 s; minute CPU uses those endpoint times.
[per-minute-cpu.csv](bead-vs-qml-shell-benchmark-20261110/per-minute-cpu.csv) also
reports all 30 minute windows per arm. Reads are not atomic, and descendants
reparented or missed between samples can escape accounting. Context switches are
summed over threads' voluntary/involuntary counters; vanished threads can be
missed. They are a lower-bound activity proxy, not hardware wakeups.

The collector runs remotely in tmux at nice 10. Each private scope has a 12 GiB
memory limit, swap disabled and an external watchdog; start requires 4 GiB
available, abort threshold is 2 GiB. Arm preflight requires host swap below
64 MiB, and each completed arm must have less than 1 MiB host swap-in and zero
shell/daemon SwapPss. The shared desktop capture lock is held in chunks of at
most 590 seconds. Contention at a boundary invalidates the arm and triggers
exact-PID/start-time-checked cleanup before waiting for the peer. No peer process
was stopped. [guards.csv](bead-vs-qml-shell-benchmark-20261110/guards.csv) records
scope exits, memory rails and OOM counters. The driver checked the host
swap-in threshold, but did not retain its exact before/after counters; successful
progress/exit is the retained evidence for that check.

The final preflight refused to start with 89 MiB swap used. After the peer
capture finished and with no arm/build/desktop CI active, the authorized guarded
reset at 11:47:01 UTC used `swapoff -a` then `swapon -p 100 /dev/zram0`.
MemAvailable was 10,158,168 KiB before, exceeding used swap plus 8 GiB; afterward
it was 10,093,996 KiB and swap usage zero. Zram capacity and priority were
preserved. [preflight.csv](bead-vs-qml-shell-benchmark-20261110/preflight.csv)
contains both guarded resets. No cache drops or kernel tuning were performed.

The controller laptop rebooted near 08:53 and 09:35 local time (07:53 and 08:35
UTC). The desktop did not reboot. All accepted arms begin and finish on one side
of each controller reboot; the first accepted QML arm completed before the
second reboot, and the other five accepted arms began after it. Smoke data are
excluded, including a smoke capture spanning the first reboot. The remote
collector continued independently through controller/account interruptions.

## Hardware and binaries

| Item | Configuration |
| --- | --- |
| CPU / memory | Ryzen 5 7600, 6 cores / 12 threads; 15,204,240 KiB RAM (14.50 GiB) |
| Kernel | `7.3.0-5-cachyos` |
| Rendering device | AMD Raphael Radeon 610M, `/dev/dri/renderD129`; RTX 4070 installed but not selected by these sessions |
| Desktop runtime | Quickshell 0.3.2, Qt 6.12.0, Mesa 26.2.5, labwc 0.20.2 |
| Compositor | nacre-niri `26.04.r3073.gdaa117cf-1`, revision `daa117cf3d7500d01671dd138fa6b55675676579` |
| CPU policy | `amd-pstate-epp`, `powersave`, `balance_performance`, boost enabled; advertised min/max 2,990,951 / 5,171,853 kHz |
| GPU policy | `power_dpm_force_performance_level=auto` |
| Bead framework pin | `8ef16a31917ce785929be49c452534edf5e290f0` |
| Build | dev-1 Arch container; Rust 1.99.1; `--release --locked`, Cargo `-j2`, thin LTO, one codegen unit, stripped; default native-GL feature available |

Bead's tested bar/core panels use their default CPU raster paths, despite native
GL being compiled in; QML uses Qt GL. Both nested compositors use the AMD render
node, verified through their open file descriptors. This is a comparison of the
product defaults, not a controlled CPU-versus-GL renderer experiment. CPU policy,
GPU policy and thermal settings were left unchanged. Required binaries were
built on dev-1; the exact packaged compositor was copied read-only.

| Additional executable | SHA-256 |
| --- | --- |
| `nacre-wallpaper-decode` | `97648f477a681294ebc35ee249034d31b79d1b14ea0b4620d6df2d878c3a5c66` |
| `bead-wallpaper-video` | `e286e24415c049431dd4020200bf531dd0f4c801f669a63421c378c9a4944574` |
| `orient` | `26642e42921c22c473e43a2dfb1feb96af1154d13dc66f2a22c7333ac92188ed` |

## Per-metric results

### Memory and daemon attribution

| Metric | QML | Bead |
| --- | ---: | ---: |
| Startup RSS (MiB) | 584.2 [582.4, 595.7] | 60.6 [60.2, 60.7] |
| Startup PSS (MiB) | 476.1 [473.9, 487.4] | 50.6 [50.3, 50.7] |
| 10 min RSS (MiB) | 627.3 [624.7, 632.4] | 62.6 [62.2, 62.7] |
| 10 min PSS (MiB) | 511.4 [509.3, 516.8] | 52.4 [52.1, 52.6] |
| Post-interaction RSS (MiB) | 651.0 [650.1, 654.4] | 86.7 [85.8, 87.3] |
| Post-interaction PSS (MiB) | 532.2 [531.5, 535.9] | 76.5 [75.7, 77.1] |
| 30 min RSS (MiB) | 553.1 [552.3, 557.9] | 74.1 [73.0, 74.2] |
| 30 min PSS (MiB) | 455.5 [454.8, 460.5] | 64.1 [63.1, 64.2] |
| Post-interaction → 30 min PSS change (MiB) | -75.9 [-77.4, -75.5] | -12.6 [-13.0, -12.3] |

| Metric | QML | Bead |
| --- | ---: | ---: |
| Daemon PSS, startup (MiB) | 37.7 [37.6, 38.0] | 93.7 [66.7, 109.9] |
| Combined PSS, startup (MiB) | 513.8 [511.5, 525.4] | 144.3 [117.0, 160.7] |
| Daemon PSS, 10 min (MiB) | 40.7 [39.3, 40.8] | 53.7 [53.3, 53.9] |
| Combined PSS, 10 min (MiB) | 552.2 [550.0, 556.1] | 105.8 [105.7, 106.5] |
| Daemon PSS, post-interaction (MiB) | 49.3 [49.0, 49.3] | 78.7 [78.6, 78.9] |
| Combined PSS, post-interaction (MiB) | 581.2 [580.8, 585.3] | 155.2 [154.3, 156.0] |
| Daemon PSS, 30 min (MiB) | 42.0 [41.8, 42.0] | 79.5 [78.9, 80.2] |
| Combined PSS, 30 min (MiB) | 497.3 [496.8, 502.4] | 143.6 [142.0, 144.4] |
| Daemon post → final PSS change (MiB) | -7.4 [-7.5, -7.0] | 0.6 [0.3, 1.4] |
| Combined post → final PSS change (MiB) | -83.4 [-84.5, -82.9] | -12.3 [-12.4, -10.9] |

Bead daemon PSS increased by a median 0.6 MiB [0.3, 1.4] from post-interaction
to 30 minutes, while combined PSS fell by 12.3 MiB [10.9, 12.4]. Longer
observation and memory attribution are needed to explain that daemon growth.

The 30-minute trend is final PSS minus the post-interaction checkpoint. It is a
short observation window, not proof of a leak or proof of leak freedom. RSS
counts shared pages in full; PSS apportions them. The owner's physical QML
session remained present, so shared resident Qt/Quickshell pages may reduce the
nested QML PSS attribution relative to a standalone deployment.

### CPU and context switches

| Metric | QML | Bead |
| --- | ---: | ---: |
| Shell idle CPU (%) | 2.02 [1.98, 2.03] | 0.10 [0.08, 0.10] |
| Daemon idle CPU (%) | 0.15 [0.13, 0.15] | 0.45 [0.43, 0.45] |
| Combined idle CPU (%) | 2.17 [2.12, 2.18] | 0.53 [0.53, 0.55] |
| Shell interaction CPU (%) | 56.68 [56.52, 58.08] | 10.73 [10.72, 10.73] |
| Daemon interaction CPU (%) | 0.62 [0.60, 0.62] | 36.62 [36.62, 36.65] |
| Combined interaction CPU (%) | 57.30 [57.12, 58.70] | 47.35 [47.35, 47.37] |
| Shell idle context switches/s | 39.4 [38.8, 40.6] | 4.9 [4.9, 5.0] |
| Shell interaction context switches/s | 491.4 [487.6, 494.6] | 137.6 [136.9, 141.2] |

The CPU figures include exited helpers where their parent retained waited-child
CPU. Shell-plus-daemon CPU is the sum within each run before summarizing; it
is not the sum of independently calculated medians. Idle activity reflects the
specified disconnected-service fixture. Context-switch counts should not be
interpreted as measured timer/interrupt wakeups or direct power consumption.

### GPU, power and heat

| Metric | QML | Bead |
| --- | ---: | ---: |
| Idle GPU busy above baseline (percentage points) | 0.00 [0.00, 0.00] | 0.00 [0.00, 0.00] |
| Interaction GPU busy above baseline (percentage points) | 5.62 [5.28, 5.87] | 1.48 [0.77, 1.65] |
| Idle VRAM above baseline (MiB) | 90.6 [90.6, 92.6] | 13.8 [13.8, 13.8] |
| Interaction VRAM above baseline (MiB) | 103.2 [102.8, 105.0] | 19.6 [19.4, 19.6] |
| No-shell AMD PPT (W) | 19.88 [19.72, 20.04] | 19.78 [19.71, 19.84] |
| Idle AMD PPT (W) | 21.35 [20.88, 21.70] | 20.40 [20.37, 20.43] |
| Idle AMD PPT above baseline (W) | 1.47 [1.17, 1.66] | 0.65 [0.53, 0.69] |
| Interaction AMD PPT above baseline (W) | 11.52 [10.57, 11.85] | 6.52 [5.99, 6.91] |
| Idle Tctl (°C) | 39.1 [39.0, 39.1] | 39.1 [39.0, 39.1] |
| Idle edge (°C) | 37.1 [37.1, 37.1] | 37.0 [37.0, 37.0] |
| Idle Tctl above baseline (°C) | 0.1 [0.1, 0.1] | 0.1 [0.0, 0.1] |
| Idle edge above baseline (°C) | -0.1 [-0.2, 0.1] | 0.0 [-0.0, 0.0] |

GPU busy and VRAM are device-wide AMD sysfs readings (`gpu_busy_percent`,
`mem_info_vram_used`). The whole host and observer contribute; baseline subtraction
cannot perfectly isolate the shell. GPU busy has integer-percent resolution; one-second sampling can miss brief spikes.
AMD hwmon `power1_input`, labelled PPT, is readable and reported as a power proxy
in watts; it is not a GPU-only or whole-system energy measurement. RAPL
`energy_uj` was root-readable only, and no readable independent GPU energy
counter was found. No privileged energy readings were taken. Thus package/GPU
energy deltas, joules and calibrated total-system watts are unavailable.
Tctl and AMD edge are readable hwmon temperatures; absolute temperatures and
per-arm baseline deltas are both retained. Small sensor deltas and ranges do not
support a precise energy-efficiency claim.

### Startup and frame pacing

| Metric | QML | Bead |
| --- | ---: | ---: |
| Launch to provider readiness (s) | 1.208 [1.201, 1.209] | 0.742 [0.736, 0.744] |

Startup begins immediately before shell process launch and ends when the daemon
accepts the shell UI provider state request; Bead additionally waits for its
`READY` log. The harness's optional boot-settle delay is explicitly zero.
Copying the Luau tree and setting up private files are outside this timing.
Neither implementation exposed a common first-presented-frame/fully-interactive
protocol or comparable frame timings in this capture, so those requested
metrics and animation frame pacing are **not measured**. IPC readiness does not
prove that the first frame was visible or that animations were smooth.

### Passive laptop battery evidence

The desktop has no system battery. Laptop access was passive file/journal
reading only, with no shell runs or workload generation. Readable UPower rate
and charge histories were copied passively. Raw Nov 5/9 records are retained in
`battery-rate-cohorts.csv` and `battery-charge-cohorts.csv`, with input hashes and
coverage in `battery-provenance.csv`. UTC grouping and the discharge cohorts were
verified against the later snapshot; both daily counts and medians match the
initial read. Descriptive discharge-rate medians are **8.15 W (912 samples) on
Nov 5** and **5.70 W (899 samples) on Nov 9**; these are irregular rate samples,
not controlled time-weighted energy measurements.

The retained journal starts Nov 5 at 10:18:55 local time. `nacre-shell` activity
appears on Nov 5/6/9 and before the Nov 10 reboot. No explicit switch dates or
QML-only day cohort could be established; `quickshell` matches can also come from
on-demand apps and version probes. Nov 10 is excluded from day comparison
because it is partial and includes controller reboots. The dates have different workload,
coverage and charging histories. **No defensible QML-versus-Bead battery estimate
can be made from this evidence.** See [battery-passive-evidence.csv](bead-vs-qml-shell-benchmark-20261110/battery-passive-evidence.csv).

## Evidence, checks and caveats

The CSV directory includes one compact one-second sample file per accepted run,
two excluded incomplete attempts, the run ledger, provenance, scope guards,
per-run summary, per-minute CPU and median/range table. Empty cells mean missing
or inapplicable, not zero. Raw CPU cumulative ticks are retained; redundant
instantaneous CPU/combined columns, a live-only CPU diagnostic and unavailable
energy channels are omitted to keep the evidence below 2 MB. KiB/MiB are binary;
power inputs are µW, temperatures m°C and VRAM inputs bytes. Baseline and active
elapsed seconds use separate origins; the `phase` column distinguishes them.
Source IDs in the ledger refer to the task directory, not another repository.

Validation on the desktop accepted all six complete windows: matched artifact,
input and geometry records; zero process SwapPss and OOM deltas; capture/scope
exit codes zero; no `UI_ERROR`; active-sample gaps below two seconds. There are
six run summaries, 180 per-minute CPU rows, and three observations in every
median/range group. An independent read-only audit recomputed all six runs,
all 180 minute windows and all 92 median/range rows from the raw CSVs. The
passive UTC discharge cohorts independently reproduce
912 / 899 samples and 8.154 / 5.699 W medians.

On dev-1 in the Arch container, the existing capture-safety suite
`python3 -m unittest test_dashboard_capture_windows.CaptureSafetyTests -v`
passed 31 tests. A focused copied-guard check also passed exact owned-PID
signalling and reused-PID refusal using mocked signals. The documentation part
of `tools/check.py`, `python3 tools/generate_usage_keybindings.py --check`,
printed `Shortcut documentation matches the contract.` (exit 0), using the
built helper/Orient overrides and a private container HOME. Initial missing
artifact/fixture errors were resolved. `tools/check.py` has no docs-only switch;
this generator covers shortcut documentation, rather than linting this research
report. Full product CI was not run for this documentation-only unit.

Three runs per implementation provide a small descriptive sample. There is no
confidence interval or significance claim. Host activity, nested compositor
cost, shared pages, caches (not flushed), disconnected services, different
shell/daemon ownership and CPU-versus-GL defaults limit transfer to a physical
session. The supplied default fixture is held constant; it does not demonstrate
look-and-feel parity. OSD, frame pacing, first-frame/interactive timing, direct
energy and causal battery comparison remain open measurement gaps. The QML
shell should remain available for any follow-up that needs those comparisons.
