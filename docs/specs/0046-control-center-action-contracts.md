# N0046 — Control-centre action contracts and Bead i18n sync

Status: **Implemented; pending lead review** (CI result below). Consumer: Bead B0049
(`docs/specs/0049-control-center-tiles.md`, "Sources and Nacre asks"). Additive: no existing method,
topic or field changes, v1 only. QML unchanged. The owner decided on no QML after the Bead switch, so
every tile the QML control centre has needs a daemon contract; each method below runs the tool the QML
ran (`nacre/shell/modules/controlcenter/*`, `services/Nacre{ControlActions,ControlTools,Audio,Vpn,...}`).

## Permission model (as N0031)

Strict `deny_unknown_fields` parameters with closed value sets; no caller text reaches a command line
except a PipeWire node name that matches `[A-Za-z0-9._:@+-]{1,128}` and is first resolved to a numeric
id. Safe mode refuses every method (`permission_denied`, `dispatched:false`) and removes it from the
registry. A method is in the welcome registry only while its backend is present (table below); a direct
call without the backend fails `unavailable`. The registry is per connection: reconnect after a backend
appears. Success `{changed:true}` records an accepted action, not state; read `controls`.
Child processes never inherit `NOTIFY_SOCKET` (systemd's own tools and the helper report errors
there, which the supervisor would read as the daemon's). Capability class `system` for hardware/session-wide writes, `desktop` for the rest. Tool work runs
off the event loop with bounded time and output.

## Methods

| Method | Params | Reply | Backend (advertised when) | QML source |
| --- | --- | --- | --- | --- |
| `audio.set_volume` | `{target: sink\|source\|node, node?, value}` | `{changed}` | `wpctl` and `$XDG_RUNTIME_DIR/pipewire-0` | `NacreAudio.setVolume/setMicVolume` |
| `audio.set_mute` | `{target, node?, muted}` | `{changed}` | same | `NacreAudio.toggleMute/toggleMic` |
| `keyboard_light.set` | `{level}` raw steps 0..max | `{changed, level, max}` | `brightnessctl` and a `kbd_backlight` LED | `NacreKeyboardLight.setBrightness` |
| `power.set_profile` | `{profile: power-saver\|balanced\|performance}` | `{changed}` | `powerprofilesctl` and the system bus socket | `PowerProfiles.profile` |
| `airplane.set` | `{enabled}` | `{changed}` | `rfkill` and a `wlan`/`bluetooth` radio in `/sys/class/rfkill` | `NacreControlActions.setAirplane` |
| `night_light.set` | `{enabled}` | `{changed}` | `controls.night_light` present (helper reports it supported) | `NacreControlTools.toggleNightLight` |
| `stay_awake.set` | `{enabled}` | `{changed}` | `systemctl`, `systemd-inhibit` | `NacreMenu.work("tool","stay-awake")` |
| `recorder.start` | `{mode: region\|screen}` | `{changed}` | `wf-recorder`, `slurp` | `record-start` / `record-screen` |
| `recorder.stop` | `{}` | `{changed}` | same | `record-stop` |
| `screenshot.capture` | `{mode: region\|screen\|window}` | `{changed}` | `grim`, `slurp` | `control-tools screenshot` (region); `capture-screen`/`capture-window` |
| `color_picker.pick` | `{}` | `{changed}` | `hyprpicker` | `control-tools color-picker` |
| `wallpaper.next` | `{}` | `{changed}` | settings service and `wallpaper.active` topic | `NacreWallpapers.advanceRotation(true)` |
| `appearance.set_mode` | `{mode: light\|dark\|auto}` | `{changed}` | settings service | `NacreColours.setMode` |

`audio.set_volume.value` is the **linear** level in [0,1] (the scale the QML slider wrote through
Quickshell's `audio.volume`, the one `AudioNode.volume` and `readback.linear_volume` report, the cap
documented in `core-services.md`). Bead's perceptual slider (cube root) converts with `value = slider^3`.
The daemon passes `cbrt(value)` to `wpctl`, whose scale is cubic, and refuses a missing device
(`unavailable`) before writing. `node` is a PipeWire `node.name`, required for `target:"node"` and
refused otherwise; it must name an audio node. Default `sink`/`source` follow the default at call time.
The reserved `CoreCall` shape `audio.set_volume {node: EntityId, volume}` stays reserved and is not
registered; this unit registers the name with the `target` form because the daemon has no `audio`
topic and so no entity ids (ask: reconcile the two when an `audio` producer exists).
Audio and brightness share the existing lanes: a concurrent call fails `busy`; nothing queues.

Behaviour notes.
- `airplane.set`: enabling remembers which of `wlan`/`bluetooth` were on in
  `state/controls-airplane.json` and soft-blocks both; disabling unblocks only those. A hard block or
  any ineffective write fails `io` after re-reading `/sys` (QML used `nmcli` + BlueZ; `rfkill` covers
  both and needs no stored password or adapter).
- `night_light.set`: refused `unavailable` while another controller owns night light
  (`controls.night_light.external`); `changed:false` when already in the requested state.
- `stay_awake.set`, `recorder.stop`, `night_light.set` return `changed:false` when nothing differs.
- `recorder.start`, `screenshot.capture`, `color_picker.pick` start an interactive tool (area
  selection, picker) and reply at once; the tool notifies the user itself and is stopped after 200 s.
  One such tool at a time (`busy`). A running recording makes `recorder.start` fail `busy`. The caller
  closes its own panels first (the QML waited 320 ms); the topic's `recorder` follows the unit.
- `wallpaper.next`: same pool, order or shuffle rules and selection command as the rotation (N0039);
  `changed:false` with fewer than two wallpapers.
- `appearance.set_mode` runs the palette command with the same preference patch as the QML
  (`paletteAutoMode`/`paletteMode`); the wallpaper palette is re-rendered, hence the 120 s deadline.
- Settings window: the QML tile opens its own settings page; there is no Nacre-side route and none is
  added. Bead opens its own window.

## Topic `controls`

State `ControlsState` (`protocol/schema/controls-state.json`, `protocol/luau/control-center.d.luau`,
golden `protocol/fixtures/control-center.jsonl`): `health`, `brightness` (map output name to
percent), and per-backend members present only while the backend is: `keyboard {level,max}`,
`airplane {enabled}`, `night_light {enabled, external}`, `stay_awake {enabled}`, `recorder
{recording}`, `appearance {mode}`. Absent member = no such control; show nothing.

Producer: like `osd`, only while a client subscribes (ticks every 2 s, night light and appearance
every 10 s, everything immediately after any control method or brightness/keyboard method). Reads
`/sys` (backlight, keyboard LED, rfkill), `systemctl --user is-active` for the two units,
`control-tools state` for night light and the `wallpaper-picker` settings store for appearance.
`brightness` has the internal panel (one eDP/LVDS output) and DDC outputs; DDC levels are read when a
client subscribes and after a brightness change, not on ticks. External changes of a DDC output are
therefore seen late. One initial read at daemon start publishes the topic (so the registry knows
whether night light exists); `night_light.set` appears after that read.

## Asks left (separate later units)

- **VPN** tile: `vpn.list` (NetworkManager `vpn`/`wireguard` profiles: `{uuid,name,active}`) as a topic and
  `vpn.set {uuid, enabled}` (`nmcli connection up|down uuid`, 30 s). The `network` topic already carries
  connections but not their type filter or an up/down method that accepts a VPN profile with saved secrets.
- **Displays**: `display.*` (arrange, scale, mode, mirror, rotation, disable) is a compositor
  configuration write and a live-switch risk (the QML ran `display-recover` and settings pages). Needs its
  own unit with rollback/confirm-timeout semantics; Bead keeps the tile unavailable until then.
- DDC brightness read between subscriptions (`brightness.get {output}`) if sub-tick freshness matters.
- Reconcile `audio.set_volume` with the reserved `{node: EntityId}` shape when an `audio` producer exists.

## i18n

`tools/i18n-sync.py` (Bead repo, origin/main `4dfa5ee`) `--write` copied 83 new `bead.*` entries into
`nacre/shell/i18n/en.json` and `context.json`: the `bead.panel.control-*` (42 at that revision) and
`bead.panel.network-*` (41) ids plus every other unsynced `bead.*` id. `tools/i18n/check.py` accepts
`bead.*` as external. (The counts differ from the 45/24 in B0049/B0050 because Bead's tables moved.)

## Acceptance

Regenerated schemas/Luau/golden (`cargo run -p nacre-protocol --example export`); protocol tests
(transcript replays through the mirror, state and parameter bounds). Helper tests with fakes only (temp
HOME and `/sys`, scripts for `wpctl`, `pw-dump`, `rfkill`, `powerprofilesctl`, `systemctl`,
`brightnessctl` and the helper engine, a private socket for PipeWire): linear to cubic volume, node
resolution, rejected targets, vanished device, absolute keyboard step with shared lane, profile
readback, airplane restore and hard block, night light external/no-op, stay-awake and recorder unit
following, one-at-a-time capture lane, appearance request, wallpaper selection, advertisement per
backend and safe mode, state reads, and topic production only while subscribed. Whole desktop job
(`.github/workflows/check.yml`) in an Arch container on a remote CI host. No live/hardware use.

## Temporary duplicates

QML control-centre readers and mutation entry points coexist with these daemon routes; delete 7 days
after the replacement is trusted. Trust condition: stable Bead core-switch use and owner-approved
retirement of the QML shell (same as N0031). Nothing else is duplicated: the methods call the existing
helper commands; there is no second settings writer.
