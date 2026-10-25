# Shell performance

## Measurement method

Measure an unchanged revision before migrating helpers. Use the same interpreter,
wallpaper fixtures, compositor, cache state, and completion predicates afterward.
Do not benchmark by changing a maintainer's private profile or preferences.

`tools/performance.py --pid SHELL_PID --output /tmp/before.json` samples for five
minutes at 50 ms intervals. It records each process's CPU and resident memory, distributions
for the shell and aggregate, and Python launches in each minute. The default scope
is that shell's descendants plus same-user detached workers under the deployed
Nacre shell and brain directories. `--descendants-only` isolates a test shell.
The sampler itself is excluded. PID plus process start time distinguishes PID
reuse; a shell restart invalidates the run.

CPU is percent of **one core**: 100% means one fully occupied core, not the whole
machine. `/proc/PID/stat` user/system ticks are divided by `SC_CLK_TCK` and elapsed
monotonic seconds. RSS uses `smaps_rollup` where available and falls back to resident pages times
`SC_PAGE_SIZE`, in MiB (2^20 bytes).
Summing RSS counts shared pages in each process; it is not exclusive physical
memory.

Report version 2 also records PSS (proportional set size), USS (unique set size),
and proportional swap from `/proc/PID/smaps_rollup`. PSS apportions shared pages
between their users; USS sums `Private_Clean` and `Private_Dirty`. These resident
metrics exclude swapped pages, unmapped GPU allocations and hugetlb accounting.
See the [kernel proc documentation](https://docs.kernel.org/filesystems/proc.html).
Permission errors, malformed rollups and vanished processes produce unknown
values, never zero. Aggregate PSS/USS is unknown if any selected process lacks a
rollup; `memory_valid_samples` supplies coverage. Process start time is checked
again after the rollup to reject PID reuse. Rollups are read only for the selected
processes; use a 250 ms interval for memory scenarios to reduce sampling overhead.
 Short-lived helpers may be missed by sampling, including their CPU; the
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

Notification history clearing publishes one array-model change before dismissing
native notifications and deferring entry destruction. Removing entries one by one
recreates every remaining history card and can block shell input as history grows.
The native regression mounts 300 synthetic entries in the production cards and
checks that clearing creates no replacement cards. Lifecycle tests also preserve
transient popups, arrivals during native close callbacks, and clearing before
saved history has loaded. These fixtures do not measure displayed frames or
compositor input latency.

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
service imports supported helpers once and gives each of its six supported helpers
one serial execution lane. Threads start lazily, with at most one active request
per helper and six active requests overall. Same-helper requests run in arrival
order; queued menu searches cannot occupy the workers needed by network or
notification requests. It bounds pending work across all lanes to 64,
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

## Phase C: panel and wallpaper memory (2026-10-12)

The ~555 MiB Phase A RSS figure is a warmed process on a different output setup.
It must not be treated as 555 MiB of exclusive RAM or compared with a fresh shell.
This investigation uses PSS/USS and a sequential panel matrix in a disposable
home, with both a private writable mode-0700 XDG runtime directory and a private
D-Bus session. No live settings, account permissions or wallpaper were changed.

`NacreIdleLoader` keeps the apps launcher content warm after its first use; it
does not instantiate it at startup. Reopening reuses the shell and delegates;
launcher reset still runs on each open. Other launcher modes unload five seconds
after their closing animation. Floating Settings, Colors
and Welcome keep their window identity, position and size, but release content
five seconds after closing. Search, page, scroll, expansion and color-inspection
state are captured and restored; backend operations and unsaved options keep
existing service owners. Dashboard pages already unload and AI session/draft
ownership is intentionally retained.

The previous full-screen wallpaper poster gets the same five-second reverse
navigation window and is then released. The current poster and the old poster
needed for an active crossfade remain intact. Wallpaper decodes already target
output dimensions; ordinary preview/icon decodes already use bounded sourceSize.
Video implementations are separate files loaded by URL only while video is
needed, so a fresh static shell avoids Qt Multimedia registration. After video
has been used, library mappings may remain; this is not a promise to unload DSOs.

Native tests cover reuse, cancellation of an idle release on reopen, real Loader
destruction, view state restoration without Apply, changing video without
reopening, muted first frames, crossfade/reverse navigation and expired poster
slots. No layout, colors, visible strings or private preferences change.


### Matched panel matrix

The controlled baseline is `fa37a28`; the candidate is that same immutable source
copy plus the performance QML changes from `9e29f43`. Unrelated subsequent main
changes (Orient pin, pointer/focus and shell-hide behavior) are held constant in
this pair. All performance QML files were hash-checked against the final code;
final source/native checks additionally run on current main with Orient dev5.
Quickshell 0.3.2, Qt 6.12, Hyprland 0.56.2 and Python 3.14 were used.

Both processes use a disposable HOME and explicit private config/cache/state/data
homes, a writable private mode-0700 runtime directory and private D-Bus session.
XDG_DATA_DIRS lists only public system directories. The fixtures are generated
3840×2160 PNGs, a 1920×1080 H.264/24 fps clip, 48 demo desktop entries and 12 demo
favorites with local 128×128 PNG icons. Shell-owned checks verified actual IDs,
12 visible favorites and ready icon images; no personal catalog was copied.
On-disk caches are warm for both processes. Initial captures with an inherited
XDG data path and absent favorites were discarded and superseded.

A headless labwc GLES2 parent supports nested Hyprland with an explicitly created
1920×1080 headless output, scale 1. Production session configuration/window rules
are not inherited. PipeWire and hardware behavior are outside this lab. Each
surface is opened for two seconds, sampled for two seconds at 250 ms intervals,
closed and allowed six seconds to settle, then sampled again. Rows are sequential,
so later rows include earlier activity. Values are shell median **RSS / PSS / USS**,
in MiB; the companion JSON retains distributions. These short endpoint captures
are not five-minute idle-CPU measurements or measurements of unmapped GPU memory.

| Scenario | Before RSS / PSS / USS | After RSS / PSS / USS |
| --- | ---: | ---: |
| Fresh start; nothing opened | 292.9 / 211.7 / 194.7 | 291.1 / 215.1 / 194.9 |
| Launcher closed | 325.8 / 241.4 / 225.0 | 325.0 / 244.9 / 227.2 |
| Today closed | 334.0 / 252.2 / 233.7 | 340.6 / 260.5 / 242.5 |
| Weather closed | 335.8 / 255.1 / 235.2 | 344.6 / 266.9 / 246.2 |
| Media closed | 334.6 / 251.9 / 233.5 | 344.9 / 263.6 / 245.9 |
| System closed | 335.7 / 255.6 / 234.5 | 342.4 / 264.8 / 244.3 |
| Focus closed | 335.6 / 252.1 / 234.3 | 342.2 / 261.0 / 243.6 |
| Settings closed | 348.6 / 262.9 / 244.6 | 357.7 / 274.7 / 257.3 |
| Colors closed | 377.9 / 294.8 / 273.4 | 382.8 / 301.8 / 280.8 |
| Welcome closed on first tour page | 448.1 / 360.5 / 341.2 | 400.4 / 317.6 / 296.5 |
| Wallpaper picker closed | 447.4 / 361.1 / 341.4 | 393.3 / 311.1 / 290.0 |
| AI sidebar closed; whole cycle | 446.7 / 359.0 / 340.8 | 387.9 / 305.7 / 284.6 |

Whole-cycle PSS falls **53.3 MiB (14.8%)**, USS 56.2 MiB and RSS 58.8 MiB in
this matched capture. The largest useful change is avoiding four full-screen
Welcome posters before their color-demo page is visible. Four 1920×1080 32-bit
pixel buffers alone would be 31.6 MiB; that is an allocation estimate, not a
separate heap measurement. The demo retains the existing full-size preload and
transition behavior when shown. A real floating-window regression test verifies
zero preloads on welcome/help, four on colors, and zero when inactive.

This is one paired capture, not a statistical confidence interval. Small row
changes and exact totals vary with shared-page apportionment, allocator high-water
marks and renderer activity. Earlier experiments showed that object unloading
alone gave only modest static savings. Fresh memory is not substantially reduced;
the former ~555 MiB RSS observation had different warming/output conditions and
must not be presented as a direct before value for this matrix.

### Static, video and output cost

The first table continues the full panel cycle: select the second still, wait for
paired presentation readiness, compare two seconds after transition and after
six idle seconds, add/remove a second identical output, play the generated clip,
then return to the first still. Endpoints are three-second 250 ms captures.
Both video decoders reported playing, first-frame readiness, no error and audio
disabled. Returning to static destroys the decoder item; it does not force the
platform's libraries or allocation pools to leave RSS.

| Scenario | Before RSS / PSS / USS | After RSS / PSS / USS |
| --- | ---: | ---: |
| Static, one output after panel cycle | 464.0 / 378.9 / 357.4 | 398.7 / 315.9 / 294.9 |
| Second still, two seconds after transition | 463.3 / 378.3 / 356.7 | 401.9 / 320.5 / 299.5 |
| Second still, after idle trim | 458.5 / 374.6 / 353.1 | 401.9 / 320.5 / 299.5 |
| Static, two outputs | 498.9 / 412.7 / 391.1 | 443.9 / 360.2 / 339.1 |
| 1080p video, one output | 645.1 / 542.9 / 514.6 | 596.3 / 497.0 / 465.5 |
| Static again after video and idle trim | 624.3 / 522.7 / 494.3 | 563.3 / 460.5 / 432.3 |

A separate matched restart sequence warms Settings, Colors and Welcome, then
measures one video output and adds a second. This isolates the output increment;
its absolute totals should not be mixed with the preceding cumulative table.

| Video output sequence | Before RSS / PSS / USS | After RSS / PSS / USS |
| --- | ---: | ---: |
| One output | 640.4 / 542.9 / 511.6 | 572.0 / 474.7 / 443.5 |
| Two outputs | 762.8 / 663.9 / 632.5 | 687.9 / 589.1 / 557.8 |

The second video output adds 114.4 MiB PSS in the candidate sequence. Each output
still has its own decoder/rendering resources; sharing decoded video across
outputs is a larger redesign and was not introduced here. The previous-poster
state check changes from two resident slots to one after the five-second idle
period, while current/transition slots and reverse navigation remain verified.

Fresh smaps mapping attribution in this fixture put about 176 MiB PSS in anonymous
mappings, 12 MiB in mapped display/shared-memory buffers, about 20 MiB in Qt/Mesa
mappings and 8 MiB in other file-backed mappings. Anonymous memory combines QML,
C++/image and renderer allocations; smaps cannot assign it to individual objects.
Fresh static candidates had no Qt Multimedia mapping. Library registration was
a small cost compared with panel/image warming and video resources.

No shell-owned unbounded thumbnail/icon map was found; app/search arrays track
the desktop catalog and the ordinary Image/icon source sizes are bounded. Qt's
shared image cache and native high-water marks are separate from object lifetime
([Image documentation](https://doc.qt.io/qt-6/qml-qtquick-image.html)). An instrumented
forced JavaScript collection reclaimed only 2.5 MiB and paused the GUI thread for
6 ms; forcing hidden-window graphics-cache release reclaimed only 3.6 MiB in its
separate probe. Neither workaround ships. A heaptrack launch stalled before a
usable warmed capture, and the QML trace was incomplete on disconnect, so no exact
native allocation-stack attribution is claimed. PNG/H.264 budgets do not cover
large animated-GIF frame caches or HDR/4K video.

### Launcher choice and timing breakdown

The native timing boundary is a temporary IPC handler around the actual production
`NacrePanelState.openMode` call. QML Date.now measures synchronous state/component
work with 1 ms resolution; it excludes CLI startup, async image readiness, animation
completion and displayed frames. Separate component/catalog/icon log markers were
added only to copied benchmark sources. Twenty opens use identical demo catalogs;
each records 12 entries and at least 12 ready icons. Local PNG icons measure image
decode/reuse, not every installed icon-theme lookup path.

| Timing boundary | Before median / p95 (ms) | After median / p95 (ms) |
| --- | ---: | ---: |
| Synchronous shell open, 19 warmed repetitions | 54.0 / 102.0 | 4.0 / 7.0 |
| Command IPC, same repetitions | 76.0 / 123.4 | 26.4 / 33.9 |
| Acknowledged open including readiness IPC, 20 runs | 131.2 / 143.6 | 46.2 / 49.1 |

Initial synchronous construction measured 73 ms before and 59 ms after in the
native run; no first-open speedup is claimed. After the long closed interval, the
baseline rebuilt in 60 ms and the kept-warm candidate reopened in
3 ms. The 20-run acknowledged series includes one first-open outlier
(116.4 ms after); it is retained in the JSON/max, rather than silently dropped.
Version 2 uses nearest-rank p95; this corrects the old sampler's off-by-one at
integer percentile ranks such as 20 samples.
- Before creation marker: median / p95 53.0 / 89.0 ms, 20 events.
- Before catalog marker: median / p95 2.0 / 6.0 ms, 83 events.
- Before icon marker: median / p95 5.0 / 32.0 ms, 493 events.
- After creation marker: median / p95 57.0 / 57.0 ms, 1 events.
- After catalog marker: median / p95 2.0 / 6.0 ms, 52 events.
- After icon marker: median / p95 40.0 / 62.0 ms, 24 events.

Catalog work is a few milliseconds; repeated UI creation dominates the original
synchronous path. Keeping data alone warm would leave most of that cost. A
five-second launcher trial reopened quickly inside the window but paid creation
again after expiry, so it was rejected for the apps mode. The chosen policy keeps
that small view after first use, adding approximately **5.6 MiB PSS** at the closed
endpoint of this dedicated profile. This is a noisy process-level estimate, not
a per-object byte count. There is no startup preloading. Other modes release after
five seconds, and switching to/from them can still require app-view construction.
The normal search/selection/prompt reset runs on every open; backend ownership,
settings, focus visibility and palette publication are unchanged.

### Proposed budget and validation

Use PSS as the main resident-memory budget and USS as a private-memory cross-check.
For 1920×1080, scale 1, the initial **shell-process** ceilings are:

| State | PSS budget (MiB) |
| --- | ---: |
| Fresh static, one output | 250 |
| Closed panels after ordinary static use, one output | 350 |
| Static after video use, one output | 500 |
| Active 1080p H.264 video, one output | 550 |
| Active 1080p H.264 video, two outputs | 650 |

Allow about 50 MiB additional static PSS per equivalent output and reserve up to
10 MiB per warmed apps view. In this lab shell descendants add about 45 MiB PSS;
account for helpers separately, and do not call the aggregate a whole-desktop
budget. These are proposed regression ceilings, not portable hard limits or GPU
budgets. Higher pixel counts, codec reference frames, scale factors, real hardware
and user catalog/history sizes need separate calibration. Track post-use plateaus
and repeated growth as well as peaks; unloading need not return native pools
immediately ([Qt memory management](https://doc.qt.io/qt-6/qtqml-javascript-memory.html)).

The complete source/native gate covers Python lint/format, QML/JS parsing/format,
shell/Lua/compositor integrity, unit suites and real Qt/Quickshell lifecycle,
state-restoration, preload, muted-video and transition tests. CI uses the pinned
engine. The benchmark source hash evidence and distributions are summarized in
[performance-memory-results.json](performance-memory-results.json). Performance
work changes no visible strings, geometry, AI permissions or live user settings.


## Persistent Orient RPC

The native SDK retains one `orient --rpc --serve` child per helper process,
serializes concurrent requests, verifies the first reply's engine pin, and checks
every reply ID. Timeouts/cancellation discard and reap the child; a subsequent
call starts a fresh child. Idle Orient blocks on stdin and exits on EOF.

Warm medians of 31 runs on Linux x86_64 (dev5 one-shot versus dev6 persistent):

| Operation | Before (ms) | After (ms) | RPC calls |
| --- | ---: | ---: | ---: |
| `image_colors`, aurora.jpg | 107.22 | 2.01 | 68 |
| `image_colors`, coast-day.png | 71.73 | 0.93 | 32 |
| `image_colors`, warm-room.png | 15.26 | 0.23 | 8 |
| `lch`, per call | 1.42 | 0.031 | 1 |
| Full isolated palette apply | 89.86 | 62.35 | 5 |

Measurements include Python/pipe overhead and were taken while other development
work was running. Preview results and generated apply files match exactly after
normalizing temporary home/source paths and engine provenance. Apply uses `live=False` and a fixed
empty monitor response; it includes branding and configuration writes, but not
compositor reloads. The remaining apply time is largely outside Orient RPC.
Child startup is paid on the first call rather than on each call.

Run the same harness in each revision with its matching native binary and SDK:

```sh
NACRE_ORIENT_BINARY=/path/to/matching/orient python tools/benchmark_orient_rpc.py \
  --output /tmp/orient-rpc.json
```


## Rust migration wave 0 (2026-10-13)

Orient dev6 exposes `orient-core` and `orient-decoder` to native consumers. The
55 frozen normalized frames and all 160 gallery/codec palette cases match Pillow,
including JPEG. The tested dependency pin, SDK descriptor and reference palette
provenance agree; no color roles or owner state changed. Nacre's Python helpers
are still present at this wave. See [migration status](rust-migration.md).

### Busy-machine evidence

Only helper CPU time, launches and helper RSS are evidence in this window.
The existing four-request workloads use disposable HOME/XDG state and 100 warm
requests plus one cold request. QML/helper source is held at `0ecb3bb7`; the
candidate applies the dev6 C8 pin, holding unrelated cleanup/SDK changes constant.
`benchmark-helpers.py --busy-machine` suppresses wall-clock and PSS results.
For the migrated runtime, pass `--helper-binary target/release/nacre-helper`
to this tool, with or without `--service`. The same four requests and CPU/RSS
methods are used. Without that override, an existing Python script selects the
baseline; after deletion, the installed binary or source release build is used.
Launch counts describe direct helper starts; descendant/session Python launches
still require the separate idle/actions capture. `benchmark-services.py` accepts
the same binary override, copies it into its disposable installed tree, and
requires migrated QML command sources. These transport changes are fixture-smoke
verified; no new Wave 1 performance comparison is claimed before its full cutover.
Fresh CPU uses Linux child `getrusage`; service CPU uses `/proc/PID/stat` user,
system and reaped-child ticks. Both exclude driver CPU and waiting time.
Service resolution is 10 ms per workload, or 0.1 ms/request for 100 requests;
a zero tick difference means below that resolution, not zero CPU consumption.

| Helper CPU user + system, mean per warm request (ms); busy machine | Fresh before | Fresh after | Service before | Service after |
| --- | ---: | ---: | ---: | ---: |
| Menu conversion validation | 73.98 | 67.95 | 0.10 | 0.10 |
| Dashboard read | 45.75 | 54.98 | 0.10 | 0.10 |
| Launcher preferences load | 39.59 | 38.26 | <0.10 | <0.10 |
| Notification history load | 19.58 | 19.77 | 0.10 | 0.20 |

| Fixed helper workload; busy machine | Before | After |
| --- | ---: | ---: |
| Fresh helper Python launches (404 requests) | 404 | 404 |
| Shared service Python launches (same requests) | 1 | 1 |
| Service RSS after all warm requests (MiB) | 26.99 | 27.01 |

These helpers are unchanged, so this pair establishes a baseline, not a claimed
wave 0 speed or memory improvement. CPU measurements remain sensitive to clock
frequency, caches and scheduling; RSS counts shared pages. Full public summaries
are in [rust-migration-results.json](rust-migration-results.json).

### Deferred quiet-window measurements

The initial five-minute nested Hyprland/labwc captures and ten action repetitions
were affected by background cache warming and paging. Final shell SwapPss was
73.5 MiB before and 96.2 MiB after. Lower shell RSS/PSS is not a memory saving;
slower action acknowledgements are not attributed to migration code. A focused
warmed repeat timed out on dashboard readiness in the unchanged baseline. These
captures are retained privately for diagnosis and discarded as performance evidence.
Whole-shell wall-clock latency, RSS/PSS and idle CPU conclusions are deferred.

The owner will announce a quiet window. Measure baseline/current in A/B/A/B order,
check `vmstat` si/so and process SwapPss before and throughout every run, and discard
any run that pages. Only a regression reproduced in that window triggers the stop
rule. All parity, test and CI gates remain mandatory while ports continue.
A quiet-window comparison is required before wave 3 (wallpaper/colors) lands;
without an announced window, stop there and ask for one.

The dashboard-owner harness now uses a private runtime directory, full XDG state,
and a private D-Bus session. It explicitly enables its `qml.debug` result marker;
inherited debug suppression previously hid successful runs. These are test-harness
changes and do not change the UI or readiness boundary.


## Phase D: current shell memory and complete-frame guard (2026-10-13)

The 15 commits on `perf/shell-memory-budget` were already squash-integrated as
`22bb7dcc` (#10). The closed-panel release, lazy multimedia, measured apps warm
policy, visible-only tour posters and proportional-memory sampler are present in
current main; replaying those commits would duplicate the work. This pass keeps
those policies and measures the complete production shell on both backends.

### Reproduction and measurement boundary

`tools/performance-session.py` starts a headless labwc parent and a nested
Hyprland or Niri compositor, with a disposable HOME, XDG directories, private
D-Bus, output and shell source copy. It never calls the live desktop. The memory
sampler is `tools/performance.py`; every scenario retains RSS/PSS/USS distributions
and PSS coverage. Shell descendants are recorded separately from the shell
process; the tables below budget the shell process alone.

Fixtures are one 1920×1080 output at scale 1, a generated 3840×2160 static PNG,
a local 1920×1080 H.264 clip at 24 fps, 48 generated application entries with
128×128 icons and 12 favorites, plus the distribution's installed applications.
The full cycle visits the launcher, five dashboard tabs, settings, colors,
welcome, wallpaper picker and AI panel, closing each and allowing six seconds
for the existing five-second release grace period. Static idle is then sampled
for 30 seconds; other endpoints use five seconds at 250 ms intervals.

The copied shell receives F8/F9 from a virtual keyboard. The clock starts in the
Qt key handler and ends at `frameSwapped` after panel geometry, opening animation,
page Loader, data queue and images settle. A 16 ms probe timer requests frames
while waiting. This includes probe overhead and uses a 1 ms clock; it excludes
physical keyboard delivery, the real shortcut's CLI/IPC dispatch and compositor presentation, so it is **not an
input-to-photon measurement**. Synchronous handler time is reported separately.
Hyprland's fixture disables animations; Niri retains them. Compare before/after
within each backend, rather than treating backend frame medians as equivalent.

Run with the pinned native engine and installed performance-session prerequisites:

```sh
PATH="$PWD/.venv/bin:$PATH" python tools/performance-session.py \
  --backend hyprland --helper-binary "$PWD/target/release/nacre-helper" \
  --quick --check-budgets --output /tmp/shell-hyprland.json
PATH="$PWD/.venv/bin:$PATH" python tools/performance-session.py \
  --backend niri --helper-binary "$PWD/target/release/nacre-helper" \
  --quick --check-budgets --output /tmp/shell-niri.json
```

Use `--revision COMMIT` for an immutable source comparison on the same harness
and host. Supply the matching prebuilt helper and pinned engine for that revision.
For concurrent development, `--control` disables only
this pass in the copied current source: hidden-decoder release, deferred refresh,
asynchronous dashboard construction. The rest of
current main, including unrelated UI changes, remains identical. Extra snapshot
properties/methods remain in both copies, so this is a conservative behavioral
control rather than a byte-for-byte reconstruction of an old shell. `--quick` skips individual panel endpoint samples,
while still visiting every panel. Omit it to retain those intermediate samples.
A matching native executable can be supplied with `--orient-binary PATH`.

### Attribution and chosen changes

Anonymous native mappings dominate this fixture. They combine QML/JavaScript,
C++ objects, decoded images, font raster caches, codec allocations and renderer
pools. File-backed Qt/QML or font mappings measure resident library/font pages,
not all engine or text memory. Shared-memory mappings identify some display
buffers; unmapped GPU allocations are outside PSS. The summary JSON retains the
mapping categories rather than inventing an exact per-component heap breakdown.

Closed panels already unload their large content. Wallpaper thumbnails, album
covers and media cards already have bounded `sourceSize` and Qt's shared image
cache for equivalent source/size requests; different decode sizes can still use
separate entries. Adding a separate cache would duplicate ownership. A live hero-size trial
caused additional decodes while geometry changed and was rejected. A fixed
1440×960 hero trial also increased the native 16:9 decode from 1440×810 to
1707×960 with `PreserveAspectCrop`, so the existing width-only cap is retained.
Qt documents both [shared image caching and decode sizing](https://doc.qt.io/qt-6/qml-qtquick-image.html).

The new decoder release applies after five seconds of fullscreen coverage,
lock or sleep, captures playback position, and lazily restores it on return.
Manual pause and ordinary tiled coverage retain the frozen frame, because the
wallpaper can remain visible through gaps. Native tests cover release, a brief
cover, partial coverage, picker visibility, muted playback and seek restoration.
Static selection continues to destroy the video component.

An experimental child-only 128 KiB glibc mmap threshold was tested to make large
buffers independently releasable. The same-source software comparison did not
establish a memory saving and post-video PSS was higher, so the default is removed.
Explicit allocator controls remain available in the isolated harness for future
GPU experiments; no allocator policy is changed in the user's session. See
[glibc allocation tunables](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html).

Dashboard pages now incubate asynchronously; dashboard data refresh and
coalesced launcher refresh run on the next event-loop turn. Geometry, opening
animation, reset behavior and data providers are unchanged. Python helper
migration is outside this pass.

Qt already selects hardware decode backends according to availability. A
standalone VA-API decode succeeded on the measurement host, but that does not
prove every Qt clip uses VA-API. No backend is forced, no codec protection is
bypassed, and no source video is transcoded. Qt's QML MediaPlayer does not expose
a portable output-resolution decode control. A low-latency playback-intent trial
increased active PSS from 311.2 to 316.7 MiB, so it was rejected. Qt's backend
switches are [private configuration](https://doc.qt.io/qt-6/advanced-ffmpeg-configuration.html),
not a portable shell memory policy.

### Budget tiers and CI

`tools/performance-budgets.json` separates one-output reference ceilings from
generous guard ceilings. Reference values guide GPU development; the software
CI tier is calibrated separately. The guard uses peak
PSS, not a median that could hide a spike.

| Shell state | GPU reference PSS (MiB) | GPU lab maximum (MiB) | Software CI maximum (MiB) |
| --- | ---: | ---: | ---: |
| Fresh static | 250 | 450 | 1200 |
| Closed after all panels / settled static | 350 | 650 | 1700 |
| Active / manually paused / resumed 1080p video | 550 | 900 | 2000 |
| Fully hidden video / static after video | 500 | 850 | 1900 |

The existing two-output video reference remains 650 MiB, with roughly 50 MiB
additional static allowance per equivalent output and up to 10 MiB per warmed
apps view. Multi-output, high-DPI, GIF, HDR and higher-resolution video need
separate calibration; these are not GPU limits.

The advisory nightly/manual **Headless shell performance** workflow runs the full
production shell in nested Niri on a pixman headless parent, exercises the full
panel/video cycle and retains JSON artifacts. Mesa OpenGL software rendering is explicit;
this profile is not comparable with the GPU lab reference. A calibration capture
used 792 MiB fresh and 1238 MiB after video, so its generous regression ceilings
are a separate `--budget-tier software`, rather than silently changing the GPU
budget. Tests verify that selecting this tier does not relax the default guard.

Current Hyprland's nested backend requires DMA-BUF/render-node support absent
from ordinary hosted runners. A pixman parent starts, but its Hyprland child
fails before the shell. A tested VKMS device provides a primary KMS node, not
the render node required by wlroots GLES2. The portable CI guard therefore uses
Niri; GPU Hyprland captures remain separate lab validation. No kernel-module
installation or privileged graphics setup is required by the final CI job.
Cold/reopen complete-frame p95 ceilings are 2000/1200 ms; synchronous-handler
p95 ceilings are 500/300 ms. Static idle CPU must average at most 5% of one core.
Video must deliver at least 10 fps during active and resumed sampling so a stalled
or missing player cannot pass merely by using little memory. Missing PSS coverage,
missing key/frame samples, memory peaks, latency tails, shell SwapPss and host
swap traffic fail the guard; focused
negative-control tests verify those failure paths. A manual workflow run defaults to the same-source control; an optional ancestor
commit instead captures that revision. Both capture a sequential before/after
pair on the same fresh runner and pinned runtime. This avoids using
a busy developer desktop as conclusive latency or resident-memory evidence.

CI uses shorter 15-second idle
and five-reopen captures to detect large regressions, rather than certify small
performance differences.


### First same-source software comparison

The [first paired workflow](https://github.com/nacre-desktop/nacre/actions/runs/38453798008)
used `f29bd4bd` on both sides, with the control described above. Versions were
Quickshell 0.3.2, Qt 6.12.0, Niri 26.04 and the dev6 native engine. Both runs had
complete shell PSS/SwapPss coverage, zero sampled SwapPss and unchanged host swap
counters. These software-rendered values are not GPU desktop budgets. This trial
still included the experimental allocator default, which has since been removed.

| Shell scenario | Control PSS / USS (MiB) | Trial PSS / USS (MiB) |
| --- | ---: | ---: |
| Fresh static | 842.7 / 790.3 | 844.1 / 790.7 |
| After every panel, closed | 1116.3 / 1049.2 | 1129.7 / 1061.6 |
| Settled static after open profile | 1123.6 / 1056.2 | 1134.1 / 1065.8 |
| Active 1080p video | 1205.9 / 1133.6 | 1219.3 / 1146.0 |
| Fully hidden video | 1205.2 / 1132.8 | 1207.2 / 1133.9 |
| Static after video | 1200.1 / 1127.7 | 1238.8 / 1165.5 |

| Complete-frame/open-path metric | Control | Trial |
| --- | ---: | ---: |
| Cold launcher complete frame (ms; one sample) | 631 | 633 |
| Warm launcher frame median / p95 (ms; five samples) | 431 / 494 | 402 / 546 |
| Warm launcher synchronous handler median (ms) | 6 | 6 |
| Cold dashboard complete frame (ms; one sample) | 619 | 554 |
| Warm dashboard frame median / p95 (ms; five samples) | 425 / 468 | 386 / 402 |
| Warm dashboard synchronous handler median (ms) | 42 | 4 |
| Settled shell CPU mean (% of one core; 15 s) | 0.0 | 0.0 |

Zero sampled CPU means below Linux tick resolution in this interval, rather than
proof of no work. Launcher median and tail moved in opposite directions and its
handler did not improve, so no reliable launcher speedup is claimed. Dashboard
frame median improved by 39 ms (9.2%) and its synchronous handler by 38 ms; this
supports moving construction and refresh out of the synchronous open handler.
The short series is a regression fixture, not a statistical guarantee on hardware.

This trial shows **no overall software memory saving**: fresh memory is unchanged
and post-video median PSS is 38.7 MiB higher. The allocator default is removed;
confirmation results follow separately. Decoder unloading is validated as a
lifecycle behavior, with no GPU memory saving claimed from this software pair.
A larger video benefit, startup footprint and full Hyprland/Niri GPU A/B/A/B
comparison remain deferred until the owner announces a quiet window. Busy-host
captures with swapping are retained privately for diagnosis, not counted as
performance evidence. The original ~555 MiB RSS and ~105/115 ms action figures
use different warming/output and acknowledgement boundaries and are not this
pair's baseline.

The public [numeric summary](performance-session-results.json) retains distributions,
versions, hashes and coverage; it omits process arguments and private fixture
paths. No live shell, account or machine setup was changed.


### Confirmation with the allocator default restored

The [confirmation workflow](https://github.com/nacre-desktop/nacre/actions/runs/38455925163)
uses the same-source control at `301a0477`; both copies retain glibc's ordinary
adaptive allocator. The native engine and graphics versions match the first
pair. Both captures again have zero sampled shell SwapPss and no host swap
traffic. The normal [regression guard](https://github.com/nacre-desktop/nacre/actions/runs/38455923539)
also passes on that revision.

| Shell scenario | Control PSS / USS (MiB) | Final PSS / USS (MiB) |
| --- | ---: | ---: |
| Fresh static | 830.5 / 774.2 | 828.2 / 775.0 |
| After every panel, closed | 1131.8 / 1060.8 | 1133.9 / 1066.2 |
| Settled static | 1128.4 / 1057.1 | 1123.3 / 1055.3 |
| Active 1080p video | 1218.7 / 1146.5 | 1205.8 / 1132.9 |
| Manually paused video | 1217.1 / 1144.9 | 1198.4 / 1125.4 |
| Fully hidden video | 1218.9 / 1146.7 | 1194.7 / 1121.8 |
| Resumed video | 1201.8 / 1129.6 | 1242.3 / 1169.4 |
| Static after video | 1197.1 / 1125.0 | 1226.1 / 1153.2 |

| Complete-frame/open-path metric | Control | Final |
| --- | ---: | ---: |
| Cold launcher complete frame (ms; one sample) | 637 | 517 |
| Warm launcher frame median / p95 (ms; five samples) | 387 / 535 | 395 / 552 |
| Warm launcher synchronous handler median (ms) | 8 | 2 |
| Cold dashboard complete frame (ms; one sample) | 635 | 515 |
| Warm dashboard frame median / p95 (ms; five samples) | 434 / 532 | 382 / 395 |
| Warm dashboard synchronous handler median (ms) | 36 | 3 |
| Settled shell CPU mean (% of one core; 15 s) | 0.0 | 0.0 |

Dashboard frame median improves by **52 ms (12.0%)**, while its synchronous
handler improves by **33 ms**. Both pairs support this direction; their short
sample counts still limit precision. Warm launcher frame median/tail are slightly
higher, so the cold single-sample improvement is not treated as a reliable overall
launcher speedup. Coalescing refresh reduces duplicate open-path work, with no
large launcher frame benefit claimed.

There is **no overall software memory reduction**. Fresh/all-panel endpoints are
similar; active video is 12.9 MiB lower and hidden video 24.2 MiB lower in this pair,
but post-video PSS is **29.0 MiB higher**. Paused-to-hidden unloading itself returns
only about 3.7 MiB in the optimized software sequence. Anonymous native/graphics
pools dominate, and component destruction need not promptly return their pages.
These state differences are observed process endpoints, not exact per-object byte
counts or proof of a hardware saving. The hidden decoder lifecycle is tested;
its GPU memory benefit and post-resume tradeoff remain for the announced quiet
window. No idle CPU reduction is claimed from the tick-quantized zeroes.

The committed changes buy a shorter dashboard open handler and complete-frame
latency, deferred/coalesced refresh, and a decoder lifetime limited to visibility
with position restoration. They do not establish a smaller startup shell, a
faster warm launcher, or a broad video-memory saving. The earlier merged panel
and visible-tour-poster work remains in place; its historical measurements are
in Phase C. The allocator and hero decode trials are rejected rather than shipped.

### Wave 1 native helpers: busy-machine evidence (2026-10-14)

The same `benchmark-helpers.py` requests used 100 warm requests per helper,
with one discarded cold request each, in disposable HOME/XDG roots and absent
private session/system bus endpoints. Baseline uses the pre-cutover Python
helpers; current uses the release `nacre-helper`. These are busy-machine CPU
and helper RSS observations, not shell latency or aggregate-memory evidence.

| Helper user + system CPU per fresh call (ms); busy machine | Python before | Rust after |
| --- | ---: | ---: |
| menu-backend invalid conversion | 55.669 | 1.948 |
| dashboard-data read | 38.502 | 1.157 |
| launcher-preferences load | 35.550 | 1.109 |
| notification-history load | 16.881 | 0.891 |

| Same 404 requests; busy machine | Before | After |
| --- | ---: | ---: |
| Fresh helper starts | 404 | 404 |
| Fresh Python starts | 404 | 0 |
| Persistent helper starts | 1 | 1 |
| Persistent Python starts | 1 | 0 |
| Settled helper RSS (MiB) | 26.953 | 10.211 |

Persistent-service CPU remained below the 10 ms Linux tick resolution for
menu, dashboard and notification batches; launcher measured 0.100 ms/request
in both builds. A reported zero means below measurement resolution. Launch
counts cover direct helpers only; full-session descendants and idle launches
remain unmeasured. The native helper RSS observation is 16.742 MiB lower (62%).

Launcher/dashboard/wallpaper/palette wall times, aggregate RSS/PSS, idle CPU,
and Python launches per minute await the owner's quiet window. No quiet window
has been announced; require interleaved A/B/A/B with zero vmstat si/so and
SwapPss before and during each run, discarding any run that pages. This gate
remains mandatory before Wave 3 lands.

Reproduce the load-resistant measurements with developer-only Python:

```sh
python tools/benchmark-helpers.py --tools BASELINE/nacre/shell-tools --count 100 --busy-machine --output before-fresh.json
python tools/benchmark-helpers.py --helper-binary target/release/nacre-helper --count 100 --busy-machine --output after-fresh.json
# Repeat both commands with --service for the persistent helper.
```

### Wave 2 pearl-state helper (busy machine)

The fresh-process method from `benchmark-helpers.py` measured 100 calls after
one warm-up per operation, using child user + system CPU and disposable HOME.
Both versions used identical request data; Rust used the release build.

| Per-call helper CPU (ms); busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| pearl-state load | 88.630 | 2.820 |
| pearl-state save | 92.912 | 3.847 |
| Direct Python starts, 200 measured calls | 200 | 0 |

These are loaded-machine observations, not shell latency evidence. A preceding
run recorded 32.74/32.79 ms Python CPU, showing substantial machine variability;
quiet-window A/B/A/B remains required. Helper RSS, shell action timings, aggregate
RSS/PSS, idle CPU and Python launches per minute are unmeasured for this helper.

### Wave 2 compositor boundary (busy machine)

The existing `benchmark-helpers.py` child CPU accounting and `performance.py`
memory sampler measured identical `CloseWindow` requests on a disposable Niri
socket. No real compositor or shell session was contacted.

| Fresh call; 100 requests, busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| Helper user + system CPU, mean (ms) | 46.070 | 1.350 |
| Direct Python starts | 100 | 0 |
| IPC connections | 100 | 100 |

The persistent Rust action worker used one connection for 1,001 requests
(one warm-up plus 1,000 measured), zero Python starts, and 7.90 MiB helper RSS.
Its CPU increment was below the `/proc` 10 ms tick resolution: under 0.01 ms
per measured action at that resolution, not proof of zero CPU. A preceding
fresh run recorded 45.265/1.285 ms CPU; loaded-machine variability remains.
Whole-shell action timings, aggregate RSS/PSS, idle CPU and launches per minute
remain deferred to the owner's interleaved quiet-window comparison.

### Wave 2 window-hide helper (busy machine)

The current Python helper and Rust release used the same native compositor
backend and disposable Hyprland socket. Each made 100 quiet hide requests after
one warm-up. CPU uses `benchmark-helpers.py` child accounting, including reaped
descendants; RSS uses `performance.py` at the first blocked private query after
execution, one sample per helper PID, excluding descendants.

| Helper measurement; busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| User + system CPU per fresh call (ms) | 46.256 | 2.186 |
| Direct Python starts in 100 measured calls | 100 | 0 |
| Helper RSS at private query (MiB) | 17.33 | 7.27 |

These are helper measurements, not shell action latency or aggregate memory.
Short-lived process peak RSS was discarded because it included memory inherited
from the benchmark launcher before execution. Quiet-window comparisons remain
pending; raw accepted values are in `rust-migration-results.json`.

### Wave 2 Niri width observer (busy machine)

The existing child CPU accounting measured 100 fresh `--check-session` calls in
the same unsupported disposable session after one warm-up. The running observer
RSS sampler used authenticated private Xvfb and a private Niri socket, after its
first constraint reconciliation; one helper PID sample each, excluding children.

| Helper measurement; busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| User + system CPU per fresh check (ms) | 41.357 | 1.075 |
| Direct Python starts in 100 measured checks | 100 | 0 |
| Running observer helper RSS (MiB) | 19.27 | 8.23 |

The native observer blocks on X11/Niri/shutdown events and retains the existing
one-shot 150 ms opening correction. Two new windows used one persistent action
connection in the private runtime check. These results do not measure shell
latency, aggregate memory or idle CPU; the quiet-window comparison remains pending.

### Wave 2 prepared-session gate (busy machine)

The existing child CPU accounting measured 100 fresh `shell` gate checks in an
unknown disposable session after one warm-up. Both implementations denied the
check with exit 1 and unchanged files.

| Helper measurement; busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| User + system CPU per fresh check (ms) | 31.178 | 0.984 |
| Direct Python starts in 100 measured checks | 100 | 0 |

No reliable fresh-helper RSS sample was collected for this short-lived command;
no aggregate memory, shell latency or idle CPU change is claimed.

### Wave 2 prepared-session runtime (busy machine)

The existing `benchmark-helpers.py` child CPU method measured 100 fresh,
identical `--help` requests with a disposable HOME and absent D-Bus/display
connections. This measures argument parsing, not compositor or session startup.

| Helper measurement; busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| User + system CPU per fresh request (ms) | 39.605 | 0.956 |
| Direct Python starts in 100 measured requests | 100 | 0 |

The runtime replaces itself when launching the compositor or watcher, so it has
no separate resident helper. No fresh peak RSS, whole-shell action timing,
aggregate RSS/PSS, idle CPU or launches-per-minute improvement is claimed.
The quiet-window A/B/A/B gate remains required before Wave 3 lands.

### Wave 2 acceptance recorder (busy machine)

Child CPU accounting measured 100 fresh identical `--help` requests. A separate
single RSS sample came from each observer after its first record write, waiting
for an absent private bus with external tools removed from PATH. Both samples
followed exec and exclude child processes; they are helper measurements only.

| Helper measurement; busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| User + system CPU per fresh parser request (ms) | 121.754 | 2.262 |
| Direct Python starts in 100 fresh requests | 100 | 0 |
| Observer RSS while waiting for absent bus (MiB; one sample) | 20.77 | 10.34 |

Managed-session latency, whole-shell RSS/PSS, idle CPU and launches per minute
remain unmeasured. The earlier quiet-window gate remains mandatory.

### Wave 2 isolated preview (busy machine)

The same child CPU method measured 100 fresh identical `--help` requests.
Preview-parent RSS was sampled once per implementation after a controlled child
signalled readiness, excluding that child. These are parser and helper-memory
measurements, not nested compositor or full-shell startup timings.

| Helper measurement; busy machine | Python reference | Rust release |
| --- | ---: | ---: |
| User + system CPU per fresh parser request (ms) | 52.584 | 1.261 |
| Direct Python starts in 100 fresh requests | 100 | 0 |
| Preview-parent RSS while waiting for child (MiB; one sample) | 21.54 | 7.30 |

Actual Bubblewrap checks confined writes to the copied state and kept the
original fixture read-only. A real nested Niri preview answered IPC, quit cleanly
and removed its private runtime. Its system bus is explicitly private because
native hardware helpers no longer pass through the blocked CLI wrappers.
Whole-shell/idle comparisons remain pending the owner's quiet window.

### Desktop-PC quiet-window attempt (2026-10-14)

The desktop PC used a Ryzen 5 7600 (12 threads), 14.5 GiB RAM, CachyOS
Linux 7.2.9-2-cachyos and the unchanged powersave governor. Quickshell 0.3.2,
Niri 26.04 and Hyprland 0.56.2 were installed. The capture used a private
headless labwc session, HOME/XDG and D-Bus; neither live desktop was changed.
All release builds completed before measurements. These are desktop-PC numbers.

One helper pair passed unchanged host swap counters; both service SwapPss values
were zero. It is useful isolated evidence, but is not the required repeated gate.

| Fresh helper median (ms) | Python baseline | Rust native |
| --- | ---: | ---: |
| Menu conversion validation | 45.861 | 2.443 |
| Dashboard read | 30.908 | 5.976 |
| Launcher preferences load | 27.392 | 1.478 |
| Notification history load | 13.975 | 1.292 |
| Service RSS after workload (MiB) | 28.41 | 12.80 |
| Service PSS after workload (MiB) | 17.04 | 10.41 |

The full A/B/A/B gate did not complete: the native full capture paged in 126
host pages; after warming, the baseline paged in 33. Every measured descendant
remained at SwapPss zero and no swap-out occurred, but the owner requires zero
host swap activity, so both captures are discarded. No paired shell action,
RSS/PSS, idle CPU or Python-launch improvement is claimed. PATH launch logging
also excludes absolute interpreter paths. Whole-shell revisions include
intervening UI changes, which further limits attribution. Baseline `cebc24ce`
and native `a36bbe82` and helper raw results are recorded in
[rust-migration-results.json](rust-migration-results.json). Wave 3 remains gated
on a new valid quiet comparison; no governor, swap or live-session change was made.

The owner subsequently revised this gate: accept a capture when measured
processes have SwapPss zero and host swap-in is under 1 MiB, recording counters
and page size. The earlier 126-page (504 KiB at 4096 bytes/page) and 33-page
(132 KiB) captures fit that allowance; their original rejection remains
historical. They still do not form a complete A/B/A/B sequence. Wave 3 ports may
proceed; repeat the interleaved comparison at the next announced quiet window
and include it before Wave 3's final status. Correctness gates remain mandatory.

### Wave 3 KDE file publisher (busy machine)

One hundred fresh publish calls used the frozen Python implementation and the
Rust release helper with identical palette inputs in disposable homes. Child
user + system CPU excludes the benchmark driver. CPU per call was 25.387 ms
for Python and 1.254 ms for Rust; direct Python starts were 100 and zero.
These are helper numbers. The remaining Python palette owner temporarily calls
the native publisher as a child; the native palette port will call its Rust
module directly. No wallpaper/palette action improvement is claimed yet.

Seventeen frozen KDE cases cover files, modes, errors and legacy CLI output;
the existing read/save corpus remains unchanged. The internal `--publish` JSON
transport replaces the former Python function call. Custom absolute XDG config
roots follow the shared Paths rule (intended difference); unexpected file read
errors are surfaced instead of ConfigParser's silent read failure.

### Wave 3 terminal branding (busy machine)

One hundred fresh plain-text logo calls used identical private HOME/input files
with Python absent from the native command's PATH. Child user + system CPU
was 29.016 ms per Python call and 1.071 ms per Rust call; direct Python starts
were 100 and zero. This measures the logo helper, not desktop action latency.

Forty-two frozen function cases and two real Bash/private-PTY cases preserve
Kitty packet bytes, registration fields/modes, prepared files and cleanup.
Absolute XDG cache roots are an intended difference. Refresh also uses O_NOCTTY;
failed registration/pruned-record errors now reach stderr rather than disappearing.
The internal `--refresh` command replaces the former Python module call.

### Wave 3 login appearance (busy machine)

The frozen 37×23 RGB input measured fresh child user + system CPU, excluding
the driver. One hundred cache-hit calls measured 50.742→0.955 ms per call; ten
independent cache misses measured 56.437→1.295 ms. Direct Python starts were
100→0 and 10→0 respectively. These tiny-image helper figures are not 4K or
whole-desktop action timings. The existing login-image-v1 cache stamps remain
readable and reuse existing files without decoding.

Thirteen image vectors match Pillow PNG bytes exactly, including ICC,
transparency, rotated JPEG and offset transparent GIF. The native publication
checks retain private/staged-home boundaries and existing file modes. Absolute
XDG roots and journaled image/publication failures are intended differences.
The generic Orient image-preview API changes the decoder build fingerprint;
its core palette output and prior frozen image/artwork outputs stay unchanged.

### Wave 3 demo wallpapers (busy machine)

One hundred fresh calls revalidating the already cached four-image collection
measured child user + system CPU of 59.058→10.385 ms per call. Direct Python
starts were 100→0. This retains full collection checks on ordinary calls; the
existing stat-receipt fast path remains available to the picker: 100 fresh fast
calls measured 43.490→0.913 ms CPU, with Python starts 100→0. These helper
numbers are not whole-desktop action timings.

Fourteen frozen cases preserve licensed collection metadata, cached image
bytes, file modes, source/destination stat receipts and damaged/missing-file
repair. The bundle remains offline and the private wallpaper library is untouched.
Absolute XDG cache roots and surfaced unexpected receipt I/O errors are intended
differences. Existing cache identities and receipts remain readable.

### Wave 3 native palette pipeline (busy machine, partial helper evidence)

The same 64 × 40 PNG and request produced identical palette colors before and
after. One hundred fresh cache-warm previews were interleaved Python/Rust;
`benchmark-helpers.py` child user + system CPU excludes driver CPU and waiting.
`performance.py` sampled only the persistent Colors helper and its descendants,
after one preview. These are laptop busy-machine numbers, not desktop-PC results.

| Measure | Python baseline | Native candidate |
| --- | ---: | ---: |
| Fresh cached preview CPU, ms/call | 65.821 | 9.984 |
| Colors helper family RSS, MiB | 34.016 | 16.848 |
| Colors helper family processes | 2 | 1 |
| Python execs in a separately traced preview | 1 | 0 |
| Orient subprocess execs in that preview | 1 | 0 |

The integrated baseline is e910ffd7, keeping concurrent Settings changes constant,
with its exact Orient pin from CI 38654603463; the candidate
uses the pinned Orient libraries in-process. RSS includes the baseline's resident
Orient worker. The native trace contains only `nacre-helper`. Binary/input hashes
and raw results are in `rust-migration-results.json`. The earlier c1417afe
comparison remains there separately; the table uses the integrated baseline.

Reproduce with `tools/benchmark-palette.py --baseline BASELINE_CHECKOUT
--baseline-engine BASELINE_ORIENT --native NATIVE_HELPER --image INPUT_PNG
--repeats 100 --output REPORT_JSON`. The script checks the engine pin and color
parity before timing, uses disposable HOME/XDG roots and stops its workers.
The missing trace tool was extracted privately; no system package was installed.

Whole-desktop wallpaper/palette latency, the last visible consumer's frame,
launcher/dashboard action timings, idle CPU and whole-shell RSS/PSS remain
unmeasured for this unit. They require the announced quiet window and complete
A/B/A/B runs under the revised swap rule. API readiness or files written do not
prove that shell, GTK/Qt, Kitty and the lock screen have displayed the colors.

Concurrent main added automatic wallpaper mode and a border-gradient preference.
Six separate original-main gradient references preserve that behavior. Existing
frozen outputs are unchanged; only the documented additive `paletteAutoMode:false`
picker default is compatible with the older Colors snapshot. Every other byte,
file mode, link and cache entry remains checked.

### Wave 3 wallpaper watcher (busy machine)

The watcher keeps its recursive inotify subscriptions and 500 ms debounce.
Twenty interleaved fresh starts included one observed change, then shutdown;
CPU is child user + system time, excluding waits and the benchmark driver.
Each persistent helper was sampled for a separate 30-second idle interval.

| Measure | Python | Native |
| --- | ---: | ---: |
| Startup + one change CPU, ms/call | 31.359 | 3.129 |
| Idle helper RSS, MiB | 16.750 | 10.195 |
| Idle user + system CPU in 30 seconds, ms | 0 | 0 |
| Python execs in a separate traced start | 1 | 0 |

Neither version accumulated an idle CPU tick at the 10 ms accounting resolution;
this is a memory saving, not evidence of lower steady idle CPU. One process
replaces one process. RSS fell by 6.555 MiB in this small private library.
These are laptop busy-machine helper numbers, not desktop-PC or whole-shell data.
The repeatable `tools/benchmark-wallpaper-watch.py` uses the existing helper CPU
and process-memory readers; raw results and binary/source hashes are tracked.

Frozen cases preserve thirteen CLI outputs/errors, six module calls, fifteen serial
event steps, directory/file bytes and modes, symlinks and SIGTERM exit status.
Absolute XDG roots follow QML; unexpected watch/scan failures are surfaced rather
than ignored. QML explicitly watches the native helper as the engine dependency.

## Wave 3 media server, busy-machine checkpoint

Laptop busy-machine helper-only samples; no desktop-PC or whole-shell claim.
`tools/benchmark-wallpaper-media.py` uses the existing helper child user+system
CPU accounting and `performance.py` RSS/process-tick readers. Both sides have
explicit private HOME, all four XDG roots, D-Bus and absent compositor sockets;
Fontconfig environment overrides are cleared. Builds finish before sampling.
The baseline is the unchanged Python media source from 85652512 (SHA-256
`74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2`).

| Measurement | Python coordinator | Native server |
| --- | ---: | ---: |
| Startup + preferences CPU, 20 interleaved calls | 86.21 ms | 11.58 ms |
| Resident RSS after preferences | 25.84 MiB | 16.73 MiB |
| Resident RSS after one real wallpaper publication | 26.18 MiB | 27.35 MiB |
| Idle CPU after preferences, 30 s / 100 Hz | 0 observed ticks | 0 observed ticks |
| Idle CPU after publication, 30 s / 100 Hz | 0 observed ticks | 1 tick (0.033%) |
| Resident media processes / Python processes | 1 / 1 | 1 / 0 |

The native server is 1.17 MiB larger after publication in this sample. The
single observed idle tick is 10 ms of CPU accounting; this establishes no
battery improvement. Measured-process SwapPss remains zero in both samples.
This checks one small static scene, not a large library, video decode or
aggregate shell RSS/PSS. Selection-to-last-visible-consumer and A/B/A/B results
remain pending the owner-announced window.

Earlier exploratory captures inherited XDG_DATA_HOME and are excluded from
these results. Their post-publication RSS suggested retained raster pages;
returning unused GNU allocator pages after a palette request remains in the
candidate. Its final effect is included in the fully private figures above.
No whole-desktop speed or quiet-window regression conclusion follows from them.

<details><summary>fully-private-preferences receipt</summary>

```json
{
  "condition": "busy machine",
  "source_sha256": "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2",
  "native_sha256": "3ea8827eff467f6076c48355a425fa3ac82dc76c9d4a09af4048b69c1303ca87",
  "method": "benchmark-helpers.py child CPU; performance.py helper RSS and process ticks",
  "workload": "startup + preferences CPU; idle memory after preferences only",
  "clock_ticks_per_second": 100,
  "results": {
    "python": {
      "calls": 20,
      "startup_request_cpu_ms": 86.21444999999997,
      "helper_rss_mib": 25.83984375,
      "idle_cpu_seconds": 0.0,
      "idle_seconds": 30.000071802001912,
      "idle_cpu_percent": 0.0,
      "process_count": 1,
      "python_process_count": 1
    },
    "rust": {
      "calls": 20,
      "startup_request_cpu_ms": 11.575200000000013,
      "helper_rss_mib": 16.73046875,
      "idle_cpu_seconds": 0.0,
      "idle_seconds": 30.000103288999526,
      "idle_cpu_percent": 0.0,
      "process_count": 1,
      "python_process_count": 0
    }
  }
}
```

</details>

<details><summary>fully-private-publication receipt</summary>

```json
{
  "condition": "busy machine",
  "source_sha256": "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2",
  "native_sha256": "3ea8827eff467f6076c48355a425fa3ac82dc76c9d4a09af4048b69c1303ca87",
  "method": "benchmark-helpers.py child CPU; performance.py helper RSS and process ticks",
  "workload": "startup + preferences CPU; idle memory after one wallpaper publication",
  "clock_ticks_per_second": 100,
  "results": {
    "python": {
      "calls": 20,
      "startup_request_cpu_ms": 79.04169999999999,
      "helper_rss_mib": 26.1796875,
      "idle_cpu_seconds": 0.0,
      "idle_seconds": 30.000243774004048,
      "idle_cpu_percent": 0.0,
      "process_count": 1,
      "python_process_count": 1
    },
    "rust": {
      "calls": 20,
      "startup_request_cpu_ms": 10.435250000000007,
      "helper_rss_mib": 27.34765625,
      "idle_cpu_seconds": 0.010000000000000009,
      "idle_seconds": 30.000156602996867,
      "idle_cpu_percent": 0.03333315933091182,
      "process_count": 1,
      "python_process_count": 0
    }
  }
}
```

</details>

### Wave 3 native lock data and preparation (busy machine)

The cached `lock-info play-icon` path used 100 interleaved fresh calls in a
private HOME with all four XDG roots and absent D-Bus/compositor sockets.
The release build completed before sampling. CPU uses the existing
`benchmark-helpers.py` child user+system accounting; wall-clock waits are excluded.

| Helper CPU per call | Python | Native |
| --- | ---: | ---: |
| Cached lock reader | 78.29 ms | 2.57 ms |
| Python launches in 100 calls | 100 | 0 |

This is a laptop busy-machine microbenchmark. It does not measure lock startup,
RSS/PSS of the desktop, idle CPU or selection-to-last-visible-consumer timing.
The native preparation path retains the same private files and frozen PNG bytes,
including paused application colors while widget data keeps updating.

```json
{
  "condition": "busy machine",
  "source_sha256": "aa38f4f85a6ba0035b23a35e7018c22df30a28eac1e0e043067e4529c74a2ad0",
  "native_sha256": "5a948f8aea4de47f5e40d8d0dce49b375ce3b5895c44f45d13d766aef3beb4d0",
  "method": "benchmark-helpers.py child user+system CPU; fresh process; cached snapshot; no shell wall-time claim",
  "results": {
    "python": {
      "calls": 100,
      "cpu_ms_per_call": 78.28809,
      "python_launches": 100
    },
    "rust": {
      "calls": 100,
      "cpu_ms_per_call": 2.570119999999996,
      "python_launches": 0
    }
  }
}
```

### Wave 3 presentation checks (busy laptop; timing gate pending)

The Python baseline `e910ffd7` and native lock/palette build `665684eb` were
exercised with private HOME/XDG roots, a private session bus and nested Hyprland
and niri. Builds completed before these functional captures. These are not
quiet-window latency, desktop memory or battery measurements.

| Consumer evidence | Python baseline | Native |
| --- | --- | --- |
| Shell selected poster/palette, frame callback and scene pixels | Confirmed | Confirmed |
| Qt themed selection: new accent in mapped window pixels | Confirmed | Confirmed |
| Kitty existing ANSI text: new accent in mapped window pixels | Confirmed | Confirmed |
| GTK 3 themed widget: expected new palette pixels | Not demonstrated | Not demonstrated |
| Actual nested lock on both compositors: acquired and new accent pixels | Confirmed | Confirmed |
| Whole-desktop completion time | No verified endpoint | No verified endpoint |

GTK's paint callback still resolves the preceding accent in both builds. Its
window pixels do not establish the expected new palette. A finite all-consumer
latency cannot be reported from this fixture. This is shared baseline behavior;
no migration regression has been established. The niri software lab lacks
the DMA buffers requested by Hyprlock's resource preload. The developer lock probe
uses `--immediate-render --no-fade-in --grace 0` in both builds; lock acquisition
and new accent pixels then pass on both compositors. Default lock startup time
was not measured, and production lock options are unchanged.

The developer harness now stages each revision's own CLI adapter and exact
prebuilt helper, records binary hashes and machine/tool versions, and samples
SwapPss during selection for the private witness/compositor processes as well
as shell descendants. See [presentation witnesses](../../tools/performance-probes/README.md).
The selected palette is precomputed: this is a warm-cache functional scenario.
Its five-second observation and two-second lock wait are not latency estimates.

Short-lived native wallpaper commands cache the executable SHA-256 in
`cache/nacre/native-engine-identity.json` (at most 4 KiB). The cache is accepted
only for an owned, regular, non-writable-by-others file matching the executable’s
full device/inode/size/mode/owner/mtime/ctime identity; invalid or unavailable
cache data falls back to hashing the pinned executable. The palette key is
unchanged. Catalogue prewarming processes only requested IDs, even though the
metadata catalogue includes other rows. Neither cache contains decoded frames.

A busy-host smoke recorded 274 host swap-in pages at 4,096 bytes/page
(1,122,304 bytes), exceeding the owner's 1 MiB gate, while sampled processes
had SwapPss zero. It supplies functional evidence only. The required desktop-PC
A/B/A/B comparison still awaits the owner's next announced quiet window; keep
its results separate from these laptop checks and discard invalid captures.

### Media response cleanup before Wave 4 (busy machine)

A matched check of the pre-Wave4 source and the same source with response cleanup
moved ahead of `malloc_trim` used `benchmark-wallpaper-media.py`: 10 fresh
startup/preferences calls and 30 seconds resident after the same private scene
publication. Builds completed first. This is helper RSS/CPU, not shell wall time.

| After-publication helper RSS | Before cleanup | After cleanup |
| --- | ---: | ---: |
| Native media server | 29.19 MiB | 27.80 MiB |
| Python control | 25.95 MiB | 26.24 MiB |

Startup/preferences CPU was 29.66→29.76 ms/call (10 samples); this is no
measured CPU improvement. Native Python launches remained zero.
The native server releases 1.38 MiB more in this check; it still uses 1.57 MiB
more RSS than the second Python control. Media and CLI state retain no decoded
frames. The old trim ran before the result/serialized response allocations were
released; trimming now follows response destruction. There is no allocator or
cache redesign. Both idle samples used zero CPU ticks over 30 seconds; this is
not evidence of a battery improvement. The original 26.18→27.35 MiB observation
remains above, and this busy-host check does not replace the quiet comparison.

### Wave 4 supervisor (busy machine)

The original Python supervisor and native replacement each launched the same
private inert QS stand-in, reached the same health response and handled SIGTERM.
Twenty fresh runs were interleaved before two 30-second resident samples. Release
builds completed first. Child user+system CPU includes identical stand-in work;
helper-group RSS includes Python's native compositor bridge and excludes QS/sleep.

| Supervisor measurement | Python | Native |
| --- | ---: | ---: |
| Startup/health/stop CPU per call | 98.58 ms | 22.38 ms |
| Resident helper-group RSS | 24.64 MiB | 10.77 MiB |
| Helper processes | 2 | 1 |
| Python launches in 20 runs | 20 | 0 |
| Helper CPU during 30 s idle | 0 ticks | 0 ticks |

This is not a shell startup latency or battery measurement. Seventeen frozen
startup/recovery/environment contracts, the retained recovery assertions and
actual supervised private niri/Hyprland sessions pass. Startup still allows one
last-known-good restore; runtime failure still returns to systemd. Rejected code,
preferences and file permissions/timestamps are preserved. The native environment
query emits only changed display/session variables, with a credential-canary test.

### Wave 4 session observer (busy machine)

Twenty fresh `check` calls were interleaved with private HOME/XDG, D-Bus,
compositor sockets and the same inert QS reply. Release builds finished first.
`benchmark-session-watch.py` uses the helper benchmark's child user+system CPU
method; the bus/compositor providers and measurement driver are excluded.

| Session measurement | Python | Native |
| --- | ---: | ---: |
| Check CPU per call | 125.79 ms | 29.50 ms |
| Python launches in 20 checks | 20 | 0 |
| Resident observer helper RSS | unavailable | 18.09 MiB |
| Helper processes during valid idle sample | unavailable | 1 |
| Helper CPU during 30 s idle, 100 Hz | unavailable | 0 ticks |

The original Python `watch` replay exits 1: its `event_socket` request reaches a
retired compositor-bridge operation. An idle comparison against that failed
process would be misleading. Native observation directly subscribes to login1
and compositor events; private real-bus/socket tests cover sleep, resume,
hotplug and disconnect cleanup. Readiness and display-restoration policy are
unchanged, including 500 ms resume and 300 ms hotplug delays. No wall-clock,
whole-shell RSS/PSS or battery improvement is inferred from this busy sample.

The original source SHA-256 is
`72a54f4ad5f3da2629a8ff2683d33c499ca99362fa2a00eeff6ad938e8673317`;
measured release helper SHA-256 is
`e3b7873621037e9a32684431f92abd451ab6dac04ef7dd2ab9737296a3e0954b`.
Frozen current-Python records, files, modes, readiness, output filters, ordered
watch effects and CLI cases remain immutable. Custom absolute XDG roots follow
QML as intended; linked/foreign/hard-linked evidence is refused and private
permissions are applied before writing. Missing desktop/wallet/compositor
observations retain their existing values and now emit journal diagnostics.

Both supervised nested niri and Hyprland checks reached real shell readiness and
ran the observer as one native process, with no Python child. The wider palette
witness also passed on both. Its first niri pixel capture missed the exact primary
color; an identical-source retry passed. The cause is unverified; this is not a
quiet-window performance comparison.

The earlier candidate sample was 98.56→30.96 ms CPU and native17.86 MiB;
the table is the final sample after the unregistered-directory writer fix and
integration of the existing Niri hover update. Both Python control samples show
busy-machine variability; no wall-clock claim follows from these values.

### Foundation F1 daemon and private link (nested, busy machine)

`tools/benchmark-daemon.py` reuses the existing headless labwc/niri/Hyprland
lifecycle, stops before any Quickshell launch, and uses `performance.py`'s CPU
and memory readers. HOME/XDG, both bus addresses and sockets are private.
The release binary is built first. Each backend uses 100 discarded warm-ups,
5,000 persistent-socket empty calls, 35 seconds for the two-thread blocking pool
to retire, then 60 seconds idle with a private 20-second watchdog configuration.
These are helper observations, not physical desktop or complete shell timings.
Final-source captures use the `smoke-helper` release artifact from CI
38797680489 at `10e860363fb5c37631925873ea81eddebd89e012`; no local rebuild
or compilation ran during capture.

| New private-link measurement | Nested niri | Nested Hyprland |
| --- | ---: | ---: |
| Empty call median / p99, including benchmark JSON/socket overhead | 0.242 / 0.387 ms | 0.245 / 0.428 ms |
| Daemon user + system CPU per empty call, 10 ms accounting ticks | 0.064 ms | 0.064 ms |
| Idle helper RSS, median | 14.13 MiB | 14.07 MiB |
| Idle CPU in 60.08 seconds, 100 Hz accounting | 0 observed ticks | 0 observed ticks |
| Threads throughout the settled idle sample | 1 | 1 |
| Main-thread sleep/wake cycles in that sample | 16 | 16 |
| Replacement process start to native client's fresh compositor data | 254 ms | 256 ms |
| Accepted quiet idle PSS / calibrated PSS budget | Pending | Pending |

This API has no pre-foundation daemon baseline. No launcher, dashboard, wallpaper,
palette or keybinding improvement is claimed at F1; QML is untouched. Zero idle
ticks means below the 10 ms accounting resolution, not zero CPU consumption.
The kernel does not expose `sched_wakeup` counts here. The cycle measurement is
the voluntary-switch delta with one thread and sleeping endpoints (about 0.27/s);
it is a documented event-loop proxy, not a direct kernel wakeup measurement.
Restart freshness ends at client data, not a rendered UI frame.

The call captures had measured SwapPss zero and host swap-in of 35/58 pages.
The idle captures retained measured SwapPss zero but host swap-in was
1,454/1,524 pages at 4,096 bytes/page, exceeding the agreed under-1-MiB gate.
Their PSS is discarded as performance evidence; no initial PSS budget or saving
is established. Helper RSS and CPU above are busy-machine observations only.
Python execs were not traced in these captures and are unknown. Separate native
integration tests work with an empty PATH and verify private notification delivery,
socket activation, restart/resubscribe, safe mode and cleanup. They do not replace
the final five-minute launch proof or the pending frozen desktop-PC A/B/A/B plan.

The earlier pre-final candidate observed p99 0.274/0.278 ms, CPU 0.024/0.020 ms
per call and helper RSS 14.24/14.21 MiB. Its raw captures remain unchanged.
These busy runs of separate builds are not a controlled before/after comparison;
no speedup or regression conclusion follows from their difference.

The ELF fingerprints and raw paging counters stay in the private capture.
Run the tool with `--backend niri` or `hyprland`, a matching prebuilt
`--helper-binary`, the existing labwc prerequisites and `--output REPORT.json`.
The daemon units are staged without enabling/starting them. A rare safe-mode
notice is delivered by a bounded native nice-10 worker using zbus directly;
it cannot activate an absent notification server and exits after acknowledgement.

#### Desktop-PC F1 quiet baseline (nested)

The desktop became reachable again. A fresh private checkout and the same CI
artifact at `10e860363fb5c37631925873ea81eddebd89e012` ran the same method above,
without compilation during measurement. These are desktop-PC nested numbers,
separate from laptop observations. CPU, RAM, kernel, governor, distro and the
Quickshell/niri/Hyprland versions are recorded in the private machine inventory.
The existing lab checkout and its staged changes were preserved. No live session,
configuration, unit or system package changed; a missing lab utility was supplied
as an existing private binary.

| Desktop-PC daemon measurement | Nested niri | Nested Hyprland |
| --- | ---: | ---: |
| Empty call median / p99, including client overhead | 0.014 / 0.018 ms | 0.015 / 0.027 ms |
| Daemon CPU per call, 0.002 ms accounting quantum | 0.006 ms | 0.008 ms |
| Idle helper RSS / PSS | 14.28 / 10.27 MiB | 14.05 / 10.08 MiB |
| Initial measured PSS + 20% warning limit, compositor-only scope | 12.33 MiB | 12.09 MiB |
| Idle CPU, 60 seconds at 100 Hz | 0 observed ticks | 0 observed ticks |
| Settled threads / voluntary sleep-wake cycles | 1 / 18 | 1 / 17 |
| New process to native client's fresh compositor data | 253 ms | 256 ms |
| Host swap-in pages, calls / idle, 4,096 bytes per page | 0 / 0 | 0 / 0 |
| Measured SwapPss, calls / idle | 0 / 0 | 0 / 0 |

Both call and idle intervals pass the agreed paging gate. The three-hour memory
plateau, direct kernel wakeup counter, rendered UI recovery and frozen-source
Wave 3 A/B/A/B remain pending. Python launches were not traced here. The lower
call times on this other machine are not a before/after migration speedup.

### Foundation F2 niri event compatibility clients (desktop-PC, nested)

The exact pre-fold-in parent `de55e286` and the event-sharing candidate were
built with the same desktop Rust toolchain, release profile and pinned Orient
source before measurement. A private wrapper reused `benchmark-daemon.py`'s
headless niri lifecycle and `performance.py`'s process/memory accounting.
Each A/B/A/B interval settled for 35 seconds, then sampled for 60 seconds at
250 ms intervals. A ran the daemon plus three direct `compositor events`
consumers; B ran the daemon plus three clients of its shared event lane.
The table averages the two per-run medians for each side.

| Event-client group measurement | Direct events | Shared daemon events |
| --- | ---: | ---: |
| Combined helper PSS | 21.04 MiB | 17.70 MiB |
| Combined helper RSS, including shared pages more than once | 66.37 MiB | 50.84 MiB |
| Native helper processes / settled threads per process | 4 / 1 | 4 / 1 |
| Idle CPU per process, 60 seconds at 100 Hz | 0 observed ticks | 0 observed ticks |
| Daemon main-thread voluntary sleep-wake cycles per interval | 17 | 17 |
| Client main-thread voluntary sleep-wake cycles per interval | 0 | 0 |

Both pairs show lower combined PSS: 20.99→17.61 and 21.09→17.80 MiB.
The average reduction is 3.34 MiB (15.9%). Host swap-in counts were 1/0/0/0
pages at 4,096 bytes/page, swap-out was zero, and all measured SwapPss samples
were zero. All four intervals pass the agreed gate. Voluntary switches are
sleep-wake proxies, not direct kernel wakeup counters.

Baseline ELF SHA-256: `894be75c3414d2467c60312d446be66d936ce9b0565b2d98361789c6a9e9f84c`.
Candidate ELF: `16ab5d038bec6c0853981291ce3c365f865c54262afc8a4a28b08e27400b2795`.
Private report digest: `a608e2da54997a756b0ffbc7bf55929a8622f6ce517ca186cf3df0fa6e909fc0`.
An earlier comparison used a CI-built F1 baseline and a desktop-built candidate;
it is retained as exploratory evidence and does not determine a regression.

This is the event-client component group, excluding the compositor and UI. The
helper count is unchanged at this step; the remaining F2 services consolidate
processes later. A native private-socket test verifies that multiple consumers
share one compositor event connection and one request connection. Existing
direct frozen outputs stay exact; the bridge additionally checks 79 frozen
state observations: 54 exact and 25 explicitly asserted consistent-reference
projections, including states before initial stream readiness. No frozen output
was regenerated. No whole-shell, physical desktop, battery or Python-launch
claim follows from this capture; the full F2 and frozen Wave 3 comparisons remain
separate gates.

The first full frozen-source baseline capture was discarded because a short-lived
helper's memory rollup became unreadable while its process record still existed.
The sampler now records process state and omits an unreadable sample only when
the post-read record confirms an exited zombie/dead process with an exit-race
error. It retains unknown live/permission-denied samples and successfully read
nonzero SwapPss, including a read immediately before exit. A missing SwapPss field
also remains unknown. A live exit-race read is retried once immediately with
another identity check; a persistent failure still invalidates the run.
A real unreaped child and explicit live/swapped controls
verify this distinction; see the [kernel proc documentation](https://docs.kernel.org/filesystems/proc.html).
The discarded reports are retained and are not retroactively repaired. A later
private-runtime cleanup encountered a disconnected GVFS FUSE mount; that mount
was removed only from the recorded disposable runtime. The lab now sets
`GVFS_DISABLE_FUSE=1`, the [documented GIO control](https://docs.gtk.org/gio/overview.html),
before launching children. This changes no installed session configuration.

### Frozen Wave 3 desktop-PC A/B/A/B (nested)

The frozen Python baseline `e910ffd7202a332c652ef227ddcf5970c39714bb`
(Orient `820e199714d9d3a1233db80705e39838e86c49fc`) and Wave 3
`81d9e520d6f6d0a915018f93e3b6279b716ddc91`
(Orient `3efe8d1e7fb50200d74a3e0193ad63699cb8ebe8`) were built first on
the desktop PC with the same toolchain and profile. No compilation ran during
measurement. Harness `ffe32f36459d3d3c2a7844bf4134da9b225b4cf5` ran each
compositor's full session and palette presentation sequence in A/B/A/B order,
using disposable HOME/XDG/D-Bus roots and headless nested compositors. These are
desktop-PC nested results, not laptop or physical presentation measurements.
Machine inventory and raw diagnostic reports remain private.

| Action or idle measurement | Hyprland Python | Hyprland native | niri Python | niri native |
| --- | ---: | ---: | ---: | ---: |
| Cold launcher frame, two runs (ms) | 98 / 100 | 94 / 100 | 419 / 419 | 459 / 461 |
| Warm launcher mean / p95 (ms) | 21.80 / 101 | 21.90 / 104 | 363.40 / 415 | 363.95 / 426 |
| Cold dashboard frame, two runs (ms) | 46 / 41 | 56 / 50 | 363 / 363 | 362 / 363 |
| Warm dashboard mean / p95 (ms) | 36.50 / 45 | 36.55 / 47 | 364.65 / 381 | 364.10 / 367 |
| Wallpaper choice to new shell frame, two runs (ms) | 184 / 201 | 94 / 94 | 195 / 196 | 113 / 91 |
| Closed shell RSS / PSS (MiB) | 650.06 / 527.42 | 637.18 / 514.51 | 645.55 / 525.51 | 640.84 / 520.39 |
| Closed aggregate RSS / PSS (MiB) | 713.23 / 564.77 | 681.82 / 535.96 | 727.11 / 567.85 | 708.53 / 547.18 |
| Idle CPU, one-core percent | 0 observed | 0.098 | 0 observed | 0 observed |

Memory and CPU cells average the two per-run medians. Warm action cells pool the
repeats; cold cells retain each observation. Wallpaper-to-shell presentation
averages 192.5→94 ms on Hyprland and 195.5→102 ms on niri. Qt, Kitty and the actual
lock screen also have new-color pixel witnesses. GTK pixel completion remains
unverified on both sources: there is **no finite whole-desktop completion time**.
The historical 225/190 ms helper-pair timings use different endpoints; this table
does not remeasure them. Sampled Python starts were zero in these intervals, a
sampling lower bound that does not establish zero execs or the five-minute proof.

All measured SwapPss samples were zero. Host counters below are page deltas,
4,096 bytes/page, in A1/B1/A2/B2 order. All swap-in deltas are below the owner's
under-1-MiB limit; swap-out alone is not a rejection under that rule.

| Capture | Host swap-in pages | Host swap-out pages |
| --- | --- | --- |
| Hyprland full | 4 / 5 / 5 / 0 | 5398 / 26903 / 26306 / 0 |
| Hyprland palette | 17 / 0 / 9 / 0 | 0 / 0 / 0 / 0 |
| niri full | 1 / 0 / 4 / 6 | 0 / 0 / 9214 / 4174 |
| niri palette | 12 / 0 / 10 / 0 | 0 / 0 / 0 / 0 |

The Hyprland cold-dashboard increase of 9–10 ms did not persist in one focused
phase-controlled A/B/A/B repeat: delaying the input probe by another 7 ms gave
33/30→32/32 ms, with data ready in 30/27→29/18 ms. A separate 100-call owner-request
probe had p95 1 ms on all four runs, its clock resolution. Dashboard QML and
controller logic are unchanged. This indicates presentation-phase sensitivity;
the original observations remain above. No product behavior or timeout changed.
The niri cold-launcher increase of 40/42 ms persists after one focused control.
Removing the inactive software preview Loader only from disposable fixture QML
on both sources gave A/B/A/B cold frames 414/470/411/460 ms. Synchronous work was
73/69/71/69 ms. All four control runs had measured SwapPss zero and host swap-in
zero; swap-out page deltas were 4042/3721/0/0. The control does not explain or fix
the extra presentation delay. Product QML, frozen sources, fixtures and timeouts
remain unchanged. At this checkpoint ports waited for the owner's decision;
the subsequent exact-step audit below supersedes that hold. Private control archive:
`2d55a8809c601790b6e30f6ab4f7a032375361c9164008ef1908e009f09990c1`.

Sanitized summary SHA-256:
`71c52e2b5e5bc3d4619af1916c10f02a7344764c8a8f08c8029cb521d438f89d`.
Invalid prerequisite, unknown-memory, coverage and excess-swap attempts are
retained separately and excluded, never repaired or overwritten. The early
foundation snapshot `33a0a897` has not yet completed its separate Wave 3/current
comparison; it must not be reported as a completed Wave 4 comparison.

### Foundation F2 session observer consolidation (desktop-PC, nested)

The parent `ffe32f36459d3d3c2a7844bf4134da9b225b4cf5` and the session-service
candidate were release-built with the same laptop toolchain/profile, then copied
to the desktop PC. Neither compiled during capture. A private wrapper reused
`benchmark-daemon.py`'s nested lifecycle and `performance.py`'s accounting, adding
private login1/Secret Service fixtures and the existing UI-state reply. A ran
the daemon plus standalone `session-watch watch`; B ran its async daemon task.
The fixture bus/provider, compositor and UI are outside this helper-group scope.
Each A/B/A/B interval settled for 35 seconds and sampled idle for 60 seconds.
Cells average the two per-run medians.

| Helper-group measurement | niri direct | niri daemon | Hyprland direct | Hyprland daemon |
| --- | ---: | ---: | ---: | ---: |
| Combined RSS (MiB) | 39.26 | 20.62 | 38.41 | 20.34 |
| Combined PSS (MiB) | 21.43 | 16.62 | 21.47 | 16.29 |
| Processes / settled threads per process | 2 / 1+7 | 1 / 1 | 2 / 1+7 | 1 / 1 |
| CPU per process, each 60-second run at 100 Hz | 0 observed ticks | 0 observed ticks | 0 observed ticks | 0 observed ticks |
| Main-thread voluntary switches, two runs | 17+0 / 17+0 | 16 / 17 | 17+0 / 18+0 | 17 / 17 |

Both paired comparisons improve PSS on each compositor. All measured SwapPss
samples were zero. Host swap-in page deltas in A/B/A/B order were 0/0/0/1 on
niri and 0/0/0/0 on Hyprland, at 4,096 bytes/page; swap-out was zero throughout.
All eight intervals pass the agreed paging rule. Voluntary switches are a
sleep/wake proxy, not a direct kernel wakeup counter; zero observed CPU ticks is
not exact zero CPU. Python execs were not traced in this native/native comparison.

Parent ELF SHA-256:
`14e96517638d6df6180ecfdffce1522923292d61f195c92b76bfb4df6d9b2dea`.
Captured candidate ELF:
`cd4141c5d445070d0b6caa11a543eec5b209d831ef73e7f598ced8108eeafc09`.
Private evidence archive SHA-256:
`6e44bf4cb37b1813b6d08819ae51031a0c197b725a2777db91ba4b684b50527c`.
The paired candidate precedes the oversized-window resync guard. A separate
one-B final-binary recheck per compositor passed: RSS/PSS 20.47/16.45 MiB on niri
and 20.38/16.30 MiB on Hyprland, one process/thread each, zero observed CPU ticks,
zero measured SwapPss and zero host swap-in/out. Both final intervals had 18
main-thread voluntary switches in 60 seconds; the paired observations above
remain separate and are not rewritten. This proxy is not an exact wakeup count.
Final ELF: `10a7b4ad1e88410b66e2e0ad9e22f643d1a33efc6e0b139e33661942af41a416`.
Final evidence archive:
`2b9cd1a124a4a6c6530ff91a0429a9f77ea54f4ac1683227682b1118accba282`.
The final wrapper inherited the paired method label; an accompanying method
receipt identifies the actual one-B run arrays and preserves the raw reports.

The direct and daemon paths share readiness/effect rules: 500 ms resume and
300 ms display settling, saved connected-display recovery and private evidence
files remain. The compatibility watch now acknowledges ownership and exits;
its on-failure unit remains one release. Default missing/older daemons retain
direct fallback, explicit endpoints do not fall back, and safe mode never starts
a duplicate watcher. A private bus/socket test exercises readiness, resume and
real output changes over one shared request and event connection, UI-provider
routing, cancellation cleanup, and a 4-KiB window title without display recovery.
No live units or session configuration were enabled or changed.

### Exact session-step cold launcher audit (desktop-PC nested niri, 2026-10-16)

The owner requested a two-hour investigation of **33a0a897** versus **84ce30ff**,
starting with at least five alternating before/candidate runs. These are distinct
from the older Python/Wave 3 sources above. Both exact clean revisions and pinned
Orient `3efe8d1e` were release-built with the same desktop toolchain/profile before
measurement. No compilation ran during capture. The existing `performance-session.py`
probe at `ffe32f36` measured the first launcher open after shell startup settling:
Qt key handler to `frameSwapped` after geometry/images settle, excluding compositor
presentation. A private wrapper omitted only subsequent video scenarios, recorded
process roles before input and retained the existing QML probe and 0.2-second delay.

| Alternating pair | Before 33a cold frame | Candidate 84ce cold frame | Paired change |
| --- | ---: | ---: | ---: |
| 1 | 454 ms | 462 ms | +8 ms |
| 2 | 439 ms | 440 ms | +1 ms |
| 3 | 423 ms | 456 ms | +33 ms |
| 4 | 454 ms | 472 ms | +18 ms |
| 5 | 426 ms | 460 ms | +34 ms |
| Median | 439 ms | 460 ms | +18 ms paired median |
| Min–max / inclusive IQR | 423–454 / 28 ms | 440–472 / 6 ms | 1–34 / 25 ms |

The difference between medians is 21 ms, inside the 31/32 ms per-side ranges and
the 28 ms baseline IQR. Per the owner's explicit spread rule it is classified as
noise for landing this step, not proof of identical performance. All paired
increases remain recorded. Synchronous open medians are both 69 ms; warm frame
medians are 357/363 ms, ranges 357–366/357–365 ms. The older two-pair frozen-source
40–56 ms observation is not evidence that this session step caused it.

All ten runs have complete measured-process SwapPss coverage, every sample zero.
Host swap-in page deltas in A/B order were 73/20, 20/49, 82/55, 22/153, 69/173 at
4,096 bytes/page; swap-out was zero. All pass the under-1-MiB rule. Three earlier
pairs with fresh SSH observations failed on candidate swap-in 270/2551/1042 pages;
all were discarded and retained. The accepted sequence used one SSH log connection
established before capture and no additional SSH commands during measurement.
This removes an observation confound without proving what caused those page-ins.

The installed QML and shell-tool sources are identical between these revisions;
the prepared instrumented QML digest also matches across all ten runs. Every
before-input process audit shows no daemon/session-watch process and no private
daemon socket. The harness starts no user units. Consequently this run cannot
contain daemon startup contention, duplicate old/new session observers, socket
activation, hello or initial daemon snapshots. The QML opener changes view state;
it does not synchronously call the daemon. The optional Niri event client checks
the absent socket once during its own startup and takes the existing direct path.
No production fix or UI change was made for this audit. Continue with the launcher
engine before remaining F2 folds; retain cold-open monitoring after each fold and
review the separate historical observation before F3.

Before ELF: `16ab5d038bec6c0853981291ce3c365f865c54262afc8a4a28b08e27400b2795`.
Candidate ELF: `b5da6024910a5a10ac5fd82a5fffd6d1b135fde545c065070e012f1c3506858e`.
Private summary: `e6328aee4d69b622e31419ac59857dfe498da20a2dd196510b9a2fce0a4fb18e`.
Archive: `f943ef0a30afc0fbbe9c7955d6c514e95f9129a61b0f27c5be3a316d8b882f5d`.
Raw reports, rejected attempts, executed-wrapper receipts and machine inventory
remain private. Sources and frozen parity outputs were not regenerated.

### Frozen font renderer compatibility (2026-10-16)

Source CI for `95853713` and the additive protocol `3042f3f2` installed FreeType
2.15.0. Its unchanged lock-font tests found 21 differing text-mask samples and
96 differing panel pixels. A private same-font Pillow comparison reproduces the
cause: all 56 cases match the frozen outputs on FreeType 2.14.3; six differ on
2.15.0, including the same 21 samples in the first failing case. The upstream
[release notes](https://sourceforge.net/projects/freetype/files/freetype2/2.15.0/)
describe changes to the v40 TrueType interpreter.

The test-only compatibility harness retains the original masks, font files,
offsets, advances and full PNG byte assertions. The two frozen font tests select
a checksum-pinned compatible renderer in a child process when needed, with a
fail-closed version check and proof that exactly one test ran. A separate test
compares all 56 Rust masks/metrics with live Pillow on the current process's
FreeType. Both five-test runs pass with 2.14.3 and 2.15.0. Formatting, Clippy and all
315 Rust tests pass; a wrong-renderer negative control fails as required.
The repository/parity run and published CI remain acceptance gates.
Clean validation also requires building the existing developer examples before
the publication fixture test. CI now places that existing build step before
`cargo test`, rather than depending on a restored unversioned example binary.
The source bootstrap lives in the uninstalled test tree and builds with `make -j2`
inside the Cargo target. No system package or production dependency is pinned.

Intended difference: system font-renderer upgrades follow Pillow on the same
FreeType; immutable pixel references use their compatible test renderer. No
installed Rust/QML code or frozen output was changed for this CI repair.

### Foundation launcher engine (desktop-PC nested, 2026-10-16)

The native library ports ranking and providers from the JavaScript/QML at
`83a9a9a493dcdc53f8fe42c94f01ac2553694035`. Its 991 immutable reference cases
cover ordered results and complete row values: 394 JavaScript, 544 provider,
18 configuration, 2 legacy-action, 31 emoji and 2 Quickshell desktop-catalog
cases. The indexed and original paths run against the same frozen expectations;
expected outputs have not been regenerated. Additional tests cover live index
invalidation, concurrent preferences, invalid edits, opaque activation tickets,
private systemd-manager activation and worker retirement.

`tools/benchmark-menu.py` reuses the existing private nested-session harness,
with synthetic 320/1,000/2,000-entry catalogs on each compositor. All binaries
are built before capture. Each size warms 100 requests, then measures 2,000
sequential requests across nine queries. Wall time is client send to the matching
completed reply, including client JSON decoding; CPU is daemon and worker
user+system ticks divided by calls, not wall time. This is a desktop-PC helper
microbenchmark, labelled "busy machine" in the raw reports, not a UI frame,
physical presentation or battery measurement. CPU and helper memory are component
evidence; the round-trip diagnostics are not whole-shell action latency.
The QML launcher is unchanged until F3.

Initial 320-entry smoke profiling found repeated provider-row preparation:
17.1 ms daemon CPU/query. Cached generations and pre-normalized ranking lowered
it to 0.9 ms in a separate 100-call smoke. These exploratory captures are not
accepted before/after UI timings. Catalog generation preparation runs on the
bounded blocking pool; focus/geometry events do not invalidate ranking inputs.
The async service continues to own catalog, tickets and preferences.

The initial indexed capture (before the retention fix) used helper SHA-256
`dce1187541f7774f02b23a46c826345485b588a7ea69803ade06e0844db82ef9`.
It produced the following accepted component results; these remain separate
from the final build rather than being rewritten after optimization.

| Compositor / apps | Daemon CPU/query (ms) | Worker CPU/query (ms) | Round trip p50 / p99 (ms) | Settled idle PSS (MiB) |
| --- | ---: | ---: | ---: | ---: |
| niri / 320 | 0.825 | 0.235 | 0.966 / 2.066 | 39.06 |
| niri / 1,000 | 0.935 | 0.240 | 1.048 / 2.462 | 58.30 |
| niri / 2,000 | 1.010 | 0.245 | 1.126 / 2.600 | 81.77 |
| Hyprland / 320 | 0.835 | 0.240 | 0.983 / 2.299 | 38.94 |
| Hyprland / 1,000 | 0.920 | 0.240 | 1.032 / 2.448 | 58.15 |
| Hyprland / 2,000 | 1.000 | 0.240 | 1.131 / 2.561 | 81.73 |

All six query/idle pairs pass the paging rule. At 4,096 bytes/page, query
host swap-in deltas were 0/0/0 (niri) and 0/0/1 (Hyprland); idle deltas were
15/22/102 and 2/1/2 respectively. Swap-out and measured SwapPss were zero
throughout. All six 60-second idle samples had one process/thread and zero
observed CPU ticks or main-thread voluntary switches. These counters are
quantized CPU and a sleep/wake proxy, not exact zero CPU or kernel wakeups.
The tick quantum is 10 ms, or 0.005 ms/query at 2,000 calls.

Hyprland uses a headless GLES2-rendered parent; niri uses Pixman. The first
Hyprland Pixman-parent attempt failed backend creation before measurement and
supplies no numbers. No live compositor, units or desktop configuration changed.
The earlier 16.45-MiB daemon-only capture had no launcher index: it is not a
matched combined-memory comparison. F3 must measure the UI and daemon together
when QML stops maintaining its own ranking state. Three-hour stability and
physical UI/8-ms integrated keystroke acceptance remain unverified.

A focused retention fix keeps activation records as their existing encoded bytes,
rather than duplicate JSON trees, and returns unused GNU allocator pages on the
existing worker-retirement event. It adds no polling or per-query trim. Matching
2,000-call niri captures at 320/2,000 apps reduce settled PSS
39.06 → 34.86 / 81.77 → 63.32 MiB. One process/thread and zero observed CPU ticks
or voluntary switches remain. The source/installed English catalog lookup was
also aligned before this final capture; no golden output was regenerated.
The retained native index is still substantial. It is not a demonstrated
combined UI/daemon regression or improvement until the F3 client cutover is
measured; no comparison against an empty daemon can establish that result.

Final retention build (same 2,000-call method; 320 and 2,000 apps):

| Compositor / apps | Daemon CPU/query (ms) | Worker CPU/query (ms) | Round trip p50 / p99 (ms) | Settled RSS / PSS (MiB) |
| --- | ---: | ---: | ---: | ---: |
| niri / 320 | 0.820 | 0.235 | 0.983 / 2.074 | 38.48 / 34.86 |
| niri / 2,000 | 1.020 | 0.250 | 1.128 / 2.631 | 66.79 / 63.32 |
| Hyprland / 320 | 0.825 | 0.230 | 0.969 / 2.073 | 37.41 / 33.79 |
| Hyprland / 2,000 | 1.020 | 0.245 | 1.118 / 2.623 | 66.32 / 62.80 |

During queries the two-process group has RSS/PSS 75.92/59.38 and
116.93/100.11 MiB on niri (320/2,000 apps), 76.01/59.63 and 116.13/99.63 MiB
on Hyprland. After the 35-second settling period all four captures have one
process/thread and zero observed CPU ticks/main-thread voluntary switches over
60 seconds. Final host swap-in query/idle page deltas are niri 0/1, 0/0;
Hyprland 0/19, 0/33. Host swap-out and measured SwapPss are zero; all pass.
The 320-app combined CPU is 1.055 ms/query on both compositors; at 2,000 it is
1.270/1.265 ms. The under-1-ms CPU aspiration is not established for the whole
query, and the separate 1-ms transport budget excludes this work. No integrated
8-ms UI-keystroke acceptance follows. Python execs were not traced: launch count
is unmeasured, not zero; empty-PATH and worker-lifecycle tests are separate proof.

Final helper SHA-256:
`58014d70a97cfe9f64d4ca6fefb25f84f88d933eb56735d91e8e135aed70b18d`.
Private accepted evidence archive SHA-256:
`c019662e3de94fe2a8e32774ae5b86b8b6ea35f7acfbad0911e74b6befc77251`.
Validation: fmt/clippy, 332 Rust tests, protocol freshness, 991 immutable launcher
cases, 929 repository tests (three existing skips), and 534 helper replay cases
pass. Local Node syntax passes; optional local Prettier was unavailable. CI
formatting and full desktop/fresh-install checks remain the publication gate.

The legacy one-shot emoji path was checked separately with the retained original
Rust implementation (`c44ad814f68b5bc6320e4eff5bf6986752fc5fc153f4e8d96aaede04159119cb`)
and the final helper above. Five alternating A/B groups of eight fresh calls
use four synthetic queries; all outputs agree. `benchmark-helpers.py` child
user+system accounting gives median group-mean CPU 63.02 → 13.15 ms/call,
group ranges 62.86–77.17 → 11.65–13.49 ms. Median per-helper peak RSS from
`wait4` is 29.62 → 26.81 MiB. This is a desktop-PC busy-machine component check,
not Python-versus-Rust timing, steady RSS, wall-clock latency or an exec trace.

Bead's catalog-refresh test found overlapping background metadata loads could
occupy both provider workers. The follow-up coalesces these loads and avoids
scheduling them for no-op scans. A private blocked-font fixture reproduces the
interactive `busy` failure with the published helper and passes with the fix,
including provider-ready notification on the `menu` topic. Client retry remains
bounded to idempotent latest queries; activation is not retried.

Follow-up helper SHA-256:
`f7edacd2015ea984261379069e0af2805bf647278ef6035d18a1e80b349fa054`.
With the unchanged nine-query workload at 320 apps, accepted niri repeat/Hyprland
captures give daemon CPU 0.840/0.840 ms plus worker 0.235/0.240 ms/query;
round-trip p50 0.976/0.975 ms and p99 2.208/2.110 ms. Settled RSS/PSS is
37.17/33.57 and 36.32/32.78 MiB, one process/thread, zero observed ticks or
voluntary switches over 60 seconds. Query/idle host swap-in is 148/12 pages on
niri, 47/129 on Hyprland; swap-out and measured SwapPss are zero. Page size is
4,096 bytes. An earlier niri idle interval had 741 swap-in pages (SwapPss zero);
its memory evidence is discarded and retained, not replaced by zeros.
These are spot rechecks, not an interleaved causal speedup comparison.

`--menu-queries` records explicit workloads; `--menu-catalog-style alpha` matches
Bead's synthetic split Alpha/Application catalog without generic-name/keyword
fields. At 320 apps and 2,000 repeated requests per prefix on desktop-PC nested
niri, `a` costs 1.595 ms daemon + 0.320 ms worker CPU (round-trip p50/p99
1.995/2.416 ms); `ap` costs 1.465 + 0.310 ms (1.890/2.125 ms).
These broad-prefix results are distinct from the nine-query mixture, and do not
establish another host's UI budget. Both query and idle intervals pass: host
swap-in query/idle is 40/139 pages for `a`, 0/4 for `ap`; swap-out and measured
SwapPss are zero. CPU remains averaged over the 2,000 calls, quantum 0.005 ms.

Follow-up validation: fmt/clippy, 333 Rust tests including all four native engine
tests, 991 unchanged reference cases, schema freshness, 929 repository tests
(three existing skips) and 534 helper parity cases pass. The old helper fails the
new gated-provider starvation test; the new helper passes. Existing golden
outputs remain unchanged. Private follow-up evidence archive SHA-256:
`4c653b505a18f2b27879dd64394919d184dc54889e0e2ed00415845dbc6d1881`.

### Launcher CPU and indexed-memory audit (desktop-PC nested, 2026-10-16)

The coalesced-refresh baseline is `9374bc8d`, helper SHA-256
`f7edacd2015ea984261379069e0af2805bf647278ef6035d18a1e80b349fa054`.
This audit uses the 320/2,000-app Alpha/Application fixture and broad `a`/`ap`
queries, rather than the earlier nine-query average. It does not establish the
Bead client's input-to-presentation CPU budget on another host.

A private diagnostic build attributes the 320-app query as follows. Outer engine
phases use the thread CPU clock; synchronous inner phases use elapsed time with
no awaits. These are approximate phase means, including warm-up, not independent
costs to add together. In particular, provider construction is inside ranking.

| Phase | Mean ms/query |
| --- | ---: |
| Complete engine query | 1.227 |
| Ranking including dynamic providers | 1.005 |
| Dynamic emoji row construction, inside ranking | 0.621 |
| Scoring and selected-row cloning, inside ranking | 0.179 |
| Activation ticket creation | 0.153 |
| Result validation and JSON-value conversion | 0.066 |
| Three transport frame encodes, outside engine | 0.045 |

Files, Brain and workflow row construction each took under 0.001 ms/query in
this fixture. No filesystem-watch events or query-time index rebuilds occurred;
only initial catalog/compositor/provider preparation was observed. This does not
rule out UI-triggered work in a wider input-to-presentation capture. Repeated
encoding is a small cost here; constructing the same 100 emoji rows is larger.

Settled diagnostic PSS was 32.42/58.27 MiB at 320/2,000 apps: anonymous mappings
17.23/42.92 MiB, helper mappings (code and data, including Unicode tables)
14.09/14.25 MiB, other libraries about 1.1 MiB. Fontconfig/FreeType mapped PSS was
about 0.16 MiB; this query path keeps icon names, not decoded icons. Unicode's
share cannot be uniquely separated from other helper file pages by `smaps`.

An isolated collator probe touched another 312 KiB of PSS while constructing the
collator and comparing ASCII, accented, Turkish, Han and emoji strings. This is a
small table-touching sample, not the full Unicode table size or a daemon PSS
partition. Constructing the full CLDR search index added about 7.53 MiB of live
allocator bytes in that probe; the daemon keeps that index in the retiring worker.

A controlled teardown measures live allocation-release deltas separately:

| Released allocation group | 320 apps (MiB) | 2,000 apps (MiB) |
| --- | ---: | ---: |
| Encoded activation tickets | 0.23 | 1.47 |
| Prepared candidate index | 5.64 | 13.57 |
| All-app and visible-app lists | 1.45 | 9.03 |
| Parsed desktop entries | 0.85 | 5.31 |
| Provider metadata snapshot | 0.69 | 0.70 |
| Remaining context, including catalogs/translations | 3.10 | 3.31 |

These allocator deltas are not a PSS partition. Allocation counters describe
objects; PSS also reflects resident pages and file sharing. Existing idle trimming
returned about 3.3/13.6 MiB of resident memory in these diagnostic processes;
unused arena bytes were not all resident. See the [glibc allocation statistics](https://sourceware.org/glibc/manual/latest/html_node/Statistics-of-Malloc.html)
and [kernel PSS documentation](https://docs.kernel.org/filesystems/proc.html).

The fix shares immutable application records across parsed/all/visible lists,
including no-op rescans. It reuses at most 256 prepared emoji rows during typing
and releases that cache on the existing provider-retirement event before trimming.
The full CLDR index stays in the idle-exiting worker. Frozen output files remain
unchanged; the cache also checks changed row contents, not just emoji identity.

The uninstrumented comparison runs A/B/A/B on nested niri, two runs per build and
catalog size. Each uses 100 warm-ups and 2,000 completed sequential `a`/`ap`
queries, then 35 seconds to retire workers and 60 seconds idle. Everything is
built first. CPU uses the same `/proc` user+system tick accounting (0.005 ms/query
quantum per process); round trips include client JSON decoding. The new helper is
`c803fe026a935049c1f19eda85c828d7ae3d081b24b9176a94ea6a790e39cd18`.

| Metric, median of two runs | 320 before | 320 after | 2,000 before | 2,000 after |
| --- | ---: | ---: | ---: | ---: |
| Daemon CPU ms/query | 1.543 | 0.743 | 1.733 | 0.938 |
| Provider CPU ms/query | 0.343 | 0.285 | 0.348 | 0.298 |
| Combined CPU ms/query | 1.885 | 1.028 | 2.080 | 1.235 |
| Round-trip p50 ms | 1.983 | 1.107 | 2.183 | 1.300 |
| Round-trip p99 ms | 2.287 | 1.287 | 2.724 | 1.641 |
| Settled daemon RSS MiB | 37.14 | 35.76 | 61.97 | 52.48 |
| Settled daemon PSS MiB | 33.52 | 32.17 | 58.34 | 48.85 |

Combined CPU run ranges are 1.880–1.890 → 1.020–1.035 ms at 320 apps and
2.065–2.095 → 1.225–1.245 ms at 2,000. PSS ranges are
32.59–34.45 → 31.26–33.08 MiB and 58.24–58.44 → 48.81–48.89 MiB.
The first 320-app pair rose 0.49 MiB; the unchanged baseline then varied
1.86 MiB, and the second pair fell. Its overlapping ranges do not establish a
repeatable regression or a precise small memory improvement. The 2,000-app
reduction is outside that spread. Idle is one helper process/thread with zero
observed CPU ticks or voluntary switches in every 60-second sample, not a claim
of zero CPU at finer resolution or a three-hour soak. The provider exits when idle.

All measured SwapPss and host swap-out counts are zero. Host swap-in pages are
listed as query/idle, in execution order; each page is 4,096 bytes:

| Run | 320 apps | 2,000 apps |
| --- | ---: | ---: |
| A1 baseline | 1 / 7 | 0 / 4 |
| B1 candidate | 0 / 16 | 0 / 6 |
| A2 baseline | 0 / 36 | 0 / 7 |
| B2 candidate | 0 / 16 | 0 / 5 |

Every interval meets the owner's measured-process SwapPss-zero and host-in-under-
1-MiB rule. These are desktop-PC nested component measurements, not laptop or
physical UI numbers. They meet the under-2-ms warm query CPU target on this host;
Bead's wider input-to-presentation clock, another host, and the first query after
a worker has exited still need separate evidence. No per-key catalog/metadata
publication was found in this diagnostic. No UI, compositor connection or
permission policy changed for this fix.

A separate high-resolution process-clock probe confirms both broad prefixes
return 101 rows with the same fixture. Across 200 warm calls it measures daemon
1.539 → 0.735 ms plus provider 0.342 → 0.294 ms. After 35 seconds idle, with no
worker left, one first query per build takes 18.30 → 18.33 ms round trip:
daemon 3.04 → 3.14 ms, new worker lifetime CPU 14.98 → 14.93 ms. Host swap-in and
measured SwapPss are zero. This is a single cold-start diagnostic, not a latency
distribution. Worker startup/CLDR initialization remains outside the under-2-ms
warm target; the cache fix does not remove it. Keeping the full index resident
would change the memory tradeoff, so this audit preserves the 30-second worker
policy. No physical panel or presentation timestamp is measured by these probes.

Final validation: fmt/clippy, 335 Rust tests, 991 unchanged launcher reference
cases, schema freshness, 929 repository tests (three existing skips) and 534
helper parity cases pass. The initial full run had one curses PNG-redraw test
failure; five focused baseline runs, five candidate runs and the unchanged full
repeat passed. No test deadline, expected output or production curses code changed.
Private audit evidence archive SHA-256:
`7de916c7d183219d5736e6a047e2ad77f7bc99c0c98df43c497bdd976ec94feb`.

### Foundation niri-widths fold-in (desktop-PC nested, 2026-10-16)

Before is `75157fd3`, helper
`c803fe026a935049c1f19eda85c828d7ae3d081b24b9176a94ea6a790e39cd18`;
the final widths helper is
`801ebe7ae474129291bc85431a8755fa9432ca96d194dac94825856ab1f6b970`.
Five alternating A/B pairs use nested niri and a dedicated authenticated Xvfb,
320 fixture apps, 35 seconds settling and 60 seconds idle. No broad query or
CLDR worker runs during these samples. All binaries are built before capture.
This measures idle observers; real X11 hint I/O and the exact proportional request
are tested separately with private Xvfb and a controlled niri transport.

| Combined daemon + legacy observer/client | Before median (range) | After median (range) |
| --- | ---: | ---: |
| RSS MiB | 45.05 (44.76–46.96) | 45.35 (45.04–49.54) |
| PSS MiB | 32.81 (32.43–34.79) | 32.56 (32.10–36.55) |
| Helper processes | 2 | 2 |
| Idle threads | 3 | 2 |
| Observed CPU ticks / 60 s | 0 | 0 |
| Voluntary switches / 60 s | 0 | 0 |

PSS pair differences are −0.52, −2.08, +4.13, +0.18 and −2.22 MiB. The ranges
overlap: this does not establish a memory saving or a reproducible increase.
The high candidate boot remains in the comparison. There is no observed idle
wakeup regression at this resolution, not a three-hour flat-memory claim. The
stateful observer moved into the daemon; the small foreground compatibility
client stays for one release. Native entry-point tests prove it opens no extra
niri event/request sockets, including while disconnected and reconnecting.

Measured SwapPss and host swap-out are zero in every run. At 4,096 bytes/page,
host swap-in pages for A/B are 7/41, 3/4, 3/2, 3/7 and 20/3. All intervals meet
the measured-process-zero / host-in-under-1-MiB gate.

The first prototype had mixed PSS differences (−0.19 and +2.00 MiB). The focused
follow-up avoids cloning full desktop snapshots for connection checks, drops
unused client handshake metadata and trims it once on connection. It also keeps
standalone compatibility without XDG_RUNTIME_DIR. Prototype reports were retained;
only the final helper above supplies the five-pair table.

The existing Qt cold/reopen frame probe also ran A/B/A/B with the same staged
`75157fd3` QML on both sides and only the helper binary changed. Daemon and width
observer/client are warmed before UI startup, both included in process/paging
audits; the dedicated private Xvfb is not the owner's display. Later video cases
are omitted exactly as in the earlier step audit. This is nested frame latency,
not physical display latency or an in-process launcher-engine benchmark.

| Frame time ms, two runs | Before | After |
| --- | ---: | ---: |
| Cold launcher | 480 / 455 | 423 / 462 |
| Reopened launcher | 357 / 342 | 352 / 357 |
| Cold dashboard | 363 / 362 | 363 / 363 |
| Reopened dashboard | 362 / 363 | 362 / 362 |

The ranges overlap; neither a reliable launcher speedup nor a repeatable slowdown
is established. This does not explain the older Python-to-Wave-3 cold-launcher
interval. The process audit confirms daemon, widths command and production
supervisor before input on both sides. Measured-process SwapPss is zero; host
swap-in for A1/B1/A2/B2 is 55/1/0/0 pages (4,096 bytes), swap-out zero. Every run
passes the revised gate. Wallpaper/palette timing endpoints are unchanged by
this fold and were not remeasured here.

Validation: fmt/clippy and schema freshness, 340 Rust tests, 991 unchanged launcher
references, 929 repository tests (three existing skips), and 534 helper parity
cases pass. The existing 22 CLI, 25 width-policy and 19 Xlib reference cases stay
byte-exact. New tests cover raw-event order/nulls/gaps, real private X11 hint
restoration and one-shot requests, actual native clients sharing connections,
explicit-session rejection, standalone fallback without a runtime directory, and
kill/restart/reconnect without opening a standalone event stream. No live desktop
or unit activation was needed.
Private widths evidence archive SHA-256:
`6461b07b16008a10a850f6383a3dd1c7579886a979850d4371a8131d1a2feb07`.


### Foundation hosted-lane prototype (desktop-PC nested, 2026-10-16)

The unpublished serve prototype uses published `1e920ea4` as its before build
(helper `801ebe7ae474129291bc85431a8755fa9432ca96d194dac94825856ab1f6b970`).
Prototype helper SHA-256 is
`c0035df10b081b38caa7ab8ab9ccf4191f557cec4f3aa5ab0cce1042c2f53cf4`.
Two alternating niri pairs use 320 fixture apps, two warm six-tool rounds,
ten calls per lane, 35 seconds settling and 60 seconds idle. The combined group
includes the daemon and retained `serve` compatibility process; providers must
have exited before idle. Private D-Bus has no NetworkManager: that lane measures
the exact unavailable error, not a successful network lookup. No live bus or
network is queried. This is not the `menu.query` keystroke benchmark.

| Prototype comparison, two runs | Before | Prototype |
| --- | ---: | ---: |
| Combined PSS MiB | 38.53 / 38.25 | 34.28 / 35.93 |
| Resident helper threads | 9 / 9 | 3 / 3 |
| Observed CPU ticks / 60 s | 0 / 0 | 0 / 0 |
| All-thread voluntary switches / 60 s | 0 / 0 | 0 / 0 |
| Full catalog reply median ms | 0.81 / 0.85 | 1.82 / 1.75 |
| Full catalog helper-tree CPU ms/call | 0.645 / 0.702 | 1.707 / 1.611 |

Prototype measured SwapPss and host swap-out are zero in all four runs.
At 4,096 bytes/page, warm host swap-in is zero; idle A/B pages are 2/0
and 60/0. All intervals meet the revised paging gate.

The catalog response slowdown repeats outside the observed spread. A separate
100-call warm profile of the same 62,249-byte stdout measures 0.619 → 1.585 ms
helper-tree CPU: prototype daemon 0.605, bridge 0.397 and worker 0.583 ms/call.
This justified one focused encoding attempt: avoid throwaway full encodes for
small replies, retain exact size checks near the limits, and emit legacy JSON
spacing/ASCII escapes during serialization. The revised artifact is
`d9ddfd69905129c7463fabf723fa200a5393872a1da5e07685925c1d755c86a0`;
its 348 Rust, 929 repository (three existing skips), 534 immutable helper parity,
format/lint and schema-freshness gates pass. Matched 100-call profiles after the
focused attempt measure niri CPU 0.652 → 1.156 ms and median round-trip
0.804 → 1.261 ms; Hyprland CPU 0.621 → 1.114 ms and median round-trip
0.759 → 1.254 ms. All measured SwapPss and host swap-in are zero, with actual
owned-process nice 10 recorded. The fix reduces prototype CPU, but the roughly
0.5-ms overhead still repeats. The owner performance stop rule initially held the fold. After the owner said
"continue" in response to the measured hold, the compatibility-path overhead is
accepted for this step and remains tracked. Two revised niri A/B idle pairs are
complete; Hyprland idle remains unmeasured. The cold-frame follow-up is below.
This is no evidence of a `menu.query` typing improvement.
The first prototype Hyprland attempt failed during nested backend creation and
produced no measurement. A GLES parent is required for its repeat; no live
session, driver, package or host policy was changed.

The host's active process-priority policy can override inherited shell priority.
The revised private probes explicitly set and record actual helper nice values
before CPU sampling. Missing assets and the expected private-network error were
corrected in the private fixture before any accepted prototype capture. Frozen
reference outputs remain unchanged. Whole-shell action, physical-display and
battery results are not supplied by this component comparison.


The focused candidate's two niri A/B pairs retain the same settling/idle method:

| Revised combined group, two runs | Before | Focused candidate |
| --- | ---: | ---: |
| RSS MiB | 53.78 / 53.96 | 46.91 / 48.03 |
| PSS MiB | 38.26 / 38.16 | 33.78 / 34.84 |
| Helpers / resident threads | 2 / 9 | 2 / 3 |
| Observed CPU ticks / 60 s | 0 / 0 | 0 / 0 |
| All-thread voluntary switches / 60 s | 0 / 0 | 0 / 0 |

PSS pair differences are −4.48 and −3.32 MiB. All four revised intervals are valid:
measured SwapPss and swap-out zero; warm host swap-in zero; idle host A/B swap-in
6/2 and 7/1 pages at 4,096 bytes/page. Providers retire before idle. The remaining
compatibility process is intentional for one release; this is not the final
one-process UI configuration, three-hour flatness or a battery measurement.

The same revised niri 100-call profile gives the full six-lane comparison:

| Helper operation | CPU ms/call before → after | Median round-trip ms before → after |
| --- | ---: | ---: |
| menu-backend state | 0.652 → 1.156 | 0.804 → 1.261 |
| dashboard-data read | 0.044 → 0.087 | 0.048 → 0.091 |
| dashboard-media empty selection | 0.017 → 0.061 | 0.027 → 0.068 |
| launcher-preferences load | 0.231 → 0.035 | 0.218 → 0.039 |
| notification-history load | 0.017 → 0.060 | 0.029 → 0.068 |
| network-state unavailable | 0.019 → 0.066 | 0.032 → 0.075 |

CPU includes daemon, bridge and provider processes. These are warm helper calls,
not UI actions or cold worker starts. Preferences benefit from joining the existing
in-memory writer. Other lanes gain a bounded worker and socket hop; the catalog
carries the largest response. The focused code removes redundant encodes but
retains that transport cost. No additional Python launch reduction is claimed:
these six operations were already native before the foundation fold.

The fold's measured compatibility overhead is retained after the owner continuation;
the main repair and fold are now published with fully green CI.
Private evidence archive SHA-256:
`d2a9fa68770e6f1a91a128a9953d307a0c610fec889a3266d3e36fcf24042c89`.
Revised source manifest/archive SHA-256:
`97fb5f6aa2f93ae5246ea7358315fd5f983e78cec3b720523527cc394c4473fe`.
No reference fixture was regenerated. After stopping the capture, the scoped
private-HOME/runtime audit found no remaining lab processes. Main advanced
independently after tested base `1e920ea4`; the two account-help differences now
have exact declared comparison rules without modifying frozen outputs. Main's
repair is `2893a96f`; Source CI38931641268 is fully green (desktop and
fresh-install). The fold is published as `92cb312c`; Source CI38942501196
is fully green in both jobs.
The integrated source recheck passes all 938 repository cases (three existing
skips), native workflow references and 534 helper cases, including the two exact
account-help extensions; the original frozen expected files are unchanged.
Private integrated-check evidence SHA-256:
`98f7cd0cf3f6a63e33943d8cbd48301ba2aa2330031f1873d74de8de0a526aa5`.


The accepted follow-up uses the existing cold/reopen Qt frame probe, unchanged
staged `75157fd3` QML on both sides, the helper hashes above, nested niri and a
private Xvfb. Daemon and width observer/client are warmed before shell startup.
No builds run during capture. This is frame presentation in a nested compositor,
not physical display latency or a `menu.query` per-key measurement.

| Frame time ms, A/B/A/B | Before | Focused candidate |
| --- | ---: | ---: |
| Cold launcher | 483 / 460 | 438 / 471 |
| Reopened launcher | 360 / 360 | 362 / 362 |
| Cold dashboard | 362 / 361 | 363 / 362 |
| Reopened dashboard | 362 / 362 | 362 / 363 |

The cold-launcher ranges overlap; no repeatable cold slowdown or reliable speedup
is established by these pairs. All four paging gates pass, measured-process
SwapPss is zero. At 4,096 bytes/page, host swap-in A1/B1/A2/B2 is 0/0/2/1 pages;
swap-out is 0/0/51/8 pages. The owner gate constrains measured SwapPss and host
swap-in, so those swap-out counts are retained rather than silently omitted.
Cold evidence archive SHA-256:
`bb76629d6af13e26cab769e9d225247a3e2737ff936e708e23ba461c97f2dea8`.


### Foundation network-watch prototype (desktop-PC nested, 2026-10-16)

Before is published `92cb312c`, helper
`d9ddfd69905129c7463fabf723fa200a5393872a1da5e07685925c1d755c86a0`.
The unpublished prototype is
`d17a48fe31469a988e785bc29b3a3ee3af181098c71555451ad2e1b40b9d1bff`.
Each compositor has two alternating pairs with 320 fixture apps, a private
D-Bus without NetworkManager, 35 seconds settling, 200 synthetic owner-change
signals and 60 seconds idle. CPU sums daemon and watcher; fixture `gdbus` CPU
is excluded. Providers retire before idle. No build/test runs during capture.
The compatibility command remains, so helper count is 2 → 2, not yet one.

| Measure, two runs | Before | Prototype |
| --- | ---: | ---: |
| niri combined RSS MiB | 48.24 / 48.21 | 45.37 / 47.02 |
| niri combined PSS MiB | 34.03 / 33.73 | 33.83 / 35.52 |
| Hyprland combined RSS MiB | 48.97 / 48.26 | 47.43 / 46.21 |
| Hyprland combined PSS MiB | 34.54 / 33.96 | 35.72 / 34.59 |
| Resident threads, either compositor | 6 / 6 | 2 / 2 |
| All-thread voluntary switches / 60 s | 126 / 126 | 0 / 0 |
| Observed CPU ticks / 60 s, 100 Hz | 0 / 0 | 0 / 0 |
| niri helper CPU ms/source event | 0.03594 / 0.03613 | 0.04311 / 0.04479 |
| Hyprland helper CPU ms/source event | 0.03354 / 0.03499 | 0.04387 / 0.04510 |

The prototype does not pass the memory gate: both Hyprland PSS pairs increase,
by 1.18 and 0.63 MiB; niri is mixed. Lower RSS and idle activity do not waive PSS.
One focused attempt removes redundant service-status publication on every signal;
its correctness and matched measurements are pending. Cold-frame measurement
has not run; no launcher, battery, physical-network or typing benefit is claimed.
No additional Python launch reduction: both sides already use native helpers.

All accepted measured SwapPss values are zero. At 4,096 bytes/page, accepted idle
host swap-in A1/B1/A2/B2 is 54/7/6/68 pages on niri and 5/118/3/0 on Hyprland;
swap-out is zero. Signal intervals have zero host swap-in/out except niri A1,
with eight pages in. The original niri B2 is discarded: 1,641 host pages in
(6.41 MiB), despite measured SwapPss zero. Its replacement is retained separately.
The first Hyprland setup failed before capture because the private harness used
`gles` rather than `gles2`; no source, driver or live configuration changed.

Prototype correctness: 351 Rust tests, 938 repository cases (three existing skips),
534 immutable helper parity cases, formatting, Clippy and schema freshness pass.
Frozen watch reference SHA-256:
`5c3a54d847556bf81100c1aac0ff882fd6c6edefe3b8c54fede339e2a8f488eb`.
Private correctness archive SHA-256:
`4b1c4f7cef822df717a305e55c3079c43d837ec9652dacfdd3eb4f8fb928a10d`.
Private pairs archive SHA-256:
`b427bc7048063a198090883a568be79eb7d9a93bddc9c03b7f97d3a2b7600a81`.


The focused attempt publishes service readiness once per reader; it changes no
reference outputs. Its helper is
`1f2bc758236c3e485bb31427def1db3cad0f6e3bcd9f503b2bc1b40b6eb77e90`,
with the same before helper above. Two new niri A/B pairs add per-role anonymous,
file and shared-memory PSS sampling, retaining the same timing and fixture method.

| Focused comparison, two runs | Before | Focused candidate |
| --- | ---: | ---: |
| Combined RSS MiB | 48.36 / 48.50 | 47.03 / 47.68 |
| Combined PSS MiB | 33.90 / 33.94 | 35.36 / 36.08 |
| Daemon anonymous PSS MiB | 16.64 / 16.63 | 17.85 / 18.86 |
| Watcher anonymous PSS MiB | 2.14 / 2.14 | 2.14 / 2.14 |
| Helpers / resident threads | 2 / 6 | 2 / 2 |
| All-thread voluntary switches / 60 s | 125 / 126 | 0 / 0 |
| Observed CPU ticks / 60 s, 100 Hz | 0 / 1 | 0 / 0 |
| Helper CPU ms/source event | 0.03560 / 0.03519 | 0.03630 / 0.03619 |

PSS increases by 1.46 and 2.14 MiB. The two-run median is 33.92 → 35.72 MiB
(+1.80 MiB, 5.3%); the ranges do not overlap. Anonymous daemon memory accounts
for most of the increase, but live allocations versus allocator retention remain
unresolved; this is not evidence of a leak or a three-hour trend. The CPU cost is
closer to baseline after removing redundant status work, but memory still fails.
At this point the owner stop rule held the fold locally. No network-watch code was
pushed, and wallpaper-watch/F3/F4 were not started. The following Hyprland baseline
was interrupted with scoped cleanup; it supplies no accepted measurement. The
focused Hyprland pairs and cold-frame follow-up remain unmeasured, not zero.

All four focused niri intervals pass paging: measured SwapPss zero, signal host
swap-in/out zero, idle host swap-in A1/B1/A2/B2 86/6/6/7 pages at 4,096 bytes/page,
swap-out zero. Actual owned helper nice 10 is recorded. All 351 Rust tests,
938 repository cases (three existing skips), 534 immutable helper parity cases,
formatting, Clippy and schema freshness pass again. No frozen output changed.
The final private capture cleanup verified zero surviving tracked processes;
the desktop checkout and immutable artifacts remain for the next comparison.
Focused source archive SHA-256:
`45c15999b6c96f8acbb76bf8bfabc62c0503207baaf7b20b854450cb043890a4`.
Focused correctness archive SHA-256:
`71689af0ceea9ab5526b73ef1fe59addeba6120a2073fd2a4c6ef69e4bddaa3e`.
Focused pairs archive SHA-256:
`78f417ca444d7c142ce871daf5be8ea7031bf45ffc8b2f1e25dc221c0a9217f4`.

Follow-up diagnostics keep that hold in place. A full-minute trace of the
unchanged baseline also shows anonymous PSS growing 16.64 → 18.78 MiB while
daemon CPU time stays constant. A subsequent candidate trace directly correlates
17.77 → 19.40 MiB anonymous PSS with anonymous huge pages increasing 0 → 14 MiB,
again without daemon CPU work. These observations implicate background kernel
page promotion in the short-settle comparison
([kernel THP documentation](https://www.kernel.org/doc/html/latest/admin-guide/mm/transhuge.html));
they do not establish a leak or
clear the memory gate. In a diagnostic control disabling huge pages only in the
new test children, both builds stay flat, with combined PSS 34.96 → 35.27 MiB.
All four control cells have measured SwapPss zero; idle host swap-in is
69/12/5/3 pages (normal before/after, disabled before/after), swap-out zero,
4,096 bytes/page. These are diagnostic results, not acceptance or timing data.
No product, global kernel policy or live-session setting changed.

The follow-up normal-policy A/B/A/B comparison uses exactly seven warm catalog
queries per cell, followed by at least 180 seconds of additional idle settling
and 90 seconds of stable anonymous PSS/huge-page counts before the 60-second
measurement. Continued promotion invalidates that cell. This method change is
explicit: the earlier 35-second-settle results above remain unchanged.

Both compositor comparisons completed, all eight cells valid on their first
attempt. Actual additional settlement was 180 seconds in seven cells and
215 seconds in the first Hyprland candidate. Normal huge-page policy remained
enabled in every measured helper; no debugger, allocator tuning or process-memory
policy override was used. The before and candidate binaries are unchanged.

| Settled comparison, A1/A2 or B1/B2 | Before | Candidate |
| --- | ---: | ---: |
| niri combined RSS MiB | 52.39 / 52.31 | 49.70 / 48.92 |
| niri combined PSS MiB | 37.96 / 38.10 | 37.99 / 37.23 |
| Hyprland combined RSS MiB | 51.67 / 52.57 | 47.27 / 49.42 |
| Hyprland combined PSS MiB | 37.52 / 38.28 | 35.69 / 37.68 |
| Helpers / resident threads, both compositors | 2 / 6 | 2 / 2 |
| All-thread voluntary switches / 60 s, every cell | 120 | 0 |
| Observed CPU ticks / 60 s, 100 Hz, every cell | 0 | 0 |
| niri helper CPU ms/source event | 0.03526 / 0.03501 | 0.03687 / 0.03631 |
| Hyprland helper CPU ms/source event | 0.03510 / 0.03460 | 0.03709 / 0.03515 |
| Python launches on this already-native path | 0 | 0 |

The earlier persistent +1.80 MiB increase does not reproduce. PSS ranges overlap;
the two-run median is 38.03 → 37.61 MiB on niri and 37.90 → 36.69 MiB on
Hyprland. The synthetic source-event path adds approximately 1–2 microseconds
of helper CPU per event; it is not a typing or physical-network benchmark.
Compatibility clients still account for one of the two helper processes.
This clears the combined-memory/idle gate with the method correction above;
cold launcher verification is separate. No battery or three-hour claim is made.

Measured SwapPss is zero in all cells. Idle host swap-in A1/B1/A2/B2 is
4/0/1/0 pages on niri and 0/0/1/0 on Hyprland; all idle swap-out and all
source-event swap-in/out are zero, at 4,096 bytes/page. Actual helper nice 10
and the absence of idle provider children are verified. Original short-settle
and diagnostic captures remain retained separately from these accepted runs.

The cold niri follow-up uses the same frozen `75157fd3` QML/harness sources and
the two exact helper binaries above, in A/B/A/B order. It measures Qt key-handler
receipt to the settled `frameSwapped` boundary, excluding compositor presentation.
No builds, tests or other captures ran alongside these measurements.

| Panel frame ms, A1/A2 or B1/B2 | Before | Candidate |
| --- | ---: | ---: |
| Cold launcher | 449 / 462 | 460 / 424 |
| Reopened launcher | 360 / 358 | 364 / 347 |
| Cold dashboard | 362 / 363 | 363 / 363 |
| Reopened dashboard | 367 / 362 | 363 / 362 |

Launcher ranges overlap; the 455.5 → 442 ms two-run medians do not establish
a speedup. No growing cold-launcher regression reproduces in this fold.
All four captures have measured SwapPss zero and no paging-gate failures;
host swap-in A1/B1/A2/B2 is 4/0/0/36 pages, swap-out zero, 4,096 bytes/page.
Correctness remains 351 Rust, 938 repository cases (three existing skips),
534 immutable parity cases, formatting, Clippy and schema freshness passing.
All 13 source/protocol/reference hashes still match the tested source archive;
only documentation changed during the memory diagnosis. No frozen output changed.
The normal memory and cold-frame gates are now clear; publication still requires
normal direct integration and full green CI before the next fold.

Private diagnostic, settled-resource and cold-frame evidence archive SHA-256:
`be077edff7de75e633eb757f83c0084fe5171f1eecb5c5e9fe43bb4a5f5aa44b`.
The archive is verified on both hosts. A final command-line/environment audit
found zero surviving private capture processes; the isolated lab is retained.

### Foundation wallpaper-watch fold (desktop-PC nested, 2026-10-16)

This moves the existing native filesystem watcher into the daemon. It does not
change image decoding, palette publication or the 500 ms debounce. The baseline
is `6a737629` with helper SHA-256
`1f2bc758236c3e485bb31427def1db3cad0f6e3bcd9f503b2bc1b40b6eb77e90`;
the candidate source archive is
`6467a1475c81870c1169f4de9d74e980cd86cb77c6f68ab1dc5c674aaaf3134c`,
and its release helper is
`d8f77affca7b9f2263ed85752c8343d013163788cdc8608aa5335b9b1776b8d6`.
All 391 crate/protocol source hashes match on the editing and measurement hosts.

Each compositor runs A/B/A/B with the same 320-app fixture, exactly seven warm
catalog queries, 35 seconds for worker retirement, and 20 debounced changes.
Each change consists of three quick image-file writes and must produce one exact
legacy JSON line. Normal memory policy is unchanged. At least 180 seconds of
additional settlement and 90 seconds of stable anonymous PSS/huge-page counts
precede each 60-second idle sample. One Hyprland baseline needed 200 seconds;
the other seven cells needed 180. No builds, tests or other captures ran alongside.
All eight cells are valid on their first attempt.

| Settled comparison, A1/A2 or B1/B2 | Before | Candidate |
| --- | ---: | ---: |
| niri combined RSS MiB | 49.48 / 47.79 | 49.63 / 49.86 |
| niri combined PSS MiB | 37.76 / 36.48 | 37.03 / 37.46 |
| Hyprland combined RSS MiB | 49.06 / 49.15 | 48.82 / 51.06 |
| Hyprland combined PSS MiB | 37.30 / 37.19 | 36.06 / 38.34 |
| Helpers / resident threads, both compositors | 2 / 2 | 2 / 2 |
| All-thread voluntary switches / 60 s, every cell | 0 | 0 |
| Observed CPU ticks / 60 s, 100 Hz, every cell | 0 | 0 |
| niri helper CPU ms/debounced change | 0.10603 / 0.11436 | 0.23199 / 0.23796 |
| Hyprland helper CPU ms/debounced change | 0.09818 / 0.10238 | 0.22639 / 0.23349 |
| Python launches on this already-native path | 0 | 0 |

Combined PSS ranges overlap on both compositors: two-run medians are
37.12 → 37.25 MiB on niri and 37.25 → 37.20 MiB on Hyprland. These runs do
not establish a memory improvement or a persistent PSS increase. Combined RSS
medians are higher by 1.11/0.84 MiB respectively; RSS counts shared pages in each
process, whereas PSS apportions them. Both totals are retained above rather than
presenting daemon-only memory as the cost of the whole path. No growing idle
activity reproduces. Zero observed ticks are limited by the 100 Hz resolution;
this is neither a battery nor a three-hour measurement.

The new publication/compatibility path adds about 0.12–0.13 ms of helper CPU per
debounced change. This synthetic probe measures event handling, not decoding,
palette application or a displayed frame. One compatibility client remains until
the shared QML connection replaces it in F3, so this step alone does not reduce
the two-process count. The baseline already uses native code and inotify.

Measured SwapPss is zero in all idle and change probes. Idle host swap-in
A1/B1/A2/B2 is 2/7/4/5 pages on niri and 1/2/37/5 on Hyprland. Change-probe
swap-in is 0/0/2/0 and 0/0/0/0 respectively; all swap-out is zero, at
4,096 bytes/page. Every measured helper runs at nice 10 without idle provider
children. Frozen watcher outputs remain unchanged. Correctness passes:
355 Rust, 938 repository cases, 534 immutable parity cases, formatting,
Clippy and committed-schema freshness.

The cold niri follow-up uses the same frozen `75157fd3` QML/harness and exact
helper binaries, in A/B/A/B order. Its boundary is Qt key-handler receipt to
the settled `frameSwapped` signal, excluding compositor presentation.

| Panel frame ms, A1/A2 or B1/B2 | Before | Candidate |
| --- | ---: | ---: |
| Cold launcher | 471 / 441 | 424 / 441 |
| Reopened launcher | 367 / 357 | 358 / 362 |
| Cold dashboard | 362 / 362 | 362 / 362 |
| Reopened dashboard | 362 / 362 | 362 / 362 |

The launcher ranges meet at 441 ms; two-run medians of 456 → 432.5 ms do not
establish a speedup. No growing cold-launcher regression reproduces. All four
captures have measured SwapPss zero and no paging-gate failures. Host swap-in
A1/B1/A2/B2 is 5/101/0/1 pages, all swap-out zero, at 4,096 bytes/page. No
physical-desktop presentation, GTK endpoint or battery claim is added.

Private resource/cold/correctness evidence archive SHA-256:
`9477126532b7a879b31dd8c3d275c6fb0c1359bc9da97e7e151848181c333559`.
Its 57 files are archived and checksum-verified on both hosts. A final scoped
process/environment audit finds zero surviving private capture processes.


### F3 arithmetic provider (desktop-PC nested, 2026-10-17)

The calculator grammar now runs inside `menu.query`, including its literal-copy
activation ticket. It preserves 313 immutable outputs captured from actual
Quickshell 0.3.2: 217 original cases, 33 numeric/whitespace edges and a separately
frozen 63-case power matrix. That matrix caught native `powf` returning 1 for
`1 ^ NaN`; the port now keeps JavaScript's undefined-result error. Existing
helper, ranking and wire expectations were not regenerated. A preview ending
halfway through a UTF-16 surrogate pair uses U+FFFD to retain valid UTF-8 on the
private link; this intended difference is documented in the library README.

This is an arithmetic component profile, not the 101-result ranking workload or
a QML before/after comparison. The QML calculator and ranking fallback are still
unchanged. `tools/benchmark-menu.py` uses the existing private nested lab, 320 and
2,000 synthetic applications, 100 warm requests, then 20,000 sequential calls
cycling four arithmetic queries. CPU is process user+system ticks divided by
calls; round trip is client send through matching decoded reply. Every binary
was built before capture, with no concurrent builds or tests. Existing host
scheduler policy stayed enabled; audits observed the benchmark, native daemon
and worker at nice 10, while wrapper/compositor priorities followed host rules.

| Compositor / apps | Daemon CPU/query (ms) | Round trip p50 / p99 (ms) | Settled daemon RSS / PSS (MiB) |
| --- | ---: | ---: | ---: |
| niri / 320 | 0.0150 | 0.02185 / 0.03004 | 34.31 / 30.74 |
| niri / 2,000 | 0.0175 | 0.02508 / 0.03127 | 57.99 / 54.43 |
| Hyprland / 320 | 0.0155 | 0.02353 / 0.02960 | 33.75 / 30.15 |
| Hyprland / 2,000 | 0.0175 | 0.02561 / 0.03098 | 57.52 / 53.90 |

Provider CPU during the warm query loops was zero observed ticks; the accounting
quantum is 0.0005 ms/query, not proof of literally zero work. Busy daemon plus
metadata-worker RSS/PSS was 60.59/44.20 and 82.72/66.67 MiB on niri;
60.36/44.59 and 81.70/65.74 MiB on Hyprland. After 35 seconds for retirement,
each 60-second idle capture had one process/thread, zero observed CPU ticks and
zero main-thread voluntary switches. These are finite counters, not a battery,
three-hour stability or exact kernel-wakeup claim. The index remains resident;
no before/after memory saving is established by this current-build profile.

All four query/idle pairs pass: measured SwapPss zero, page size 4,096 bytes.
Host swap-in query/idle deltas were niri 0/9 and 0/5, Hyprland 0/18 and 0/1;
host swap-out was zero throughout. Python execs were not traced: launch count
is unmeasured, not zero. The old calculator already ran in JavaScript; this
change does not claim to remove a Python launch or establish a UI speed gain.

The separate cold niri A/B/A/B gate uses the unchanged shell/harness at
`75157fd32fdea26ef8defcfb0430e39be439fd89`, comparing the typed-provider helper
`ee12053478b17a786b73c11d95915ab3300eddd271e7c537a0f15f96f45ec359`
with arithmetic helper
`2d66feef2bc80c8d45c293c2fd9cc193edbcdab1e966612e8d9eb3d17bff1ebb`.

| Frame-ready action (ms) | Before runs | Arithmetic runs |
| --- | ---: | ---: |
| Cold launcher | 470 / 439 | 442 / 462 |
| Reopened launcher | 357 / 358 | 344 / 357 |
| Cold dashboard | 362 / 363 | 362 / 363 |
| Reopened dashboard | 362 / 362 | 367 / 362 |

Cold-launcher ranges overlap (439–470 versus 442–462 ms); medians 454.5 → 452 ms
do not establish a reliable speedup or repeatable regression. These are nested
frame observations, not physical presentation latency. All four captures have
measured SwapPss zero; host swap-in A/B/A/B is 47/1/0/1 pages, swap-out zero,
within the owner's under-1-MiB gate. Original captures remain retained.

Validation: 367 Rust tests, 938 repository tests (three existing skips), all 534
immutable helper parity cases, fmt/Clippy/schema freshness/examples/minimal
consumer pass on the same 399-file runtime/protocol manifest. Accepted evidence
archive SHA-256:
`3d29f8dc09da99cd948968c89ec87185cedbf97736a7b7d423fd0b11c3c6f784`
(37 files, 147,757 bytes), copied and SHA-verified on both hosts. Private capture
process audit is empty; the lab is retained, with no live desktop/configuration,
units, kernel policy or installed-runtime changes. Published CI remains the
next gate before the native attach/shared-QML work.


## Foundation F3 shared QML client: paired gate held, 2026-10-17

Historical gate: the original candidate was held for a repeated Hyprland
memory increase. The expanded whole-group follow-up below clears that hold.
The client is opt-in (`NACRE_DAEMON_CLIENT=1`); the legacy
path remains the default. No installed runtime or live session was changed.

The frozen before revision is `e0a81ce76758de4831f008b90cb37b54dacc0baf`,
helper SHA-256
`2d66feef2bc80c8d45c293c2fd9cc193edbcdab1e966612e8d9eb3d17bff1ebb`.
The unpushed candidate is `00d2430b381893b6f06acb2f17fb76f696ace854`,
helper SHA-256
`58d4f0efa0d8e62648f47a1618800405e05b1270c5a1d5f29e95ba2109195d81`.
The baseline already contains the earlier native ports; it is not the original
Python baseline. All 749 baseline and 2,933 candidate source entries matched
before and after their runs. Nothing compiled during measurement.

Method: nested desktop-PC A/B/A/B on each compositor, with cold actions first,
then matched ten-minute warmups and complete sixty-second idle windows. The owner
approved fixed windows after the unchanged baseline also failed the preceding
2-MiB/90-second settling condition; those rejected captures remain invalid.
Measured SwapPss must still be zero and host swap-in strictly below 1 MiB.
Every one of these eight captures passes those checks on its first attempt.

Launcher timing starts immediately before creating the UI command process and
ends at a ready Qt `frameSwapped`: legacy `bash control.sh`/`qs ipc` versus native
`nacrectl ui`. It excludes physical input and compositor presentation. Dashboard
timing retains the frozen F9 Qt-handler boundary and pinning; it must not be
compared as though it includes command startup.

| Nested action, ms | Before median (range) | Native median (range) |
| --- | ---: | ---: |
| Niri cold launcher, two samples | 456.78 (456.07–457.48) | 401.19 (400.47–401.91) |
| Niri reopened launcher, six samples | 380.72 (366.28–392.57) | 282.56 (281.95–303.27) |
| Hyprland cold launcher, two samples | 442.68 (437.32–448.04) | 417.32 (414.13–420.51) |
| Hyprland reopened launcher, six samples | 371.55 (354.48–400.76) | 314.71 (310.03–333.22) |
| Niri cold dashboard, two samples | 390 (389–391) | 357.5 (357–358) |
| Niri reopened dashboard, six samples | 377.5 (365–388) | 375.5 (365–392) |
| Hyprland cold dashboard, two samples | 364 (363–365) | 365 (365–365) |
| Hyprland reopened dashboard, six samples | 365 (364–365) | 364 (363–366) |

The original memory scope includes the shell, its descendants and the explicitly
started daemon/widths roots. Follow-up found that it omits the shell supervisor,
which is a parent process. These resource values remain original-scope evidence;
the owner's expanded whole-group gate requires a new comparison including that
parent and private detached helpers. Each idle value below is the median of two
complete-window medians; ranges are the two window medians, while peaks include
sampled transient update processes. CPU is percent of one core, using the median
of the two window means.

| Nested resource | Before median (window range); observed peak | Native median (window range); observed peak |
| --- | ---: | ---: |
| Niri PSS, MiB | 561.57 (557.59–565.54); 637.42 | 556.97 (556.64–557.30); 621.12 |
| Niri RSS, MiB | 689.37 (685.11–693.63); 784.22 | 671.02 (670.51–671.53); 753.50 |
| Hyprland PSS, MiB | 542.91 (540.45–545.36); 624.90 | 552.90 (551.83–553.97); 635.06 |
| Hyprland RSS, MiB | 658.06 (655.73–660.40); 759.02 | 666.55 (665.21–667.90); 766.60 |
| Niri CPU, % one core | 0.366 (0.349–0.383); 45.60 | 0.357 (0.348–0.365); 38.72 |
| Hyprland CPU, % one core | 0.481 (0.431–0.531); 50.94 | 0.591 (0.481–0.702); 58.18 |

Niri has no observed regression. Hyprland PSS increases in both pairs, by
13.52 and 6.47 MiB; the typical window ranges do not overlap. The increase is
mainly Quickshell: its per-process medians are 501.67/505.60 before versus
513.17/513.21 MiB native. Helper medians vary in both directions, by +2.02 and
−1.18 MiB. This is a measured hold, not an accepted feature cost or an attribution
to kernel huge pages. A focused private allocation/collection diagnostic is next.

The higher Hyprland sampled CPU comes from existing `pacman` update checks:
Quickshell's window means are 0.0993/0.0990 before and 0.0825/0.1013% native;
helper CPU is below the sampler's tick resolution in these windows. Neither
claim means literally zero helper work. Periodic update processes also explain
why transient memory peaks exceed typical idle use. Idle process medians are
7→6 on niri and 5→5 on Hyprland; remaining standalone commands are retained.
Per-thread context-switch counts are a scheduling proxy, not hardware wakeups.

Host swap-in, A/B/A/B: niri 5/19/3/3 pages; Hyprland 10/6/98/8 pages. Pages are
4,096 bytes, all swap-out counts are zero, and measured process SwapPss is zero.
The largest host count is 392 KiB. These are nested frame/resource observations,
not physical presentation, three-hour stability, battery life, or an exec-traced
zero-Python claim. Per-helper call latency and fresh Python launch counts were
not remeasured by this whole-shell comparison.

Correctness on the exact candidate: 384 Rust tests, 950 repository tests (one
existing optional skip), all 534 immutable helper parity cases, fmt, workspace
Clippy with warnings denied, schema freshness, independent JSON Schema validation,
examples and the minimal consumer pass. Original golden outputs were unchanged.
Correctness evidence SHA-256:
`7f99fe171dbaa35c07d2725c6e4a4ccb5ab2d595a0228562977329b588fc939c`.
The eight-run raw comparison archive is 82 files / 386,990 bytes, SHA-256
`c5ed7db87d5d4970ee4ce7cb14dec5d104a0ff5de9b6744149481de53ddb7c41`,
copied and SHA-verified on both hosts. Its private capture process audit is empty.

A separate diagnostic pair enabled Qt allocator logging and explicitly collected
JavaScript after the complete idle window, followed by thirty seconds of sampling.
It is not replacement acceptance evidence: instrumentation changes allocation
behavior. Reserved JavaScript memory was 82,514,432 versus 82,384,896 bytes;
regular live bytes after explicit collection were 18,259,200 versus 18,153,120.
Neither run released whole chunks. Combined typical PSS changed 546.76→546.15 MiB
before and 548.95→550.46 MiB native, so adding `gc()` is not a demonstrated fix.
Both diagnostic runs pass their paging checks (host-in 9/10 pages, out zero).
The diagnostic archive is 28 files / 100,684 bytes, SHA-256
`cff6690ebe537d521504d4a8e7c74c73147b2016ed04964ebe642298773948a5`,
copied and SHA-verified on both hosts.

A normal-logging diagnostic then ran the exact candidate source and helper with
the daemon client disabled, using the same 600-second warmup and complete
60-second window. Combined median PSS was 540.18 MiB (observed peak 622.97),
including Quickshell at 500.45 MiB. These typical values fall within the old
baseline range. This single diagnostic supports an active-client cost rather
than the new source alone; it does not identify an allocation or replace the
normal A/B/A/B gate. Host swap-in was five 4,096-byte pages, swap-out zero, and
measured SwapPss zero. Its private command permitted the existing default-socket
fallback because the UI provider was deliberately disabled; its action timings
are not acceptance measurements. Source and helper hashes matched before and
after. The diagnostic archive is 13 files / 34,816 bytes, SHA-256
`08cfa207c1f2e14dfd1844d852e189f836b32e1314bbbf59995f9fcef6d6533c`,
copied and SHA-verified on both hosts.

The focused investigation found no demonstrated fix. Publication remains blocked
under the regression stop rule: the repeated normal Hyprland increase remains
unexplained, and neither forced collection nor this client-disabled diagnostic
waives it. No product workaround, allocator policy or live configuration changed.
F4 and core-service implementation have not started.

The owner authorized another bounded profiling round, checking whole-group
coverage, duplicate Hyprland compositor subscriptions, view subscription scope
and JavaScript churn in that order. Landing requires an explained QML increase
with equal/lower whole-group memory, or a clearly explained whole-group increase
below 5 MiB. Larger or unexplained increases stop publication.

An actual private UI audit confirms that Hyprland subscribes to `network.watch`
and adds `menu` only while the launcher is open; it holds no daemon compositor
topic. Niri retains compositor topics for its visible workspace/window views.
Network watching also serves the always-visible status indicator. A closed
launcher retained about 7 KB of serialized rows. The candidate now releases
unused topic data after the last unsubscribe and rows after the panel finishes
closing, retaining active-view last-good state and the existing close animation.
The paired follow-up below measures this final candidate; the serialized row
size alone does not establish the cause of the old resident increase.

A separate heaptrack pair records 8,152,132→7,583,208 allocation calls and
1,575,079→1,502,519 temporary allocations; its reported peak tracked heap is
150.38M→152.02M. It profiles Quickshell without its supervisor and uses direct
private IPC, so it is allocation evidence rather than action or whole-group
acceptance data. The additional tracked heap does not explain the original
resident increase. No garbage-collection, allocator or kernel workaround ships.
The 59-file / 584,374-byte analysis receipt has SHA-256
`7ea1bad7099f8f26fcf76354c0dad9e6beb3878a54ec3d3b288873a1cbb67671`;
it and both raw profiles are copied and SHA-verified on both hosts. Final-source
correctness passes (384 Rust, 952 repository and 534 frozen parity cases); the
revised whole-group comparison is running with no builds or tests active.

### F3 expanded whole-group follow-up: gate passes, 2026-10-17

The final measured feature revision is `7775d7cd299db5413a37fdb67bda90a6a4eaa27f`.
The baseline remains `e0a81ce76758de4831f008b90cb37b54dacc0baf`; helper hashes
are unchanged from the preceding table. All eight fixed-window captures pass
on their first attempt. The expanded scope includes the supervisor parent,
its private branch, detached native/Python helpers and known daemon/widths roots,
deduplicated by PID with same-user checks. Unknown/live memory omissions reject
a run. This replaces the smaller scope for the owner's whole-group decision.

Each cell retains the matched ten-minute warmup, complete sixty-second idle
window and original action boundaries. The profiling/code-change round ended
before validation finished; the last scheduled candidate completed separately
after the batch's conservative start-time cutoff. That scheduling exit is retained;
no accepted cell was rerun, no window shortened and no memory/paging rule waived.
No builds, tests or profiling overlapped these captures.

| Nested whole-group resource | Before median (window range); observed peak | Native median (window range); observed peak |
| --- | ---: | ---: |
| Niri PSS, MiB | 569.35 (565.45–573.24); 646.13 | 555.87 (552.13–559.61); 634.65 |
| Niri RSS, MiB | 709.62 (705.67–713.57); 804.83 | 681.01 (677.07–684.95); 777.95 |
| Niri CPU, % one core | 0.55 (0.50–0.60); 44.16 | 0.42 (0.18–0.65); 57.75 |
| Hyprland PSS, MiB | 554.87 (554.59–555.14); 637.60 | 540.69 (535.95–545.43); 627.55 |
| Hyprland RSS, MiB | 682.56 (682.27–682.86); 784.32 | 666.16 (661.34–670.98); 771.98 |
| Hyprland CPU, % one core | 0.22 (0.10–0.33); 48.96 | 0.72 (0.68–0.77); 59.56 |

Whole-group PSS falls by 13.47 MiB on niri and 14.17 MiB on Hyprland; both
pairs are lower on each compositor. QML PSS medians also fall: niri
515.70→505.61 MiB and Hyprland 509.56→494.96 MiB. Median process counts are
10→9 and 8→8 respectively. These are current nested totals, not the final
one-daemon process-count target or proof of zero short-lived Python launches.

The Hyprland CPU mean is higher, retained above. Its observed per-process means
attribute 0/0.250% before versus 0.600/0.666% after to existing periodic `pacman`
work; QML itself is 0.100/0.083% before versus 0.083/0.100% after. Endpoint
voluntary context switches are 331/334→329/321 over sixty seconds. Niri gives
127/133→118/125. These are scheduling proxies, not hardware wakeups or a battery
claim; complete-window CPU/peaks include the transient processes.

| Nested action, ms | Before median (range) | Native median (range) |
| --- | ---: | ---: |
| Niri cold launcher | 460.19 (453.78–466.59) | 395.66 (393.64–397.68) |
| Niri cold dashboard | 372.50 (357.00–388.00) | 389.00 (388.00–390.00) |
| Niri reopened launcher | 380.12 (368.05–384.02) | 376.24 (366.85–381.95) |
| Niri reopened dashboard | 383.50 (355.00–388.00) | 366.50 (355.00–387.00) |
| Hyprland cold launcher | 443.75 (440.50–447.00) | 413.24 (412.76–413.71) |
| Hyprland cold dashboard | 365.00 (364.00–366.00) | 365.00 (365.00–365.00) |
| Hyprland reopened launcher | 380.76 (364.57–387.05) | 397.22 (379.67–415.83) |
| Hyprland reopened dashboard | 364.50 (364.00–365.00) | 365.00 (363.00–365.00) |

Cold launcher improves in these samples; reopened actions do not show a uniform
gain. In particular Hyprland reopened launcher is +16.46 ms at the median, with
overlapping ranges. Keep that tracked for the later physical/longer comparison;
do not claim all actions are faster. Dashboard boundaries and sample counts
remain those defined above. No finite whole-desktop wallpaper completion time,
physical presentation, three-hour bound or battery improvement is established.

Measured SwapPss is zero throughout. Host swap-in pages in A/B/A/B order are
niri 38/221/84/11 and Hyprland 72/12/27/145; swap-out is zero in all cells.
At 4,096 bytes/page every run is strictly below 1 MiB; the maximum is 884 KiB.

The small change releases unused topic data and fully hidden menu rows while
preserving closing presentation and active-view last-good state. No duplicate
Hyprland compositor subscription was present. The exact allocation responsible
for the original held capture remains unproved; no collector, allocator or
kernel workaround ships. The current candidate meets the owner's lower-total
rule without a QML increase; the original held captures remain retained.

Validation on the exact final feature source: 13 actual Qt tests, 384 Rust
and 952 repository tests (one existing optional skip), all 534 frozen parity
cases, fmt, Clippy, schema/independent JSON Schema, examples and minimal consumer.
The 103-file / 507,373-byte final receipt has SHA-256
`7d0ceba49afa6ca235fc6a49461ebad91cff144da9c3dbfe9dc25525f9626658`;
it is copied and SHA-verified on both hosts. Private capture-process audit is zero.
Live desktop/configuration and installed runtime remain unchanged. Published
CI must be green before the additive core-schema follow-up or F4.
