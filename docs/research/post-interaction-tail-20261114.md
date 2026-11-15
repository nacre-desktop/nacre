# Post-interaction CPU tail, thread by thread (N0326, 2026-11-14)

Measurement only, no product change. Where shell, daemon and niri CPU and
wakeups go in the 20 minutes after the standard panel workload, on Nacre main
86433e4a4 (Bead pin 841de925, which has B0271 and not B0272). Follows the
scoreboard's finding 2 ([noctalia-scoreboard.md](noctalia-scoreboard.md)) and
[N0321](../specs/0321-daemon-scoreboard-fixes.md) section 2.

## Method

`tools/bench/noctalia` (N0308) on dev-1 or dev-2 in `nacre-ci/arch-nacre-session:current`,
software rendering, Nacre arm only, release binaries with symbols. Profile `tail`:
30 s baseline, 300 s idle, the standard 6 cycles (launcher 0.8 s, dashboard
0.8 s, four workspace switches, four notifications per 10 s), then 1200 s rest.
The harness samples the trees once a second; a local patch also dumps every
thread's schedstat ns, utime+stime ticks and voluntary + involuntary context
switches every 10 s (aligned to the rest start). PSS from `smaps_rollup`.
CPU is percent of one core; wakeups are context switches per second.
Idle level = the idle window (30 s after the bar maps to the first interaction).
Two clean arms and one diagnostic arm run in parallel in one container
(each with its own private bus, compositors and runtime). The diagnostic arm
adds 60 s `strace -f -C` windows at rest minutes 1, 5 and 15 and `perf record`
(DWARF call graphs) at minutes 2, 6 and 16; its CPU is perturbed and stays
out of the table.

The wrapper, harness patch, analysis script and raw per-sample CSVs are kept
outside the repo (the unit's `.lead/n0326/`); the wrapper re-runs this against
another Nacre commit (`tail.sh stage COMMIT`, then `tail.sh all NAME`).

## Results

Run on dev-2 (2026-11-15 01:16 to 01:44, exit 0): Nacre 86433e4a4, Bead 841de925, niri 26.04-1, one 6-CPU
container. The three arms (clean1, clean2, diag) ran in parallel in that one container, so absolute idle numbers are
not comparable with a solo run; the rest-phase shape is. Idle window 64-333 s, interaction 333-394 s, rest to ~1590 s.
Raw CSVs stay in `.lead/n0326/results/` (not committed). Clean1 and clean2 agree within noise.

### Tail duration

The tail lasts about 2 minutes (rest bins 0 and 1) and then drops to the pre-interaction idle level in rest bin 2
(60 s bins, clean1/clean2/diag all alike). The analyzer's "back at idle max from minute 15-19" flags are single noisy
niri bins above the idle max (niri idles at 0.17-0.21 %), not a tail. 120 s is exactly Bead's `PANEL_KEEP_ALIVE`.

### 60 s bins (clean1; CPU %, wakeups/s)

| bin | shell CPU | shell wake | daemon CPU | daemon wake | niri CPU | niri wake |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| idle (mean of 4) | 0.016 | 1.9 | 0.050 | 6.4 | 0.19 | 52 |
| interaction | 3.97 | 186 | 1.03 | 77 | 59.2 | 16094 |
| rest-00 | 0.672 | 26.2 | 0.155 | 40.2 | 0.172 | 47.6 |
| rest-01 | 0.580 | 21.9 | 0.151 | 46.1 | 0.395 | 113.1 |
| rest-02 | 0.020 | 2.0 | 0.052 | 6.3 | 0.172 | 48.9 |
| rest-03 .. rest-18 | 0.014-0.019 | 1.9-2.1 | 0.049-0.055 | 6.1-6.6 | 0.17-0.21 | 45-60 |

clean2: rest-00 shell 0.655 % / 23.9, daemon 0.140 % / 46.5; rest-01 shell 0.571 % / 17.8, daemon 0.137 % / 52.2,
niri 0.403 % / 119.6; rest-02 back at idle. Top threads in the tail: `nacre-shell` main/render 0.56-0.66 % and
18-25 wakeups/s (idle 0.014 % / 1.3), daemon `nacre-helper` 21-24 wakeups/s (idle 4.2), `tokio-rt-worker`
10-19 wakeups/s, `nacre-offload` 6-8 wakeups/s (idle 2.0). niri has one bin at 113-120 wakeups/s (rest-01) and is
otherwise at idle. Whole-run rest averages (0.0552 % shell render thread, helper 6.1/s) are dominated by those two bins.

### PSS (MiB, clean1 / clean2 / diag)

| point | shell | daemon |
| --- | ---: | ---: |
| startup | 41.1 / 39.0 / 39.0 | 49.1 / 31.7 / 22.4 |
| pre-interaction | 45.3 / 45.4 / 46.6 | 31.6 / 32.3 / 32.4 |
| post-interaction | 69.0 / 68.8 / 70.4 | 33.9 / 34.6 / 33.6 |
| rest +5 min | 56.4 / 56.2 / 57.5 | 33.9 / 34.6 / 33.6 |
| rest +10 / +15 / +20 min | 57.1 / 57.2 / 57.0 (clean1) | 33.9 / 33.9 / 34.0 |

Shell PSS falls 12 MiB in the first 5 minutes and then stays about 12 MiB above pre-interaction (flat from +5 to +20
min: no leak, but a retained high-water mark). The daemon grows only 2 MiB.

### Attribution (diag arm; strace -f 60 s windows, perf DWARF)

strace slows the traced process, so minute-1 call and wakeup rates are inflated (diag rest-01 shell 1932 wakeups/s
versus 18-22 in the clean arms); read them as which calls, not how many.
- Minute 1 (in the tail): shell render thread 4306 does `madvise` (44148 calls in 60 s) and `clock_gettime`, the
  shell main thread `madvise`, `epoll_ctl`, `timerfd_settime` (about 100 calls/s total): the allocator returning
  memory plus per-timer re-arming. Daemon `nacre-offload` does readlink/openat/getdents/statx scans (94 calls/s)
  and `nacre-helper` reads fd 106 (about 13/s): service subscriptions still being served.
- Minute 5 and minute 15 (idle level): shell main thread about 32 calls/s (`madvise`, `epoll_ctl`, `timerfd_settime`,
  one 69/min timer), render thread 17 calls/s, `nacre-offload` 27 calls/s of getdents/openat/statx (the idle
  baseline, identical at 5 and 15 min), niri `Filesystem Watcher` 31 calls/s of readlink/statx. Minutes 5 and 15 are
  indistinguishable: nothing ramps or decays after the tail.
- perf DWARF reports have no symbols for the Nacre binaries in this image (libc/kernel addresses only), so they add
  nothing beyond the strace picture.

### What the data says about PanelVisibility

The 2-minute tail matches the 120 s keep-alive: shell render-thread CPU is ~40x idle and daemon helper/tokio
wakeups ~5-10x idle for exactly the two minutes after the last panel closes, then both fall to the idle level in
one bin. That is what you would see if closed panels keep their daemon subscriptions (and the widget/timer work
they drive) until Bead's `PANEL_KEEP_ALIVE` drops the panel, because Nacre's adapter never unsubscribes on
`PanelVisibility(false)`. This run does not prove the cause (no arm with the unsubscribe), but the size is small:
about 0.6 % of one CPU for 2 minutes, then identical to idle. Closing the gap is Shell's fix; the expected gain is
removing those 2 minutes, not any steady-state idle cost. Steady idle (shell 0.016 %, 2 wakeups/s; daemon 0.05 %,
6.4/s) is unchanged before and after.

## Does Nacre's Bead adapter implement the PanelVisibility hook?

No. B0271's hook lives in Bead's own host: `crates/bead/src/host.rs:1421-1433`
(at 841de925) unregisters a closed, kept-alive panel's service wakeups on
`PanelVisibility(false)` and registers again on `PanelVisibility(true)`.

Nacre's adapter is `impl SceneSource for Runtime`
(`crates/nacre-shell/src/reload.rs:5347`). Its `services()`
(`reload.rs:5507`) subscribes every source the view uses (dashboard
`reload.rs:5914-5918`, launcher, quick launcher, notifications, notices,
desktop, connection, prefs, routes, tray, widgets) into fields that live as
long as the `Runtime`. `stage()` (`reload.rs:6145`) uses `PanelVisibility`
only for the motion duration (`:6165`), smooth-open timing (`:6191`, `:6197`)
and store values (`:8977-9018`: `panel.open`, hover flags, dock page reset,
chat close, widget editor); it never drops or retakes a subscription. A closed
panel therefore keeps its daemon subscriptions until Bead releases the panel
`PANEL_KEEP_ALIVE` = 120 s later (`bead-wayland/src/panel.rs:5`, `:1282`).
Dropping them on `PanelVisibility(false)` and resubscribing on `true` (as
Bead's host does) is Shell's fix.
