# N0049 — Contracts the Bead Settings pages need

Status: **Implemented; pending lead review** (Claude worker, branch `task/n0049-settings-contracts-20261023`). Consumer: Bead
B0058 (`docs/specs/0058-settings-pages-batch-1.md` in Bead), "Nacre asks". Additive, v1 only, outside daemon safe mode.
QML unchanged. CI: the whole `desktop` job (fmt, clippy -D warnings, all cargo suites incl. 66 result lines / 987 tests,
minimal consumer, release helper, parity sandbox, i18n check/generate, `tools/check.py`, galleries) passed in an Arch
container on prod. Types are `nacre-protocol::system_settings` (MIT OR Apache-2.0); schemas, Luau types and the golden
transcript are generated (`protocol/schema/{time-state,time-set-timezone,capabilities-state}.json`,
`protocol/luau/system-settings.d.luau`, `protocol/fixtures/system-settings.jsonl`).

## What B0058 asked for, and the answer

| Ask | Answer |
| --- | --- |
| 1. Date & time | New `time` topic and `time.zones`, `time.set_timezone`, `time.set_ntp` (below). 12/24 h is the store key `desktop.clockFormat`, written with `settings.set`; no method. |
| 2. Notification history | **Nothing to add.** N0017 has it: topic `notifications` carries the bounded `history` window, `history_count`, `history_more` and `history_storage`; `notifications.clear {scope:"history"}` clears it (`retained` also dismisses retainable active notices). The QML page's `NacreNotifs.retained.length` is `history_count`; its list is the window newest first. |
| 3. Workflows (`device-content`) | **Nothing to add.** The QML page only calls the desktop controller: `settings.desktop {arguments:["preset", name], rev}` and `{arguments:["save-workflow", preset, rolesJson], rev}`, and shows `data.preset` and `message` from the `settings.desktop` topic. `rev` is the `settings.store.desktop` revision (stale gives `stale_revision`). The sets are fixed by the controller: presets `normal focused presentation minimal meeting music docked`; roles `Browser`, `Nacre AI`, `Discord`, `Spotify`, `Mail`, `Brain` (the QML literals; Bead keeps them as data in its page). |
| 4. Lock & login | Lock appearance and privacy options are plain store keys already writable with `settings.set` (`desktop.lockMedia`, `lockWeather`, `lockNotifications`, `lockNotificationContents`; the daemon applies the lock effect). Idle and lid keys (`power.idle.*`, `power.hardware.lid`) are `planned` in the registry and hidden by QML as well. "Lock now" is `session.request {action:"lock"}` (N0031). Only the privileged theme install is new: `login.install_theme` (below). "Preview layout" (`nacre-shell lock-preview`) is a QML-shell window; Bead previews its own lock. |
| 5. Capabilities | New `capabilities` topic (below). `monitorWorkspaceActions` is a setting key, not a capability: its `enabledWhen` is `niriFixedWorkspaces`, which is published. |

## Time (`time` topic, `time.*` methods)

Source: `org.freedesktop.timedate1` on the system bus, the service `timedatectl` talks to (QML `TimezoneSettings` ran
`timedatectl list-timezones`; the writes went through `pkexec timezone.py`, which ends in `timedatectl set-timezone`).

- Topic `time` (`TimeState`): `health`, `timezone`, and `ntp {supported (CanNTP), enabled (NTP)}` once timedated answered.
  One read at daemon start, then refreshed on its `PropertiesChanged` and when its name appears; the name vanishing (timedated
  exits when idle) does not trigger a read, so it is never restarted in a loop. Without timedated the topic is
  `health.available:false` and the methods are not advertised.
- `time.zones {}` -> `{zones:[...]}` (`ListTimezones`, names validated, at most 2048).
- `time.set_timezone {zone}` -> `{changed}`; `time.set_ntp {enabled}` -> `{changed}`. Same value: `{changed:false}` without
  a bus write or a prompt. Zone names are IANA names only (`[A-Za-z0-9_+-]` parts separated by `/`, at most 100).
- Privilege: both writes call `SetTimezone`/`SetNTP` with `interactive:true`, so **polkit asks the session's authentication
  agent** (`org.freedesktop.timedate1.set-timezone` / `set-ntp`, default admin authentication) and the call waits for the
  answer; the method deadline is 120 s. A dismissed prompt, a wrong password or no agent in the session is
  `permission_denied` (`dispatched:false`, `retry:true`, nothing changed). Bead must have an agent running (the compositor
  session normally does) or the user sees only that error. Capability class `system`.
- Not covered: the travel-timezone automation (`timezone.py` `install|update|automatic|confirm|manual`, the
  `/etc/nacre/timezone.json` units). It installs system units as root and is Nacre-specific; Bead keeps "Automatic timezone"
  out until its own unit. Setting the zone with `time.set_timezone` while that automation is installed and on can be undone by
  its next run.

## `login.install_theme {}`

Runs the README flow, `pkexec /usr/bin/python3 <tools>/login-install.py --user <account> --theme <login folder>`, then the
helper's own `login-appearance` (publishes colours and wallpaper for the greeter). No parameter reaches a command line; the
account is the daemon's uid, the folders are the daemon's tool folder and `<tools>/../login` (the installer's own default).
Reply `{installed:true, appearance_published, manifest?}` (`manifest` is the rollback manifest path the installer prints).
`pkexec` exit 126/127 (dialog dismissed, not authorised) is `permission_denied`; any other failure is `io`; installer output
is never relayed. One install at a time (`busy`). Advertised only when `pkexec`, python, the installer and `login/Main.qml` exist.
**Finding:** the installed tool folder (`~/.local/share/nacre/shell/tools`) contains `login-install.py`, but the installer does
not ship the `login` theme folder, so on an installed system the method is not advertised until the theme is installed there
(it works from a source checkout layout). Shipping the theme is an installer change and not part of this unit.

## `capabilities` topic

`CapabilitiesState {health, flags}`: a map of the flags the registry's `visibleWhen`/`enabledWhen` name, computed as QML's
`NacreSettingsCapabilities` did. From published topics: `niriWorkspaces`, `input.controls`, `input.touchpadSpeed`,
`windowCapture`, `panelDragging`, `desktop.{gapsIn,gapsOut,borderSize,rounding,blur,shadow,followMouse,naturalScroll}`
(all true while the compositor lists no `desktopOptions`, as QML), `niriFixedWorkspaces` (niri and `desktop.niriWorkspaceMode`
is `fixed-all`), `ai`, `brain` (`profile` components). Probed once per daemon start: `touchpad` (a touchpad/trackpad in
`/proc/bus/input/devices`), `packages` (`pacman` on PATH), `powerProfiles` (`powerprofilesctl get` succeeds),
`performanceProfile` (`powerprofilesctl list` offers it). Recomputed when `compositor`, `settings.store.desktop` or
`settings.store.profile` change. `batterySaver` and `cursorShake` are named by the registry but QML never defined them: they
are absent, which consumers treat as false (the QML result). A touchpad or `powerprofilesctl` hot-plug is seen after the next
daemon start.

## Acceptance

Protocol: `cargo run -p nacre-protocol --example export` regenerates the artifacts; `tests/system_settings.rs` replays the
transcript through the mirror and validates every state. Helper tests with fakes only (private `dbus-daemon` with a fake
`org.freedesktop.timedate1`; a fake `pkexec` script and fake helper engine; temp input-device list and a fake
`powerprofilesctl`): topic mirrors timedated, `interactive:true` on both writes, no-op writes make no call, refused prompt is
`permission_denied` and changes nothing, unsupported NTP, outside changes published, timedated absent then appearing, zone
validation never reaches the bus, install argv exactly as documented, dismissed prompt, failure without output echo,
one-at-a-time, safe mode, flag derivation per backend, probes, live topic follow. No test runs `pkexec`, polkit, the real
clock or a real bus. Whole desktop job in an Arch container on a remote CI host (see the queue row).

## Temporary duplicates

None added. The QML time page and `timezone.py` stay until the QML shell retires (separate decision, as N0046).
