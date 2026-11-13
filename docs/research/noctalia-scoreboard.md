# Noctalia vs Nacre scoreboard

Living record of the head-to-head. Goal (owner, 2026-11-13): Nacre matches or
beats Noctalia on resource use. One row per measured set, newest last; cells are
**Noctalia / Nacre** medians over the row's runs. Harness and the one-command
runs: [`tools/bench/noctalia/README.md`](../../tools/bench/noctalia/README.md).
History and the original method: [N0300](noctalia-vs-nacre-benchmark-20261112.md).

## Rows

| Row | Runs N/Nc | Nacre | Noctalia | Idle CPU % | Idle niri CPU % | Idle wakeups/s | Interaction CPU % | Interaction stack CPU % | Interaction GPU % | PSS MiB | Unreclaimable MiB | GPU buffers MiB | Growth MiB | Launch s | Launcher first frame ms | Launcher late frames | PPT idle W | PPT interaction W |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| N0300 desktop 2026-11-13 (baseline, prior harness) | 3/3 | 382f36de6 | v5.4.1 | 0.12 / 0.42 | n/a | 7.1 / 15.3 | 1.2 / 23.5 | n/a | host busy +9.9 / +2.9 pp | 119 / 127 | 63 / 68 | n/a | +3.7 / +31.4 | 0.219 / 0.113 | n/a | n/a | -0.05 / -0.25 | +0.58 / +0.45 |
| desktop smoke 2026-11-13 (`short`, one run each; indicative) | 1/1 | 5cb5e22a2 | v5.4.1 | 0.127 / 0.079 | 0.022 / 0.025 | 8.0 / 15.2 | 1.3 / 17.4 | 3.1 / 18.6 | 2.8 / 0.0 | 119 / 131 | 62 / 71 | 99 / 0 | +2.1 / +49.8 | 0.131 / 0.018 | 18 / 12 | 0 / 0 | +0.08 / -0.27 | +0.93 / +0.73 |
| dev-2 2026-11-13 (`short`, software rendering) | 2/2 | 26fd8a62e | v5.4.1 | 0.074 / 0.063 | 0.117 / 0.194 | 9.2 / 10.5 | 13.4 / 9.0 | 124.4 / 110.0 | n/a | 190 / 110 | 122 / 58 | n/a | +2.6 / +25.7 | 0.159 / 0.262 | 17 / 13 | 0 / 0 | n/a | n/a |
| desktop 2026-11-13, 3/3 `chunk` at 26fd8a62e | pending | | | | | | | | | | | | | | | | | |

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
  The 60 s rest of `short` makes its growth "memory kept after first use".
- **Desktop 3/3 pending**: the six `chunk` arms are staged at 26fd8a62e on the
  desktop, but the desktop rule (swap < 64 MiB) blocks them: the owner's
  session holds 2.6 GiB in zram after a game session. They start with
  `host.sh desktop chunk nacre,noctalia,noctalia,nacre,nacre,noctalia` once
  swap is reset.

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

1. **Interaction CPU on real hardware.** Desktop smoke (before N0306): 17.4 %
   of a core against 1.3 % (Noctalia renders on the GPU: +2.8 % GPU, niri 1.5 % against
   1.0 %). At 26fd8a62e about a third of it is the daemon re-rendering the
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
2. **CPU does not return to idle after first use.** In the minute after the
   interaction the shell uses 0.95 % and the daemon 0.31 % (idle before: 0.018
   and 0.046 %): shell main thread 0.54 %, its `nacre-link` thread 0.40 %,
   daemon main thread 0.23 % at 17-18 wakeups/s. N0300 saw the same over 19
   minutes. Noctalia is back at 0.076 % at once.
3. **Memory kept after first use.** Shell PSS 59.6 → 84.5 MiB and
   unreclaimable 36 → 60 MiB after the first panel opens, kept to the end
   (dev-2; desktop smoke +49.8 MiB at 5cb5e22a2). Noctalia grows 2-3 MiB.
4. **Idle wakeups.** 10.5 against 9.2 per second (dev-2; 15.2 against 8.0 on
   the desktop smoke, before N0305): daemon main thread 4.1/s,
   `nacre-offload` 2.1/s, a shell `blocking-1` thread on a 500 ms timer 2.0/s.
   Idle CPU is still lower than Noctalia's.
5. **Dashboard open stalls.** In every Nacre run on both hosts, 2-3 of 5
   dashboard opens have one 54-97 ms gap between composited frames; the
   launcher has none and Noctalia's control center has none.
6. **Launch on dev-2.** Bar listed after 0.262 s against 0.159 s with software
   rendering; on the desktop Nacre lists its bar first (0.018 s against
   0.131 s), so this is CPU-bound start-up cost, not GL initialisation.

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
