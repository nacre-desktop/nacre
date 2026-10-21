# N0016 — Complete daemon network controls

- Status: Review (implementation complete; lead reviews and integrates)
- Date: 2026-10-21

**Goal.** Implement the reserved `network.set_wifi`, `network.connect` and `network.disconnect` v1 methods in the daemon NetworkManager service, with published state remaining authoritative. These controls are the highest-value next core service step because Bead’s order places device controls after daemon contracts and before hosted notifications (Nacre `docs/architecture/standing-rules.md:21-29`).

**Context.** The daemon already publishes a core `network` snapshot and starts the NetworkManager service (`crates/nacre-helper/src/daemon/network.rs:90-116`; `crates/nacre-helper/src/daemon.rs:109-117`). The reserved contract defines these methods and their payloads (`docs/architecture/core-services.md:43-53`), while the existing `network.watch` lane explicitly provides neither snapshots nor action APIs (`docs/architecture/protocol.md:442-461`). Reserved schemas alone are not runtime capabilities; advertise only implemented, ready methods (`docs/architecture/core-services.md:3-8`).

**Scope / non-goals.** Add the three reserved calls through the existing service, advertise only supported methods, validate opaque daemon-issued IDs against fresh state, return stale/unavailable/policy failures without optimistic publication, then publish authoritative readback. Keep connection loss non-replay semantics and 2,048-entity bounds (`docs/architecture/core-services.md:26-41`). No new service/process, scan, secrets, other device domains, UI work, or weather/updates provider. Keep the signal-driven reader event based; any rare blocking work must use daemon-managed nice-10 workers with timeouts/concurrency cap and exit after 30 seconds idle (`docs/architecture/foundation.md:28-37`).

**Steps.** 1) Trace current NetworkManager reader, daemon call dispatch and method advertisement; confirm the three handlers are absent. 2) Implement backend calls with stale-target and daemon-boot checks; never replay a mutation after link loss (`docs/architecture/core-services.md:26-33`). 3) Advertise each method only when usable; test success/readback, stale IDs, unavailable service, denied/failing operations, disconnect ambiguity and reconnection. 4) Regenerate protocol artifacts/transcripts only if required by the existing reserved contract; keep schemas stable unless a concrete incompatibility requires a separately reviewed protocol request.

**Acceptance + evidence.** Exact methods/params/results match the reserved v1 contract; successful calls are followed by topic readback, failures never fabricate state, and `welcome.methods` matches actual availability. Provide unit/integration evidence for each failure boundary, generated-artifact consistency if touched, and exact-head CI. Cite `docs/architecture/core-services.md:3-8,26-41,43-53` and `docs/architecture/protocol.md:442-461` in implementation notes.

**Stop conditions.** Stop for missing/ambiguous NetworkManager semantics, a required contract change, unsafe retry/identity mapping, or inability to keep blocking work bounded. Do not broaden to another service.

**Size / heavy slot.** Medium, one service-focused unit. No desktop heavy slot expected; CI/build validation only.

## Lead clarification — 2026-10-21

The initial stop was accepted: NetworkManager collapses missing-agent and
cancelled-prompt failures to the same public Wi-Fi `NO_SECRETS` reason
([upstream source](https://github.com/NetworkManager/NetworkManager/blob/ed279d788f3b1443254f8ba5a8a3217cf1cf9596/src/core/devices/wifi/nm-device-wifi.c#L2128)).
The lead resolved the behavior without a schema change: success is backend
admission, not activation completion. Only admission failures are method errors.
Later secret/agent failures use one sanitized retryable topic error. The short
network note in [core-services](../architecture/core-services.md#topics-and-calls)
records this network-specific qualification of the general permission boundary.

## Implementation notes

The existing network task serializes controls and refreshes on its one connection.
Its queue holds at most eight pending actions plus one in flight; actions have a
maximum ten-second deadline, snapshots ten seconds, and introspection two seconds.
All backend work is asynchronous; no worker process, scan or secret read was added.
Radio writes use `Properties.Set`, saved profiles use `ActivateConnection`, and
AP activation uses `AddAndActivateConnection2` with an empty template and
`persist:volatile`. Disconnect deactivates only an unambiguous single-device active
connection. Unsupported backend methods/properties are omitted from welcome;
safe mode, read failure, owner loss and task cancellation disable controls.

Only published IDs surviving fresh readback may resolve. AP/device association,
profile kind and `AvailableConnections` are checked. IDs include a daemon-boot
namespace and owner generation. A disappearance observed during preflight retires
its ID even if the same path returns before publication. Mutations use the pinned
unique owner, never a replaceable destination or replay after lost calls. Existing
N0005 and `04ab16f6` owner pinning, sticky retirement and watcher ownership remain.

Replies acknowledge admission; topic snapshots still come only from backend reads.
Authoritative `FAILED/NO_SECRETS` signals survive coalesced property reads, scoped
to published device IDs. Per-observation markers prevent an admission reply from
erasing a newer failure. New admission clears only an older matching observation;
reconnection, radio-off readback and device retirement clear retained errors.
This implements `core-services.md:3-8,26-41,43-53`; the legacy
`protocol.md:442-461` trigger contract and all protocol artifacts stay unchanged.


## Validation evidence — 2026-10-21

Base: `92af444eb7b0244aeedba8be63fe5af9bc93621e`. Final product source:
`0fd8cc5ad3c79ba35b7e5c9e983d542f6c96a824`. Approximately 725 added Rust
lines outside the new fixture file; no dependency or protocol artifact changes.
No local builds/tests, real system bus/NetworkManager, live desktop, installation,
push or publication were used. Source review includes the N0003/N0005 lifecycle
requirements and `04ab16f6`; the lead still owns integration and final acceptance.

All validation runs use the task-private `ghcr.io/archlinux/archlinux:latest`
container, nice 10, Cargo jobs limited to two, and private D-Bus owner fixtures.
At the final product source, successful command output establishes:

```sh
cargo fmt --check
cargo clippy -j2 --all-targets -- -D warnings
cargo test -j2 --lib -p nacre-helper core_network_ -- --nocapture
# 15 passed, including owner replacement, stale publication and watcher teardown.
for run in $(seq 1 20); do
    cargo test -j2 --lib -p nacre-helper network_controls_ -- --nocapture
done
# 20 runs x 9 tests = 180 passed, zero failures.
```

The nine control fixtures cover success/admission and authoritative readback,
stale/wrong-kind/cross-device IDs, denied/failing operations, shared-connection
ambiguity, unavailable/read-only/safe-mode capabilities, boot scoping, actual
socket loss with queued cancellation, owner replacement and explicit reconnection,
timeout/queue bounds, preflight retirement, and authentication failures before and
after the admission reply, including transitions gone before the property read.

The initial candidate `f20219a3` completed every desktop workflow shell check:
workspace fmt/Clippy/examples/tests, minimal protocol consumer, locked release
Orient/helper builds, 28 sandbox-integrity cases, translation checks, source/UI
regressions, 534 helper parity cases (zero failures), and native English/pseudo,
AUR pager and wide-font galleries. Tools ran 435 tests with the workflow's one
opt-in Wayland-soak skip; AI 116, Brain 45 and shell tools 472 all passed. Gallery
audits reported 178 states per language. A bind-mounted checkout ownership error
initially stopped the source checker; making only the disposable checkout
root-owned fixed its private-HOME Git check. No host trust/configuration changed.

The worker's final report records the whole desktop-job rerun on the evidence
commit, including exact command/result and the retained raw-log location. The
replication executes `.github/workflows/check.yml` desktop shell steps with
`CI=true`, `CARGO_BUILD_JOBS=2`, private HOME/buses and the workflow sandbox options.
Logs/gallery artifacts remain outside Git. Hosted CI and live-provider behavior
are not claimed; the task requests unpublished private-fixture validation.
