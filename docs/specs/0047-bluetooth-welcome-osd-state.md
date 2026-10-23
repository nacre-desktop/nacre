# N0047 — Bluetooth Welcome fix, OSD resilience, Bluetooth state and controls

Status: **Implemented; pending lead review and exact-head CI; unpublished.** Consumers: Bead B0051 (OSD), B0055 (Bluetooth UI).

## Slice 1 — Welcome refused whenever BlueZ was on the bus (urgent)

N0045 advertised `bluetooth.pair` with a 155 000 ms timeout, but the protocol accepts method timeouts of `1..=120000`
(`Message::validate`). With BlueZ on the system bus the daemon's Welcome therefore failed validation for every client.
CI has no BlueZ, so nothing caught it.

- Fix: `PAIR_TIMEOUT` is 110 s; the advertised timeout is `PAIR_TIMEOUT + 5 s = 115 000 ms`. The user-visible prompt
  timeout (60 s) is unchanged.
- Guard tests: `hub::tests::every_static_method_set_passes_welcome_validation` validates a Welcome built from every
  module that advertises methods without a backend; the Bluetooth fixture validates the full `Hub::welcome()` once the fake
  BlueZ is present, and the network fixture's `advertised()` helper validates every Welcome it receives (all network
  backends faked). Any advertised timeout outside `1..=120000` fails these tests.

## Slice 2 — OSD resilience (Bead B0051)

`osd::session` returned an error when `pactl subscribe` ended (pactl present, no audio server answering); `serve` propagated
it, the supervisor restarted the service, and after five failures the whole `osd` service was disabled, so night light, caps
lock, brightness and power-profile events were lost too.

- Now a lost or failed audio subscription never ends the session. The audio stream is respawned in place with backoff
  (2 s, doubling to 64 s; reset by the first line received), and each respawn takes a silent fresh sink/source baseline.
  Caps lock, night light, power profile and keyboard backlight producers are untouched by it.
- Test `a_failing_pactl_degrades_only_audio_and_is_retried`: a fake `pactl` that exits at once; seven restarts happen, the
  service keeps running and a caps-lock event is still reported. `Env.retry` is the first backoff (30 ms in tests).

## Slice 3 — Bluetooth state, power, discovery, connect (Bead B0055)

All wire changes are additive; QML is unchanged. Methods are class `system`, version 1, advertised only while `org.bluez`
is on the bus and not in safe mode. Generated artifacts (schemas, Luau types, `core-bluetooth.jsonl`) come from
`cargo run -p nacre-protocol --example export`.

- Topic `bluetooth` (the reserved `BluetoothState`): `health` (unavailable and empty without BlueZ), `adapters` (id = BlueZ
  object name such as `hci0`; `name` = Alias, `powered`, `discovering`) and `devices` (id = BlueZ object name such as
  `dev_00_11_22_33_44_55`, the same IDs `bluetooth.pair` uses; `adapter`, `name` = Alias, `icon` = BlueZ icon name such as
  `audio-headset`, `paired`, `connected`, `trusted`, `pairing` (a pairing of this device is running), `battery` from
  `Battery1`, new `rssi` in dBm while BlueZ reports it). Devices are ordered like the QML page: connected first, then
  name, then ID. Names lose control characters and are cut at 64 characters. Read from `GetManagedObjects` and refreshed
  (100 ms coalesced) on any signal from `org.bluez`, after each call and when `org.bluez` appears or vanishes; a failed
  read keeps the last good state marked stale.
- `bluetooth.set_power {adapter, powered}` (the reserved name): sets `Adapter1.Powered`; `{changed:false}` when already so.
  **Naming:** `bluetooth.set_power` is canonical (adapter-addressed). The pill method `bluetooth.set_enabled {enabled}`
  (default adapter, `bluetoothctl`) keeps working unchanged and is a temporary duplicate: delete it 14 days after Bead's
  status pill uses `bluetooth.set_power` and is trusted.
- `bluetooth.set_discovery {adapter, discovering}` (new): `StartDiscovery`/`StopDiscovery`. A started discovery stops by
  itself after 60 s (restarting it resets the timer; powering the adapter off cancels it) so a UI that disappears never leaves
  the radio scanning. Starting an active discovery is `changed:true` (timer reset); stopping an idle one `changed:false`.
- `bluetooth.connect` / `bluetooth.disconnect {device}`: `Device1.Connect` (30 s limit, advertised 35 s) / `Disconnect`;
  already in that state is `{changed:false}`. Failures use fixed strings: `unavailable` "Bluetooth connection failed",
  `busy` "Bluetooth device is busy", `timeout` "Bluetooth connection timed out". Unknown or vanished IDs are
  `stale_revision`; IDs are only compared with what BlueZ publishes, never used as paths.
- Tests (fake BlueZ on a private `dbus-daemon`; no real Bluetooth): topic content and ordering, device added later through
  BlueZ's own signals, power, discovery start/stop/auto-stop (300 ms timer in the test), connect, disconnect, failed connect,
  stale and invalid IDs, plus the Welcome guard from slice 1.
- Known gap: the QML page also shows the device address; the contract has no address field (not requested), so Bead shows
  name and icon only until one is added.
