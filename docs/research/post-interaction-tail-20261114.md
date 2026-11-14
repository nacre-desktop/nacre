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

Pending: on 2026-11-14 the job queued from 23:33 behind full global slot
queues on both build hosts and had not started by the 00:35 cutoff. It runs
after 02:00 and this section is filled from it.

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
