# QML desktop power baseline

## Scope and conditions

2026-10-14, owner's laptop, charger connected. The owner asked to retain all apps
and other AI agents. This is therefore a baseline of the **current background
workload**, not an app-free idle baseline or the shell's isolated power share.
No power profile, brightness, refresh rate, compositor or Nacre settings changed;
no existing processes stopped/restarted and no packages installed.

| Setup | Observed value |
| --- | --- |
| CPU | Intel Core Ultra 7 255H, 16 logical CPUs |
| GPU | Intel Arrow Lake-P / Arc Pro 130T/140T, PCI 8086:7d51, i915 |
| Kernel | 7.2.9-2-cachyos |
| Compositor | Niri 26.04 (8ed0da4) |
| Internal display | 2880 × 1800, scale 1.5, 120 Hz; VRR disabled |
| Brightness | 400 / 400 (100%) |
| Profile / governor / EPP | performance / intel_pstate powersave / performance |
| AC / battery state at preflight | AC online; battery charging |
| Latest deployment marker | 68daa258f374542975ed21a2686341f7198f1e23 |
| Deployment scope | Mixed installed files; latest marker is not a whole-tree identity |

Running at preflight: one production Quickshell, its shell supervisor,
compositor event bridge, wallpaper watcher, wallpaper-media server, Python helper
service and chat client; multiple Codex/AI workers and their terminals; a nested
native-shell/compositor test. Chrome, Discord and Spotify also remain running.
The observed Python helper differs from current main's native helper path.
The Niri session-preparation marker is caa84210a13c193491c5828438e4a14c3c8044ea;
this is a preparation revision, not proof of current deployed Nacre contents.

## Measurements

Read-only capture: **14:20:55–14:30:56 CEST** (12:20:55–12:30:56 UTC),
600.22 seconds and 60 ten-second intervals. Owner-away and continuously-screen-on
conditions are **unconfirmed**. Display DPMS was On at the end; resolution, scale,
refresh, brightness and AC/charging state matched preflight. No protected capture
was supplied, so this is an **incomplete power baseline**, not a wattage result.

| Metric | 10-minute baseline |
| --- | --- |
| RAPL package power, average / p95 of 10-second intervals | Unavailable: RAPL energy_uj requires owner-operated root reading |
| Package C-state residency | Unavailable with installed tools |
| GPU RC6 residency, GT0 / GT1 average | 45.955% / 99.644% |
| GPU RC6 residency, GT0 / GT1 p95 | 62.773% / 100.050% (sampling skew near 100%) |
| Nacre + AI CPU seconds / voluntary switches per second | 465.350 s / 2869.67/s |
| Production Quickshell CPU / voluntary switches | 54.910 s (9.15% of one core) / 62.29/s |
| Other processes CPU seconds / voluntary switches per second | 211.660 s / 4691.21/s |
| Sampler CPU / voluntary switches | 1.410 s / 2.64/s |
| Nacre PSS, start / end | Partial: 746.88 MiB (74/144 readable) → 682.67 MiB (81/151 readable) |
| True per-process wakeups/s | Unavailable; do not substitute context switches |

Production Quickshell retained the same PID/start identity for all 61 snapshots.
Two additional test Quickshell identities appeared; one was observed for 70 s.
The aggregate includes concurrent AI/test work, not just the production shell.
Partial PSS has changing coverage: **do not interpret its change as memory saved**.
The assistant parent/progress rendering is included, except the sampler itself.

Manual QML/JS/qmldir content hashes at 14:14 and after the run match:
`93efd0e072e5b52125ac23ea5bb2178b8b0327bfbd373c760002dd7735559942`.
The original automatic fingerprint was unavailable because `NACRE_QS_CONFIG`
pointed to `shell.qml`. The delivered script accepts a file or directory and
records connected-display DPMS per sample; neither improvement was active during
this capture. Raw original files remain unchanged.

Top activity proxies, **not attributed hardware wakeup sources**:

| Process/role | Voluntary switches/s | CPU seconds |
| --- | ---: | ---: |
| Codex parent | 475.12 | 28.93 |
| Niri compositor | 310.22 | 79.37 |
| Agent terminal (Kitty) | 215.03 | 16.34 |
| Another agent terminal (Kitty) | 191.23 | 14.93 |
| Encrypted-storage write worker | 169.13 | 0.51 |
| Kernel RCU worker | 163.71 | 0.41 |
| Spotify process | 148.44 | 11.52 |
| Production Quickshell | 62.29 | 54.91 |

RAPL measures the CPU package, **not whole-laptop watts**. AC charging, display,
SSD, radios and conversion losses prevent treating this as wall power. Intel
GT0 and GT1 idle counters are reported separately, not added together.

## Repeating the capture

Run `tools/measure-power.sh --help`. Default capture: 600 seconds with a 10-second
cadence. Set `--revision` to the verified **live** revision, which can differ from
the sampler checkout. Set the live `NIRI_SOCKET` or
`HYPRLAND_INSTANCE_SIGNATURE` to record compositor/output information.
The small compositor-query adapter lives under `nacre/shell-tools/compositor_backends/`;
run the sampler from this checkout when measuring Bead. Use `--lead-in 60` to allow stepping away. The script does not prevent blanking
or locking; verify those conditions with the owner. Stop working/building during
measurement. Match background workload and wallpaper playback for Bead.

Raw data and summaries default to the ignored `.power-baseline/` directory, with
private permissions. No arguments, window titles or process environments are
written. `--scope ERE` defines Nacre's process roots from command lines, includes
descendants, and excludes sampler descendants. Default scope includes AI workers
and both QML/native shells; review `processes.tsv` before comparison.

If the owner chooses root readings, they run
`sudo bash tools/measure-power.sh --protected --duration 600 --interval 10 --lead-in 60 > .power-baseline/protected.tsv`
themselves alongside the normal capture. Only the owner shell opens the output
file. This streams package energy and start/end PSS without installing tools,
changing permissions, loading modules or writing system state. Merge the stream
with `tools/measure-power.sh --summarize RUN_DIRECTORY --protected-data FILE`.
Protected and normal samplers have independent timestamps; report both windows.

Energy differences account for counter wrap and actual monotonic elapsed time
(`/proc/uptime`), then convert microjoules to watts. The mean is time-weighted;
p95 is the nearest-rank percentile of interval averages, not instantaneous peaks.
CPU time uses `/proc/PID/stat` ticks and `CLK_TCK`. Thread voluntary-switch deltas
use `/proc/PID/task/TID/status`; PID/thread start times distinguish reuse.
Short-lived processes/threads between scans can be missed. PSS is proportional
mapped memory; it excludes unmapped GPU allocations. Incomplete PSS must include
readable/expected process counts. Sampler overhead is grouped separately;
the assistant's own AI parent and progress rendering are not fully excluded.

Counter units are described in the [kernel powercap documentation](https://www.kernel.org/doc/html/latest/power/powercap/powercap.html).
[Kernel proc documentation](https://www.kernel.org/doc/html/latest/filesystems/proc.html)
explains context-switch and memory counters. Context switches neither establish
hardware wakeups nor attribute which interrupt/timer caused them. No tracing
configuration is changed to obtain unavailable wakeup/C-state metrics. This host
had `kernel.sched_schedstats=0`; it was left unchanged.

## Follow-ups, without fixes

Source inspection is evidence of possible work, not measured power attribution:

- `ChatWindowTitle.qml`: 15-second scan while AI task windows exist. This condition
  holds in the current workload; measure its subprocess/activity cost later.
- `modules/background/NacreDesktopVideo.qml`: infinite media looping when dynamic
  wallpaper playback is allowed. `NacreWallpaperScene.qml` gates playback on
  sleep/lock/pause/battery/coverage. Confirm actual playback before attributing watts.
- `NacreHelpers.qml`: one-second timeout check only while requests are pending.
  Busy agents may sustain work; it is not an unconditional idle poll.
- Brightness and keyboard-light reads every three seconds are controls-visible
  only; dashboard network/system polls every two/ten seconds require its visibility.
  Desktop-settings polling is also visible-only. Verify visibility/lifetime if
  these appear active with panels closed.
- `Updates.qml`: unconditional 30-minute check, potentially one burst in a long
  run. Record whether a check overlapped before comparing averages.

Optional shell-stopped, 60 Hz and 15-minute battery comparisons require separate
owner approval. Approval questions were presented; no such comparisons were approved or performed.

Validation: Bash syntax, read-only smoke captures and synthetic summary cases
passed, including RAPL wrap/nearest-rank p95, RC6 averaging, PID reuse and protected
stream merging. No builds or package installs were needed for these checks.
Local source/JSON/shell/Lua/QML formatting, generated-config and compositor-boundary
checks passed. The broader suite reached 271 tools tests with three environment
errors (missing Cargo for staging, and installed Orient differing from the pin);
Ruff was unavailable. No environment changes or unrelated fixes were made.

This commit uses `[no-publish]`: source CI still runs, while the automatic Arch
package job skips that marker. Manual package runs remain explicit. This prevents
the normal source → package → release/deployment chain for this measurement task.
