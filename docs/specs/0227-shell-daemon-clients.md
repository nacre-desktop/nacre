# N0227 The shell stops running its own system clients

Shell lead decision 2026-11-13 on Platform's architecture review E, recommendation 1. `nacre-shell --bar` started Bead's in-process `Desktop` services with the default `Config`: a PipeWire client thread plus a system-bus thread (NetworkManager, UPower and BlueZ reads on every signal, a StatusNotifierHost tray client, login1, timedate1 and the clock). The daemon already runs those clients for its `audio`, `network`, `bluetooth`, `battery` and `tray` topics. [Standing rules](../architecture/standing-rules.md): the daemon owns system clients; the shell is a view. Daemon half: Platform N0314 (the daemon owns the PipeWire client).

## Inventory

| Shell surface | In-process store (Bead `Snapshot`, pin 5c0e3136) | Daemon topic / control | After N0227 |
| --- | --- | --- | --- |
| Network pill | `bead.desktop.network` ← `network.connected` | `network` (wifi device connected, `wireless_enabled`, health) | Daemon via `reload/bar_daemon.rs` (B0022), which overwrites the in-process value on every event |
| Bluetooth pill | `bead.desktop.bluetooth` ← `bluetooth.powered` | `bluetooth` (any adapter powered) | Daemon, `bar_daemon.rs` |
| Battery pill | `bead.desktop.battery*` ← `battery.{available,charging,percentage}` | `battery` (`percentage`, `charging`, health) | Daemon, `bar_daemon.rs` |
| Audio pill, scroll, mute | `bead.desktop.audio*` ← `audio.{volume,muted,default_sink}`; `services.audio*` → Bead `Action::Volume/Mute` | `audio` (default output readback); `audio.adjust_volume`, `audio.toggle_mute` (`bar_routes.rs`) | Daemon; in-process audio client and action arms deleted |
| Sound popout | none | `audio` lease, `audio.set_volume`/`set_mute` (`popouts::core_sound`, `sound/core.luau`) | Unchanged (already daemon) |
| Volume OSD | `audio_changed` on in-process audio (`--notification-bus` fixture mode only) | daemon `osd` events (`--notices`, production) | In-process path deleted |
| Tray pill/list | `bead.desktop.tray` ← Bead bus-lane `system_tray::Client` (a StatusNotifierHost; `tray_watcher` only adds the watcher) | `tray` topic + tray routes (title, visibility, icon, attention and menu); no tooltip description field | Bar list/actions use the daemon; tooltip shows its title only. The Bead host still runs while `bus` is on (B1) |
| Media | none: the pinned Bead `Desktop` has no MPRIS client | `dashboard.player` | Nothing to switch. Bead main's new opt-in MPRIS/power-profile/Bluetooth host services stay off in Nacre |
| Clock | `bead.desktop.clock*` ← bus lane (local time; timedate1 and login1 resume resync) | `time` has the timezone only; formatting stays local | Stays in-process |
| Wallpaper-motion gates | `battery.on_battery`, `session.{locked,sleeping}` (`wallpaper_motion.rs`) | `battery.on_battery`; lock/sleep not published (gap P1) | Stays in-process |

## Change

- `desktop::start_services()` is the one place Nacre configures Bead's `Desktop`: `audio: false, bus: true, tray_watcher: false`. Every product start site uses it (bar, Widgets app warm runtime and factory). Bead spawns one thread per enabled lane (`runtime.rs:171-180`), so each `Desktop` drops from two worker threads to one. The PipeWire thread starts whether or not PipeWire answers, and no product path starts it now. No test counts threads: the parallel test binary has no stable baseline.
- The `--notification-bus` fixture runtime no longer starts a `Desktop`. Its only consumer was the in-process volume OSD.
- Deleted dead glue: the `services.audio` / `services.audio.volume` arms in `Runtime::desktop_action`, `with_desktop()`, `private_service_proof` and `--services-ui-probe`, the `--audio-panel` legacy sound panel with `legacy_sound_panel_enabled` and its test, the audio-pill panel prepare (`prepare_panel` for `services.audio`), `Notices::audio_changed` and its state, and the legacy sound-panel action test. `services.audio` stays registered: daemon bar mode authorizes it and routes scroll to `audio.adjust_volume` and right-click or activation (the non-popout pill, the desktop sound widget's Mute button; before, an unrouted no-op in daemon mode) to `audio.toggle_mute`.
- Bar goldens are unchanged. The pill stores and their projections did not move; only their in-process fallback stopped carrying audio.
- No daemon change, no Bead change.

Temporary: `shell/sound/main.luau` is no longer reachable from the product. It stays only as the QML-paired fixture for right/left edge-attached panels (`placement_pixels_tests`, `contour.rs`, route smoke, hover lifecycle). Delete it, its i18n ids and the `sound`/`sound-left` placement cases when those tests move to a product attached panel. Expiry: 2026-11-27.

## Gaps and requests

- Platform lead (P1): no topic publishes logind lock and sleep state. The daemon already watches it (`behaviours.rs` `Session`, `wallpaper-media session --watch`), but the `session` topic carries the session-policy record. Request a published `locked`/`sleeping` pair, so wallpaper motion and the clock's resume resync can leave the shell's system bus.
- Platform lead (P2): `tray` has no tooltip-description field. The daemon-backed bar displays the item title only; request the description if parity with Bead's richer tooltip is required. This is a tooltip metadata gap, not a missing status-pill field.
- Bead lead (B1): `bead_services::Config` cannot keep the clock without the rest of the `bus` lane. NetworkManager/UPower/BlueZ refresh, the system-wide signal match and the tray `Client` all run on it (`bus.rs:89-175`). Request a switch for a clock-only lane: clock and timedate1, plus login1 until P1 lands, with no device sensors and no StatusNotifierHost. Follow-up N0227b: set it, then read `on_battery` and lock/sleep from daemon topics.

## Validation

dev-1, one fresh `n0200-build` container on the rebased tree (2026-11-13): `cargo fmt --all --check` pass; `cargo clippy -p nacre-shell --all-targets -- -D warnings` pass; whole `cargo test -p nacre-shell --bin nacre-shell` 891 passed, 0 failed, 4 ignored (bar goldens unchanged, no golden updated); `tools/check_product_shell.py` Luau analyzer OK (182 modules; its `wallpaper_video_smoke` step needs `sway`, which the container cannot start). Net code delta +26/−312 lines in `crates/nacre-shell`. No thread-count test: the CI container has no PipeWire and the parallel test binary has no stable thread baseline; the removed thread is the `bead-services` audio lane (`runtime.rs:171-180`).
