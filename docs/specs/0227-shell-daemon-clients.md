# N0227 The shell stops running its own system clients

Shell lead decision 2026-11-13 on Platform's architecture review E, recommendation 1. `nacre-shell --bar` started Bead's in-process `Desktop` services with the default `Config` (PipeWire client plus a system-bus thread with NetworkManager, UPower and BlueZ clients), duplicating the clients the daemon already runs for its `audio`, `network`, `bluetooth` and `battery` topics. [Standing rules](../architecture/standing-rules.md): the daemon owns system clients; the shell is a view.

## Inventory

| Shell surface | In-process store (Bead `Snapshot`) | Daemon topic / control | After N0227 |
| --- | --- | --- | --- |
| Network pill | `bead.desktop.network` ← `network.connected` | `network` (wifi device connected, `wireless_enabled`, health) | Daemon, via `reload/bar_daemon.rs` (B0022); the in-process value was already overwritten on every event |
| Bluetooth pill | `bead.desktop.bluetooth` ← `bluetooth.powered` | `bluetooth` (any adapter powered) | Daemon, `bar_daemon.rs` |
| Battery pill | `bead.desktop.battery*` ← `battery.{available,charging,percentage}` | `battery` (`percentage`, `charging`, health) | Daemon, `bar_daemon.rs` |
| Audio pill, scroll, mute | `bead.desktop.audio*` ← `audio.{volume,muted,default_sink}`; `services.audio*` → Bead `Action::Volume/Mute` | `audio` (default output readback); `audio.adjust_volume`, `audio.toggle_mute` (`bar_routes.rs`) | Daemon; the in-process action arms, the `--audio-panel` legacy panel and the `--services-ui-probe` proof are deleted |
| Sound popout | — | `audio` lease + `audio.set_volume`/`set_mute` controls (`popouts::core_sound`, `sound/core.luau`) | Unchanged (already daemon) |
| Volume OSD | `audio_changed` on the in-process audio (legacy `--notification-bus` mode only) | daemon `osd` events (`--notices`, production) | In-process path deleted |
| Tray | `bead.desktop.tray` ← Bead tray watcher | `tray` topic | Unchanged (`tray_watcher: false` since B0022) |
| Media | none: Bead 5c0e3136 has no MPRIS client in `bead-services` | `dashboard.player` | Nothing to switch |
| Clock | `bead.desktop.clock*` ← Bead bus thread (local time, timedate1, login1 resume) | none (local time must not travel over IPC) | Stays in-process |
| Motion wallpaper gates | `battery.on_battery`, `session.{locked,sleeping}` (`wallpaper_motion.rs`) | `battery.on_battery`, `session` | Stays in-process until the bus split below |

## Change

- `desktop::start_services()` is the one place Nacre configures Bead's `Desktop`: `audio: false, bus: true, tray_watcher: false`. Every product start site uses it (bar, notification worker, Widgets app warm runtime and factory). The PipeWire client thread is gone from the shell and from each Widgets app runtime (`bead-services/src/runtime.rs:171-180` spawns one thread per enabled lane; no test measures it because the CI container has no PipeWire).
- Deleted dead glue (net −246 lines): in-process `services.audio` / `services.audio.volume` arms in `Runtime::desktop_action`, `with_desktop()`, `private_service_proof` and its `--services-ui-probe` flag, the `--audio-panel` legacy sound panel and `legacy_sound_panel_enabled`, `NotificationHost::audio_changed` and its state. `services.audio` stays registered: the bar's scroll/right-click on the audio pill is renamed to it before routing to the daemon.
- Bar goldens unchanged: the pill stores and their projections did not move; only their in-process fallback source stopped carrying audio.
- No daemon change, no Bead change, no temporary duplicates.

## Gaps and requests

- Bead lead: `bead_services::Config` has no way to keep the clock (and login1/timedate1) while dropping the NetworkManager/UPower/BlueZ match rules and `refresh_sensors` (`bus.rs:152-175, 300-390`); both run on the one `bus` lane. Request a `sensors: bool` (default true) so Nacre can set it false. Follow-up N0227b: set it, and move `wallpaper_motion`'s `on_battery` / locked / sleeping reads to the daemon `battery` and `session` topics.
- Platform lead: no pill field is missing from the daemon topics.

## Validation

Filled in from the dev-1 run (see the report).
