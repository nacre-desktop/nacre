# N0307 — wallpaper output pacing

Status: dev-1 validated; GL/VAAPI and live acceptance pending; Platform reviews before landing.

Symptom: the owner reports uneven motion on an external output but smooth laptop motion,
with both outputs near 120 Hz on one GPU. The physical cause is not yet confirmed.

Source diagnosis: mpv wakeups draw every output in vector order, while pending callbacks
suppress draws. The default early render target can block that same event loop before
another output gets its slot. EGL already requests nonblocking swaps. Shared-context GPU
serialization remains unproven. There was no `report_swap` call: doubled reporting is ruled out.

Fix: bootstrap the first image, then draw each dirty output only in its own callback.
Rearm callbacks without rendering unchanged images, retaining the latest available frame.
Set `video-timing-offset=0` only on the Wayland player; preview keeps its original timing.
Drain queued frames before synchronous pause/release control and suppress unchanged play
writes. First-frame acknowledgement, fade, protocol, SHM buffers and N0304 policy remain intact.

Review deviation: keep optional swap reporting disabled. Introducing callback-delayed
reports into the synchronous control path can add a 200 ms flip wait; Platform must review
this deviation. See the [render contract](https://github.com/mpv-player/mpv/blob/master/include/mpv/render.h)
and [VO handshake](https://github.com/mpv-player/mpv/blob/master/video/out/vo_libmpv.c).

Matched A30/B30/A60/B60: one 30 s sample each, Arch container, pixman headless Sway,
1920×1080 synthetic H.264, identical `--hwdec=vaapi --render=shm` flags.
H1 = 3440×1440 scale 1; H2 = 2880×1800 scale 1.5; both advertised 120 Hz.
Intervals are mean/p95/max milliseconds. U = unique mpv render targets; D = decoder returns.

| fps/output | Before intervals | After intervals | U/D before → after | Duplicates/gap before → after |
| --- | --- | --- | --- | --- |
| 30/H1 | 126.37/246.31/435.26 | 100.71/101.27/483.30 | 231/809 → 298/882 | 6/578 → 0/584 |
| 30/H2 | 126.37/246.30/435.26 | 100.71/101.27/483.30 | 233/809 → 298/882 | 4/576 → 0/584 |
| 60/H1 | 104.41/100.67/1127.64 | 98.76/104.32/195.65 | 286/1654 → 304/1798 | 1/1368 → 0/1494 |
| 60/H2 | 104.41/100.67/1127.64 | 98.76/104.32/195.65 | 287/1654 → 304/1798 | 0/1367 → 0/1494 |

Gap = decoded count minus unique targets in the wall-clock sample: boundary-sensitive
coverage deficit, not a pixel-verified drop counter. Refresh feedback was 0 (unknown).
Player CPU (100% = one core): 30 fps **114.90 → 140.13%**; 60 fps **138.33 → 147.70%**.
Software rendering is saturated around 8–10 presentations/s. CPU rose alongside throughput;
30 fps p95 improved but max worsened; 60 fps max improved but p95 slightly worsened.
This is scheduling/fallback evidence, not proof of the owner's GPU fix or its CPU guard.

Checks: local rustfmt; Arch `cargo fmt --all --check`, strict all-target shell clippy,
`cargo test -p nacre-shell --locked -j2` (shell 893 passed/4 ignored; video 8 passed;
preview integration passed), compositor boundary, and all 23 SHM smoke checks passed.
Preview's first run exposed the overly broad timing override; scoping it fixed that failure.
Pause/release smoke CPU was 0%; source switch, first frame, resume and retained image passed.

Base `8e6f4ae05`, dev build, line-table debug, no incremental; instrumented binary hashes
start `99f1bf72` (before) / `996eecec` (after). Full provenance, traces and harness stay in `.lead/`.
Four dev-1 containers used; owned containers, sessions, source and 7 GiB target cleaned.
GL/VAAPI is ready for a separately granted desktop window; no desktop capture ran.
The lead's laptop live audit remains the acceptance gate. No push or publication.
