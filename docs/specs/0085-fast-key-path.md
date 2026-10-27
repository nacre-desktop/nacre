# N0085: instant key path and cumulative audio steps

## Goal and scope

Volume, display brightness, and keyboard-light key presses use a one-shot local
daemon request. A fresh client sends one validated request immediately after
connecting; it does not wait for or parse the welcome or its topic catalogue.
The daemon answers `daemon.ping` off the producer runtime. Hardware steps enter
a bounded queue and run outside producer polling, so slow producers do not
hold up acknowledgements. Audio steps read the active PipeWire sink's current
channel vector, add or subtract 0.05 from every channel, clamp to 0..1, and
write the complete vector back. This fixes the 8-channel pro-audio sink reset
caused by using a stale scalar from the cached audio topic.

Generated niri bindings use `nacre-helper ctl key-step`; a missing/refused
daemon endpoint returns 69 and selects the existing direct hardware fallback.
Other daemon errors do not trigger fallback. The private fast sidecar checks
same-user ownership, endpoint identity, request bounds, supported methods, and
method-specific parameter schemas. No second compatibility client is kept.

## Validation

Owner-reported laptop baseline: `nacre-helper --help` 10 ms,
`nacre-helper ctl call daemon.ping {}` 43–46 ms, and `nacrectl status` 45–80
ms. These are the live measurements that motivated this follow-up; the laptop
was not run or modified during validation.

The earlier socket-only 100-call comparison (synthetic 128-method welcome)
measured the generated `bash -c` + `date +%s%3N` client shape at p50/p95
3.130/4.004 ms before and 2.938/3.491 ms after. This does not model producer
load and is retained only as a client-startup baseline.

The loaded-daemon capture is in `.lead/evidence/n0085-daemon-load.json` with
host/cgroup guard logs alongside it. It used a prebuilt helper (SHA256
`e51154aaa243710ce3d0d13f50ff9973539e06798e4824dfd168fa0ca9754d28`) in a
private nested Niri session on the desktop test host, with notifications and
behaviours producer authorities enabled and every advertised topic subscribed
(61/61). It ran 100 fresh helper processes for each fast method at a 125 ms
audio repeat interval. Ping p50/p95 wall was 2.230/2.423 ms; its daemon-side
server time was 0.023/0.031 ms. Audio p50/p95 wall was 2.865/3.057 ms; server
time was 0.048/0.054 ms. Thus both measured total client-to-reply paths met
5 ms and the daemon fast answer was below 2 ms. The capture's overall daemon
CPU averaged 0.26% on the 12-core host; this does not reproduce the laptop's
reported 10–17% CPU utilization, so the producer-load timing is a lower-load
remote reproduction, not a claim of equivalent hardware saturation.

The private PipeWire fixture began with eight channels at 0.30 and ended at
0.30 after 100 alternating up/down steps; each invocation's fake `pw-cli`
rejected payloads unless all eight `channelVolumes` were present. The focused
`step_updates_all_eight_current_pipewire_channels` test checks two consecutive
steps and verifies every channel changes cumulatively. `FAST_CALL` tracing
provided 100 daemon-side timings per method. The host guard recorded no swap
growth and minimum `MemAvailable` of 11,349,020 KiB. The fake PipeWire fixture
validates the all-channel data path, not physical sink hardware.

Full Nacre CI was run in the task-named Arch CI container on the exact Nacre
source revision recorded in `.lead/report.md`; the script and log are retained
under `.lead/evidence/`. Bead has no code changes in this follow-up; its prior
full CI pass and unresolved OSD frame-presentation discrepancy are recorded
below and in the Bead worktree report. Nacre commits include `[no-publish]`.

## Limits and cleanup

The fast sidecar, audio worker, and persistent worker threads add bounded
always-on memory; full CI's ignored daemon memory check covers the daemon
baseline, but no paired host memory comparison was made for this isolated key
path. Physical owner-laptop latency and audio output were not tested. The Bead
hold capture still has one 32.07 ms presentation interval and does not meet its
strict zero-drop gate; no zero-drop claim is made.

No duplicate implementation or temporary product path remains. Removed
product lines/files: see `.lead/report.md`.
