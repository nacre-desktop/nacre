# N0320 — compositor journal drop-in and rotation on `session.state`

**Status:** B committed with focused dev-2 checks green; not pushed yet (overnight checkpoint, hosted CI backlog). A not implemented: a `LogLevelMax=` drop-in has no effect on a user unit (evidence below); lead decision.

## B. Daemon readers use `session.state`

The wallpaper rotation and the weather photo each ran a
`nacre-helper wallpaper-media session --watch` child, a second logind watcher
next to the daemon's own `session-state` service (N0317). Both run inside the
daemon, so they now read the hub's `session.state` value directly:
`session_policy::locked_or_asleep` is true while `locked` or `sleeping`, and
until logind has been read (`revision` 0, or no value yet), as the old watcher
failed closed before its first line.

- Rotation: pauses while locked or asleep (unarmed, readiness `sleeping`) and
  resumes on the next `session.state` change; a lock or sleep change no longer
  clears the "fewer than two wallpapers" block, as before.
- Weather photo: wakes on `session.state` and shows `paused` while locked or
  asleep, as before.
- Deleted: the `Session` child wrapper, the `wallpaper-media session --watch`
  mode (its only callers were these two), the watcher's own failure path and
  its N0317 stop-signal test. `wallpaper-media session` (one reading) stays.

One fewer process per daemon whenever rotation is on or a photo is leased.
Line delta: product +62/−189, tests +23/−102.

## A. xwayland-satellite journal flood: `LogLevelMax=` does not reach user units

The laptop journal (read-only, 12 h): 3,004 priority-7 lines from
xwayland-satellite (`_TRANSPORT=syslog`) under
`wayland-wm@nacre\x2dniri.service`; niri's own lines are priority 6 stdout.
Nacre's sessions run the compositor as `wayland-wm@nacre\x2dniri.service`
(nacre-niri via uwsm), `wayland-wm@niri.service` / `wayland-wm@niri.desktop.service`
(stock niri) and `nacre-niri.service` (the fork without uwsm), all user units.

journald applies `LogLevelMax=` from `/run/systemd/units/log-level-max:<unit>`,
looked up by the sender's *system* unit. Only the system manager writes those
files (`unit_export_state_files` returns before them for user managers,
systemd v262 and main), so every process of a user unit is looked up as
`user@1000.service`. A drop-in on `wayland-wm@.service` would load
(`systemctl --user show -p LogLevelMax` shows it) but filter nothing.
systemd.exec(5) says the same for `LogFilterPatterns=` ("only available in
system services, not in per-user services").

Container experiment (dev-1, `nacre-ci/arch-nacre`, systemd 262 as PID 1 with
journald and a real `user@1000.service`): a stub `wayland-wm@.service` in the
runtime unit dir (where uwsm writes it) plus
`~/.config/systemd/user/wayland-wm@.service.d/nacre-log-level.conf`
(`[Service] LogLevelMax=info`). `systemctl --user show` on
`wayland-wm@nacre\x2dniri.service` reports the drop-in and `LogLevelMax=6`,
yet journald stored its `<7>` stdout line at priority 7 under
`USER_UNIT=wayland-wm@nacre\x2dniri.service`. The control, a system unit with
the same setting and the same commands, had its `<7>` stdout line dropped.
(Its short-lived `logger` syslog lines were not attributed to any unit, so they
prove nothing either way; satellite is long-lived and is attributed.)

Remaining levers, none Nacre-only: `LogLevelMax=info` on the system unit
`user@.service` (root, filters every user-session process),
`MaxLevelStore=info` in journald.conf (root, whole system), or an upstream
xwayland-satellite change to honour `RUST_LOG` in syslog mode.
