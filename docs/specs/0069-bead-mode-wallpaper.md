# N0069 — Wallpaper in the installed Bead mode

Status: implementation and remote validation in progress. No live installation.

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
wallpaper, pausing immediately and releasing the decoder after five seconds;
covered nonfloating windows pause when configured. Session lock/sleep and battery
policy also apply; state resumes when visible. A held decoder releases after
30 seconds. Renderer frame starvation is an additional occlusion safeguard.

Like QML `NacreDesktopVideo.qml`, playback is silent, loops, preserves aspect
with crop, and retains the last visible frame while paused. The renderer uses
native output dimensions (viewporter) or logical dimensions times integer scale,
with VA-API copy-back and software fallback. No resolution or frame-rate reduction
is added by this unit. QML uses QtMultimedia while Bead uses libmpv software
presentation into wl_shm; pixel-exact resampling and hardware efficiency are
backend-dependent and require the paired desktop evidence below.

## Validation and evidence

Pending: fake-systemctl switch/install suite, disposable real-systemd crash-loop
rehearsal, full Arch desktop CI, private nested desktop video/fullscreen evidence.
Evidence and CPU/memory results will be recorded after those runs.

## Apply and undo (lead only)

From the reviewed durable Nacre checkout, first run the controller-refresh
Python block in [N0054's owner runbook](0054-trial-switch-readiness.md#owner-runbook).
It calls `releases.install_controller(root)` under the existing release lock;
a shell-only code install does not refresh this controller. Do not run a full
shell installer for this fix. Then use a matched Bead build whose binary directory
contains `nacre-shell`, `bead-wallpaper-video`, and `nacre-wallpaper-decode`:

```sh
BEAD_BUILD="$(realpath "$HOME/.local/share/nacre/ai/lead/bead-preview/current")"
nacre ui-shell qml
nacre ui-shell install --binary "$BEAD_BUILD/bin/nacre-shell" --shell "$BEAD_BUILD/shell"
nacre ui-shell bead --notifications daemon
nacre ui-shell status
# Undo, including an interrupted switch:
nacre ui-shell qml
```

The daemon must advertise `wallpaper.active` with `current.motion` for video;
installation does not synthesize state or start a second wallpaper authority.
No temporary product implementation is added. Existing retained versioned installs
are rollback artifacts: delete seven days after the replacement is trusted;
trust means a successful video/fullscreen/resume run plus QML rollback. Existing
retained QML windows and fallback units follow N0054's retirement policy.
