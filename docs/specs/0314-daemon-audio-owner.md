# N0314: the daemon owns the PipeWire client

Status: implemented (Platform, 2026-11-13). Daemon half of Shell N0227 and review E
recommendation 1: `standing-rules.md` says headless services live in the daemon, yet the
daemon reached PipeWire through child processes while the shell ran its own native client.

## Before: the audio inventory

| Spawn | Where | Fed |
| --- | --- | --- |
| `pactl subscribe` (long-lived child, restarted with backoff) | osd.rs | wake-up for everything below |
| `pactl --format=json list sinks/sources/sink-inputs/source-outputs`, `pactl get-default-sink/source` (6 one-shots per change, 25 ms debounce) | osd.rs | `audio` topic: devices, per-app streams, defaults |
| `wpctl get-volume` + `wpctl inspect` on `@DEFAULT_AUDIO_SINK@`/`@DEFAULT_AUDIO_SOURCE@` (up to 4 per change) | osd.rs | OSD `volume`, `microphone`, `output` events |
| `wpctl inspect` + `pw-dump` (sink read), `pw-cli set-param` (write), `wpctl set-volume/get-volume` fallback, on a worker thread with a cached sink | pill_controls.rs | `audio.step` (fast socket), `audio.adjust_volume`, `audio.toggle_mute` (also `wpctl set-mute`) |
| `wpctl get-volume/set-volume/set-mute/set-default`, `pactl set-{sink-input,source-output}-{volume,mute}`, `pw-dump` (name to id) | control_center.rs | `audio.set_volume`, `audio.set_mute`, `audio.set_default` |
| `parec --device=@DEFAULT_SOURCE@` 8 kHz mono (lease-only child) | audio_peak.rs | `audio.peak` |
| `parec --device=@DEFAULT_MONITOR@` 48 kHz mono (lease-only child, reopened on default change) | audio_spectrum.rs | `audio.spectrum` |
| `wpctl`/`pw-dump` presence + `pipewire-0` socket | pill_controls.rs, control_center.rs | method advertisement |

Not daemon paths, left alone: `pactl` in `launcher_workflows.rs` (audio reset workflow),
`wpctl` in `desktop_actions.rs` (CLI), `pw-dump` in `diagnostics*.rs`.

## After: one client, no child, no poll

`crates/nacre-helper/src/daemon/audio.rs` (the `pipewire` crate, `=0.10.1`, the version
`bead-services` already pins in the lock file; no second copy is linked):

- One thread runs the PipeWire main loop for the daemon's life: registry with bound proxies
  for `Audio/Sink`, `Audio/Source`, `Stream/Output/Audio`, `Stream/Input/Audio` nodes
  (`Props` subscribed: `channelVolumes`, `mute`), `Audio/Device` devices (`Route`
  subscribed, for hardware volumes) and the `default` metadata. Reconnects with backoff
  (0.5 s doubling to 8 s) when the server is away; the topics are then honestly unavailable.
- Every change the server pushes becomes one whole `audio` state on a channel to the async
  side, which coalesces bursts (25 ms) and publishes the topic plus the OSD `volume`,
  `microphone` and `output` events from the default devices, with the same baseline, echo
  and duplicate rules as before (`osd.rs` keeps caps lock, night light, power profile and
  keyboard backlight; its `Env` lost `wpctl`/`pactl`).
- Actions are commands on the loop's channel, stored as `hub.audio` (a `Handle`):
  `audio.set_volume`/`set_mute` for the default sink/source, a node by `node.name` or a
  stream by topic id; `audio.set_default` through `default.configured.audio.{sink,source}`
  metadata; `audio.step`/`adjust_volume`/`toggle_mute` on the default sink. Writes go to
  the device route when the active profile has one for the node (what `wpctl` does), else to
  the node's `Props`; the local copy is updated at once so a held key never waits on an echo.
  The fast socket sends `Command::Step` straight to the loop (no worker thread, no cache, no
  coalescing needed: a step costs one message), and the OSD is published from the step's
  outcome as before (`OSD_STEP` marker kept).
- `audio.peak` and `audio.spectrum` are capture streams on the same connection, created when
  their topic gains a subscriber and destroyed when it loses the last one: 8 kHz / 48 kHz,
  16-bit mono, whole windows (66 ms / one 60th) to the async side; `audio_peak.rs` keeps the
  peak math, `audio_spectrum.rs` the analyser, leases and projection. A stream without a
  target follows the default device, so the spectrum no longer reopens on a sink switch.

Topic contracts are unchanged (fixtures and schemas untouched). Stream ids stay
`sink-input:<id>`/`source-output:<id>`: pipewire-pulse's indices were the PipeWire object
ids. Device volumes are read from node `Props` (linear); `readback.perceptual_volume` is
the cube root as before, now to 4 decimals instead of pactl's `/65536`.

Behaviour differences worth knowing: `audio.adjust_volume` now snaps to whole percent like
`audio.step` (it shared no caller with a finer delta); the client connects at daemon start
and stays connected (idle cost is zero wakeups; the shell subscribes `audio` permanently
after N0227 anyway), where the old `pactl subscribe` ran only under a lease.

## Measurements (dev-1 container, private PipeWire + WirePlumber + pipewire-pulse, null sink)

TBD

## Tests

- `daemon::audio::tests`: graph to topic, target resolution, step math, OSD event rules,
  `run` fed with states/steps/PCM through the client channel with a fake handle (topic,
  OSD, capture leases, stall, onset); `live_pipewire_roundtrip` (ignored) against a real
  server: echo of a write, a step outcome, set-default, both captures delivering windows.
- `control_center`, `pill_controls`, `fast` tests assert the commands sent to a fake handle;
  `osd` tests lost their `wpctl`/`pactl` scripts; `tests/daemon.rs` asserts an audio lease
  spawns nothing; the memory harness's spectrum phase is lease-only without a server.

## For the Shell lead (N0227): what the bar pills read from the in-process Desktop

| Shell field (bead `Snapshot`) | Daemon topic | Note |
| --- | --- | --- |
| `audio.volume`, `audio.muted`, `audio.default_sink` | `audio`: `default_output` + `nodes.items[id].readback.{perceptual_volume,muted}`, `name` is the description | covered |
| `audio.mic_volume`, `audio.mic_muted` (OSD) | `audio`: `default_input` likewise | covered |
| `battery.{available,percentage,charging,on_battery}` | `battery` | covered |
| `network.{connected,wireless_enabled,name,strength}` | `network`: `primary_connection`, `connections`, `access_points[].strength/active` | derivable |
| `bluetooth.{powered,connected_devices}` | `bluetooth`: `adapters[].powered`, `devices[].connected` | covered |
| `session.{locked,sleeping}` (wallpaper_motion pause) | `session` (record) | confirm the record carries lock and sleep; otherwise a two-field topic is cheap |
| MPRIS `Player.{track_id,art_url,position_at}` | `dashboard.player` has `art_path`, `position_updated_ms`; no `track_id` | shell does not read `track_id` today |
| `clock`, `date` | none | local to the shell |

Nothing was added to the daemon topics: no field was missing for a pill.
