# N0317 — daemon start hygiene and two Shell topics

**Status:** Implemented; focused dev-2 checks passed; landed on main, hosted CI pending.

## Shell contracts

**`session.state`** (new topic; the `session` topic stays the policy record):

```json
{"locked": false, "sleeping": false, "preparing_sleep": false, "revision": 0}
```

A `session-state` daemon service reads logind on the system bus. `locked` is
the display session's `LockedHint`, the value the wallpaper rotation already
pauses on. `preparing_sleep` is true from `PrepareForSleep(true)` until
`PrepareForSleep(false)`, so its fall to false marks a resume. `sleeping` has
the same value: nothing runs while the system is suspended, so the daemon cannot
observe a separate state. The topic is published only on change and
snapshotted on subscribe. `revision` is 0 until logind has been read, then rises
by one per change. It runs apart from the `session-watch` policy observer,
whose resume checks wait 500 ms and query the compositor, so a resume is
published at once. Types: `protocol/schema/session-state.json` and
`protocol/luau/session.d.luau`. Transcript: `protocol/fixtures/session-state.jsonl`.

**Tray tooltips:** each `tray` item gains
`tooltip: {title, description}` from the StatusNotifierItem `ToolTip`
property. `title` is the existing item name. The strings are empty when an
item has no tooltip, and they are capped at 256 and 1,024 characters. The
description is passed through raw and may contain the SNI markup subset.

## Start hygiene (laptop audit)

- **DDC brightness discovery.** The controls producer re-probed every external
  output on each subscription, so a monitor without DDC/CI cost two failing
  `ddcutil` runs (~0.3 s, EREMOTEIO journal errors) per daemon start. The first
  failure is now logged once at info and recorded in
  `$XDG_STATE_HOME/nacre/ddc-unsupported.json`. The record is keyed by EDID
  fingerprint, together with the fingerprints of all connected monitors.
  Discovery skips a recorded display until that monitor set changes. The idle-dim
  helper shares the cache. Explicit brightness writes and the legacy
  `light-devices discover` CLI still probe.
- **"tray" connection failed.** This was not start ordering. Every deploy
  restarts the daemon and then the shell, and the journal shows the failure at
  the moment the shell exits. Bead's tray client queues for
  `org.kde.StatusNotifierWatcher`, so the shell owns the watcher once the daemon
  restarts. When the shell exits, its other bus names can drop before the
  watcher name, and the daemon's tray loop then queried the departing watcher
  and failed. A query that fails because the watcher changed owner is now a
  handover: the tray waits for the next owner and does not restart.
- **"wallpaper-rotation" connection failed.** It was logged at every daemon
  *stop*, not start: systemd signals the whole cgroup, so the rotation's
  `wallpaper-media session --watch` child died first. A watcher ended by SIGTERM
  now waits up to the unit's 5 s stop timeout to be cancelled. Any other end is
  still reported and restarted.
- **"Session legacy UI state is unavailable".** This was a dead Quickshell
  fallback, and N0311 deleted it (3c34448f8, "remove the Quickshell
  session-state fallback"). Nothing more is needed here.
- **xwayland-satellite journal flood: not changed.** Nacre neither configures
  nor launches it: niri spawns it on demand with `-listenfd`. In that integrated
  mode satellite logs to syslog with a hard-coded `Debug` maximum and ignores
  `RUST_LOG`. The laptop journal showed 2,554 debug lines in 4 h, all priority 7.
  No niri-config or environment setting changes this. The remaining lever is
  `LogLevelMax=info` on the compositor unit (`wayland-wm@.service`). niri's own
  stderr is journaled at info, so this drops only debug-priority messages. It is
  a new installed drop-in in both session installers, which N0312 is editing
  now, so it waits for a lead decision. Alternatively, satellite could honour
  `RUST_LOG` in syslog mode upstream.

## Checks

The focused tests are `session_state_starts_unread_and_publishes_only_changes`,
`session_state_transcript_replays_with_rising_revisions`, the tooltip assertions
in `registered_item_owner_and_methods_are_checked_on_private_bus`,
`a_watcher_leaving_mid_refresh_is_a_handover_not_a_failure`,
`failed_ddc_probe_is_cached_until_the_monitor_set_changes` and
`a_session_watcher_stopped_with_the_daemon_is_not_reported`.

They ran in one dev-2 `nacre-ci/arch-nacre` container (`--network none`,
vendored cargo set, `-j2`) on the tree rebased over N0311. These checks passed:
`cargo fmt --all -- --check`, the `nacre-protocol` export `--check`, clippy
`-D warnings` on `nacre-protocol` and `nacre-helper` (all targets), the
`nacre-protocol` tests, and the protocol minimal consumer. `cargo test -p
nacre-helper` passed 1,344 tests and failed 1:
`desktop_settings::direct_user_manager_call_binds_private_context_and_exports_no_credentials`.
That test needs the `desktop-settings-test` example under the workspace
`target/`, which CI builds first; this container used a private target dir. The
file is untouched here. The N0314 rebase merged without conflicts, and hosted CI
is the full gate.

Line delta: product +344/−55 (≈45 of the additions re-indent the tray loop
into the handover block), tests +243/−3, generated protocol files +67/−2.

Follow-up (not done): the wallpaper rotation could read `session.state` instead
of keeping its own `wallpaper-media session --watch` child.
