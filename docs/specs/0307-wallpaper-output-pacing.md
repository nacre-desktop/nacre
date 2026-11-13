# N0307 — wallpaper output pacing

Status: Platform accepted the scheduling design and swap-report decision; follow-up
checks pass. Repeated SHM CPU confirmation and GL/VAAPI acceptance remain pending.

Symptom: uneven external-output wallpaper motion, with both owner outputs near
120 Hz on one Intel GPU. The physical cause still needs the lead's laptop audit.

Source evidence: mpv wakeups drew outputs in vector order; an outstanding callback
could postpone an output's latest frame. EGL already requests nonblocking swaps;
shared-context GPU serialization remains unproven. Baseline never called `report_swap`.
The first callback design then rearmed on every playing vblank without checking stale,
causing idle callbacks/empty commits; post-draw acknowledgements also added callbacks.

Fix: bootstrap once, then coalesce to the latest frame in each output's callback.
Request a slot only for stale visible playback; acknowledge only the first draw.
No new frame means no callback request. Fade timers mark undrawn work; paused/covered
outputs request nothing. Keep the accepted frame drain before pause/release, play-state
cache, and player-only `video-timing-offset=0`. Protocol and N0304 policy are unchanged.

Platform accepted leaving optional swap reporting disabled: mpv uses default `video-sync`,
not `display-*`, so `report_swap` does not drive timing. Introducing callback-delayed
reports can add a 200 ms synchronous flip wait. See the [render contract](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h)
and [VO handshake](https://github.com/mpv-player/mpv/blob/master/video/out/vo_libmpv.c).

Dev-1 matched main/candidate, 30 s each: Arch, headless pixman Sway, synthetic
1920×1080 H.264; identical `--hwdec=vaapi --render=shm` flags (software fallback).
H1 = 3440×1440 scale 1; H2 = 2880×1800 scale 1.5; advertised 120 Hz; feedback refresh 0.
Intervals below are mean/p95/max ms. U = unique render targets; D = decoded returns.

| fps/output | Main intervals | N0307 intervals | U/D main → N0307 | Duplicates/gap main → N0307 |
| --- | --- | --- | --- | --- |
| 30/H1 | 107.42/133.02/166.02 | 105.41/110.61/114.03 | 280/900 → 284/899 | 0/620 → 0/615 |
| 30/H2 | 107.42/133.02/166.02 | 105.54/110.61/114.03 | 280/900 → 284/899 | 0/620 → 0/615 |
| 60/H1 | 100.32/104.43/108.79 | 101.61/108.27/112.67 | 299/1801 → 295/1800 | 0/1502 → 0/1505 |
| 60/H2 | 100.32/104.43/108.79 | 101.73/108.94/112.67 | 299/1801 → 295/1800 | 0/1502 → 0/1505 |

Gap is decoded minus unique render targets: a boundary-sensitive coverage deficit,
not pixel-verified drops. Software rendering saturates near 10 presentations/s.
Player CPU (one core = 100%): 30 fps 138.70 → 139.40%; 60 fps 149.07 → 147.27%.
Voluntary switches/s: 709.33 → 721.97 (30); 1041.56 → 1071.43 (60).
Compositor CPU: 0.07 → 0.17% at both rates. The 30 fps CPU difference is +0.70 pp;
three alternating pairs were queued to quantify noise, but dev-1 became unreachable.
Cadence is mixed at 60 fps; these SHM results do not establish GPU acceptance.

Checks: local rustfmt; Arch `cargo fmt --all --check`; strict all-target shell clippy;
`cargo test -p nacre-shell --locked -j2` (shell 893 passed/4 ignored, video 9, decode 3,
preview integration 1); focused video/preview tests; compositor boundary; all 23 SHM
smoke checks pass. Pause/release CPU 0%; first-frame/source-switch/resume checks pass.

Main player sources unchanged through `4e598a1fb`; binary hashes start `99f1bf72`
(main) / `68c49459` (N0307). Raw evidence and deferred GPU harness remain in `.lead/`.
Run 6 exports evidence and deletes its source/5.4 GiB target on exit; completion is
unverified while dev-1 is unreachable. No desktop access occurred in this follow-up:
the grant file was absent. GL/VAAPI needs three alternating pairs in one private
120 Hz labwc session, checking jitter, both CPUs and player switches/s. Lead reviews
before landing; laptop audit remains the live gate. No push or publication.
