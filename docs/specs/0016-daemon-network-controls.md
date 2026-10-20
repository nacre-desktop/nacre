# N0016 — Complete daemon network controls

- Status: Blocked (native-agent error semantics; lead decision required)
- Date: 2026-10-21

**Goal.** Implement the reserved `network.set_wifi`, `network.connect` and `network.disconnect` v1 methods in the daemon NetworkManager service, with published state remaining authoritative. These controls are the highest-value next core service step because Bead’s order places device controls after daemon contracts and before hosted notifications (Nacre `docs/architecture/standing-rules.md:21-29`).

**Context.** The daemon already publishes a core `network` snapshot and starts the NetworkManager service (`crates/nacre-helper/src/daemon/network.rs:90-116`; `crates/nacre-helper/src/daemon.rs:109-117`). The reserved contract defines these methods and their payloads (`docs/architecture/core-services.md:43-53`), while the existing `network.watch` lane explicitly provides neither snapshots nor action APIs (`docs/architecture/protocol.md:442-461`). Reserved schemas alone are not runtime capabilities; advertise only implemented, ready methods (`docs/architecture/core-services.md:3-8`).

**Scope / non-goals.** Add the three reserved calls through the existing service, advertise only supported methods, validate opaque daemon-issued IDs against fresh state, return stale/unavailable/policy failures without optimistic publication, then publish authoritative readback. Keep connection loss non-replay semantics and 2,048-entity bounds (`docs/architecture/core-services.md:26-41`). No new service/process, scan, secrets, other device domains, UI work, or weather/updates provider. Keep the signal-driven reader event based; any rare blocking work must use daemon-managed nice-10 workers with timeouts/concurrency cap and exit after 30 seconds idle (`docs/architecture/foundation.md:28-37`).

**Steps.** 1) Trace current NetworkManager reader, daemon call dispatch and method advertisement; confirm the three handlers are absent. 2) Implement backend calls with stale-target and daemon-boot checks; never replay a mutation after link loss (`docs/architecture/core-services.md:26-33`). 3) Advertise each method only when usable; test success/readback, stale IDs, unavailable service, denied/failing operations, disconnect ambiguity and reconnection. 4) Regenerate protocol artifacts/transcripts only if required by the existing reserved contract; keep schemas stable unless a concrete incompatibility requires a separately reviewed protocol request.

**Acceptance + evidence.** Exact methods/params/results match the reserved v1 contract; successful calls are followed by topic readback, failures never fabricate state, and `welcome.methods` matches actual availability. Provide unit/integration evidence for each failure boundary, generated-artifact consistency if touched, and exact-head CI. Cite `docs/architecture/core-services.md:3-8,26-41,43-53` and `docs/architecture/protocol.md:442-461` in implementation notes.

**Stop conditions.** Stop for missing/ambiguous NetworkManager semantics, a required contract change, unsafe retry/identity mapping, or inability to keep blocking work bounded. Do not broaden to another service.

**Size / heavy slot.** Medium, one service-focused unit. No desktop heavy slot expected; CI/build validation only.


## Investigation evidence — 2026-10-21

Source baseline: `92af444e` (no product changes made). Read the reserved contract,
foundation/protocol/standing rules, N0003/N0005, the daemon reader, dispatch and
welcome registry, and the `04ab16f6` reader-start race fix. The three handlers are
absent. `network.watch` remains the compatibility change trigger described in
[protocol](../architecture/protocol.md#network-change-trigger-v1), not an action API.

**Stop condition reached: native-agent errors cannot be distinguished through the
identified public API.** The [permission boundary](../architecture/core-services.md#permission-boundaries)
requires `permission_denied` for agent cancellation/denial, but `unavailable` for a
missing agent. NetworkManager keeps distinct errors internally, then collapses
both cases into the same public Wi-Fi device failure reason:

- Upstream NetworkManager revision `ed279d788f3b1443254f8ba5a8a3217cf1cf9596`,
  [nm-agent-manager.c](https://github.com/NetworkManager/NetworkManager/blob/ed279d788f3b1443254f8ba5a8a3217cf1cf9596/src/core/settings/nm-agent-manager.c#L828),
  emits internal `NM_AGENT_MANAGER_ERROR_NO_SECRETS` when no agent remains;
  [the cancellation branch](https://github.com/NetworkManager/NetworkManager/blob/ed279d788f3b1443254f8ba5a8a3217cf1cf9596/src/core/settings/nm-agent-manager.c#L927)
  instead emits `NM_AGENT_MANAGER_ERROR_USER_CANCELED`.
- In the same revision,
  [wifi_secrets_cb](https://github.com/NetworkManager/NetworkManager/blob/ed279d788f3b1443254f8ba5a8a3217cf1cf9596/src/core/devices/wifi/nm-device-wifi.c#L2128)
  handles both errors by publishing `NM_DEVICE_STATE_FAILED` with
  `NM_DEVICE_STATE_REASON_NO_SECRETS` (line 2183). The distinction remains in a
  private log message, not that state transition.
- The public [AgentManager API](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.AgentManager.html)
  exposes registration/unregistration, with no agent inventory or availability
  query. The public [active-connection API](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Connection.Active.html)
  represents an activation attempt; receiving its object path does not establish
  that subsequent native-agent authentication succeeded.

Consequently, returning success on activation-method acknowledgement would not
report a later prompt cancellation as the contract requires. Waiting for failure
still does not recover the missing-agent/cancelled-prompt distinction. A fixture
that sends distinct activation-method errors for those cases would not prove
real NetworkManager behavior. No log scraping, credential interception, agent
replacement or guessed classification was introduced.

**Lead decision needed.** Identify a supported, non-secret public source that
preserves this distinction, or review a behavior clarification for ambiguous
`NO_SECRETS` failures. A candidate clarification is a sanitized `unavailable`
result for that ambiguous reason, retaining `permission_denied` for explicit
policy/authorization errors. This is a proposal, not an implemented contract
change. Also specify whether success acknowledges activation admission or waits
for activation completion when authentication continues after the method reply.

**Implementation outline after resolution.** Keep controls in the existing
single-connection network service and serialize admission with refreshes. Resolve
only published opaque IDs after a fresh same-owner read; reject wrong-kind and
cross-device APs, and use the device's authoritative
[AvailableConnections](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.html)
for saved-profile eligibility. Scope IDs to daemon boot as well as owner generation
(the present `net:generation:counter` form can repeat across daemon boots). Send
mutations to the pinned unique owner, honor request deadlines, never queue/replay
across link loss, and publish only validated same-owner readback. Retain the
sticky owner-boundary and watcher cleanup protections from N0005/`04ab16f6`.
Advertise each method only while its handler/backend is usable, per
[core contracts](../architecture/core-services.md#framing-and-identity). All D-Bus
work can remain asynchronous; no blocking worker is currently indicated.

**Validation disposition.** Read-only inspection and pinned upstream source
retrieval succeeded. No implementation or private fixtures were added, so fmt,
Clippy, full helper tests, protocol consumer, helper parity, full desktop-container
CI and repeated race tests are **NOT RUN**. Stopped at the explicit semantics gate
before implementation/validation; this is not acceptance evidence. No local build
or test, real bus/NetworkManager call, live desktop change, push or publication
occurred. Read-only remote Docker/tmux discovery created no task container,
checkout, session or process requiring cleanup.
