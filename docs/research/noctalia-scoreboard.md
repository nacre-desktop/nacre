# Noctalia vs Nacre scoreboard

Living record of the head-to-head. Goal (owner, 2026-11-13): Nacre matches or
beats Noctalia on resource use. One row per measured set, newest last; cells are
**Noctalia / Nacre** medians over the row's runs. Harness and the one-command
runs: [`tools/bench/noctalia/README.md`](../../tools/bench/noctalia/README.md).
History and the original method: [N0300](noctalia-vs-nacre-benchmark-20261112.md).

## Rows

| Row | Runs N/Nc | Nacre | Noctalia | Idle CPU % | Idle niri CPU % | Idle wakeups/s | Interaction CPU % | Interaction stack CPU % | Interaction GPU % | PSS MiB | Unreclaimable MiB | GPU buffers MiB | PSS+GPU MiB | PSS+GPU after use MiB | Growth MiB | Launch s | Launcher first frame ms | Launcher late frames | PPT idle W | PPT interaction W | Interaction energy J |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| N0300 desktop 2026-11-13 (baseline, prior harness) | 3/3 | 382f36de6 | v5.4.1 | 0.12 / 0.42 | n/a | 7.1 / 15.3 | 1.2 / 23.5 | n/a | host busy +9.9 / +2.9 pp | 119 / 127 | 63 / 68 | n/a | n/a | n/a | +3.7 / +31.4 | 0.219 / 0.113 | n/a | n/a | -0.05 / -0.25 | +0.58 / +0.45 | n/a |
| desktop smoke 2026-11-13 (`short`, one run each; indicative) | 1/1 | 5cb5e22a2 | v5.4.1 | 0.127 / 0.079 | 0.022 / 0.025 | 8.0 / 15.2 | 1.3 / 17.4 | 3.1 / 18.6 | 2.8 / 0.0 | 119 / 131 | 62 / 71 | 99 / 0 | 218 / 131 | n/a | +2.1 / +49.8 | 0.131 / 0.018 | 18 / 12 | 0 / 0 | +0.08 / -0.27 | +0.93 / +0.73 | n/a |
| dev-2 2026-11-13 (`short`, software rendering) | 2/2 | 26fd8a62e | v5.4.1 | 0.074 / 0.063 | 0.117 / 0.194 | 9.2 / 10.5 | 13.4 / 9.0 | 124.4 / 110.0 | n/a | 190 / 110 | 122 / 58 | n/a | n/a | n/a | +2.6 / +25.7 | 0.159 / 0.262 | 17 / 13 | 0 / 0 | n/a | n/a | n/a |
| desktop 2026-11-16, 3/3 `chunk` at f3997131c (Bead 285aede6) | 3/3 | f3997131c | v5.4.1 | 0.127 / 0.034 | 0.023 / 0.028 | 6.8 / 4.2 | 1.1 / 5.1 | 2.7 / 6.3 | 2.9 / 0.0 | 133 / 116 | 63 / 57 | 101 / 0 | 234 / 116 | 211 / 158 | +1.7 / +42.8 | 0.133 / 0.028 | 19 / 12 | 0 / 0 | -0.23 / -0.13 | +0.13 / -0.02 | +7.9 / -1.4 |
| desktop 2026-11-25 at 37939e8ae (Bead a797e500), with lease drop | 3/3 | 37939e8ae | v5.4.1 | 0.122 / 0.032 | 0.022 / 0.027 | 6.8 / 4.2 | 1.2 / 3.3 | 3.0 / 4.6 | 2.9 / 0.0 | 129 / 117 | 64 / 58 | 101 / 0 | 230 / 117 | 204 / 163 | +0.8 / +45.7 | 0.132 / 0.017 | 18 / 12 | 0 / 0 | +0.04 / -0.16 | +0.58 / -0.21 | +35 / -13 |

Columns: CPU is % of one core. "Idle", "Interaction", PSS, unreclaimable and
GPU buffers are shell + daemon (Noctalia has no daemon). "Stack" adds the nested
niri and its labwc parent, which both shells drive identically. Idle wakeups
are voluntary + involuntary context switches per second. PSS, unreclaimable
(anon + shmem + swap) and GPU buffers (amdgpu fdinfo resident, not in PSS) are
taken at the end of the idle window; growth is PSS at the end of the arm minus
that. Launch is shell start to its bar layer listed by niri. First frame is the
IPC open call (client start included) to the first frame niri composites; late
frames are animation gaps over 25 ms. PPT is package power above the arm's own
no-shell baseline. Rows before N0308 used one-tick CPU over the last idle
minute and host-global GPU busy; their columns are not comparable cell for cell.
PSS+GPU is the memory v0.1 row 73 gates (GL buffers are outside PSS): at the end
of idle, and "after use" at the end of the arm. Interaction energy is that PPT
over the interaction window, in joules; it is reported, not gated. The 11-16 row's
new cells come from re-summarizing its runs.

Panel open latency (first frame ms, late frames in brackets; Noctalia / Nacre),
five opens per panel, each the one call that shell's key sends. Noctalia's
control center stands in for Nacre's dashboard and control panel; Nacre's session
(power menu) and clipboard are launcher routes:

| Row | Runs N/Nc | Nacre | Launcher | Dashboard | Control | Session | Clipboard |
| --- | --- | --- | --- | --- | --- | --- | --- |
| desktop 2026-11-16 at f3997131c | 3/3 | f3997131c | 19 / 12 (0 / 0) | 19 / 12 (0 / 0) | n/a | n/a | n/a |
| desktop 2026-11-25 at 37939e8ae | 3/3 | 37939e8ae | 18 / 12 (0 / 0) | 18 / 12 (0 / 0) | 18 / 12 (0 / 0) | 18 / 12 (0 / 0) | 18 / 12 (0 / 0) |

`summarize.py` prints both rows and the row 73 gate: Nacre at or under Noctalia on
idle CPU, idle wakeups, launch, PSS+GPU before and after first use, interaction
CPU (shell + daemon against Noctalia's whole shell) and every panel's first frame.
At f3997131c and at 37939e8ae only interaction CPU fails (5.1, then 3.3 against
1.2 %).

Row notes:

- **26fd8a62e** contains N0306 (lock-content PNG), N0307 (wallpaper pacing),
  N0227, NN0036 and N0236, whose Bead pin `a6fbc10f` predates Bead's B0254/B0255
  hot-path batch (`9f828aaa`); every row here is before that batch. Nacre does
  not set Bead's `warm_source_budget`, so all Nacre runs have warm panels off.
- **Desktop smoke**: the harness validation pair at 5cb5e22a2 (before N0305,
  N0306, N0307, N0227 and NN0036), one `short` run per arm, so read it for
  direction only.
- **dev-2**: llvmpipe for niri and Noctalia, pixman for labwc, CPU raster for
  Nacre; Noctalia pays its GL work in CPU there and maps llvmpipe into its PSS.
  The 60 s rest of `short` makes its growth "memory kept after first use". The
  shared host swapped (2.5 GiB host-wide swap-in during one Nacre arm, none in
  the measured trees); both Nacre runs still agree within 0.1 point on CPU.
  Nacre's launch was 0.016 s and 0.507 s (the slow one under that swap-in), so
  its median launch cell means nothing here. These runs used
  `nacre-ci/arch-nacre` plus in-job packages; the harness now uses
  `nacre-ci/arch-nacre-session`, which carries the same packages.
- **Desktop 2026-11-16** supersedes the pending 26fd8a62e `chunk` row (those
  arms never ran; the desktop gate blocked them). Run on the desktop TEST host
  (AMD iGPU, real power) as `host.sh desktop chunk nacre,noctalia,noctalia,nacre,nacre,noctalia`,
  staged from a dev-2 build of f3997131c with Bead `285aede6`, in a private
  nested session; all six arms ran once with no retry, swap 0 B, no host swap-in,
  and 3/3 arms per shell agree within a few percent. Since 26fd8a62e: Bead's
  B0254/B0255 hot-path batch and B0294, N0306 (lock-content PNG), N0321, N0325
  (recorder topic, off-thread fast ticks) and N0332 (daemon-topic diet). Against
  the desktop smoke (5cb5e22a2): interaction CPU 17.4 to 5.1 %, idle wakeups 15.2
  to 4.2/s, idle PSS 131 to 116 MiB, growth +49.8 to +42.8 MiB. Nacre now wins
  idle CPU, idle wakeups, idle memory and launch; it still loses interaction CPU
  (5.1 against 1.1 %; stack 6.3 against 2.7 %, Noctalia spends 2.9 % GPU, Nacre
  none) and memory growth. Top Nacre thread in the interaction window: shell
  `bead-worker` at 2.6 % of a core (about half of Nacre's total); no Nacre thread
  is above 0.02 % in the idle window. The 90 s after the cycles ("rest") stays
  above idle: shell 0.43 % and daemon 0.35 % against Noctalia's 0.13 %.
- **Desktop 2026-11-25** (v0.1 row 73 baseline): built natively on the desktop
  at 37939e8ae (Bead `a797e500`, Shell's dashboard lease drop be3d40187
  included), Mesa 26.3.1. Noctalia's kept v5.4.1 build loads libjxl 0.12 from
  `stage/noctalia/lib` (the host has 0.13). Noctalia arms 2, 3 and 6 of the
  ABBA set ran 00:28-01:11; its Nacre arms failed at the frame probe (the
  session call lacked the power menu's empty query) and were rerun 01:13-01:45
  as three Nacre arms; swap 0 B, no host swap-in; per shell the three runs'
  interaction CPU spans 7 % (Noctalia) and 2 % (Nacre).
  Against 11-16: interaction CPU 5.1 to 3.3 % (shell 2.5 %: `bead-worker`
  1.1, main 0.9, `nacre-link` 0.5; daemon 0.8 %), and the rest window is back at
  idle (daemon 0.35 to 0.014 %, shell 0.43 to 0.015 %): a closed dashboard no
  longer holds the `dashboard.system` sampler. Growth stays: daemon PSS +26.6
  MiB and shell +19.4 MiB from first use. All five panels open in 12 ms against
  18 ms. Noctalia's interaction draws +0.58 W over its no-shell baseline (35 J);
  Nacre's is within baseline noise.

Pins: Noctalia v5.4.1 is `815aebe1cfd79c9a18197441eea92f0b275035fc` (built from
source in the row's stage); stock niri is the distribution package of the day
(`pins.json` and each run's `result.json` keep binary hashes).

## Where Nacre still loses

Ranked by size, from the rows above and a `perf` profile of the interaction
window (dev-2, 26fd8a62e, `BENCH_PERF=1`; shares are of all shell + daemon
samples). Where Nacre already wins: shell + daemon memory before first use
(131 MiB PSS with no GPU buffers against Noctalia's 119 MiB PSS plus 99 MiB of
GPU buffers on the desktop), idle CPU, launcher open (first frame 12-13 ms
against 17-18 ms, no late frames) and package power (within noise).

1. **Interaction CPU on real hardware.** Desktop 2026-11-16 (f3997131c): 5.1 %
   of a core against 1.1 %, shell `bead-worker` thread 2.6 % (down from 17.4 %
   at the smoke, before N0306 and Bead's batch; the profile below is the older
   26fd8a62e one). Noctalia renders on the GPU: +2.9 % GPU, niri 1.4 % against
   1.0 %. At 26fd8a62e about a third of it is the daemon re-rendering the
   lock-screen dashboard image while notifications arrive, in `nacre-helper`
   `tokio-rt-worker` threads: `image_paint::composite` 15.9 %,
   `orient_decoder::resample::pass_region` 8.8 %, `image_paint::paste`,
   `rounded_mask` and `polygon` 5.9 %, HarfBuzz text about 3 %,
   `lock_dashboard::render` 1.4 %, `fdeflate` 0.9 % (N0306 already removed most
   of the PNG cost). The shell main thread is about 23 %: Luau
   (`luau_execute` 2.8 %, GC `luaM_freegco_` 1.3 %, `mlua` table iteration
   1.2 %) and view building and layout (`bead_ui::layout::ViewSpec::from_raw::build`
   1.1 %, taffy flexbox 1.1 %, `Props::get` 0.9 %, `TreeCache::prepare` 0.8 %,
   `FrameDamage::update` 0.7 %), which is what Bead's B0254/B0255 batch targets.
   The shell's `nacre-link` thread (daemon link: `serde_json::Value::clone`,
   framing deserialize) is 3.5 % and daemon-side JSON serialization 1.4 %.
2. **CPU does not return to idle after first use** (still true at f3997131c:
   rest 0.43 % shell, 0.35 % daemon against 0.13 %; figures below are 26fd8a62e). In the minute after the
   interaction the shell uses 0.95 % and the daemon 0.31 % (idle before: 0.018
   and 0.046 %): shell main thread 0.54 %, its `nacre-link` thread 0.40 %,
   daemon main thread 0.23 % at 17-18 wakeups/s. N0300 saw the same over 19
   minutes. Noctalia is back at 0.076 % at once.
3. **Memory kept after first use.** Shell PSS 59.6 → 84.5 MiB and
   unreclaimable 36 → 60 MiB after the first panel opens, kept to the end
   (dev-2; desktop smoke +49.8 MiB at 5cb5e22a2). Noctalia grows 2-3 MiB.
4. **Idle wakeups** (no longer a loss on the desktop at f3997131c: 4.2 against
   6.8 per second). Earlier: 10.5 against 9.2 per second (dev-2; 15.2 against 8.0 on
   the desktop smoke, before N0305): daemon main thread 4.1/s,
   `nacre-offload` 2.1/s, a shell `blocking-1` thread on a 500 ms timer 2.0/s.
   Idle CPU is still lower than Noctalia's.
5. **Dashboard open stalls: mostly a harness artifact, with a Bead residue
   (N0331).** The rows above show 2-3 of 5 Nacre dashboard opens with one
   54-97 ms gap between composited frames. Shell's capture
   (`~/.local/share/nacre/ai/lead/shell-allhands/dash-gap-evidence/`) shows the
   slide itself commits every 16.0 ± 0.3 ms. The 54-57 ms gaps are the
   dashboard's 2 s system-gauge re-stage, drawn within 100 ms after the slide's
   last frame, which the probe's 100 ms burst rule merged into the open. The
   3.03 s probe period drifts that phase, hence "2-3 of 5". In Shell's capture
   the 90 ms gap on the first (cold) open is the slide's terminal pose waiting
   for that next redraw. Since N0331 the harness opens the dashboard with one
   call, as Super+O does, and a gap over 25 ms after the 360 ms motion ends the
   burst. The rows above predate that, and the matrix was not re-run. What
   remains is Bead's, sent to the Bead lead: the late cold-open terminal pose
   and a 36 ms wait for the first frame callback on warm opens, which niri's
   frames do not show. The launcher and Noctalia's control center have no
   gaps.

## Method (N0308)

Review of N0300 (the baseline row): careful about isolation and provenance,
but its headline compared different things. "Interaction CPU 1.2 % vs 23.5 %"
set a CPU-raster shell plus a daemon against a GL shell, without the compositor
work each shell causes, and its idle CPU had one-tick resolution (0.017 % of a
core, so 0.117 % was exactly 7 ticks in every run). Package power, the one
renderer-neutral number, had Nacre lower and sat in the small print. What the
harness does now:

| Change | Why |
| --- | --- |
| niri and labwc trees sampled per arm; "stack" CPU = shell + daemon + niri + labwc | A CPU-raster shell's SHM buffers make niri upload textures on the CPU; a dmabuf shell's don't. Same compositors and config in both arms, so their difference is shell-caused. |
| CPU from per-thread `schedstat` (ns) over the whole idle window (570 s) | N0300's ticks over 60 s could not resolve the idle numbers it published; `cpu_pct_ticks` keeps the old method for cross-checks. |
| Per-thread CPU and wakeups, named by tree, process and thread | Turns "daemon 11.8 %" into a thread the owners can act on. Context switches stay a wakeup proxy, not timer wakeups. |
| GPU time and GPU buffer memory per tree from amdgpu `fdinfo` (desktop) | A GL shell's textures and swapchain are outside PSS; Nacre's SHM buffers are inside it. Memory is only comparable with both. |
| Frame probe: niri runs with `WAYLAND_DEBUG=client`, so each composited frame is a timestamped commit to labwc | Open-to-first-frame latency and animation gaps, identical for both shells; replaces the 100 ms-polled "layer listed" proxy. |
| Fixture: the `pactl subscribe` stand-in sleeps in-process | N0300's `tail -F` child woke once a second inside Nacre's measured tree only. |
| Private runtime dir under `/tmp` | Sockets need paths under 108 bytes, as in `/run/user/UID`; the desktop's long task path disabled Noctalia's dmenu socket. |
| Lock chunks release for 2 s every 450 s; a lost boundary or a failed memory gate retries the arm | Queued jobs get the shared desktop lock; no stitched windows. |

Still not measured: a matched-features arm (Nacre with the daemon producers
Noctalia lacks turned off needs a product switch); timer wakeups (`perf sched`);
idle growth over an hour; audible notification sounds (the private PipeWire has
no sink link, so Noctalia's sound streams time out). Noctalia clones its plugin
index from GitHub at startup (before the idle window), a product default kept as
is. Both shells see the same fake machine (two sinks, one source, a battery, one
Bluetooth adapter) and a static 1920x1080 wallpaper.
