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

Topic contracts are unchanged (fixtures and schemas untouched). Sources are `Audio/Source` and
`Audio/Source/Virtual` nodes (EasyEffects or noise suppression can be the default), as pactl
listed them; sink monitors stay out. Stream ids stay
`sink-input:<id>`/`source-output:<id>`: pipewire-pulse's indices were the PipeWire object
ids. Device volumes are read from node `Props` (linear); `readback.perceptual_volume` is
the cube root as before, now to 4 decimals instead of pactl's `/65536`.

Behaviour differences worth knowing: `audio.adjust_volume` now snaps to whole percent like
`audio.step` (it shared no caller with a finer delta); the client connects at daemon start
and stays connected (the shell subscribes `audio` permanently after N0227 anyway), where the
old `pactl subscribe` ran only under a lease. With no PipeWire server the client retries
with backoff up to every 8 s.

## Measurements (dev-1, `nacre-ci/arch-nacre:current` plus pipewire, wireplumber, pipewire-pulse; null sink)

Release daemons from origin/main 111f3bb3b (before) and the same base plus N0314 (after),
run alternately, each alone with a fake niri socket and one client subscribed to `audio` +
`osd` (`.lead/n0314/measure.py`; two runs each, values per run).

| | before | after |
| --- | --- | --- |
| child processes while `audio` is subscribed | `pactl subscribe` (+ up to ten one-shots per change) | none |
| threads | 3 (main, `nacre-audio-step`, fast lane) | 3 (main, `nacre-audio`, fast lane) |
| idle 60 s: context switches/s | 7.07, 7.03 | 7.00, 6.93 |
| idle 60 s: CPU | 0.27 %, 0.30 % | 0.32 %, 0.28 % |
| idle 60 s: switches on the audio thread | 0, 0 | 0, 0 |
| 10 external `wpctl set-volume` changes: OSD latency median | 95.3 ms, 95.3 ms | 31.1 ms, 31.3 ms |
| same: CPU, context switches | 4.0 %, 3.6 %; 641, 632 | 0.23 %, 0.0 %; 97, 97 |
| 10 `audio.step` key steps on the fast socket: OSD latency median | 0.8 ms, 0.9 ms | 0.5 ms, 0.5 ms |
| same: CPU, context switches | 5.0 %, 5.7 %; 671, 686 | 0.0 %, 0.0 %; 126, 121 |

Idle wakeups: the first measurement (5.75 → 7.23/s, on another base) was run-to-run noise.
Per thread, every idle switch in both daemons is on the main thread (~420 per 60 s); under
`strace -f` the `nacre-audio` thread made no system call in a 60 s idle window. The main
thread's wakeups are the 500 ms caps-lock LED tick, each spawning a short-lived read thread
on that base (133 clones in 60 s; N0311's offload-thread reuse, now on main, keeps one): the
harness's fake niri reports no lock state, so NN0036's compositor path is not taken. That is
N0305/NN0036 territory, not the audio client.

The external-change latency is the 25 ms coalescing window plus one server round trip; the
old path spawned up to ten processes per change. Both daemons ended the step sequence at the
same linear level (0.421875 = 75 % perceptual), so the step math is unchanged.

## Tests

- `daemon::audio::tests`: graph to topic, target resolution, step math, OSD event rules,
  `run` fed with states/steps/PCM through the client channel with a fake handle (topic,
  OSD, capture leases, stall, onset); `live_pipewire_roundtrip` (ignored) against a real
  server: echo of a write, a step outcome, set-default, both captures delivering windows.
- `control_center`, `pill_controls`, `fast` tests assert the commands sent to a fake handle;
  `osd` tests lost their `wpctl`/`pactl` scripts; `tests/daemon.rs` asserts an audio lease
  spawns nothing; the memory harness's spectrum phase is lease-only without a server.
- Run on dev-1 in `nacre-ci/arch-nacre:current` with the vendored cargo set, on main 09c152aac:
  `cargo fmt --check`, `cargo clippy -p nacre-helper -p nacre-protocol --all-targets -- -D
  warnings`, `cargo test -p nacre-protocol` (121 passed) and `cargo test -p nacre-helper
  --no-fail-fast` passed, except `tests/desktop_settings.rs`, which needs the
  `desktop-settings-test` example that CI builds first (`--examples`) and this job did not.
  `live_pipewire_roundtrip` passed against PipeWire, WirePlumber and pipewire-pulse with a
  null sink and an `Audio/Source/Virtual` null source as the default source. On the previous
  base (dev-2), the memory harness (`memory::`) and the nacre-shell audio consumers
  (`nacre_source`, `audio_runtime`, `status_views`, `control_center`: 158) passed.

## For the Shell lead: what N0227's daemon-backed pills read

N0227 (on main) moved the bar audio pill, the desktop sound widget and the volume OSD to daemon
topics. Compared field by field with what this client publishes:

| Shell reader | Reads | Published here |
| --- | --- | --- |
| `reload/bar_daemon.rs` (bar audio pill, sound widget), `status_views.rs` (sound page) | `audio`: `health.{available,stale}`, `default_output`/`default_input`, `nodes.items[id].{kind,name,ready,volume,muted}`, `readback.{perceptual_volume,muted}` | all; same contract as before |
| `notice_service.rs` (OSD) | `osd` events `volume`/`microphone`/`output`: `value`, `muted`, `label`, `device` | all, from the default nodes and from every step |
| `bar_routes.rs` (scroll, right-click) | `audio.adjust_volume {delta}`, `audio.toggle_mute {}` | both; `adjust_volume` now shows the volume OSD at once instead of via the `pactl` echo |
| Media (dashboard, launcher) | `dashboard.player` | not N0314 (MPRIS, unchanged) |

No audio field is missing for Shell. N0227's two open requests were outside audio and have
since landed in N0317: the `session.state` topic (logind lock and sleep) and tray tooltips.
