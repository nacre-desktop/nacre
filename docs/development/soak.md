# Desktop soak testing

The current `tools/soak.py` exercises Niri and Hyprland sequentially in nested sessions with
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
The driver waits for docking to finish and stops on an incomplete handoff before
creating another window. `--recover-docks` is a separate bounded-memory control:
it closes leftover fixture pearls after a timeout and records every recovery.
Such a run can have `memory_workload_healthy: true` while
`workload_healthy: false`; it does not establish successful docking.
The memory-health field validates duration, responsiveness and the window limit;
assess the memory trend separately.
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

## Historical results and fixes

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
three-hour AFTER behavior was unmeasured in that confirmation. Baseline growth continued in its final
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
The production-launcher follow-up below supplies the longer measurement on a
different fixture. These historical busy-host/swapped runs are not quiet hardware
performance evidence.

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
The standalone draft below uses Quickshell as its QML host; a Qt-only host has
not been verified. Warning-fix work is stopped, and filing is the owner's decision.

## Native runtime: three-hour production-launcher follow-up

On 2026-10-14/15, the source was frozen at the then-current main `c76b3e7d`
(capture HEAD `2289d90b` adds only the warning-report draft below). Both native
engine/helper binaries came from that exact main revision's successful [Source
checks run `38624789594`](https://github.com/nacre-desktop/nacre/actions/runs/38624789594). The source fingerprint remained unchanged throughout
the original Niri/Hyprland pair. These captures use the real `nacre/shell-tools/launch.sh`, including
its allocator policy, rather than the historical direct-launch harness.

Each compositor ran sequentially with seed 20261013, ten minutes of warmup and
180 measured minutes. The original pair completed without shell exits or RPC/probe failures,
but the later window-lifetime audit invalidated Hyprland's workload-health claim.
The isolated remote fixture ran Arch userspace on an Ubuntu
25.04 host: Quickshell 0.3.2, Qt 6.12.0 (Arch package -3), Niri 26.04, Hyprland 0.56.2 and
jemalloc 5.4.0. A private headless Labwc parent used the existing NVIDIA 570.172.08
render driver. The secondary shell profiles entered Mesa 26.2.4 llvmpipe software-rendering
code; parent GPU use does not establish client hardware acceleration. Parent and
workers used sibling Bubblewrap sandboxes with private
HOME, PID/network namespaces and buses; the compositors remained nested. The
host's namespace security policy, desktop and configuration were unchanged.
Priority was `nice -n 10`; existing native CI artifacts avoided another build.

| Compositor, after native migration | Measured minutes | Full shell PSS + SwapPss, MiB/h (R²) | Hour 1 | Hour 2 | Hour 3 | Final shell MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Niri | 180 | +20.9 (.105) | +51.4 (.072) | +18.9 (.010) | −42.9 (.072) | 607.7 |
| Hyprland, invalid window cap | 180 | +322.1 (.960) | +408.9 (.917) | +144.6 (.715) | +471.4 (.954) | 1432.8 |
| Hyprland, corrected bounded control | 180 | −3.0 (.003) | +24.5 (.018) | +25.6 (.024) | −8.5 (.005) | 458.4 |

Hourly columns also give MiB/h and R². Boundary sampling can differ by one
sample; fits use actual sample timestamps. Niri's hourly mean shell sizes were
577.1, 607.7 and 622.5 MiB, with a large cyclic working set: hour three ranged
549.6–772.8 MiB. **Niri showed no sustained positive final-hour trend**;
this finite run cannot prove a permanent upper bound.
**Hyprland's live-window count exceeded the intended cap and kept growing
during the run:** its final hour rose approximately 471 MiB/h, with 36 launcher
pearls in its saved end state.
This is not a valid constant-workload leak comparison. The original healthy
flag missed 3,752 rejected dock handoffs; Niri had zero rejected handoffs and
zero pearls left at the end.

Docking waits for two destination frames and safely retains the floating source
when that handoff fails. The old harness attempted to dock excess pearls and
immediately created another, without checking asynchronous completion. Each
retained launcher kept its QML view, render thread and native graphics storage.
The intended four-window cap was therefore ineffective in the headless Hyprland
fixture. The corrected driver verifies completion, enforces the cap and audits
saved legacy state before accepting a healthy result. The application's frame
barrier and its data-preserving rollback remain unchanged.

The corrected driver serializes dock transactions because they share a
destination panel. A focused native rehearsal confirmed that Niri completes
these handoffs, while the headless Hyprland fixture can still time out with just
one floating window. Strict mode now stops at that failure. The separately
labelled `--recover-docks` control closes only disposable fixture windows after
a timeout, counts every recovered window and keeps the live-window limit at
four. Recoveries make `workload_healthy` false even when
`memory_workload_healthy` is true. The latter validates duration, responsiveness
and window cardinality; it does not classify the memory trend or certify docking.
Headless Hyprland's destination-frame handoff remains a functional follow-up.

The corrected Hyprland control used frozen harness revision `5af1d536`, with
application/UI files unchanged from `c76b3e7d` and the same native binaries.
It repeated the seed, mixed five-second actions, ten-minute warmup and three
measured hours through the production launcher, without allocation profiling.
The source fingerprint and binary hashes matched after completion. All 181
measured samples were captured; there were no shell exits, action failures or
probe failures, and the observed peak was four pearls. It required **101 fixture
window recoveries**, so memory-workload validation passed while full functional
workload validation failed. Focused native rehearsals separately exercised
successful Niri handoffs, strict Hyprland failure and explicit Hyprland recovery;
22 unit tests cover the driver and summary behavior.

**Verdict: an observed plateau in the three-hour bounded control; no sustained
memory growth was observed.** Hyprland's hourly mean shell sizes were 364.9, 359.8 and
355.1 MiB; hourly minima were 302.8, 305.0 and 304.7 MiB. Hour three ranged
304.7–458.4 MiB and its fit was −8.5 MiB/h with very low R². Together with
Niri's lack of a sustained positive final-hour trend, this supports a fluctuating
working set within the measured windows. It does not prove a permanent bound
or validate the unresolved headless docking handoff. It covers repeated window
creation and destruction, rather than one pearl held open indefinitely. The original Hyprland
+471.4 MiB/h final-hour result measured accumulating live windows; correcting
the workload removed that large trend without changing application rendering.
No further application memory fix was justified by this follow-up.

Long-lived helpers stayed essentially flat (largest positive Niri fit below
0.1 MiB/h; original Hyprland helper fits were slightly negative). In the bounded
control, native-helper, wallpaper-media and wallpaper-watch fits were only
+0.07, +0.05 and +0.01 MiB/h respectively. All three capture endpoints had zero
retained notifications, popups, native references, image references and stale
dismissal keys. The notification retention defect remains fixed. The large
Hyprland slope includes deliberately retained live windows, rather than proving
lost memory. These results cannot be
subtracted from the historical table as an isolated fix effect: source, native
runtime, launch policy and hardware all changed.

Every sampled test process had **SwapPss = 0**. The host already had about 673 MiB
of swap in use; approximately 22 MiB more was paged out during the original pair, with no
swap-ins, then usage stayed flat. Host paging therefore occurred, but the sampled
test processes themselves were not swapped. Host CPU was usually about 1–4%,
with brief peaks; this was a lightly loaded remote fixture, not a claim that all
other services were absent. Headless/nested graphics are not physical-display
or laptop performance evidence, and PSS does not count unmapped GPU allocations.

The corrected bounded control also had **SwapPss = 0** for every sampled test
process. Host swap-in/out counters did not increase; pre-existing swap usage
fell from 695.0 to 665.8 MiB. Host CPU averaged 0.77%, with a 1.53% peak across
minute samples. These host-wide counters do not attribute other services'
memory to the fixture. All task-owned capture, profiling, compositor and monitor
processes were stopped afterward; unrelated sessions were preserved.

### Allocation profiling and its limits

Normal-engine GC logging and jemalloc profiling separated JavaScript heap
fluctuations from native storage. This diagnostic used the same Hyprland fixture,
application source and seed, with the original mixed actions every five seconds;
memory samples were every thirty seconds. In its pre-cleanup interval,
post-collection JavaScript memory repeatedly returned to roughly 20–25 MiB;
its recorded peak was 68.3 MiB. A controlled close of nine accumulated pearls
reduced threads from 94 to 85, native allocated memory from about 579.7 to
207.6 MiB, and PSS from 700.8 to 388.0 MiB after reclamation. This is direct
evidence that substantial memory was released when the live windows were
actually destroyed. The rest of that diagnostic is a cleanup experiment;
its whole-hour slope is not a comparable soak result.

The largest growing allocation sites in the 45.5-minute pre-cleanup comparison
were as follows. Jemalloc used a mean sampling interval of 64 KiB, with matching debug
symbols and live snapshots before shutdown; sizes are sampled estimates, not PSS.
Total sampled net native growth was 235.3 MiB (allocator statistics: 237.4 MiB).

| Growing allocation site | Estimated live growth, MiB |
| --- | ---: |
| Mesa shader-storage slabs: `gc_alloc_size` → `create_slab` → `ralloc_size` | +121.7 |
| Qt string buffers: `QArrayData::allocate2` → `QString::reallocData` | +19.8 |
| Qt QML binding guards: `QQmlJavaScriptExpressionGuard` → `QRecyclePoolPrivate::allocate` | +6.4 |

The pearl model's live-window list was growing: failed handoffs correctly kept
source windows alive. Native allocation growth did not by itself identify
a Mesa or Qt leak. The small, safe correction belongs in the workload driver;
no application rendering policy or frame-proof check was weakened.

QML allocation tracing itself proved intrusive. In a separate forty-minute
diagnostic, one nominal three-second window emitted 16.5 million events;
symbolized `QV4::Profiling::Profiler::reportData` stacks retained roughly
0.9 GiB of additional trace buffers. Its raw process-memory trend is excluded
from the primary table. Five short windows showed JavaScript reserved/occupied
heap totals rising from 79.6/26.3 MiB near warmup to a transient 180.8/179.8 MiB,
then falling to 146.5/43.6 MiB at the final window. These are heap counters,
not resident PSS or proof that every native allocation belongs to the profiler.

Qt allocates JavaScript heap pages separately from `malloc`; QObject instances
and string payloads can live in native allocations while JavaScript owns their
references. Native allocation stacks therefore locate the storage without
necessarily identifying the layer responsible for its lifetime. See
[Qt's JavaScript memory model](https://doc.qt.io/qt-6/qtqml-javascript-memory.html).

## Qt bug report draft (not submitted)

**Title:** Wayland text-input-v3 surface focus warnings during native window
hide/show and activation transitions.

**Observed environment:** Qt 6.12, Quickshell 0.3.2, nested Niri on Linux/Wayland.
Quickshell hosts the reproducer; its windows and text fields are ordinary Qt
`Window`/`TextInput` controls with no Nacre UI. Reproduction in the stock Qt QML
launcher has not been verified. Record exact toolkit/compositor versions when
filing; this is a draft for the owner to review, not an upstream diagnosis.

**Steps:** In a disposable nested Wayland test session, save the following as
`/tmp/qt-focus-repro/shell.qml`:

```qml
import QtQuick
import QtQuick.Window
import Quickshell
import Quickshell.Io

ShellRoot {
    Window {
        id: first
        title: "Plain Qt focus A"
        width: 400
        height: 200
        visible: false
        TextInput { id: firstInput; focus: true; text: "Public fixture A" }
    }
    Window {
        id: second
        title: "Plain Qt focus B"
        width: 400
        height: 200
        visible: false
        TextInput { id: secondInput; focus: true; text: "Public fixture B" }
    }
    IpcHandler {
        target: "nacre"
        function state(): string { return '{"ready":true}'; }
        function launcher(query: string): void {
            first.visible = true;
            first.requestActivate();
            Qt.callLater(() => firstInput.forceActiveFocus());
        }
        function close(): void { first.visible = false; }
    }
    IpcHandler {
        target: "settingsApp"
        function open(page: string): void {
            second.visible = true;
            second.requestActivate();
            Qt.callLater(() => secondInput.forceActiveFocus());
        }
        function close(): void { second.visible = false; }
    }
}

```

Run the standalone shell and repeatedly activate/hide its two windows. The IPC
target names are only labels; no Nacre services or configuration are loaded.

```sh
quickshell -n -p /tmp/qt-focus-repro &
repro_pid=$!
for i in $(seq 1 30); do
    quickshell ipc --pid "$repro_pid" call nacre launcher demo
    sleep 0.25
    quickshell ipc --pid "$repro_pid" call settingsApp open general
    sleep 0.25
    quickshell ipc --pid "$repro_pid" call nacre close
    quickshell ipc --pid "$repro_pid" call settingsApp close
done
kill "$repro_pid"
wait "$repro_pid" || true
```

**Actual:** Repeated warnings from `QWaylandTextInputv3::enableSurface` and
`disableSurface` report differing enabled/focused surfaces. Bounded call stacks
include Qt's `_q_updateFocusObject`, `processFocusWindowEvent` and Qt Quick focus
clearing. The original seeded control recorded 83 warnings in one measured
minute. Removing both explicit `forceActiveFocus()` calls still recorded 36;
static `focus: true` and native activation remained. Counts depend on timing and
are not expected to match the shorter loop above exactly.

**Expected:** Normal native-window activation/visibility changes maintain
consistent text-input surface state without repeated warnings. A functional
input-method/composition failure has not been established by these diagnostics.
No warning suppression or input-method disabling was used.

The owner decides whether to file this report. Further warning-fix experiments
are out of scope for the soak follow-up.
