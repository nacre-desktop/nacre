# N0062 — `vpn` topic and `vpn.connect` / `vpn.disconnect`

Status: **Implemented; pending lead review and exact-head CI; unpublished.** Consumer: Bead B0087 (VPN tile and page).
Claude worker. Wire changes are additive; QML is unchanged and keeps working.

## What the QML shell does today (exact)

`nacre/shell/services/NacreVpn.qml` (singleton) and `modules/controlcenter/NacreControlVpn.qml` (detail page):

- **List**: `nmcli --colors no --terse --escape yes --fields UUID,TYPE,NAME,DEVICE connection show` (a process, run when the
  control centre overview opens with the `vpn` tile enabled, when the page opens, and after each change). Rows are kept only
  when the UUID is a canonical UUID and `TYPE` is `vpn` or `wireguard`. `active` is "DEVICE is neither empty nor `--`", so a
  profile that is still activating counts as active. Order is nmcli's. A non-zero exit sets `ready=false`, clears the list and
  shows "NetworkManager VPN controls are unavailable."
- **Connect / disconnect**: `nmcli --wait 30 connection up|down uuid <uuid>` (one at a time; the UI is disabled while busy).
  Only UUIDs of listed profiles are accepted. No device is given (NetworkManager chooses).
- **Status**: tile status text is the first connected profile's name, else "Off"; the tile is `on` when any profile is
  connected. Page rows show "Connected" / "Off". Page hint: "Connect to a saved VPN. Add or configure profiles in
  NetworkManager before using them here." Empty list: "No saved VPN profiles found."; busy: "Updating VPN connections…".
- **Errors**: a failed up/down shows "Could not change the VPN connection. Check its saved credentials and try again."
  `nmcli --wait 30` is what lets QML see an activation error; there is no other detail.
- **Secrets**: QML never prompts for or passes a VPN secret. `nmcli connection up` uses the secrets stored in the profile
  and otherwise asks NetworkManager's registered secret agents (none from Nacre); a profile that needs a password nobody
  supplies fails and QML shows the generic error.
- **Tile**: id `vpn`, icon `vpn_key`, in the default tile list, `navigation` (opens the page), hidden by the user's tile
  preferences like any tile.

## Contract

Topic `vpn` (`VpnState`, class of `network`), published by the existing NetworkManager task next to `network`:

| Field | Meaning |
| --- | --- |
| `health {available, stale, error?}` | Same rules as `network`: starting and NetworkManager-absent are `available:false` with no profiles; a failed read keeps the last profiles, `stale:true`. |
| `connections` | `Entities<VpnConnection>`, ordered by name (case-insensitive), then UUID. IDs are the `network` topic's `connections` IDs for the same profile. |
| `VpnConnection.name` | Profile id, display text, at most 128 characters, control characters escaped. |
| `VpnConnection.kind` | `vpn` (VPN plugin) or `wireguard`. Other profile types are not listed. |
| `VpnConnection.uuid` | NetworkManager UUID, canonical 8-4-4-4-12 hex. Profiles without a valid UUID are skipped (as QML does). |
| `VpnConnection.state` | `disconnected`, `connecting`, `connected`, `disconnecting`, `failed`. From `ActiveConnection.State` (1 activating, 2 activated, 3/4 deactivating/deactivated); `failed` is described below. |
| `VpnConnection.active` | An active connection exists for the profile (`connecting`, `connected`, `disconnecting`); exactly the nmcli "has a device" rule QML uses. `failed` and `disconnected` are not active. |

Methods (class `system`, v1, timeout 10 s like the network calls; params `{connection: EntityId}`; result `{changed}`):

- `vpn.connect`: `ActivateConnection(profile, "/", "/")` on the unique NetworkManager owner. Already active is
  `{changed:false}`. Clears the profile's `failed` mark first.
- `vpn.disconnect`: `DeactivateConnection(active path)`. Not active is `{changed:false}`.
- Errors are the network ones: `stale_revision` for an ID that is unknown, vanished or not a VPN profile (an ethernet or Wi-Fi
  `network` ID is refused), `validation` for malformed parameters, `permission_denied` / `unavailable` from the backend with
  the fixed sanitized texts of N0016 (no D-Bus detail). Both are advertised only while NetworkManager answers and is not in
  an owner change, and not in safe mode, with the network methods' gating (`ActivateConnection` for connect,
  `DeactivateConnection` for disconnect). No NetworkManager means no topic data and no methods.
- Like N0016, success is **backend admission, not completion**. The topic is authoritative; nothing is updated optimistically.

### Secrets (N0016 / N0042 rules)

No VPN secret, key, certificate, user name or `vpn.data` value is read, published, accepted or logged: the daemon reads
`GetSettings` only (which omits secrets), never `GetSecrets`, and no method has a secret field. A profile that needs a
secret uses what NetworkManager has saved or asks NetworkManager's own secret agents, exactly as `nmcli connection up` does for
QML today. If nobody supplies it, NetworkManager ends the activation with a reason: the profile then shows `failed`.
A UI that wants to offer an in-app prompt needs a later contract that registers an agent (out of scope).

### `failed`

`nmcli --wait 30` made QML see activation errors; D-Bus admission cannot. The task watches the already subscribed
`org.freedesktop.NetworkManager.Connection.Active.StateChanged` signal: state `deactivated` (4) with reason IP config
invalid (5), connect timeout (6), service start timeout (7), service start failed (8), no secrets (9) or login failed (10)
marks the profile `failed` (`active:false`). The mark stays until the profile is active again (`connecting` or `connected`),
`vpn.connect` is called for it, or it vanishes. User disconnects (2), device drops (3), service stops (4) and removals (11)
are not failures.
The mark lives in memory only (lost on daemon restart or NetworkManager owner change).

## Design notes

- No second D-Bus reader or process: `vpn` is projected from the same `read_raw` snapshot as `network` (`project_vpn`), so it is
  published with it and follows the same 20 ms coalescing, 10 s snapshot timeout, owner pinning and last-good rules. The one
  added read is `Connection.Active.State` for active VPN profiles only.
- N0053: nothing on the readiness path. The `vpn` topic starts unavailable with `network` in `publish_core_starting`; the
  NetworkManager task already runs after READY with 5 s method timeouts and the 10 s snapshot bound.
- Temporary duplicates: none. `nmcli` stays in QML only until QML is retired (delete `NacreVpn.qml`'s process readers with the
  QML control centre, 7 days after Bead's tile is trusted).

## Tests (fake NetworkManager on a private `dbus-daemon`, `network/controls/vpn_tests.rs`)

- topic content and order, wired profile excluded, IDs equal to the `network` IDs, no secret or user name in the topic, no
  `GetSecrets` call;
- connect/disconnect admission, `changed:false` repeats, `connecting` → `connected` → `disconnecting` → `disconnected`
  read-back, the exact D-Bus calls;
- late activation failure (reason 9) becomes `failed`, a retry clears it, a user disconnect (reason 2) does not set it;
- stale, wrong-kind (wired ID), malformed and denied calls with sanitized errors;
- unavailable without NetworkManager (no methods, starting state), stale keep on a failed read, Welcome validation.
- Protocol: `reserved_transcripts_replay...` now covers 14 topics / 26 calls; generated schemas, Luau types and
  `protocol/fixtures/core-vpn.jsonl` come from `cargo run -p nacre-protocol --example export`.

## Validation

- Full `desktop` job of `.github/workflows/check.yml` replicated in `ghcr.io/archlinux/archlinux:latest` on a remote build host
  (12 CPUs, 24 GiB; exact head `e994f3ce`, rebased on `8b704fb1`): `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
  `cargo build --examples`, `cargo test` (74 test binaries, all green; the five `vpn_*` daemon tests and the extended
  `reserved_transcripts_replay_typed_calls_and_complete_snapshots` included), the minimal protocol consumer, the release helper,
  helper sandbox integrity (29 tests), translation coverage and generation, `tools/check.py` (all suites) and the native English and
  pseudo-language galleries. `cargo run -p nacre-protocol --example export -- --check` is current after the rebase.
- Consumer evidence: Bead B0087 drives the topic and both calls against this daemon build and a fake NetworkManager on a private
  bus in nested sessions beside the QML shell (see Bead `docs/specs/0087-vpn.md`).
- Size: product code +411/-12 lines (`network.rs` +146, `controls.rs` +159, protocol types +85, `session.rs` +11, artifacts +11); tests
  +579 (`vpn_tests.rs` after rustfmt, +2 in protocol); generated schemas, Luau types and `core-vpn.jsonl` are the exporter's output.
