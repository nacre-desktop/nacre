# N0061 — Wi-Fi scan, application audio streams and the microphone peak

Status: Implemented; pending lead review. Bead B0086 consumes it. Three additive
v1 contracts for gaps B0073/B0076 left ("Wi-Fi Scan remains disabled", "stream
volume/mute and input peak are not ported").

## Contracts

- `network.scan {device}` -> `{changed}` (class System, 10 s deadline, advertised
  with the other `network.*` controls while NetworkManager has a writable
  `WirelessEnabled`). The daemon resolves the published device ID, refuses a
  non-Wi-Fi device (`stale_revision`) or a switched-off radio (`unavailable`), calls
  `Device.Wireless.RequestScan` on NetworkManager's unique owner and then waits for
  `LastScan` to change (polled at 4 Hz, at most 7 s, never beyond the call deadline,
  stopped early when the caller goes away). The reply therefore marks the end of the
  scan, which is what QML's `nmcli --rescan yes` ran for the "busy" state. A scan
  NetworkManager refuses as too soon after the previous one (`...Device.NotAllowed`)
  answers `changed:false`. Access points arrive through the existing `network`
  topic; nothing is cached or polled while no scan runs.
- `audio` topic streams: every application playback stream (`sink-input`) and
  capture stream (`source-output`) is a node of kind `playback`/`capture` with id
  `sink-input:<n>` / `source-output:<n>` (pactl index), `name` = application name
  (else media name, possibly empty), optional `icon` = `application.icon_name`,
  linear volume, mute and readback like devices. At most 64 per kind. The same
  `pactl subscribe` and 25 ms debounce serve them: `sink-input`/`source-output`
  events refresh the topic, still only while `audio` or `osd` has a lease. The peak
  monitor's own capture is excluded. `target` stays unset.
- `audio.set_volume` / `audio.set_mute` gain `target:"stream"` whose `node` is a
  stream ID from the topic (`sink-input:<digits>` or `source-output:<digits>`, any
  other text is `validation`). Only the numeric index reaches
  `pactl set-{sink-input,source-output}-{volume,mute}`; volume is the linear
  contract value converted with a cube root to pactl's raw units (65536 = 100 %).
  A stream that ended is `unavailable`. The same audio lane lock serializes it.
- `audio.peak` topic `{health, level}` (level 0..1, hundredths): the default
  microphone's peak. It exists only while a client subscribes: one
  `parec --device=@DEFAULT_SOURCE@ --raw --format=s16le --rate=8000 --channels=1
  --latency-msec=66 --client-name=nacre-peak` child, one peak per 66 ms window (at
  most 15 publishes a second, only on change), killed when the last lease ends.
  Nothing is probed at startup (N0053): initial snapshot is unavailable; a missing
  or failing `parec` publishes unavailable and retries every 2 s while leased.
  The service is the `audio-peak` supervisor task; no new dependency.

## Checks

Fakes only (private D-Bus NetworkManager, scripts for `pactl`/`parec`): scan
waits for `LastScan`, refuses radio-off/wired/invalid, tolerates NotAllowed, hides
backend detail; stream nodes, ordering, peak-client exclusion, update on stream
events; stream volume/mute argv and refusals; peak levels, lease start/stop (the
child exits with the lease), retry and missing tool. Protocol generator output and
fixtures regenerated (15 topics, 27 calls). The control tests that share the
process-wide audio lane now take turns (a latent race, exposed by the extra calls).

Temporary duplicates: none. QML keeps its own PipeWire/nmcli paths until the core
switch retires it.
