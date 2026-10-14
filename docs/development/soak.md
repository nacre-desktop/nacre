# Desktop soak testing

`tools/soak.py` exercises Niri and Hyprland sequentially in nested sessions with
public fixtures and disposable HOME/XDG, D-Bus, network and PID namespaces.
The host filesystem is read-only; only the parent Wayland endpoint is exposed.
It does not install anything, restart the live shell or change user settings.
Use a task worktree, keep its source frozen during both runs, and run in tmux:

```sh
nice -n 10 .venv/bin/python tools/soak.py \
  --hyprland-headless --wayland-debug --seed 20261013 \
  --orient-binary deps/orient/target/release/orient \
  --output /tmp/nacre-soak-before
```

Use the [development environment](../contributing.md), Bubblewrap, Quickshell,
both compositors, D-Bus tools, `notify-send` and `wl-copy`. FFmpeg optionally
creates a synthetic video. Build prerequisites with `-j2`. The driver also lowers
its own priority. It invokes the actual renderer `launch.sh`, including Qt compatibility checks and
renderer-scoped allocator defaults. Build the candidate native helper with
`cargo build -j2 --locked --release -p nacre-helper` and optionally select it with
`NACRE_HELPER_BINARY`; it is copied into the disposable fixture. Hyprland uses a guest
virtual output because its nested Wayland-backed output reproduced a presentation
stall; this does not establish physical-output performance.

Defaults: ten-minute warmup, three measured hours per compositor, 60-second
samples and five-second seeded actions. Confirmation adds `--hours 1` and a new
output directory. Actions include launcher queries, settings/dashboard pages,
pearl detach/dock, notifications, clipboard, windows/workspaces, wallpaper/video
and palettes; Niri also changes scale. At most four detached pearls accumulate.
`--actions settings close --action-interval 0.5` selects a supplemental remapping
probe; `--notification-images` selects raw D-Bus image payloads and requires
`dbus-next`. Keep these supplemental probes separate from matched comparisons.

Artifacts: `samples.csv`, `events.jsonl`, `report.json`, logs and a bounded 4 MiB
Wayland trace. Copy captures into ignored `.research/soak/` before reboot; `/tmp`
is temporary. Reports record the revision and source fingerprint, duration,
workload health, exits, failed actions/probes and helper lifetimes. No restart
hides a crash. Two minutes of failed IPC ends an invalid workload. Example:

```sh
.venv/bin/python tools/soak_analyze.py \
  .research/soak/before/niri .research/soak/before/hyprland
NACRE_RUN_SOAK_REGRESSIONS=1 .venv/bin/python -m unittest discover \
  -s tools/tests -p test_soak_settings.py
```

Analysis excludes warmup and separates PID/start-time lifetimes. Resident PSS
apportions shared pages; swapped PSS counts paged-out memory. Use their sum on a
busy host: falling resident memory alone does not prove release. Missing memory
coverage remains unknown. The summary prints full/first/final-hour trends and
mixed-action correlations, never raw captures. Sustained growth is an investigation
signal, not proof of infinite growth; finite runs cannot prove an eventual limit.
These counters omit unmapped GPU allocations. Short helper lifetimes and one
zombie observation do not establish unreaped processes. Dashboard redesign and
Python helper internals remain report-only; port acceptance is tracked in
[rust-migration](rust-migration.md#fix-during-port).

## Results and fixes

Seed 20261013; baseline source `b17827f7`, confirmation source `092ebad0`.
Both baselines completed 180 measured minutes after ten-minute warmup; both
confirmations completed 60 measured minutes after the same warmup. Every run had
zero shell exits, action failures and probe failures; each long-lived service and
wallpaper watcher started once. These frozen captures predate the native helper
migration and are not measurements of the newer integrated runtime.

| Compositor/run | Measured minutes | Shell PSS + SwapPss, MiB/h (R²) | Dismissal keys at end | Qt focus warnings, disable / enable |
| --- | ---: | ---: | ---: | ---: |
| Niri before | 180 | +119.9 (.816); matched first hour +163.3 (.576) | 711 | 647 / 471 |
| Hyprland before | 180 | +144.7 (.880); matched first hour +58.3 (.228) | 718 | 705 / 421 |
| Niri after | 60 | +177.2 (.670); final 30 minutes −45.3 (.095) | 0 | 321 / 43 |
| Hyprland after | 60 | +171.8 (.738); final 30 minutes +25.4 (.030) | 0 | 286 / 146 |

The notification defect is fixed; **broad memory-growth flattening is not
established**. Whole-hour shell fits remain positive. Final half-hour fits are
weak, consistent with fluctuating working sets, but do not prove a durable plateau;
three-hour AFTER behavior is unmeasured. Baseline growth continued in its final
hour. Long-lived helpers stayed near flat in both comparisons; the largest after
fit was about +0.4 MiB/h. Short-lived processes remain insufficiently sampled.
Both after endpoints had zero retained rows, popups, tracked native notifications,
native references, images and stale dismissal keys. Baselines did not measure all
of those native counters; their scripted notification workload contained no images.

### Launcher-policy correction

The historical harness launched Quickshell directly, bypassing `launch.sh`'s
jemalloc policy. Therefore **all table rows are matched default-allocator stress
captures**, despite retaining the public Qt toolkit policy. They are not normal
launcher-policy memory measurements. The harness now invokes the actual launcher
so these defaults cannot silently drift again.

A separate three-minute same-seed Niri probe verified effective allocator settings:
direct launch had background reclamation disabled, dirty decay 10000 ms and muzzy
decay 0; normal launch enabled background reclamation with both decays 2000 ms.
Final jemalloc allocated / active / resident / metadata MiB were
165.8 / 190.3 / 234.2 / 17.2 directly and
171.0 / 194.8 / 218.2 / 17.3 through the launcher. This demonstrates reclamation
policy and some allocator overhead, not attribution of the entire hourly signal.
Allocator resident statistics are not process PSS and omit allocations outside
jemalloc. Passive confirmation mapping summaries locate most residual growth in
anonymous shell mappings, with much smaller shared-buffer/file-backed changes.
A longer normal-policy AFTER measurement remains necessary for production trend
claims. These busy-host/swapped runs are not quiet hardware performance evidence.

### Application fixes and remaining Qt issue

Dismissal tombstones protect only the initial asynchronous history load; successful
restore drops them. Removed entries immediately release native references,
snapshots/images and timers. Long-timeout and DND native expiry no longer depends
on popup visibility, while retained text remains in history; zero-timeout notices
remain permanent. Expired history drops live image URLs. Regressions cover initial
load/reentrant races, repeated clears, image release and hidden/DND expiry. A real
isolated D-Bus/Quickshell check passed 30 raw-image cycles and automatic expiry.

Settings remapping leaves native rendering enabled and lets Qt suppress unexposed
frames. Toggling Quickshell's update gate requested a frame during remapping;
failed traces identified Settings attaching a buffer before initial configure.
The native rapid-remap regression passed and both hour-long confirmations were
healthy. Earlier failed/interrupted attempts remain preserved; the AFTER tests
cover one measured hour per backend, not every historical failure interval.

Repeated Qt Wayland text-input warnings **remain unresolved in Qt 6.12**. Bounded
call stacks identify Qt's focus-window dispatch and focus-object clearing. Plain
Qt Window/TextInput controls, with no Nacre UI, reproduced 83 warnings in one
minute; removing explicit text-field focus requests still reproduced 36. Private
pre-hide and activation-timing Nacre candidates passed focus-preservation unit
checks but did not reduce native warnings (13 control, 15 / 14 candidates), so
both were withdrawn. No warning suppression or input-method disabling was added.
The minimal reproducer is retained with private evidence for a Qt-side correction.
