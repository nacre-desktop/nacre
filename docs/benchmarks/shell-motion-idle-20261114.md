# Shell motion and idle benchmark, 2026-11-14

> **Swap gate waived** (desktop swap 2.5 GiB from the owner's tty1 session); shell/daemon
> SwapPss = 0, host pswpin per capture recorded. Informational benchmark numbers for judging
> tonight's fixes, not acceptance evidence (the laptop live audit stays the gate).

Baseline of Nacre `main` at `86433e4a4` (Bead `841de925`): panel open/close frame gaps, idle
cost and first-open vs warm-open, against the owner's gate of **max gap <= 12 ms** (120 Hz).

Every row is labelled: swap gate waived (desktop swap 2.5 GiB from the owner's tty1 session);
shell/daemon SwapPss = 0 in every window, host pswpin per capture 0 to 51 pages (<= 204 KiB);
no capture hit the 8 MiB / busy-host void rule; the owner's tty1 session stayed idle (`who -u`).

## Summary

- **Steady motion is on pace**: the median frame gap is 8.0 ms (120 Hz) in every panel and the
  mid-motion maximum is 8.4-10.6 ms for launcher, control, chat and control-click.
- **Almost every traced transition fails the 12 ms gate once, at its start** (230 of 236
  transitions): Bead commits the first 3-4 frames within ~8 ms without waiting for frame
  callbacks, then frame 4-5 arrives 12-20 ms later (one skipped vblank). Attribution: **Bead**
  frame scheduling; those frames cost 0.2-3.6 ms each, so it is not render time.
- **The last frame of an open lands late, but shows nothing new** (**Bead**): the slide's final
  frame, a full-panel repaint (dashboard 359k px, launcher 736k px, control 495k px of damage),
  is committed only on the next unrelated wakeup: the dashboard's 2 s gauge tick (11 of 20
  opens: 34-761 ms) or ~5.05 s later for launcher and control (every held open; PANEL_SLIDE
  then reports `duration_ms=5350 max_gap_ms=5054`). Screenshots 1 s and 6.5 s after an open
  are pixel-identical for the launcher and differ in 0-29 px for control (a live value), so the
  frame before it already shows the end pose: not a visible stall, but it makes PANEL_SLIDE
  `max_gap_ms` useless as a gate and leaves a full repaint pending for seconds.
- **Mid-motion misses are rare**: 6 single-vblank gaps (16 ms) in 236 transitions, all in the
  dashboard (5) and the edge-hover open (1).
- **Cold first open** costs 35-50 ms more first-frame latency than warm (dashboard 62 vs 13 ms,
  launcher 55 vs 13, control 48 vs 14, control-click 38 vs 10); the cold dashboard also settles
  108-495 ms late. Attribution: **shell/Bead** view build on first open.
- **OSD and toasts pass** (shell PANEL_SLIDE max 9.1-10.9 ms). The OSD appears ~39 ms after the
  volume call, cold and warm alike; that includes the fixture's Python `wpctl` stand-in, so it
  is not a product latency.
- **Idle is cheap**; the open dashboard is not: 0.40 % CPU and 9.4 wakeups/s (its CPU and
  memory gauges repaint every 2 s) against 0.02-0.06 % and 2-4/s for the other open panels.

## Frame gaps

Two runs of 10 opens and closes per panel (18 warm + 2 cold transitions per row; chat's cold opens are untraced),
each panel first opened in a fresh session. Shell columns
are the shell's committed frames (UI_FRAME), niri columns what the nested compositor composited.
"Start" is the largest gap among the first six frames, "mid" the largest after that up to the
settle, "late settle" the gaps over 25 ms that end a transition. Bold = over the 12 ms gate.

| panel | dir | shell p50 / p95 / max ms | start max (share > 12 ms) | mid max | late settle | first frame cold / warm ms | niri p50 / p95 / max | attribution of the over-gate value |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| dashboard (top) | open | 8.0 / 8.3 / **761.4** | **19.9** (20/20) | **16.2** | **11/20: 34-761 ms**; cold 108, 495 | 63.9, 61.1 / 12.8 | 8.0 / 8.2 / 16.6 | Bead start pacing; Bead final-pose frame waits for the 2 s gauge tick |
| dashboard | close | 8.0 / 11.6 / **16.0** | **16.0** (19/20) | **16.0** | 0/20 | 10.8, 10.4 / 11.2 | 8.0 / 8.2 / 8.3 | Bead start pacing |
| launcher | open | 8.0 / 8.3 / **16.2** | **16.2** (20/20) | 8.6 | final pose ~5 s late (held opens) | 54.1, 55.2 / 12.9 | 8.0 / 8.2 / 21.9 | Bead start pacing; Bead final-pose frame |
| launcher | close | 8.0 / 8.4 / **16.1** | **16.1** (20/20) | 8.5 | 0/20 | 13.2, 13.9 / 12.8 | 8.0 / 8.2 / 8.4 | Bead start pacing |
| control, key/IPC (right) | open | 8.0 / 9.0 / **15.5** | **15.5** (18/20) | 9.5 | final pose ~5 s late (held opens) | 48.0, 48.9 / 13.6 | 8.0 / 8.2 / 8.4 | Bead start pacing; Bead final-pose frame |
| control, key/IPC | close | 8.0 / 8.4 / **16.1** | **16.1** (20/20) | 10.1 | 0/20 | 12.3, 12.4 / 11.1 | 8.0 / 8.2 / 8.4 | Bead start pacing |
| control, edge hover | open | 8.0 / 8.4 / **16.1** | **15.5** (17/20) | **16.1** | 0/20 | 430.9, 432.3 / 392.4 (incl. 0.9 s dwell request) | pointer frames | Bead start pacing |
| control, edge hover | close | 8.0 / 8.4 / **16.1** | **16.1** (20/20) | 8.4 | 0/20 | 43.1, 42.4 / 42.0 | pointer frames | Bead start pacing |
| control, bar click | open (click) | 8.0 / 8.3 / **15.1** | **15.1** (20/20) | 8.7 | 0/20 | 37.6, 38.0 / 10.0 | pointer frames | Bead start pacing |
| control, bar click | close (key) | 8.0 / 8.3 / **17.9** | **17.9** (20/20) | 8.4 | 0/20 | 41.1, 41.3 / 41.5 | pointer frames | Bead start pacing |
| chat / AI (left) | open | 8.0 / 9.0 / **16.2** | **16.2** (18/18) | 9.2 | 0/18 | n/a (niri 18.4, 18.6) / 12.2 | 8.0 / 8.2 / 8.4 | Bead start pacing |
| chat / AI | close | 8.0 / 10.3 / **16.0** | **16.0** (18/18) | 10.6 | 0/18 | n/a / 10.4 | 8.0 / 8.2 / 8.4 | Bead start pacing |
| OSD (volume) | show+hide | PANEL_SLIDE max 10.9 | - | - | - | niri 39.2, 36.7 / 38.2 | 8.2 / 8.6 / 10.9 | ok (latency includes the fixture's wpctl) |
| toast | show+hide | PANEL_SLIDE max 9.1 | - | - | - | niri 4.2, 4.4 / 4.4 | 8.0 / 8.2 / 27.8; cold 38.0 | ok shell-side; niri gaps are bar updates and the first toast mapping its surface |

Notes: the hover "open" phase of control-click draws no traced motion (hovering the bell only
highlights it); its niri columns are cursor frames, as are all niri columns of pointer-driven
rows. The edge-hover first frame includes the 0.9 s dwell the harness waits before reading.

## First open vs warm open

| panel | first frame cold | warm median | cold extra | cold settle |
| --- | --- | --- | --- | --- |
| dashboard | 62.5 ms | 12.8 ms | +50 ms | 108 / 495 ms late final frame |
| launcher | 54.7 ms | 12.9 ms | +42 ms | - |
| control (key) | 48.5 ms | 13.6 ms | +35 ms | - |
| control (click) | 37.8 ms | 10.0 ms | +28 ms | - |
| control (edge hover) | 431.6 ms | 392.4 ms | +39 ms | - |
| chat / AI | not traced (app trace starts 2 s after spawn); niri 18.5 ms | 12.2 ms | ~+7 ms to first composited frame | - |
| OSD | 38.0 ms | 38.2 ms | none | - |
| toast | 4.3 ms | 4.4 ms | none | - |

## Idle cost

Shell process tree (chat adds its app process) and daemon; two sessions per row.

| state | shell CPU % | shell wakeups/s | shell RSS peak MiB | shell PSS MiB | daemon CPU % | daemon wakeups/s |
| --- | --- | --- | --- | --- | --- | --- |
| no panel, 300 s | 0.019 / 0.019 | 1.9 / 1.9 | 81.2 / 73.9 | 61.6 / 61.4 | 0.075 / 0.061 | 6.3 / 6.3 |
| dashboard open, 60 s | **0.398 / 0.405** | **9.4 / 9.4** | 101.4 / 101.3 | 85.5 / 85.7 | 0.202 / 0.202 | 11.7 / 12.2 |
| launcher open | 0.032 / 0.033 | 2.2 / 2.3 | 96.1 / 95.8 | 79.3 / 79.2 | 0.077 / 0.076 | 6.2 / 6.3 |
| control open (key) | 0.026 / 0.025 | 2.3 / 2.2 | 94.0 / 93.9 | 78.8 / 79.0 | 0.075 / 0.074 | 6.2 / 6.3 |
| control open (edge hover) | 0.023 / 0.025 | 2.0 / 2.0 | 92.7 / 91.6 | 77.7 / 76.3 | 0.060 / 0.067 | 6.2 / 6.2 |
| control open (bell click, pointer on the bell) | 0.063 / 0.063 | 8.9 / 8.8 | 100.0 / 100.0 | 85.0 / 84.6 | 0.072 / 0.074 | 6.2 / 6.3 |
| chat open (3 processes) | 0.026 / 0.026 | 3.6 / 3.7 | **160.6 / 158.3** | 111.2 / 110.6 | 0.058 / 0.062 | 6.2 / 6.3 |

The shell's RSS rises 11-20 MiB over the no-panel idle once panels have been opened (the
known post-first-open growth, N0251); the chat sidebar adds ~65 MiB RSS (~32 MiB PSS) as its
own process. OSD and toast hide themselves, so they have no open-idle row.

## Top 3 to fix

1. **Bead: pace the first frames of a transition.** Do not commit 3-4 frames before the first
   frame callback; that is the one visible 12-20 ms gap in 97 % of opens and closes of every
   panel, and the only gate failure for launcher, control, chat and control-click. Without it
   their maximum here is the mid-motion 8.4-10.6 ms.
2. **Shell: warm the panel views.** Cold first opens cost +28-50 ms first-frame latency (and a
   108-495 ms late last frame on the dashboard): prebuild the dashboard, launcher and control
   views at startup (warm panels, N0231).
3. **Bead: commit the end-of-motion repaint with the motion (or drop it when nothing changes).**
   It waits up to 5 s for an unrelated wakeup, so every PANEL_SLIDE marker for launcher and
   control reports a ~5 s "gap" and the dashboard's 34-761 ms; fixing it makes the marker a
   usable gate for tonight's fixes and removes a stray full-panel repaint.

Also seen: the open dashboard's 2 s CPU/memory gauge repaint costs 0.40 % CPU and 9.4
wakeups/s, 15-20x any other open panel; and the dashboard has the only mid-motion misses.

## Method

- **Host**: desktop TEST host (`ssh desktop`), AMD Ryzen 5 7600 (12 threads), kernel
  7.3.0-5-cachyos, labwc 0.20.2 (wlroots 0.20.2) with gles2 on the Radeon 610M render node
  (`/dev/dri/renderD129`), stock niri 26.04, Mesa 26.2.5, libwayland 1.26.0.
- **Session**: the N0308 private nested session (`tools/bench/noctalia/bench.py`): private
  D-Bus, headless labwc at **1920x1080@120 Hz**, nested niri at **scale 1** (its winit output
  advertises 60 Hz; the shell reads `OUTPUT_FRAME_INTERVAL ns=8333333`), private PipeWire, fake
  UPower/BlueZ, the static N0300 wallpaper and the dark palette. The shell runs with its product
  flags (`--bar --core-views --ui-provider --notices`) and default renderer (CPU raster for bar
  and panels). The chat sidebar is an on-demand app process; the harness runs it directly
  (no user manager in the private session).
- **Binaries**: release build of `86433e4a4` (`nacre-ci/arch-nacre:current`, rustc 1.99.1,
  unstripped, otherwise the product profile; nacre-shell sha256 `9abcde84…`), the shell tree and
  shell-tools from the same commit, Bead's `private-pointer` example as the private
  virtual-pointer actuator (sha256 `fb457686…`).
- **Frames**: `BEAD_FRAME_TRACE=1` makes the shell print one `UI_FRAME` line per committed frame
  (`gap_ms` since the previous frame of the same kind of surface, `motion=<id>` while a motion
  runs). Per transition the harness takes the motion frames between that call and the next one;
  gaps are pooled over the warm repeats for p50/p95/max. "niri" columns are the frames the
  nested niri composited to labwc (`WAYLAND_DEBUG=client` commits), from the call to the end of
  the last burst of 3+ frames: what reached the screen. `PANEL_SLIDE` is Bead's own per-slide
  marker (logged only on some paths). Overlays (OSD, toast) are not in the UI_FRAME trace: their
  shell-side stall measure is `PANEL_SLIDE max_gap_ms`, and their niri gaps are taken inside
  bursts only (< 100 ms), since show and hide are two bursts around the hold.
- **First frame**: call to the first motion frame (ms). Dispatched panels go through
  `nacre-helper ctl call ui.dispatch` (the keybinding path, client process start included);
  `control-hover` includes its 0.9 s edge dwell; `control-click` is timed from the click.
  Cold = the first open of that panel in a fresh session; warm = the median of the others.
  The UI_FRAME trace starts 2 s after a process starts, so the chat app's cold open has no
  shell-side frames (niri columns only).
- **Openers**: dashboard `drawers toggle dashboard`; launcher `nacre launcher`; control
  `drawers toggle right`; control-hover pointer to the right-edge grabber (1917, 540) for 0.9 s;
  control-click hover the bar bell (1762, 25) for 0.6 s, then click (the click opens the
  docked control panel on its notifications page; the hover draws no traced motion); chat
  `nacre left`; OSD a daemon `audio.set_volume`; toast a D-Bus `Notify` (2.5 s). Closes:
  `nacre close` (for control-click too: a second bell click keeps the panel open on main), the
  same toggle for chat; OSD and toast hide themselves.
  Each transition waits 1.2 s (OSD/toast 4 s).
- **Idle**: shell and daemon process trees sampled from `/proc` every 5 s: CPU % of one core
  from per-thread schedstat ns, wakeups/s as voluntary + involuntary context switches, RSS
  (sum of VmRSS, peak) and PSS (end). "No panel" is 300 s after a 30 s settle; "panel open" is
  60 s with that panel held open, starting 3 s after it opened.
- **Guards**: each capture runs under `flock -o ~/nacre-lead/desktop-heavy.lock` in a 4 GiB
  no-swap user scope at nice 10, and is void if host swap-in passes 8 MiB, load1 >= 4 or CPU
  pressure avg10 >= 5 % (lead waiver, 2026-11-14). `who -u`, `free`, `pswpin` and load are
  logged before, at lock time and after.

## Limits

- Nested 1920x1080 at scale 1: the owner's laptop (2880x1800@120, scale 1.5) paints ~2.5x the
  pixels on the CPU raster path, so warm frames costing up to ~3.6 ms here could approach the
  8.3 ms budget there. A scale-1.5 row is the obvious next capture.
- One host, one nested compositor stack; the laptop live audit remains the acceptance.
- The first capture of control-click and toast (m3) is excluded: a second bell click no longer
  closes the docked panel on main, so it stayed open and suppressed the toasts.
- Captures: idle1, idle2; m1, m1b (dashboard, launcher, control); m2, m2b (edge hover, chat);
  m3 (OSD only), m3b, m3c (control-click, OSD, toast). Raw logs stay on the desktop host under
  `~/nacre-lead/bench/out`.

## Re-run

```sh
# stage: bin/{nacre-shell,nacre-helper,bead-wallpaper-video,nacre-wallpaper-decode,wlr-randr,
# private-pointer}, fonts/ and nacre/{shell,nacre/shell-tools,config/niri} as in noctalia/stage.sh
rsync -a tools/bench/shell_motion.py tools/bench/noctalia desktop:nacre-lead/bench/harness/
H=~/nacre-lead/bench/harness/shell_motion.py; S=~/nacre-lead/bench/stage
run() { flock -o ~/nacre-lead/desktop-heavy.lock systemd-run --user --scope -p MemoryMax=4G \
  -p MemorySwapMax=0 -- nice -n 10 python3 -I $H --stage $S --pointer $S/bin/private-pointer "$@"; }
run --out out/idle --settle-s 30 --idle-s 300
run --out out/m1 --panels dashboard,launcher,control
run --out out/m2 --panels control-hover,chat
run --out out/m3 --panels control-click,osd,toast
python3 -I $H --analyze out/m1 out/m2 out/m3
```
