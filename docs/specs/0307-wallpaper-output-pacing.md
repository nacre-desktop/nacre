# N0307 — wallpaper output pacing

Status: Platform accepted the video scheduling design and swap-report decision. The
second-output wallpaper-change fix awaits review. GL/VAAPI acceptance remains pending.

Symptom (owner): dynamic wallpapers and wallpaper changes lag on the external output
(DP-1 3440×1440@120 scale 1) and are fine on the laptop panel (eDP-1 2880×1800@120
scale 1.5), both on one Intel GPU. The lead's laptop audit is the live gate.

## Dynamic wallpaper (`bead-wallpaper-video`)

Cause: mpv wakeups drew outputs in vector order; an outstanding callback could
postpone an output's latest frame. Fix: bootstrap once, then coalesce to the latest
frame in each output's callback; request a slot only for stale visible playback and
acknowledge only the first draw, so no new frame means no callback. Fade timers mark
undrawn work; paused/covered outputs request nothing. The frame drain before
pause/release, the play-state cache and `video-timing-offset=0` stay. Protocol and
N0304 policy are unchanged. Swap reporting stays off: with default `video-sync` (not
`display-*`) `report_swap` does not drive timing, and callback-delayed reports can add
a 200 ms synchronous flip wait ([render.h](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h),
[vo_libmpv.c](https://github.com/mpv-player/mpv/blob/master/video/out/vo_libmpv.c)).

Dev-1, 30 s per arm, headless pixman Sway, synthetic 1920×1080 H.264, `--render=shm`.
H1 = 3440×1440 s1, H2 = 2880×1800 s1.5, 120 Hz advertised. Intervals mean/p95/max ms;
U/D = unique render targets/decoded frames (gap = boundary-sensitive coverage deficit).

| fps/output | Main intervals | N0307 intervals | U/D main → N0307 |
| --- | --- | --- | --- |
| 30/H1 | 107.42/133.02/166.02 | 105.41/110.61/114.03 | 280/900 → 284/899 |
| 30/H2 | 107.42/133.02/166.02 | 105.54/110.61/114.03 | 280/900 → 284/899 |
| 60/H1 | 100.32/104.43/108.79 | 101.61/108.27/112.67 | 299/1801 → 295/1800 |
| 60/H2 | 100.32/104.43/108.79 | 101.73/108.94/112.67 | 299/1801 → 295/1800 |

No duplicates either way. Software rendering saturates near 10 presentations/s, so this
is scheduling evidence, not GPU acceptance. Oyster repeat, three alternating 30 fps
pairs: player CPU (one core = 100%) 155.3→146.8, 157.5→159.7, 157.0→146.6 (median
157.0→146.8); voluntary switches/s lower in all three (median 608→564); compositor
0.13→0.17%; H1 interval p95 131.5→115.4 ms, max 141.0→119.8 ms.

## Wallpaper change on the second output

The static change is drawn by Bead's wallpaper surface in `nacre-shell`, not the video
player. Daemon-driven changes are already one coordinated swap: oyster release build,
same two heads, nine changes: first present 61.7/59.0 ms mean, output skew p95 0.006 ms;
the opt-in `--animate` fade steps both heads alike (~175 ms CPU-bound ticks).
Cause: picker selections go through `Preview`, whose predecoder and B0171 prefetch kept
one target, the last output to prepare. Only that output's size was predecoded or
cached (B0171 sized it 2880×1800); alternating prepares also discarded each other's
predecode. The other output waited for the daemon ack, then cold-decoded (release
worker, synthetic 3840×2160 JPEG, dev-2: p50 60.6 ms at 3440×1440, n = 9; B0171 cites
400 ms+ on real sets) and swapped in after the first. Fix: one predecoder per output;
prefetch warms every output size. Cost: one extra decode and disk entry per extra
output size (within the 512 MiB cache bound); one ≤2 s predecode raster per output.

Checks (Arch container on dev-2, rustc 1.99.1, `3e3db0c38`): `cargo fmt --all --check`;
strict all-target shell clippy; shell `wallpaper` tests 56 (new per-output test included);
whole `cargo test -p nacre-shell` (shell 894 passed/4 ignored, video 9, decode 3, preview
1, worker 1); compositor boundary; all 23 SHM smoke checks (pause/release CPU 0%).

Video main sources unchanged through `4e598a1fb`; binary hashes start `99f1bf72`
(main) / `68c49459` (N0307). Raw evidence and the GL harness stay in `.lead/`. The
desktop grant file was absent, so no desktop access: GL/VAAPI needs three alternating
pairs in one private 120 Hz labwc session. Lead reviews before landing; no push.
