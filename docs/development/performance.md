# Shell performance

## Measurement method

Measure an unchanged revision before migrating helpers. Use the same interpreter,
wallpaper fixtures, compositor, cache state, and completion predicates afterward.
Do not benchmark by changing a maintainer's private profile or preferences.

`tools/performance.py --pid SHELL_PID --output /tmp/before.json` samples for five
minutes at 50 ms intervals. It records each process's CPU and RSS, distributions
for the shell and aggregate, and Python launches in each minute. The default scope
is that shell's descendants plus same-user detached workers under the deployed
Nacre shell and brain directories. `--descendants-only` isolates a test shell.
The sampler itself is excluded. PID plus process start time distinguishes PID
reuse; a shell restart invalidates the run.

CPU is percent of **one core**: 100% means one fully occupied core, not the whole
machine. `/proc/PID/stat` user/system ticks are divided by `SC_CLK_TCK` and elapsed
monotonic seconds. RSS is resident pages times `SC_PAGE_SIZE`, in MiB (2^20 bytes).
Summing RSS counts shared pages in each process; it is not exclusive physical
memory. Short-lived helpers may be missed by sampling, including their CPU; the
live spawn count is explicitly a lower bound. Use a tracer when available, or the
launch logger below, for a complete count of the covered interpreter launches.

For an isolated shell, place a symlink named `python3` to
`tools/performance-python3.sh` first in PATH. Set `NACRE_PERF_PYTHON` to the absolute
real interpreter and `NACRE_PERF_SPAWN_LOG` to a private log. Pass that log to the
sampler with `--spawn-log`. The wrapper logs boot seconds and PID, then execs the
interpreter. It never logs arguments. This counts every PATH `python3` launch,
including short-lived ones, but excludes absolute interpreter paths. The wrapper
adds a small shell/append overhead, so use it identically before and after.

Action specifications are JSON objects keyed by action name. Each entry supplies
`prepare`, `call`, `state`, `expected`, and `cleanup` IPC argument arrays. Multiple
readiness conditions can be supplied in `checks`. For example:

```json
{
  "launcher": {
    "prepare": [["nacre", "close"]],
    "call": ["nacre", "launcher", ""],
    "state": ["nacre", "state"],
    "expected": {"launcher": true},
    "cleanup": [["nacre", "close"]]
  },
  "dashboard": {
    "prepare": [["nacre", "close"]],
    "call": ["nacre", "tab", "0"],
    "checks": [
      {"state": ["nacre", "state"], "expected": {"dashboard": true}},
      {"state": ["dashboardState", "state"], "expected": {"shown": true, "busy": false, "queued": 0, "message": ""}}
    ],
    "cleanup": [["nacre", "close"]]
  },
  "settings": {
    "prepare": [["settingsApp", "close"]],
    "call": ["settingsApp", "open", "appearance"],
    "state": ["settingsApp", "state"],
    "expected": {"visible": true, "created": true},
    "cleanup": [["settingsApp", "close"]]
  }
}
```

Run `python3 tools/performance.py --pid SHELL_PID --actions /tmp/actions.json
--repeats 10 --output /tmp/before.json`. Timing starts before the action IPC call
and ends after the ready predicates are observed. It includes IPC subprocess and
polling overhead. Launcher/settings predicates measure acknowledged opening;
dashboard additionally waits for its data queue. These are **not first-frame or
input-to-photon measurements**. Cleanup closes opened surfaces without saving
settings. Only run the example when those surfaces were initially closed.

For wallpaper, use two disposable 1920x1080 PNGs with distinct colors. Prepare by
selecting A, wait two seconds, then call `wallpaper set B`. Require wallpaper state
`selected == B`, `applied == B`, `busy == false`, `error == ""`, and presentation
state `activeReady == true`, `pendingMatchesActive == true`, `error == ""`. Cleanup
selects A. For palette, prepare `wallpaper configure {"paletteMode":"dark"}`,
wait two seconds, apply light, and require no wallpaper busy/error and presentation
mode light with an active matching pair. Cleanup restores dark. Increase the
timeout to 60 seconds for these two operations. Run them only in a disposable home.

The isolated run used a headless labwc compositor (`WLR_BACKENDS=headless`,
`WLR_RENDERER=pixman`, `WLR_LIBINPUT_NO_DEVICES=1`), a short private XDG runtime
directory (Unix socket paths must fit), a temporary HOME/XDG config/cache/state,
and `dbus-run-session`. Copy the baseline QML/tools so later source edits cannot
affect it. Supply the pinned palette interpreter and public branding beside the
tools. A sandbox-only CLI wrapper ran the existing palette CLI and
`classic-state.apply_palette(home, poster, live=False)`; `wallpaper.postHook=true`
disabled prepared live publication. This preserved paired shell presentation
without reloading real desktop applications or modifying their settings. Use the
same wrapper for both revisions. Hyprland IPC/global shortcuts and PipeWire were
unavailable in this compositor; hardware behavior is covered by regression tests,
not inferred from this run. labwc rejected the floating Settings window with an
existing xdg-surface protocol error, so Settings was measured on live Hyprland.

`tools/benchmark-helpers.py --output /tmp/helpers-before.json` runs 100 warm
requests per helper plus one cold request in a disposable home. It exercises a
bounded invalid conversion, dashboard read, launcher preference load, and history
load. The conversion intentionally avoids an external calculator and isolates
Python startup/import cost. With `--service`, the identical requests use the
persistent worker. The benchmark checks stable response content, excluding the
dashboard's intentionally changing checked timestamp, and records service startup
separately. This is a helper microbenchmark, not a UI measurement.

## Phase A baseline (2026-10-12)

Source: `7b504f2`, Quickshell 0.3.2, Python 3.14, Qt 6 on Arch Linux. Both runs used
five minutes and 50 ms sampling. No production code was changed before capture.
The live QML matched this revision. An initial exploratory run accidentally
included sandbox workers; it was discarded and the process scope corrected.

| Five-minute idle capture | Shell CPU mean (one core %) | Aggregate CPU mean (%) | Shell RSS mean / p95 (MiB) | Aggregate RSS mean / p95 (MiB) | Python launches by minute |
| --- | ---: | ---: | ---: | ---: | --- |
| Live desktop + detached Nacre workers | 0.033 | 3.489 | 583.1 / 584.1 | 1101.5 / 1105.0 | ≥1, ≥1, ≥3, ≥1, ≥1 |
| Isolated shell descendants | 0.024 | 0.514 | 554.4 / 555.7 | 611.1 / 611.9 | 1, 1, 1, 0, 1 |

The live session had ambient Nacre brain/chat work and normal wallpaper rotation;
the aggregate is not a controlled whole-desktop idle budget. The isolated capture
was warmed with a static fixture, closed panels, and rotation disabled. Its launch
count covers PATH python3 only. Idle results do not establish a high renderer CPU
cost; frequent short actions are a separate source of interpreter overhead.

| Action (10 runs) | Live median / p95 (ms) | Isolated median / p95 (ms) | Isolated PATH python3 launches during timed interval |
| --- | ---: | ---: | --- |
| Launcher open acknowledged | 114.4 / 150.5 | 104.3 / 137.0 | 3 each |
| Dashboard open and data ready | 124.8 / 135.3 | 117.1 / 127.7 | 2 normally; 3 twice |
| Settings open acknowledged | 78.5 / 127.3 | Unsupported compositor | Not collected live |
| Wallpaper pair ready | Not run on private settings | 228.8 / 501.8 | 0–1 |
| Palette pair ready | Not run on private settings | 192.8 / 514.7 | 1–2 |

Launches in an action interval may include background events, not just the action.
Wallpaper/palette already use a persistent publisher, so they are comparison and
regression targets rather than an assumption that a new publisher is needed.

| Fresh-interpreter helper (100 warm requests) | Median / p95 (ms) |
| --- | ---: |
| Menu conversion validation | 58.11 / 63.76 |
| Dashboard read | 42.39 / 48.81 |
| Launcher preferences load | 36.64 / 41.07 |
| Notification history load | 18.99 / 20.23 |

These four workloads started 404 Python processes, including cold requests.
Before/after results and failure validation are added after migration.
