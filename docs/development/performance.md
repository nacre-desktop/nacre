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

## Phase B: shared frequent-helper service

`NacreHelpers` starts one `helper-service.py` child with JSON lines over its private
stdin/stdout pipes. Request IDs route replies to `NacreHelperTask` owners. The
service imports supported helpers once, runs at most four requests concurrently,
and serializes operations within each helper. It bounds pending work to 64,
requests to 4 MiB, and responses to 8 MiB. Only explicit operations are supported;
there is no arbitrary script, shell command, menu tool, or process-kill dispatch.

Twelve QML Process blocks now use the service: launcher preference load/writes,
dashboard reads/focus/preferences, network snapshots, notification history
load/save, menu status/files/conversions/packages/ownership, and media details.
Existing standalone CLIs remain available. Their validation, atomic writes,
advisory locks, output content, and caller queues are retained. The wallpaper
publisher, streaming spectrum, chat transport, compositor helpers, resource
readings, and maintenance/doctor paths remain with their existing owners.

Small files already have asynchronous FileView readers. The migrated preference
and notification operations also initialize, validate, filter, or lock state, so
duplicating them as raw JSON reads in QML would not simplify their behavior.
The /proc and /sys readers belong to the hardware task and were left untouched.

On a crash, pending actions fail once and the shell restarts only the helper, with
250 ms–10 s retry backoff. Startup has a ten-second deadline; outstanding requests
have a 45-second deadline. Timers stop when there is no outstanding work after
startup stabilizes. Interrupted requests are never replayed. An explicit stopping
state preserves fresh requests queued during shutdown for the replacement worker.
This fixes the shutdown race found by the native failure tests. A canceled or
destroyed read owner drops its reply; already dispatched backend work can finish.
State-changing owners remain persistent and retain their existing serial queues.

### Before/after results

Candidate production code: `c927c9a`. These are single five-minute captures and
ten action repetitions under the same isolated labwc setup, fixture paths,
interpreter, warmed caches, and predicates used above. Idle means and small timing
differences are not statistically established improvements.

| Isolated idle (five minutes) | Before | After |
| --- | ---: | ---: |
| Shell CPU mean (% of one core) | 0.024 | 0.024 |
| Aggregate CPU mean (%) | 0.514 | 0.451 |
| Shell RSS mean / p95 (MiB) | 554.4 / 555.7 | 560.0 / 560.3 |
| Aggregate RSS mean / p95 (MiB) | 611.1 / 611.9 | 643.7 / 643.5 |
| PATH python3 launches by minute | 1, 1, 1, 0, 1 | 1, 1, 1, 1, 0 |

The persistent worker trades about **32.6 MiB additional aggregate RSS in this
capture** for interpreter reuse; RSS includes shared pages and cache variation.
It does not remove the idle launches from the excluded helpers. Its own healthy
idle path blocks on stdin and performs no polling.

| Action | Before median / p95 (ms) | After median / p95 (ms) | PATH python3 launches before → after |
| --- | ---: | ---: | --- |
| Launcher open acknowledged | 104.3 / 137.0 | 107.9 / 140.5 | 3 each → 2 each |
| Dashboard open and data ready | 117.1 / 127.7 | 113.6 / 121.5 | normally 2 → normally 1 |
| Wallpaper pair ready | 228.8 / 501.8 | 225.2 / 245.1 | 0–1 → 0–1 |
| Palette pair ready | 192.8 / 514.7 | 188.7 / 196.7 | 1–2 → 1–2 |
| Settings acknowledged, nested Hyprland | 149.2 / 192.9 | 143.8 / 205.1 | Not collected |

Settings uses a separate, comparable before/after pair: Hyprland 0.56.2 nested in
a headless **GLES2** labwc compositor, with a disposable home and D-Bus session.
`AQ_DRM_DEVICES=/dev/null` selected the nested Wayland fallback, and the shell used
the nested instance signature/socket. A pixman parent cannot provide the required
dmabuf backend. This setup supported the unchanged floating Settings window. The
live desktop's source, running shell, profiles, wallpaper and preferences were
not changed. Do not compare nested Settings timing directly with live Hyprland or
labwc panel timing.

Other event-driven work sometimes added one background launch during a dashboard
interval. The consistent removal of one launch per open is the useful result.
CLI IPC and sampling mask much of the request improvement: launcher opening was
slightly slower by this measurement, and the dashboard difference was small.
The unchanged wallpaper publisher also had fewer outliers on the second run;
this does not establish that the helper service caused its improvement.

| Identical helper workload (100 warm requests) | Fresh Python median / p95 (ms) | Service median / p95 (ms) |
| --- | ---: | ---: |
| Menu conversion validation | 58.11 / 63.76 | 0.039 / 0.475 |
| Dashboard read | 42.39 / 48.81 | 0.091 / 0.113 |
| Launcher preferences load | 36.64 / 41.07 | 0.048 / 0.067 |
| Notification history load | 18.99 / 20.23 | 0.029 / 0.057 |

The same 404 requests now use **one Python process**, a 99.75% reduction for this
workload. Worker startup was 31.1 ms; first module imports were measured separately
(menu 30.0 ms, dashboard 4.7 ms). Warm figures do not hide startup/import cost.

`tools/benchmark-services.py` additionally measures the **actual production QML
dashboard request owner** from `request("read")` to focus snapshot publication,
using native Quickshell and only fixture panel/settings dependencies. It renders
no window and avoids CLI IPC subprocess/polling overhead. Run it against the
baseline `--services` and `--tools` directories, then the candidate, with
`--output /tmp/owner.json`. Each run issues 100 warmed reads in a fresh disposable
home. QML Date.now supplies wall-clock timestamps with 1 ms resolution; clock
adjustments invalidate a run. This measures data publication, not a rendered frame.

| Production dashboard data refresh (100 requests) | Before | After |
| --- | ---: | ---: |
| Mean (ms) | 39.50 | 0.49 |
| Median / p95 (ms) | 39 / 46 | below 1 / 1 |

This confirms that common dashboard data requests are faster even though opening
timing includes other UI and CLI overhead. It does not imply a corresponding
39 ms improvement in input-to-photon latency.

### Validation and scope

The protocol tests cover legacy CLI response equivalence, preference/history
writes, invalid and non-finite requests, size/queue bounds, independent requests,
module reuse, and rejection of unsupported menu operations. Native Quickshell
tests kill a worker with two pending requests, inject malformed responses, and
expire deadlines; each verifies failed requests, successful recovery, and no
replay. Existing native network, notification, dashboard-focus and menu tests use
the same caller state machines with a substituted transport. Source checks cover
Python lint/format, QML/JavaScript parsing/format, shell/Lua syntax and regression
suites. Run `python tools/check.py` with its pinned development environment;
redirect stdin from `/dev/null` when running unattended in tmux.

No visual/layout changes were made. The only overlap with compositor-related
source is replacing the five helper transport blocks in NacreMenuContext; its
window dispatch, logout, keybinding sources and menu-backend.py were not changed.
Remaining launches and first-frame latency require separate work with their
owners rather than claiming this migration removes every Python process.

### Shared helper runtime

The shared worker runs with Nacre's managed palette interpreter at
`~/.local/share/nacre/palette-runtime/venv/bin/python`. Media artwork requests
need its pinned Orient and Pillow packages; the remaining migrated helpers use
the standard library. A system `python3` can successfully serve other requests
while failing artwork extraction, so media equivalence tests use a fresh isolated
runtime with the candidate Orient source and no inherited `PYTHONPATH`.
