# N0069 — Wallpaper in the installed Bead mode

Status: Done — versioned installation, private video/pause/resume, full desktop
CI and real-systemd lifecycle rehearsal passed. Lead application remains separate;
no live install was performed.

The N0054 installer copied only `nacre-shell` and `shell/`. Bead B0021/B0046
already attach the wallpaper consumer when `--daemon-socket` is supplied, but
require two sibling executables. Without the static decoder the poster cannot
be painted; without the video renderer motion is disabled. The preview bundle
contains both, explaining why preview and installed mode differ.

## Production contract and lifecycle

Read against Bead `c9a103218b3cb0776329d8b5f241c9b6d10b5d38`:
`docs/specs/0021-static-wallpaper-surface.md`,
`docs/specs/0046-dynamic-wallpaper.md`, and
`docs/decisions/0005-single-ui-process.md` (wallpaper exception).

`nacre-shell --bar --core-views --ui-provider --daemon-socket SOCKET` starts
`wallpaper::Consumer` and `wallpaper_motion::Controller`. The consumer leases
`wallpaper.active` v1 and `settings.store.wallpaper-picker`. N0027's daemon
publishes the still poster plus optional `current.motion` (video path and file
identity); N0039 retains selection and rotation ownership. The static decoder
`nacre-wallpaper-decode` takes one bounded JSON request on stdin, returns RGBA,
and exits. It has no help command. The existing shell controller starts
`bead-wallpaper-video --hwdec=vaapi` and supplies source/state JSON over stdin.
That helper is a separate process with no UI-provider or daemon registration.

Keep this existing supervision rather than introduce a companion service:
source/state replay, bounded crash backoff, decoder release, and poster fallback
already belong to it. Switching to QML or crash-loop fallback stops the shell's
control group, including both helpers; the drop-in now states
`KillMode=control-group` explicitly. N0054's journals, phases and restoration of
prior companion bytes/modes remain unchanged.

`ui-shell install` now requires and copies all three executable siblings into
one versioned directory. The same regular-file, ownership and writable-mode
checks apply to every binary. A missing/unsafe helper refuses before a journal
or manager change. `launch check` checks decoder executability, libmpv availability
with `bead-wallpaper-video --check`, then the shell's `--help`. Failed checking
never selects the incomplete version. Previous versions stay usable.

## Playback and quality

The existing controller reads `motionMode` (`full`, `battery`, `still`), `paused`
and `pauseCovered` (default true). Active-workspace fullscreen windows hide the
wallpaper, pausing immediately and releasing the decoder after five seconds
when the compositor exposes fullscreen state; covered nonfloating windows pause
when configured. Session lock/sleep and battery
policy also apply; state resumes when visible. A held decoder releases after
30 seconds. Renderer frame starvation is an additional occlusion safeguard.

Like QML `NacreDesktopVideo.qml`, playback is silent, loops, preserves aspect
with crop, and retains the last visible frame while paused. The renderer uses
native output dimensions (viewporter) or logical dimensions times integer scale,
with VA-API copy-back and software fallback. No resolution or frame-rate reduction
is added by this unit. QML uses QtMultimedia while Bead uses libmpv software
presentation into wl_shm; pixel-exact resampling and hardware efficiency are
backend-dependent and require the paired desktop evidence below.

Niri currently advertises `fullscreenState:false` and omits a fullscreen member
from its window IPC. On that backend the default `pauseCovered:true` pauses
nonfloating fullscreen covers through the covered-window rule, with renderer
frame starvation as another safeguard. Fullscreen-specific five-second release
cannot be claimed from Niri's missing state; the held-decoder timeout still
applies. The rehearsal proves the real cover with its action, output-sized
geometry and screenshot coverage rather than asserting an absent IPC field.

## Validation and evidence

The 24 private-HOME fake-systemctl switch/install tests passed as UID 1000.
The full remote Arch desktop CI replica passed, including 761 native unit tests
plus integrations, 493 tools tests (one opt-in Wayland soak skipped by the job),
120 AI tests, 52 Brain tests, 487 shell-tool tests, and 534 frozen helper parity
cases with zero failures. Ruff, QML/JS format/parse, Lua, translation checks,
the 244-state pseudo gallery and 185-state wide-font gallery passed. The private
container needed scoped Git safe-directory settings and CARGO_BUILD_JOBS=2 for
nested fixture builds.

The earlier real-systemd initialization attempts failed with cgroup/bootstrap
errors. An unmasked container boot started sysctl/udev setup units and was stopped.
Lead review on 2026-10-24 confirmed the host's own Arch sysctl configuration
already sets the recorded values: no effective host change; incident closed.
The continuation uses image-time unit masks, `container=docker`, read-only
`/proc/sys` and `/sys`, and a private cgroup namespace with no host cgroup-root
bind. Inspect output was saved before container removal.

The safe continuation passed the real systemd user-manager fixture (exit 0):
install → Bead switch → persisted restart → crash-loop QML fallback → rollback
→ repeat. The renderer-child cleanup assertion passed. The first available host
under HOST SELECTION was used because the preferred host had all four task slots
occupied. The container was not privileged: only `SYS_ADMIN` was added to mount
cgroup2 inside its private cgroup namespace. Docker's default read-only system
paths remained intact, `/sys` was bound read-only without recursive host mounts,
and no host cgroup root was bound. Before PID1, a guard verified the container
marker, image masks, private namespace and read-only kernel paths. Critical
masked units stayed inactive with zero start timestamps and no journal entries.
The container, both owned image versions and remote task directory were removed.
Raw before/after/stopped inspect JSON, mask states, boot guard and fixture output
are retained in `.lead/safe-systemd-evidence/`; exact scripts and the mask list
are in `.lead/safe-systemd-{run,probe}.sh` and
`.lead/safe-systemd-context/`. No product code changed during this continuation.

Follow-up ask (outside N0069): `.github/workflows/arch-packages.yml` starts a
privileged systemd PID1 container with a host cgroup bind and no explicit masks
or container marker. Review/harden that similar unmasked pattern separately;
it is deliberately unchanged here.

Private desktop rehearsal passed with matching Niri configuration, palette,
video, 2880×1800 physical / 1920×1200 logical output and scale 1.5. Both arms
covered 100% of the output, visibly played and resumed; the Bead child stayed
alive while covered and stopped consuming CPU. Paired PNGs and raw JSON are
private task artifacts in `.lead/desktop-wallpaper-evidence/final/{qml,bead}/`:
`wallpaper-play-a.png`, `wallpaper-covered.png`, and `wallpaper-resumed-a.png`.
The coordinator visually checked wallpaper framing and palette. Captures are
at different video timestamps; pixel-exact video resampling parity is unproven.

| Process | CPU %, play / covered / resume | PSS MiB, play / covered / resume |
| --- | --- | --- |
| QML (QtQuick OpenGL / QtMultimedia) | 5.5 / 0.5 / 5.5 | 713.717 / 693.222 / 716.154 |
| Bead shell (CPU wl_shm) | 0 / 0 / 0 | 46.015 / 45.961 / 46.211 |
| Bead video (libmpv software presentation) | 50.999 / 0 / 49.498 | 266.883 / 261.932 / 266.776 |

CPU intervals were four seconds per process (100% = one core); memory values
are medians of per-second samples. The video helper's playing anonymous/shmem/file PSS was
101.965/27.775/137.143 MiB; the shell's was 17.328/10.574/18.112 MiB.
All sampled SwapPss values and both arm page-in deltas were zero. Renderer DRM
accounting and the actual hardware decode mode were unavailable; VA-API was
requested, not demonstrated. The higher visible-playback CPU cost is an existing
renderer limitation, not a quality reduction or an optimization claim.

### Separate finding for the next unit: decoder and presentation paths

At Bead `c9a1032`, `crates/nacre-shell/src/wallpaper_motion.rs` launches
`--hwdec=vaapi`. The renderer's `main.rs` maps it to libmpv `hwdec=vaapi-copy`
(the default); `--hwdec=auto` maps to `auto-copy`, not `auto-safe`.
`crates/nacre-shell/src/bin/bead-wallpaper-video/mpv.rs` passes that option to
mpv, uses `vo=libmpv`, and creates a render context with `MPV_RENDER_PARAM_API_TYPE`
`sw`, writing BGR0 frames into wl_shm. Hardware decoding can therefore coexist
with CPU conversion/scaling/copy-back and software presentation. Failing hardware
decoding falls back to software; the captured log lacks a `hwdec-current` mode,
so actual VA-API decoding in the 51%-CPU run is not demonstrated.

QML `NacreDesktopVideo.qml` uses Qt Multimedia `MediaPlayer` / `VideoOutput`
without forcing a decoder or a `QT_FFMPEG_DECODING_HW_DEVICE_TYPES` value. The
rehearsal log explicitly reports the FFmpeg backend (n9.0.2), H.264 input at
1920×1080/60 fps, and QtQuick OpenGL rendering. OpenGL presentation does not
prove hardware decoding; the selected FFmpeg hardware decoder is unreported.
The 51.0% versus 5.5% CPU gap needs a separate decoder/presentation investigation
with actual decoder telemetry on both sides. No renderer options or code are
changed for that finding in N0069.

The desktop had no libmpv. The rehearsal used private Arch mpv 1:0.41.0-6
libraries and changed only a copied libmpv DT_NEEDED path from absolute
`/usr/lib/libmujs.so` to `libmujs.so`; no host package or library was changed.
Package/binary hashes and the exact private patchelf command are retained in
`.lead/desktop-wallpaper-evidence/final/summary.json` and
`.lead/desktop-wallpaper-report.md`. Native source:
Bead `c9a1032`, Nacre `3cc38598`, release preview binaries; controller SHA-256
`ca123d1be4c26ae0a002a2ba7c0d80a6db8a4b52777052871437d9839e217563`.

The task scope had MemoryMax=12 GiB and MemorySwapMax=0. Its outside watchdog
sampled once per second; available memory started at 12,395 MiB and never fell
below 11,746 MiB, with no events. Pre-arm swap was below 64 MiB; scope/watchdog
exited zero and owned PIDs, scope, tmux waiter and remote task directory were
removed. Earlier setup/preflight failures are retained as invalid attempts.
The runtime check is one matched QML/Bead functional rehearsal with short
per-process CPU intervals and median memory samples, departing from the default
five-pair optimization gate. This unit changes installation, not the renderer;
the measurements characterize playback/pause and do not claim a laptop memory
acceptance result or a statistically established performance improvement.

## Apply and undo (lead only)

From the reviewed durable Nacre checkout, first run the controller-refresh
Python block in [N0054's owner runbook](0054-trial-switch-readiness.md#owner-runbook).
It calls `releases.install_controller(root)` under the existing release lock;
a shell-only code install does not refresh this controller. Do not run a full
shell installer for this fix. Then use a matched Bead build whose binary directory
contains `nacre-shell`, `bead-wallpaper-video`, and `nacre-wallpaper-decode`.
The video renderer also requires the distribution's libmpv runtime (Arch package
`mpv`); `--check` must succeed before switching. The worker never installs host
packages; a private runtime bundle may be used only for its isolated rehearsal.

Run the following in Bash; retain the current notification choice:

```sh
set -euo pipefail
BEAD_BUILD="$(realpath "$HOME/.local/share/nacre/ai/lead/bead-preview/current")"
"$BEAD_BUILD/bin/bead-wallpaper-video" --check
BEAD_NOTIFICATIONS="$(nacre ui-shell status | python3 -c 'import json,sys; print(json.load(sys.stdin).get("notifications") or "none")')"
nacre ui-shell qml
nacre ui-shell install --binary "$BEAD_BUILD/bin/nacre-shell" --shell "$BEAD_BUILD/shell"
nacre ui-shell bead --notifications "$BEAD_NOTIFICATIONS"
nacre ui-shell status
```

Undo, including an interrupted switch:

```sh
nacre ui-shell qml
```

The daemon must advertise `wallpaper.active` with `current.motion` for video;
installation does not synthesize state or start a second wallpaper authority.
No temporary product implementation is added. Existing retained versioned installs
are rollback artifacts: delete seven days after the replacement is trusted;
trust means a successful video/fullscreen/resume run plus QML rollback. Existing
retained QML windows and fallback units follow N0054's retirement policy.
