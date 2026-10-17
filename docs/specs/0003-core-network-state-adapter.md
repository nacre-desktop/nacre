# N0003 — Core network state adapter

- ID: N0003
- Repo: nacre-desktop/nacre
- Implementer: NACRE
- Status: Ready
- Depends on: N0001/N0002 Done; F4 published with full CI green
- Date: 2026-10-17
- Size: one focused session, maximum 4 hours

## Goal

Implement the reserved network snapshot before adding device actions. Bead needs
real daemon-owned identities and authoritative readback before controls can safely
target devices.

## Context

Start at the reviewed published N0002 endpoint, descendant of5f4c0bf3; record full
hash. Read architecture/{foundation,protocol,core-services,standing-rules}.md,
development/security-review.md, daemon/network.rs and network_state.rs.
Current runtime has network.watch generation signals and a legacy network-state
helper snapshot; the richer core network topic is still reserved. Do not claim
that the reserved network schema is already a running service.
Contract: nacre-protocol/src/core_services.rs NetworkState/Entities and5f4
compatibility transcripts. Upstream reference:
[NetworkManager API](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.html)
and [AccessPoint API](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.AccessPoint.html).

## Scope

In: daemon-owned read-only NetworkState projection, opaque IDs, event refresh,
private mock tests and exact published consumer handoff. Out: Wi-Fi toggles,
connect/disconnect, active scans, passwords/secret-agent changes, other devices,
Bead edits, real network actions, measurement, live/default opt-in.

## Steps

1. Extend daemon/network.rs with a core projection owned by its existing service;
   use its single asynchronous zbus connection and existing event subscriptions.
   Preserve network.watch and legacy helper output byte contracts. Avoid a new
   persistent helper/second bus connection or blocking snapshot call on the loop.
2. Build core_services::NetworkState: health, wireless_enabled, devices,
   access_points, connections, optional primary_connection. Each Entities uses
   items keyed by opaque EntityId plus a matching unique order. Use a per-service
   generation and monotonic counter (e.g. net:<generation>:<counter>), retain IDs
   while a source object survives, retire on disappearance, never recycle within
   a boot. Keep object paths/SSID bytes/backend addresses server-side.
3. Map device Interface→name and DeviceType to kind:1 ethernet,2 wifi,other unknown;
   State==100→connected. WirelessEnabled is the radio value. AP Ssid→display name
   using existing safe display-text policy; Strength0..100, Flags/WpaFlags/RsnFlags
   determine secured, ActiveAccessPoint determines active. AP.device must resolve.
   Use NetworkManager Settings.ListConnections/GetSettings for profile names and
   connection types, never GetSecrets. ActiveConnections identify active profiles;
   PrimaryConnection is an active-connection object: resolve its Connection to
   the corresponding saved profile ID, omit when absent/unresolvable. Never infer
   primary by iteration order. Only set optional AP.connection when backend
   identity proves the association; do not guess by SSID/name.
4. Initial read and event refresh must validate the complete CoreSnapshot before
   publication. Bound entity collections at2048; if exceeded, publish sanitized
   too_large/unavailable health rather than dangling/truncated identity sets.
   Coalesce signal bursts with existing latest-wins delivery. On backend loss
   retain last-good data as stale; no initial data means unavailable, never an
   invented connected network. Unavailable health makes retained scalar defaults
   non-authoritative. No polling, RequestScan or credential read.
5. Register topic network v1 only after implementation is wired. Keep all three
   network.* mutators unadvertised. Add private tests core_network_ids_survive_update,
   core_network_removed_ids_never_reused, core_network_primary_uses_active_mapping,
   core_network_ap_device_references_are_valid, core_network_never_scans_or_reads_secrets,
   core_network_backend_loss_marks_last_good_stale and
   core_network_collection_limit_never_emits_dangling_refs. Reuse private D-Bus
   fixtures from network_state.rs; never attach tests to the real system bus.
6. Replay original network and5f4 compatibility transcripts unchanged. Run source /
   fmt/Clippy/tests/freshness/security checks and full CI, publish producer main,
   then send exact revision, topic version, fixture hashes and provider limitations
   to spec agent. Controls follow a separate spec; Bead must not infer them.

## Measurements

None. Deterministic private backend tests only. Request separately scheduled
whole-group acceptance if retained state/connection behavior adds material memory;
no unscheduled captures or build/test overlap with a measurement host.

## Acceptance criteria

- [ ] Actual network v1 topic matches published schema/primary compatibility.
- [ ] Stable non-recycled opaque IDs, coherent references/order and bounded maps.
- [ ] Private event/loss/identity/secret/scan tests and original fixtures pass.
- [ ] Single service connection; no blocking loop, extra persistent helper or poll.
- [ ] No mutation capabilities advertised; no live/network/credential action.
- [ ] Full published-head CI green and consumer handoff names exact producer hash.

## Stop conditions

Missing field cannot be guessed. Stop for required schema changes, unavailable
backend association, a second connection/new framework need, >4h scope or
unexplained retained state. Report exact gap for a follow-up; never silently
claim full network controls or core hosting completion.

## General rules

Apply the linked standing-rules.md in full. Author and committer use the approved
GitHub noreply identity; no private paths or inventory. Isolated worktree, direct
normal push to main, inspect outgoing history, check CI after every push; rebase
only own commits. Commands nice -n 10; Cargo -j2; private/nested tests only.
No live desktop/install/config/keybinding/autostart/unit changes, package/tag /
release publication, permission-policy changes, or paru skipreview changes.
Stop only owned PIDs; no host pressure/cache drops/kernel changes. Nacre commits
include [no-publish]. Framework remains permissive, publish=false, backends frozen
except fixes; no launcher engines in Bead. Preserve immutable fixtures. Extras
are report-only. Any new security advisory goes to the spec agent; vulnerabilities
go to the owner. Measurement requires an explicit exclusive slot grant.

## Report format

Send spec ID/status, start and final full commit IDs, changed files, each acceptance
criterion with PASS/FAIL/NOT RUN and command evidence, published-head CI URLs,
protocol/artifact hashes where relevant, deviations/blockers and raw private report
locations (never commit private paths). For captures include per-run numbers,
medians/min/max/peaks, quiet counters, attribution and owned-PID cleanup. Missing
is unknown. Ask the spec thread directly; do not improvise or ask the owner for
routine design. Submit Review, then start the next eligible Ready spec.
