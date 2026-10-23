# N0045 — Bluetooth pairing agent

Status: **Implemented; pending lead review and exact-head CI; unpublished**. Consumer: Bead B-C3 (pairing UI).
All wire changes are additive; QML is unchanged and keeps its own `bluetoothctl --agent` path.

## Why

The QML pairing flow (`NacreBluetoothPairing.qml`) drives a `bluetoothctl --agent KeyboardDisplay` process and scrapes its
output for PIN, passkey and confirmation prompts. The daemon had the reserved `bluetooth.pair` schema but nothing served it
(no `bluetooth` topic producer and no agent), so a native UI could not pair a device.

## Wire changes (class `system`, version 1)

- Topic `bluetooth.pairing`: `{health, prompt?}`. `prompt` is present only while a pairing started by `bluetooth.pair`
  waits for the user: `{device, kind, name, code?, timeout_ms}`. `kind` is `pin` (ask for the device's PIN, up to 16
  characters), `passkey` (ask for the six-digit passkey the device shows), `confirm` (does `code` match the device),
  `authorize` (may the device/service connect) or `display` (show `code`, the user types it on the device; no response).
  `code` exists for `confirm` and `display` only. `timeout_ms` is how long the prompt waits (60 s) before it is withdrawn and
  the pairing fails with `timeout`. The topic is published at start-up with `available:false` and is `available:true` while
  `org.bluez` has an owner on the system bus.
- `bluetooth.pair {device}` (already reserved) is now served: it resolves `device`, registers the agent, calls
  `Device1.Pair()` and returns `{changed:true}` when pairing succeeded, `{changed:false}` when the device is already
  paired. It waits for the whole pairing (method timeout 155 s), so a UI sends it without blocking and watches the topic.
- `bluetooth.pair.respond {device, accept, pin?, passkey?}`: answers the current prompt. `accept:false` rejects and carries
  neither secret. `pin` (1..16 printable ASCII characters) only for `pin`, `passkey` (0..999999) only for `passkey`,
  neither for `confirm`/`authorize`; a mismatch is `validation` and leaves the prompt open. No waiting prompt (or another
  device) is `stale_revision`. Returns `{changed:true}`.
- `bluetooth.pair.cancel {device}`: calls `Device1.CancelPairing` and fails the waiting `bluetooth.pair`. `{changed:false}`
  when nothing is in progress for that device.
- Errors (all fixed strings, backend text is dropped): rejected `permission_denied` "Pairing was rejected"; cancelled
  `permission_denied` "Authentication was cancelled" (the existing contract); wrong PIN/passkey `permission_denied`
  "Pairing authentication failed" (`retry:true`); prompt or BlueZ timeout `timeout` "Pairing timed out" (`retry:true`);
  another pairing running `busy`; unknown device or an ID BlueZ does not publish `stale_revision`; no BlueZ `unavailable`.
- Methods are advertised only while `org.bluez` is on the bus and not in safe mode. `bluetooth.connect`, `.disconnect`,
  `.set_power` and the `bluetooth` state topic stay reserved and unserved (Bead keeps `bluetooth.set_enabled`).
- Device IDs are BlueZ's own object names (`dev_00_11_22_33_44_55`). The ID is only compared with what
  `GetManagedObjects` returns under `/org/bluez/`; it is never used to build a path. A future `bluetooth` state producer
  must use the same IDs.

Generated with `cargo run -p nacre-protocol --example export` (schemas, Luau types, goldens `core-bluetooth-pairing.jsonl`,
`core-errors.jsonl`); nothing hand-edited.

## Agent behaviour and the choice of registration

`Agent1` (`Release`, `RequestPinCode`, `DisplayPinCode`, `RequestPasskey`, `DisplayPasskey`, `RequestConfirmation`,
`RequestAuthorization`, `AuthorizeService`, `Cancel`) is exported at `/org/nacre/bluetooth/agent` on the daemon's own system-bus
connection. Choice: it is registered (`RegisterAgent`, capability `KeyboardDisplay` as the QML used) only while one
`bluetooth.pair` runs and unregistered afterwards, also when the caller disappears or the pairing fails. It is **not** made the
default agent: BlueZ routes a pairing started by this client to this client's agent, and not claiming the default means a
nearby device's own pairing request, or another desktop's agent, is never intercepted or replaced. Consequence: incoming
(remote-initiated) pairing and service authorization for already paired devices are not handled by this unit.
BlueZ owns the link keys; the daemon never trusts a device automatically (`Trusted` is not set).

## Security note

- Who may call: the N0031 model, the socket accepts only the daemon's UID (`SO_PEERCRED`), class `system`. A same-user
  process can already pair through `bluetoothctl`; this adds no authority and is not an isolation boundary.
- The agent object accepts calls only from the unique name that owns `org.bluez` (resolved when the pairing starts) and only
  for the device being paired, and only while a pairing runs; everything else gets `org.bluez.Error.Rejected` and never
  produces a prompt. Evidence: `only_bluez_may_call_the_agent`.
- Auto-accept is impossible: every confirm/authorize/PIN/passkey needs an explicit `bluetooth.pair.respond`; an unanswered
  prompt is cancelled after 60 s. A displayed or confirmation `code` is public by nature (it is on both screens) and is
  published in the topic; it is not logged.
- PINs and passkeys travel only in `bluetooth.pair.respond`, are handed to BlueZ once and are never written by Nacre (no
  file, settings, state, cache, golden), never published in a topic, never echoed in a reply, never put in an error and never
  logged. `BluetoothPairRespond` has a manual `Debug` that prints `<redacted>`; parse failures drop the payload. Not
  zeroized in memory (decoded strings and D-Bus buffers are ordinary heap); accepted as for N0042.
- Evidence: `pins_and_passkeys_never_reach_logs_errors_state_or_debug` runs success, wrong PIN, malformed and wrong-device
  answers for a PIN and a passkey under a process-wide TRACE log capture and asserts neither secret appears in logs, `Debug`,
  any error, the topic or the welcome. Protocol tests cover bounds, exclusivity and redaction.

## Tests

Private bus with a fake BlueZ (`ObjectManager`, `AgentManager1`, `Device1` that calls the daemon's agent) in
`daemon/bluetooth/tests.rs`: confirmation, authorization, PIN, passkey and display flows with the published prompt;
registration as non-default `KeyboardDisplay` and removal afterwards; wrong PIN, rejection, cancellation (`CancelPairing`),
prompt timeout; response/device mismatch, busy, unknown/paired devices and malformed IDs; foreign callers; a vanished caller;
advertisement only while BlueZ is on the bus; the secret scan. No real Bluetooth, system bus or session bus was touched.

## Temporary duplicates

None added. The QML `bluetoothctl` agent stays until the owner retires QML (Bead B-C3 consumes this first). The shared
`daemon/test_logs.rs` replaces the private copy in the N0042 tests (no duplicate left).
