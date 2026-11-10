# N0160: Bead-mode daemon interaction CPU

Status: Implemented and measured; complete dev-1 desktop checks passed. Owner/lead
live acceptance remains pending. No live session access or deployment.

## Scope and acceptance

Reproduce and profile N0154's daemon interaction cost, reduce the demonstrated
root cause while preserving behavior, and compare idle and interaction CPU with
the same method. Product changes belong in Nacre unless the profile proves the
framework owns the cause. Add regression coverage and replicate the complete
Nacre `desktop` CI job in a task-private Arch container on dev-1 before landing.

The desktop TEST host is occupied and excluded by the owner. The authorized
measurement environment is a dev-1 container (8 CPUs, 16 GiB), private HOME,
activation-free D-Bus, software-rendered headless compositor, private daemon and
the real Bead shell. A focused replay of the shell's daemon IPC is the fallback
if this topology cannot run. No host audio/system services or live settings.

Match the six ten-second N0154 interaction cycles: launcher open/close,
dashboard open/tab/close, workspaces 2→1→3→1, four notifications with 2.5-second
expiry, then rest. Report daemon self and descendant CPU separately so moving
work to a child cannot appear as an improvement. Record artifact identities,
idle/interaction raw samples, profile evidence and environmental limitations.

## Evidence

Starting source: `eb0525b3e`. N0154 reports 36.62% daemon interaction CPU and
approximately 0.43% idle CPU in Bead mode, versus 0.62% interaction in QML mode.
Those hardware-backed nested numbers are context, not a same-host comparison
against the software-rendered container.

The dev-1 headless smoke passed with labwc's pixman renderer and software EGL
in nested distribution Niri: output `winit`, 1280×720, scale 1. The Nacre-patched
compositor artifact from N0154 is not available in this lab; the exact binary
hash and this limitation must accompany the results. Private writer roots live
under the container's private temporary directory; the host-owned source mount
is not suitable ancestry for a root-owned daemon's writer lease.

An unprivileged `perf` invocation was denied by the host's existing policy.
A task-scoped privileged container execution enabled profiling without changing
that policy or any host setting. Temporary timing probes in the remote source
copy also cover compositor publication, hub method/status generation,
notification snapshots and storage; their wall times include I/O waits.
The probes are removed from product source before CI. The preserved baseline
binary and recovered harness are temporary duplicates: delete zero days after
the replacement is trusted, during final task cleanup.

A notification-only diagnostic (no shell, compositor or subscribers) sent 24
native notifications 200 ms apart, with 2.5-second expiry. Over 8.320681 seconds,
the daemon used 8 CPU ticks at 100 Hz: 0.96146% of one core. This does not
reproduce N0154 or prove the subscribed path is cheap. Storage saves took up to
1.07 seconds of wall time while consuming little CPU; wall time alone does not
identify the CPU root cause.

## Root cause and change

Diagnostic replays using the real shell reproduced large worker-thread bursts;
the hot thread was a Tokio blocking worker, not the main IPC loop. Earlier
diagnostics and failed harness attempts are excluded from the final comparison.
The final replay has a mocked-clock timing check for all 30 UI calls, 24
workspace actions at 0.3-second spacing and 24 notifications across exactly
six ten-second cycles. A separate sampler check captured 29 CPU ticks from a
short-lived worker thread, proving that thread exit does not lose process CPU.

The final validated-replay CPU profile (`cpu-clock:u`, 99 Hz, 1,571 samples,
zero lost samples) attributes 90.52% of samples to six raster hot paths:
resampling 27.56%, blur box pass 25.59%, image pixel indexing 16.55%, PNG
filtering/scoring 15.15%, compositing 5.67%. Another 1.27% constructs blur pixels.
`daemon::lock_content` prepares the lock dashboard after notification, player,
weather and settings changes. Each changed presentation rebuilt the unchanged
wallpaper's fitted, cropped, resized, blurred and tinted background. Its large
temporary images explain the original allocation sawtooth. Serialization and
notification persistence were not the CPU bottleneck.

The renderer now caches only this static background raster on disk. Its identity
includes wallpaper path and file identity/timestamps, surface color, output
geometry/transform and raster dimensions. Dynamic lock content still renders on
every relevant change; existing filters, pixels, publication and buffer release
remain intact. The disk cache is bounded to nine private PNGs and retains no idle
image allocation. Missing, malformed or wrong-sized caches are recomputed.

Focused checks passed in the dev-1 container:

- `cargo test -j2 --locked -p nacre-helper --lib background_cache_tests`:
  2 passed, 0 failed. Covers cached/fresh pixels, reuse without rewriting,
  missing/corrupt/wrong-sized caches, invalidation and missing-wallpaper fallback.
- `cargo test -j2 --locked -p nacre-helper --test lock_dashboard`:
  5 passed, 0 failed, including frozen Pillow rendering parity.

No temporary product duplicates are planned. Recovered historical benchmark
scripts and task build artifacts remain untracked scratch and are deleted on
completion. Owner/lead live audit remains separate from container evidence.

## Matched measurements

The summary tables below report five alternating before/after pairs, numbered
03–07; the [method note](../research/n0160-daemon-cpu-20261110/README.md) preserves
measurement accounting and build details. Each fresh private
session warms for 30 seconds, measures idle for 30 seconds, runs the six-cycle
60-second interaction, then measures another 30 idle seconds. Both arms use the
same release flags, symbols, timing overlay, real shell, palette and static
1920×1080 wallpaper. Output is 1280×720 at 60 Hz, scale 1. All daemon processes
run at nice 10. No task builds run during captures.

CPU is a percentage of one core, computed from summed raw `/proc/PID/stat`
ticks divided by actual elapsed time (100 ticks/second). The process-level
counter includes all worker threads, including workers that exit between samples.
It must not be replaced by a sum of currently surviving thread counters.
Waited-child CPU is reported separately and included in the conservative total;
the live process-tree diagnostic alone can miss a short-lived child.

| Median of five runs | Before | After |
| --- | ---: | ---: |
| Daemon process, idle before | 0.1332% | 0.1332% |
| Daemon process, interaction | 30.8161% | 8.8618% |
| Daemon process, idle after | 1.1655% | 1.1323% |
| Daemon + waited children, idle before | 0.9991% | 0.9989% |
| Daemon + waited children, interaction | 31.1492% | 9.1617% |
| Daemon + waited children, idle after | 1.4652% | 1.4653% |

The daemon process interaction reduction is 71.2%; the inclusive reduction is
70.6%, so no child-work shift explains it.
Each arm completed all 30 UI calls, 24 workspace actions and 24 notifications.
Every accepted arm had zero startup and measured `UI_ERROR`s. Earlier diagnostic
sessions intermittently emitted the unchanged bar's `unknown state widgets.outputs`
and `unknown state widgets.layout` during startup. The harness records these
explicitly, permits only those exact startup messages, and rejects every measured
error or other startup error. Failed captures and two early pairs accidentally
run at nice 19 are excluded; none contributes to these medians.

Memory uses anon + shmem + SwapPss, with the software backend above. Median
paired candidate-minus-control deltas are +168 KiB idle before, −332 KiB during
interaction and −320 KiB idle after, passing the +512 KiB regression gate.
SwapPss is zero throughout. Cache evidence shows one 22,082-byte background PNG
with unchanged inode/mtime across all three phases in each candidate arm.
No decoded wallpaper image is retained at idle.

These reproduce the large daemon cost and its cause on the authorized dev-1
topology. They are not hardware-backed N0154 numbers or the owner/lead's live
acceptance audit. The occupied desktop TEST host and laptop live session were
never accessed. No UI layout or animation changes are made; frozen rendering
parity and cached/fresh pixel equality cover behavior preservation here.

Product delta: +133/−44 lines (net +89); regression tests add 103 lines.
The old inline background computation is removed in the same change.

## Full desktop validation

Every check in the complete `desktop` job from `.github/workflows/check.yml`
passed in `ghcr.io/archlinux/archlinux:latest` on dev-1, including `CI=true`,
pinned Luau analyzer, cargo-deny, `tools/check.py`, native gallery and ignored
daemon memory tests.
The six probe-modified files were restored first; `rg N0160Timer` has no matches.
The canonical changed source SHA-256 is
`dc3e5b5a487de96d6e69c9f595a8f39098da4edd9f178ea4b66d8dabe0a3fa76`.
The source copy's Orient Git metadata was restored to the exact locked public
revision `3efe8d1e7fb50200d74a3e0193ad63699cb8ebe8`; a diff against that revision
confirmed the already-copied Orient content was unchanged. Disposable container
Git snapshot commits are not product commit identities and are never pushed.

The [CI summary](../research/n0160-daemon-cpu-20261110/ci-results.md) records the
nice-10 run, which passed formatting, Clippy,
examples, all workspace tests, minimal consumer, release helper, feature checks,
shell/wallpaper builds, Luau/product smoke, cargo-deny, npm and translations.
The source guard then rejected untracked benchmark Python under `.lead`; moving
that scratch out of the canonical checkout fixed the environment. The failed
source/UI step and all remaining steps were rerun with the exact workflow
context using the archived completion recipe: source/UI checks, gallery and
isolated daemon raster-memory test all passed; completion exit code was zero.
No product code or test assertions changed to resolve CI failures.

The CPU-rendered memory regression passed with peak anon 261,532 KiB and settled
anon 20,476 KiB (RSS 54,364 KiB; clean file PSS 19,723 KiB). This existing guard
is separate from the five-pair relative unreclaimable-memory gate above.
After generators and checks, `git diff --exit-code` against the verified
canonical index passed. Benchmark probes are absent from the production helper.
