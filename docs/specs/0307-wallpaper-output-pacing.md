# N0307 — wallpaper output pacing

Status: Platform accepted the video scheduling, the swap-report decision and the
second-output change fix. Desktop GL/VAAPI: jitter gate passes; CPU gate is the lead's call.

Symptom (owner): dynamic wallpapers and wallpaper changes lag on the external output
(DP-1 3440×1440@120 scale 1) and are fine on the laptop panel (eDP-1 2880×1800@120
scale 1.5), both on one Intel GPU. The lead's laptop audit is the live gate.

## Dynamic wallpaper (`bead-wallpaper-video`)

Cause: mpv wakeups drew outputs in vector order; an outstanding callback could
postpone an output's latest frame. Fix: bootstrap once, then coalesce to the latest
frame in each output's callback; request a slot only for stale visible playback and
acknowledge only the first draw, so no new frame means no callback. GL opacity fade
steps (compositors without alpha-modifier) also count as undrawn work, even with a
paused decoder, and the last step is requested before the fade clears; otherwise
paused/covered outputs request nothing. The frame drain before
pause/release, the play-state cache and `video-timing-offset=0` stay. Protocol and
N0304 policy are unchanged. Swap reporting stays off: with default `video-sync` (not
`display-*`) `report_swap` does not drive timing, and callback-delayed reports can add
a 200 ms synchronous flip wait ([render.h](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h),
[vo_libmpv.c](https://github.com/mpv-player/mpv/blob/master/video/out/vo_libmpv.c)).

Desktop GL/VAAPI (AMD radeonsi iGPU, private headless labwc GLES2; H1 = 3440×1440 s1,
H2 = 2880×1800 s1.5, both 120 Hz; 1920×1080 30 fps H.264; three alternating 30 s pairs,
medians; N0307 = `4ed5ad01f`):

| Output | Main interval mean/p95/max ms | N0307 | Jitter p95 ms | Unique/dup of 900 |
| --- | --- | --- | --- | --- |
| H1 | 114.68/133.59/133.77 | 33.33/33.62/33.95 | 100.26 → 0.34 | 121/142 → 900/0 |
| H2 | 114.74/133.56/133.73 | 33.33/33.62/33.95 | 100.23 → 0.34 | 118/146 → 900/0 |

Main showed about 13% of decoded frames; N0307 shows 7.5× the frames (3.5× the commits):
player CPU 3.40 → 6.57% (ranges 3.37–3.43 / 6.57–6.60), compositor 0.20 → 0.50%,
voluntary switches/s 561 → 1460. Per shown frame the player costs ~4× less. The run's
literal CPU gate (≤ main + noise) fails; the jitter gate passes on both heads.

Software (`--render=shm`, headless pixman Sway, same heads) saturates near 10
presentations/s, so it is scheduling evidence only. Dev-1, 30 fps:
H1 p95/max 133.02/166.02 → 110.61/114.03 ms, no duplicates. Oyster, three alternating
30 fps pairs: player CPU (one core = 100%) median 157.0 → 146.8%, voluntary switches/s
lower in all three (608 → 564), compositor 0.13 → 0.17%.

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
On `4ed5ad01f` (fade fix): fmt, strict clippy, video 10, shell `wallpaper` 56, preview 1.

Video main sources unchanged through `4e598a1fb`. Instrumented binaries: main `99f1bf72`,
N0307 `ef5c174c` (desktop) / `68c49459` (SHM runs). Raw evidence and the harness stay in
`.lead/`; the desktop was cleaned and its grant removed. Lead lands; no push.
