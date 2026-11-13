# N0305: Daemon idle wakeups

## Outcome

Keep telemetry subscriber-scoped, stop retrying an absent NVML library after its
first load failure, and watch night-light state files instead of rereading them
on every OSD tick. When no Caps Lock LED exists, use the LED-directory watcher
to discover hotplugged LEDs rather than starting a blocking scan every 500 ms.
When an LED exists, retain the 500 ms readback so Caps Lock OSD timing does not
change.

The remaining 500 ms Caps Lock poll is the unresolved product waker on devices
that expose that LED. Follow-up NN0036 moves it to a compositor event.
The N0300 fake-audio `tail -F` child also wakes once per second in the benchmark
process tree and is fixture overhead, not a Nacre product loop.

## Measurements

Both runs used the same private N0300 `bead-stock` session on dev-1, with a
300.3-second idle phase. Both idle windows completed. The harness's later
notification interaction failed in both runs, matching the N0300 fixture issue.
`strace` was used only for attribution because it adds CPU overhead.

| | Before (`8b92ed0fa`) | Candidate |
|---|---:|---:|
| Daemon CPU, % of one core | 0.41 | 0.45 |
| Daemon-tree context switches/s | 12.21 | 12.41 |
| Missing NVML log lines | 3 | 1 |

The before trace counted 4,042 main-thread `epoll_wait` calls (292.3 seconds
blocked) and 297 one-second sleeps from the fake audio `tail -F` child. Dashboard
system metrics already stop without a topic lease; the control-center's 2 s
timer is likewise lease-gated. The OSD Caps Lock safety
readback remains at 500 ms on dev-1 because its input keyboard exposes a Caps Lock
LED. Idle CPU is unchanged within measurement noise; the requested near-zero
target is not met. The remaining poll is assigned to follow-up NN0036.

## Validation

- NVML ABI test verifies an absent `libnvidia-ml.so.1` is probed and logged once.
- OSD tests verify subscriber lifecycle, file-watch delivery, Caps Lock fallback,
  audio events, and D-Bus-backed power/backlight changes.
- Nacre-focused dev-1 tests, Clippy, formatting, Ruff and compositor-boundary
  checks are recorded in `.lead/report.md` after the final run.

No temporary product duplicate paths were added.
