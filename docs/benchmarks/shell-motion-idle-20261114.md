# Shell motion and idle benchmark, 2026-11-14

> **Swap gate waived** (desktop swap 2.5 GiB from the owner's tty1 session); shell/daemon
> SwapPss = 0, host pswpin per capture recorded. Informational benchmark numbers for judging
> tonight's fixes, not acceptance evidence (the laptop live audit stays the gate).

Baseline of Nacre `main` at `86433e4a4` (Bead `841de925`): panel open/close frame gaps, idle
cost and first-open vs warm-open, against the owner's gate of **max gap <= 12 ms** (120 Hz).

**First pass (one run per panel, 9 warm + 1 cold transitions); the second pass and the
merged table follow below when complete.**

| panel | dir | shell p50 / p95 / max ms | max before final frame | cold max | first frame cold / warm ms | niri p50 / p95 / max | gate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| dashboard | open | 8.0 / 8.3 / **761.4** | **19.9** | **108.2** | 63.9 / 12.8 | 8.0 / 8.2 / 16.0 | over (final frame, start) |
| dashboard | close | 8.0 / 11.6 / **16.0** | 16.0 | 12.5 | 10.8 / 11.4 | 8.0 / 8.2 / 8.3 | over (start) |
| launcher | open | 8.0 / 8.3 / **16.2** | 16.2 | 15.4 | 54.1 / 12.9 | 8.0 / 8.2 / 21.9 | over (start; final frame ~5 s late) |
| launcher | close | 8.0 / 8.4 / **16.0** | 16.0 | 15.9 | 13.2 / 12.9 | 8.0 / 8.2 / 8.4 | over (start) |
| control (key/IPC) | open | 8.0 / 8.4 / **15.1** | 15.1 | 8.7 | 48.0 / 13.7 | 8.0 / 8.2 / 8.4 | over (start; final frame ~5 s late) |
| control (key/IPC) | close | 8.0 / 8.3 / **16.1** | 16.1 | 16.0 | 12.3 / 11.2 | 8.0 / 8.2 / 8.4 | over (start) |
| control (edge hover) | open | 8.0 / 8.4 / **16.1** | 16.1 | 8.4 | 430.9 / 392.4 (0.9 s dwell) | pointer-polluted | over (start) |
| control (edge hover) | close | 8.0 / 8.4 / **16.1** | 16.1 | 15.8 | 43.1 / 42.1 | pointer-polluted | over (start) |
| chat (AI, left) | open | 8.0 / 9.0 / **16.2** | 16.2 | n/a (trace) | n/a / 12.1 (niri 18.4 / 11.1) | 8.0 / 8.0 / 8.3 | over (start) |
| chat (AI, left) | close | 8.0 / 10.3 / **16.0** | 16.0 | n/a | n/a / 10.4 | 8.0 / 8.2 / 8.3 | over (start) |
| OSD (volume) | show+hide | not traced; PANEL_SLIDE max 8.8 | - | - | niri 39.6 / 38.6 | 8.0 / 8.6 / 8.8 | ok |


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
  control-click hover the bar bell (1762, 25) for 0.6 s, then click (the click grows the peek
  into the control panel; the hover itself draws no traced motion); chat `nacre left`; OSD a
  daemon `audio.set_volume`; toast a D-Bus `Notify` (2.5 s). Closes: `nacre close`, the same
  toggle for chat, a second click for control-click; OSD and toast hide themselves.
  Each transition waits 1.2 s (OSD/toast 4 s).
- **Idle**: shell and daemon process trees sampled from `/proc` every 5 s: CPU % of one core
  from per-thread schedstat ns, wakeups/s as voluntary + involuntary context switches, RSS
  (sum of VmRSS, peak) and PSS (end). "No panel" is 300 s after a 30 s settle; "panel open" is
  60 s with that panel held open, starting 3 s after it opened.
- **Guards**: each capture runs under `flock -o ~/nacre-lead/desktop-heavy.lock` in a 4 GiB
  no-swap user scope at nice 10, and is void if host swap-in passes 8 MiB, load1 >= 4 or CPU
  pressure avg10 >= 5 % (lead waiver, 2026-11-14). `who -u`, `free`, `pswpin` and load are
  logged before, at lock time and after.

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
