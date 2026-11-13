# Noctalia vs Nacre benchmark harness

Repeatable head-to-head of Noctalia (pinned release) and Nacre (a commit) in a
private nested session. Results feed `docs/research/noctalia-scoreboard.md`.
Method review and history: N0300 (`docs/research/noctalia-vs-nacre-benchmark-20261112.md`).

## Files

| File | Role |
| --- | --- |
| `stage.sh` | builds Noctalia (`NOCTALIA_TAG`, default v5.4.1) and a Nacre checkout (unstripped release) in the `nacre-ci/arch-nacre` image into one stage dir: `bin/` (nacre-shell, nacre-helper, helpers, stock compositor copy, wlr-randr), `noctalia/`, `nacre/` (Luau shell, shell-tools, niri rules), `fonts/`, `pins.json` |
| `bench.py` | one arm: private D-Bus, headless labwc, nested stock niri 1920x1080@60, private PipeWire, fake UPower/BlueZ, then the shell; samples shell, daemon, niri and labwc trees once a second; runs the N0300 workload, then the frame probe; writes `result.json`, `samples.json`, logs and three screenshots |
| `run.sh` | runs a comma sequence of arms (an arm lost to lock contention or the desktop gate is retried), then `summarize.py` |
| `summarize.py` | median [min, max] table, the scoreboard row, top threads by CPU and wakeups |
| `host.sh` | the one-command wrapper: `stage` and `run` as detached Platform-slot containers on a build host, `desktop` on the desktop TEST host |
| `fixtures/` | activation-free D-Bus config, fake UPower/BlueZ, the pactl/wpctl/parec stand-in |

## Profiles

`short`: 30 s baseline, 240 s idle, 3 cycles, 60 s rest (about 7 min per arm).
`chunk`: 30 s baseline, 300 s idle, 6 cycles, 90 s rest (about 520 s): one
desktop-lock hold per arm, so other desktop jobs interleave between arms.
`full`: 60 s baseline, 600 s idle, 6 cycles, 1140 s rest (N0300's 30 min; on a
busy desktop, any job queued at a renewal invalidates the arm).
A cycle is 10 s: launcher open 0.8 s, close, dashboard/control center open
0.8 s, close, workspaces 2,1,3,1 acknowledged, four notifications.

## Commands

Build host (dev-2 or dev-1; software rendering: labwc pixman, llvmpipe. CPU there
penalises the GL shell; memory, wakeups and relative changes are usable):

```sh
rsync -a tools/bench/noctalia/ dev-2:nacre-lead/n0308/harness/
ssh dev-2 nacre-lead/n0308/harness/host.sh stage <full nacre commit>   # Noctalia + Nacre release build, about an hour
ssh dev-2 nacre-lead/n0308/harness/host.sh run short nacre,noctalia,noctalia,nacre
# results: dev-2:nacre-lead/n0308/out/<timestamp>/{summary.txt,table.md,run-*/}
```

`BENCH_PERF=1 host.sh run short nacre,noctalia perf` adds a flat `perf record`
of the shell and daemon trees over the interaction window (`perf.txt` per run,
sorted by thread, library and symbol; the staged Nacre binaries keep their
symbols). The container gets `CAP_PERFMON` and `perf` a file capability. These
arms are diagnostic: sampling perturbs CPU, so keep them out of the rows.

Both container steps use the prebuilt `nacre-ci/arch-nacre:current` image (N0415)
under `~/nacre-lead/platform-slot-run.sh`. `stage` checks the commit out to
`~/nacre-lead/n0308/src` on the host, mounts it read-only at `/workspace`, and
installs only what the image lacks for Noctalia: meson, ninja, nlohmann-json,
stb, wireplumber, libqalculate, libical, jemalloc, wlr-randr. Cargo fetches
crates itself for now: the read-only N0415 vendor set lacks Bead's `protocols/`
directory, which `bead-wayland` includes from outside its crate. `run` installs
only labwc, the PipeWire daemon and Noctalia's runtime libraries. `deps/orient` is
private; on a host without GitHub access unpack it into `src/deps/orient` first
(`git -C ~/Projects/orient archive <rev> | ssh dev-2 tar -x -C nacre-lead/n0308/src/deps/orient`).

Desktop TEST host (AMD iGPU render node, real power). Copy a build host's `stage/`
(without `bin/stock-niri`) to `~/nacre-lead/n0308/stage` there, then one command:

```sh
rsync -a tools/bench/noctalia/ desktop:nacre-lead/n0308/harness/
ssh desktop nacre-lead/n0308/harness/host.sh desktop chunk nacre,noctalia,noctalia,nacre,nacre,noctalia
# results: desktop:nacre-lead/n0308/out/<timestamp>/{summary.txt,table.md,run-*/}
```

The `desktop` step runs `run.sh` with `--render-node` (the AMD node), `--lock
~/nacre-lead/desktop-heavy.lock`, `--mem-rail-gib 3` and `--niri /usr/bin/niri`.
`--lock` first waits, without the lock and for up to 12 h, until the desktop
rules hold (swap used < 64 MiB, MemAvailable > swap + 8 GiB) and the host is
quiet (load1 < 4, CPU pressure avg10 < 5 %), so arms never run over the owner's
own use; it then takes the shared lock and checks again. The lock is held in
chunks under 590 s (released for 2 s every 560 s; if another job takes it, the
arm is invalid and `run.sh` retries it). `run.sh` wraps each arm in a 4 GiB
no-swap user scope with 90 s between arms. `--niri` selects the host's stock
compositor instead of the staged copy. Summarize again any time:
`summarize.py OUT --label 'desktop 2026-11-13'`.

## Metrics (per run, in `result.json`)

- `windows.<idle|interaction|cycle1|cycles_rest|rest>.<tree>`: `cpu_pct`
  (per-thread schedstat ns, % of one core; misses children that live under a
  second), `cpu_pct_ticks` (the N0300 utime+stime+reaped-children method, tick
  resolution, for cross-checking), `ctx_per_s` (voluntary + involuntary
  switches, a wakeup proxy), `gpu_pct` (amdgpu fdinfo gfx time, desktop only).
  `combined` is shell + daemon; the summary's `stack` adds niri and labwc.
  `idle` starts 30 s after the bar is mapped and ends at the first interaction.
- `windows.*.threads`: top threads by CPU and by wakeups, summed per tree,
  process and thread name.
- `memory.<startup|pre-interaction|post-interaction|end>`: PSS, unreclaimable
  (Pss_Anon + Pss_Shmem + SwapPss) and `gpu_mib` (amdgpu fdinfo resident
  buffers, desktop only; GL buffers are not in PSS, shared buffers count in the
  compositor too) per tree; `memory_growth_mib` is combined PSS at end minus at
  the end of idle.
- `readiness_s`: shell launch to its bar layer listed by the compositor
  (polled at 5 ms).
- `open_frames.<launcher|dashboard>`: the frame probe, five opens after the
  measured windows. niri runs with `WAYLAND_DEBUG=client`, which timestamps its
  own commits to labwc, one per composited frame, for both shells alike.
  `first_ms` is the IPC call (client process start included) to the first
  composited frame, `frames`/`span_ms` the open animation burst (frames closer
  than 100 ms), `max_gap_ms` its longest gap and `late` its gaps over 25 ms.
- `power.<window>`: PPT, GPU busy and VRAM means minus the last 30 s of the
  no-shell baseline (host sensors; the baseline already contains the
  compositors and fixture).
- `renderer`, `notification_owner`, `wallpaper_video_helper`, `host_swapin_mib`
  and `pins` record the fixture.

## Known limits

- Both arms see the same fake machine (two sinks, one source at 45 %, a
  battery at 63 %, one Bluetooth adapter); Noctalia reads PipeWire, Nacre's
  daemon reads the `pactl` stand-in. No real audio, network or brightness.
- The frame probe sees what niri composited, not the shell's own frame clock; it
  costs niri a log line per request, equally in both arms.
- Software rendering on the build hosts makes Noctalia's GL work CPU work.
- The Nacre daemon refuses state under a group-writable directory; `host.sh`
  clears group write on its root.
- Noctalia clones its plugin index from GitHub at start (before the idle window).
- Nacre keeps its product-default renderer (CPU raster for bar and panels) and
  Bead's default `warm_source_budget` (0: no warm dormant panels).
