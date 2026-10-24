# N0068 - daemon takeover mode works on the real machine; `ui-shell bead` manages the daemon flags

Status: **Implemented; pending lead review** (Claude worker, branch `task/n0068-daemon-takeover-20261024`). Urgent: the owner
runs Bead without notifications until this lands. Product change: Rust about +80/-20 (+110 test), Python about +95 (+190 test).

## Incident (2026-10-24 18:47-18:52, laptop, 3cc38598)

With `nacre-daemon --notifications=daemon --behaviours=daemon`: `WARN Managed notifications stopped` about 60 ms after every
start (also with the bus name free), then `ERROR Daemon task panicked; payload omitted` x4 and
`thread caused non-unwinding panic. aborting.` (core dump) on stop. The legacy mode was healthy.

## Root causes (reproduced on the desktop: real Arch session, private bus and HOME, the owner's QML history copied in)

1. **Notifications policy lookup.** `notifications::policy` read `NacreNotifications.qml` from `<assets>/../config/` (the source
   layout, `shell/config`). The installed tree keeps it at `shell/source/config/` (`shell/tools` is the assets folder), so the
   open failed with ENOENT, `notifications::run` returned `Err` before touching the bus, and the supervisor logged the stop.
   Containers and tests used the source layout and never saw it. Fix: `policy_source` tries `shell/config`, `source/config`,
   `config`. The warning now carries the error text as `reason` (messages are fixed strings or OS errors, no payloads).
2. **Abort on stop.** After the early `Err`, shutdown did `timeout(10 s, &mut service)` (which consumed the finished
   `JoinHandle`), fell into the `_` arm and awaited the same handle again. Tokio panics on a second poll
   ("JoinHandle polled after completion", `task/core.rs:427`); the panics unwound through the zbus executor tasks and Rust
   aborted (`panic in a destructor during cleanup`). Those were the "four panics". Fix: only a timeout aborts and awaits;
   an already-finished task is logged and not polled again.
3. **Panic location.** The panic hook now logs `file` and `line` only (never the payload), so a repeat shows where.

## Released names are retaken without a restart

When another server owns `org.freedesktop.Notifications` the daemon used to give up for the life of the process. Now it keeps
the service in `refused`, watches `NameOwnerChanged` on a second connection, and when the name is free runs the normal start
again (still `DoNotQueue`, so it never queues or steals). This was the smaller option: the existing reacquire loop is reused,
no new state. If the watch cannot be set up it retries every second. `notifications.*` calls answer `unavailable` meanwhile.

## `nacre ui-shell` manages the daemon flags (tools/ui_shell_switch.py)

`bead --notifications daemon` (new managed file `nacre-daemon.service.d/60-nacre-ui-shell-daemon.conf`, journaled):

```
write-units, reload-units, write-daemon-dropin, write-dropin, reload,
stop-shell      QML releases org.freedesktop.Notifications
restart-daemon  ExecStart = the unit's own command + --notifications=daemon --behaviours=daemon
daemon-ready    daemon.status lists topic `notifications` and service notifications=running (30 s)
restart-shell   Bead starts (--notices)
health
```

`qml` (also the automatic rollback, and `--recover`): `remove-dropin` removes both drop-ins, `reload`, `stop-windows`,
`restore-companions`, `reload-restored`, then, when the transaction involved the daemon drop-in, `restart-daemon` (legacy
defaults; best effort so the shell is always restarted), `restart-shell` (QML), `daemon-ready`, `health`. The daemon flag is
carried in the journal, so an interrupted rollback converges on rerun. `--notifications none` is unchanged (no daemon restart).
The old precondition (daemon already advertises the topic) is gone: the topic only exists after the restart. A foreign drop-in
overriding the daemon `ExecStart` (e.g. the lead's `60-shell-switch-bead.conf`) is refused; remove it with `shell-switch qml` first.

## Tests and evidence

- Rust: `policy_is_found_in_source_and_installed_layouts` (unit); `managed_notifications_takeover_private_bus`: installed
  layout, QML-written history loaded, name pre-owned then released with no daemon restart, and SIGTERM with managed
  notifications unable to start exits without abort (all three failed before the fix).
- Python: model tests for the handover order (stop shell, restart daemon, start shell), exact rollback bytes, no Bead before
  the daemon owns notifications, no QML beside the managed daemon, crash at every forward and rollback phase, failing
  notifications restore QML, foreign daemon override refused, rollback survives a failing daemon restart. 27 -> 33 tests.
- Real systemd user manager (`tools/tests/rehearse_ui_shell_systemd.py`, extended: stub daemon records its flags):
  [output](../evidence/n0068/systemd.txt) ends `PASS: ... daemon takeover and back`.
- Desktop reproduction: before, `Managed notifications stopped reason=No such file or directory` + abort exit 134; after,
  `notifications: running`, owner `org.freedesktop.Notifications`, clean stop (exit 0).

## Temporary duplicates

`~/.local/bin/shell-switch` (lead wrapper, owns its own daemon drop-in `60-shell-switch-bead.conf`): delete 3 days after
`nacre ui-shell bead --notifications daemon` is trusted on the laptop; use `ui-shell` only.
