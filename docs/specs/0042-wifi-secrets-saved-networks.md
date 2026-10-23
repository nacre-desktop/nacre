# N0042 — Wi-Fi secrets, hidden SSIDs, saved networks and forget

Status: **Implemented; pending lead review and exact-head CI; unpublished**. Consumer: Bead B-C2
(Wi-Fi password, hidden SSID, saved/forget UI). Producer first: Bead pins this only after it
is published. All wire changes are additive; QML is unchanged and keeps its own nmcli path.

## Why

The QML control centre joins secured and hidden networks and lists/forgets saved ones through
`nmcli`. The daemon's `network.connect` had no secret field, so a native UI could not do the same.

## Wire changes (class `system`, version 1, unchanged advertisement rules)

- `network.connect {device, target, secret?, security?}`. `target.kind` gains `hidden_ssid`
  (`id` is the SSID text, 1..32 bytes, not an entity ID). `secret` (1..128 bytes, no NUL) is valid
  for `access_point` and `hidden_ssid` only; a saved `connection` target uses the secret NetworkManager
  already holds. `security` (`open`|`wpa_psk`|`sae`) is only for `hidden_ssid` and defaults to `wpa_psk`
  with a secret, otherwise `open`; an access point's security comes from its own flags (PSK, else SAE;
  WEP and 802.1X are refused). WPA-PSK secrets must be 8..63 characters or 64 hex digits.
  A hidden join sets `hidden=true`; the daemon never scans. "hidden?" from the request sketch is
  expressed by the target kind.
- `network.forget {connection}` -> `{changed:true}`. Deletes one saved Wi-Fi profile (other kinds are
  `stale_revision`). Advertised with `network.connect`.
- Saved list: `network.connections.items[id]` gains optional `ssid`, `security`
  (`open|wep|wpa_psk|sae|eap|other`), `autoconnect`, `last_used` (epoch seconds, omitted if never),
  for Wi-Fi profiles only. Read from NetworkManager `GetSettings`, which never returns secrets.
- Errors reuse existing codes (no new code): needs-secret is a pre-flight `validation` with the stable
  message `Network secret required`, `retry:true`, `dispatched:false`, raised when a secured access point
  has no secret and no saved profile with that SSID, or a secure hidden choice has no secret. Wrong
  password stays the asynchronous retryable topic error `health.error` "Network authentication required
  or cancelled" (N0016 admission rule); the UI re-prompts on it. Malformed secret/SSID/security is
  `validation`; stale IDs `stale_revision`; policy `permission_denied`.

Generated with `cargo run -p nacre-protocol --example export` (schemas, Luau types, goldens
`core-network.jsonl` and `core-errors.jsonl`); nothing hand-edited.

## Daemon behaviour

Backend is the existing NetworkManager D-Bus task (no new service, no worker, no process spawn). A secret
is added as `802-11-wireless-security {key-mgmt, psk}` in `AddAndActivateConnection2` with
`persist=to-disk`, so NetworkManager owns storage (its root-owned keyfile) and autoconnect; a join without
a secret keeps the previous `volatile` behaviour. Because NetworkManager stores a profile before the
password is verified, a profile created here is tracked per device and deleted again through the profile's
`Delete` method if the same device then fails authentication (state FAILED, reason NO_SECRETS), so a wrong
password is never left saved. Tracking is dropped once the profile is active or the device disappears.
`GetSecrets` is never called.

## Security review note

- Who may call: exactly the N0031 shell-action model. The socket accepts only the daemon's own UID
  (`SO_PEERCRED`); there is no per-method ACL. The method is class `system` (not a polkit bypass), is
  advertised only while the NetworkManager owner is healthy and not in safe mode, and fails closed on owner
  change. Any same-user process can already run `nmcli` with a password, so this adds no new authority;
  it is not an isolation boundary against a compromised session process.
- The secret is held in the request only for the call. It is never written by Nacre (no file, settings,
  state, cache, golden), never published in a topic, never echoed in a reply, and never put in an error
  (all failure messages are fixed strings; backend error text is dropped). `NetworkConnect` has a manual
  `Debug` that prints `<redacted>`. The daemon logs no call parameters; zbus trace logging prints only
  message signatures. Not zeroized in memory (decoded strings and the D-Bus buffers are ordinary heap);
  accepted, as the daemon is same-user and nmcli would hold it the same way.
- Evidence: `network_secrets_never_reach_logs_errors_state_or_debug` runs connect/failure paths (denied,
  broken backend, stale, validation, wrong password) under a process-wide TRACE log capture and asserts the
  secret is absent from logs, `Debug`, every error, the topic and the welcome. The fixtures also assert the
  secret reaches only the NetworkManager `AddAndActivateConnection2` call.

## Tests

Private session bus with a fake NetworkManager and temp HOME (`controls/tests.rs`): join with a secret
(settings, `to-disk`, profile kept once active), saved-list fields, forget plus stale/wrong-kind/malformed
targets, needs-secret, hidden SSID (PSK, SAE, open, invalid combinations, non-Wi-Fi device), wrong password
(profile deleted, error published, retry succeeds and keeps the profile), and the log scan. Protocol tests
cover validation bounds and redacted `Debug`. No real NetworkManager or network was touched.

## Temporary duplicates

None added. The QML nmcli paths remain until the owner retires QML (Bead B-C2 consumes this first).
