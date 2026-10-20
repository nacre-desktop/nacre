# N0016 — Complete daemon network controls

- Status: In progress (Sol worker, lead review)
- Date: 2026-10-21

**Goal.** Implement the reserved `network.set_wifi`, `network.connect` and `network.disconnect` v1 methods in the daemon NetworkManager service, with published state remaining authoritative. These controls are the highest-value next core service step because Bead’s order places device controls after daemon contracts and before hosted notifications (Nacre `docs/architecture/standing-rules.md:21-29`).

**Context.** The daemon already publishes a core `network` snapshot and starts the NetworkManager service (`crates/nacre-helper/src/daemon/network.rs:90-116`; `crates/nacre-helper/src/daemon.rs:109-117`). The reserved contract defines these methods and their payloads (`docs/architecture/core-services.md:43-53`), while the existing `network.watch` lane explicitly provides neither snapshots nor action APIs (`docs/architecture/protocol.md:442-461`). Reserved schemas alone are not runtime capabilities; advertise only implemented, ready methods (`docs/architecture/core-services.md:3-8`).

**Scope / non-goals.** Add the three reserved calls through the existing service, advertise only supported methods, validate opaque daemon-issued IDs against fresh state, return stale/unavailable/policy failures without optimistic publication, then publish authoritative readback. Keep connection loss non-replay semantics and 2,048-entity bounds (`docs/architecture/core-services.md:26-41`). No new service/process, scan, secrets, other device domains, UI work, or weather/updates provider. Keep the signal-driven reader event based; any rare blocking work must use daemon-managed nice-10 workers with timeouts/concurrency cap and exit after 30 seconds idle (`docs/architecture/foundation.md:28-37`).

**Steps.** 1) Trace current NetworkManager reader, daemon call dispatch and method advertisement; confirm the three handlers are absent. 2) Implement backend calls with stale-target and daemon-boot checks; never replay a mutation after link loss (`docs/architecture/core-services.md:26-33`). 3) Advertise each method only when usable; test success/readback, stale IDs, unavailable service, denied/failing operations, disconnect ambiguity and reconnection. 4) Regenerate protocol artifacts/transcripts only if required by the existing reserved contract; keep schemas stable unless a concrete incompatibility requires a separately reviewed protocol request.

**Acceptance + evidence.** Exact methods/params/results match the reserved v1 contract; successful calls are followed by topic readback, failures never fabricate state, and `welcome.methods` matches actual availability. Provide unit/integration evidence for each failure boundary, generated-artifact consistency if touched, and exact-head CI. Cite `docs/architecture/core-services.md:3-8,26-41,43-53` and `docs/architecture/protocol.md:442-461` in implementation notes.

**Stop conditions.** Stop for missing/ambiguous NetworkManager semantics, a required contract change, unsafe retry/identity mapping, or inability to keep blocking work bounded. Do not broaden to another service.

**Size / heavy slot.** Medium, one service-focused unit. No desktop heavy slot expected; CI/build validation only.
