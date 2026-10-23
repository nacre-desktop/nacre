# N0039 — Daemon-owned always-on behaviours

Status: **Implemented; pending lead review and exact-head CI; unpublished**. Origin: Bead B0035
(`docs/specs/0035-settings-stores.md` on Bead) found three behaviours that only a QML
timer or script performs, so they would silently stop when Bead replaces the QML shell.

## What moves

| Behaviour | QML source | Daemon service (supervised) |
| --- | --- | --- |
| Wallpaper rotation (`wallpaper-picker` `rotation*`) | `NacreWallpapers.qml`, `wallpaper-rotation.js` | `wallpaper-rotation` |
| Automatic power profile (`power` `enabled`/`ac`/`battery`) | `NacrePowerPolicy.qml` | `power-profile` |
| Low-battery notices (`battery-alerts` ledger, `power.alerts`) | `NacreBatteryAlerts.qml`, `NacreBatteryAlertPolicy.qml` | `battery-alerts` |

Logic is ported unchanged; no new setting, wire message, schema or golden was added.

**Rotation.** Due time is `max(anchor + interval, retry)`; anchor is the latest of
`media.json appliedAtMs`, `presentation.json changedAtMs`, `rotationAnchorMs`, else daemon
start; interval is `rotationMinutes` clamped to 5-1440 (default 30). At the deadline it runs the
helper's `wallpaper-media catalog` (no shell, 16 MiB / 120 s bound), builds the pool for
`rotationKind`, picks with the script's rules (sequential wraps; shuffle uses a bag that never
repeats the current image and resets when kind/shuffle/pool change), then runs the picker's own
`wallpaper-media select <path>` (300 s bound). That is the existing path that writes the media
state and presentation record, so `wallpaper.active` and the palette follow. Failure or a
catalog error retries after 60 s. Fewer than two wallpapers pauses until `wallpaper.watch`,
`wallpaper.active` or the picker store changes. Asleep/locked pauses using the same logind watcher
(`wallpaper-media session --watch`) the QML used. Nothing polls while idle: one sleep to the
deadline. Selection is serialized (one child at a time).

**Power profile.** Desired profile = `power.<ac|battery>` when the `power` store exists, has no
error and `enabled === true`, else none. UPower `OnBattery` picks the source. Applied once per
change (source flip clears the applied value; no retry after a failure until the source or the
setting changes); `performance` only when the daemon lists it; values outside
`power-saver|balanced|performance` are never sent. Uses D-Bus directly
(`org.freedesktop.UPower.PowerProfiles`, falling back to `net.hadess.PowerProfiles`,
`ActiveProfile` property); no `powerprofilesctl`.

**Battery alerts.** UPower DisplayDevice sample (`Type==2 && PowerSupply`, present, state
Discharging/PendingDischarge/Empty while `OnBattery`). Level 1 at <=20 %, level 2 at <=15 %; one
notice per level per discharge; charging, >20 % or a removed battery re-arms and voids in-flight
deliveries. 150 ms settle after the latest change, 10 s retry up to three failures, 5 s delivery
watchdog. Text/ids are the QML's (`battery.low.title` "Battery Low", `battery.low.body`
"Remaining: {percent}%", app "Nacre Battery", icon `nacre-notifications`, urgency critical for
level 2) in the user's language. The severity ledger is the existing `battery-alerts` store
(`battery-alert-state.json`), so a restart does not repeat a delivered level and a QML-to-daemon
switch keeps it. The Power page switch `power.alerts` (registry default on, previously unread by the QML) now
silences the notices when set to false. QML does not consult DND/inhibition here; neither does the daemon: the notice is
posted like `notify-send` would (`org.freedesktop.Notifications.Notify` on the session bus) and
the owning server (QML shell, or the daemon's managed N0017 producer) applies its own DND and
inhibition rules. The daemon never claims the name for this, so N0017 ownership rules are untouched
and there is no second notification path.

## Ownership switch (one, no double firing)

Startup authority, parallel to `--notifications`: `nacre-helper daemon --behaviours=legacy|daemon`
(default `legacy`; each option at most once). Only `daemon` starts the three services, and only
outside safe mode, after which the `daemon` topic's `services` map lists
`behaviours: {status: "running"}`. That entry is the single signal.

QML (`NacreDaemon.behavioursHeld`) performs none of the three when `NACRE_DAEMON_CLIENT=1` and
either the first `daemon` snapshot has not arrived or it lists `behaviours` as running. Unset or
`NACRE_DAEMON_CLIENT=0`, or a client-mode daemon started with `--behaviours=legacy`, leaves the
QML behaviour exactly as before. Manual wallpaper next/previous, the settings UI and the
rotation status text are unchanged. Bead, as a socket client, never performs them.

Consequences, deliberately accepted: in client mode the shell waits for the daemon's first
snapshot before performing any of them (no double rotation at startup); if the daemon never
answers, a client-mode shell performs none of the three. Selecting the `daemon` mode is an
owner/controller step: this unit adds the flag only; the F4 controller and unit files are not
changed, and nothing live was touched.

## Known divergences from the QML

- The QML paused rotation while the wallpaper picker, the Appearance/Wallpaper/Colors Settings
  pages or the Welcome demo were open. Those are UI-only states the daemon cannot see; in daemon
  mode rotation continues (a rotation still waits for any in-flight daemon selection).
- The QML in-memory "previous wallpaper" history does not record daemon rotations; Previous falls
  back to pool order.
- A power-profiles-daemon restart resets its profile; like the QML, the daemon does not reapply
  until the source or the setting changes.

## Temporary duplicates

The QML implementations stay as the `legacy` mode. Delete them (the three services' bodies, the
`behavioursHeld` gates and their tests) 30 days after the owner runs the daemon mode as the
default and the QML shell is retired by Bead.

## Verification

Rust tests use temp HOMEs, a private `dbus-daemon` with fake UPower, power-profiles-daemon and
Notifications services, a fake clock and a fake helper script (`behaviours/tests.rs`). QML tests
cover the stand-down in each service. No real power profile, wallpaper or notification is touched.
