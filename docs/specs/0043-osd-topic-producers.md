# N0043 — `osd` topic and producers

Status: **Implemented; pending lead review**. Consumer: Bead B-B1 (OSD kind set). Additive: no
existing method, topic or field changes. Origin: the owner decided on no QML after the Bead switch,
so every kind the QML OSD (`nacre/shell/modules/osd/*`) shows needs a daemon source. There is no
keyboard-layout OSD in the QML and none is added.

## Contract

Topic `osd`. State is `OsdState` with `health` only (`{available:true,stale:false}`); `current` is
never set (see Temporary duplicates). Every change is an `ev` message named `changed`:

```json
{"t":"ev","topic":"osd","name":"changed","seq":7,"data":{"kind":"volume","value":0.5,"muted":false,"label":"Speakers","device":"alsa_output.pci-0000_00_1f.3.analog-stereo","ts":1800000000100}}
```

`seq` is the per-topic event sequence of the envelope (shared ring rules: `lost` ranges on overflow;
events are never replayed). `data` is `OsdEvent` (`protocol/schema/osd-event.json`,
`protocol/luau/osd.d.luau`, golden `protocol/fixtures/osd.jsonl`):

| `kind` | `value` | other fields | QML channel | Source |
| --- | --- | --- | --- | --- |
| `volume` | fraction of the default output's level (may exceed 1) | `muted`, `label` (description), `device` (node name) | volume | `wpctl` readback after `pactl subscribe` |
| `output` | new default output's level | same | output | default output node name changed |
| `microphone` | default input level | same | microphone | `pactl subscribe` + `wpctl` |
| `brightness` | 0..1 | `device` = output name | display | `brightness.set`/`adjust` |
| `keyboard` | 0..1 | `max` = raw steps | keyboard | `keyboard_light.adjust`; UPower `BrightnessChanged` |
| `night_light` | 1 on, 0 off | | night-light | `control-tools` ownership record |
| `power_profile` | | `label` = `power-saver`/`balanced`/`performance` | power-profile | power-profiles-daemon `ActiveProfile` |
| `capslock` | 1 on, 0 off | | capslock | `/sys/class/leds/*::capslock/brightness` |

Texts, glyphs and the "show the output name only with several screens" rule stay in the shell, which
localizes from `kind`, `label` and `value` as the QML does. Unknown optional fields must be ignored.

## Behaviour (as the QML)

- Events mean a real, user-visible change. Producers read a silent baseline when the first
  subscriber arrives, so nothing fires at startup or on a later subscriber; `output` fires only
  when one default output replaces another; a new default microphone is silent (the QML did not
  show it either).
- Bursts coalesce for 40 ms into the last event, except that an `output` event is never replaced by
  the `volume` event its device change causes (QML `pendingChannel`). `pactl` events are debounced
  25 ms. An identical event within 250 ms of another is dropped (a method report plus the bus
  signal for one key press).
- Brightness and keyboard backlight are reported from the daemon methods, on every call that
  succeeds (like the QML `adjusted` signal, including a keyboard step that stays at 0). Changes
  made behind the daemon's back (a firmware key, a foreign tool) are shown only for the keyboard
  backlight, through the UPower signal; displays have no portable change signal.
- Night light is the existing Nacre control: `enabled` with a non-empty `owner` in
  `control-tools.json`/`control-tools-niri.json`, i.e. exactly `nightLightEnabled` of the QML.
  An externally managed night light is not detected, as in the QML.
- Caps Lock reads the LED the QML read. This is the published-safe path: no evdev, no
  `/dev/input` access, no compositor protocol. A keyboard without a Caps Lock LED, or a
  compositor that does not drive it, yields no event. It is read every 250 ms (as the QML did)
  only while subscribed; night light every second.
- Nothing runs while no client subscribes to `osd` (no `pactl` child, no timer, no bus match).
  The service is supervised (`osd` in the `daemon` topic's `services`) and never starts in
  safe mode. If the audio subscription ends, the supervisor restarts the session, which takes a
  new silent baseline.
- Runtime dependencies are optional and already used: `wpctl` (pipewire), `pactl` (libpulse,
  `pipewire-pulse`), the system bus. A missing one disables only its kinds.

## Not in scope

No client-driven OSD method, no persistence, no QML change (the QML still builds its own OSD), no
display brightness change detection outside the daemon, no keyboard-layout OSD, no DND kind (the
QML has a `dnd` channel for Do Not Disturb; the `notifications` topic already publishes
`dnd_changed`).

## Temporary duplicates

- `brightness.*` reply field `level_percent` (N0036) duplicates the `brightness` event for a
  caller that also subscribes. Delete it 30 days after Bead renders OSD from the events.
- `core_services::OsdState.current` / `OsdItem` (the earlier, never produced state-patch shape with
  four channels) is superseded. Delete it with its fixtures `core-compat-osd.jsonl` and tests
  30 days after Bead consumes `osd` events.
- The QML OSD stays until the Bead switch, as everything QML.

## Verification

Generated with `cargo run -p nacre-protocol --example export` (never by hand). Tests: protocol
validation and transcript tests; daemon tests with fake `wpctl`/`pactl` scripts, a temp LED tree
and night-light record, a private session bus with fake power-profiles-daemon and UPower
`KbdBacklight`, and a hub client subscription proving nothing runs before the first subscriber and
that a later one starts it again. No real audio, hardware, session bus or system bus is touched.
