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
  --backend hyprland --quick --check-budgets --output /tmp/shell-hyprland.json
PATH="$PWD/.venv/bin:$PATH" python tools/performance-session.py \
  --backend niri --quick --check-budgets --output /tmp/shell-niri.json
```

Use `--revision COMMIT` for an immutable source comparison while keeping the same
harness, engine and host. For concurrent development, `--control` disables only
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

The separate **Headless shell performance** workflow runs the full production
shell in nested Niri on a pixman headless parent, exercises the full panel/video
cycle and retains JSON artifacts. Mesa OpenGL software rendering is explicit;
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
