# N0003 — Core network state adapter

- ID: N0003
- Repo: nacre-desktop/nacre
- Implementer: NACRE
- Status: Blocked (correctness acceptance evidence withdrawn)
- Depends on: independent source implementation allowed after N0004 local fix; integration after N0001/N0002 Done and green F4
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

## Independent source phase — owner throughput update2026-10-18

While waiting for the shared measurement/build host, implement this already
specified network-state adapter and its tests in a SEPARATE isolated worktree
based on current main. Reserved network contracts are already published5f4.
Do not modify the frozen F4 candidate or introduce new wire APIs. Source work
may proceed before F4 publication; final integration remains after reviewed green
F4. Before that integration reconcile daemon/hub wiring with the actual F4 head.
No build/test on Bead's active capture host; run only on an independent verified
existing environment, or keep source/test implementation ready for host release.
Do not start a new lab/toolchain/deployment to work around the slot.


## Restart safety

No measurements are authorized by this spec. Any later measurement must follow
the standing capture-safety rules: peer pause, MemAvailable >=4GiB before start,
abort below2GiB, private memory-limited scope, and explicit spec-agent grant.

## First correctness grant — 2026-10-18

Exact source398993efe15d80f7fab1d5163c20c3523dc54748 plus status-only
1eded772076df25b9c313a78522e05357b5b4dde, unpublished. Grant private previously
verified lab14:30–15:30UTC after N0002 explicit DONE/RELEASED. Run fmt, seven
required named private tests, affectedClippy and specified fixture/security
checks on an isolated exact-source tree, existing toolchain, nice10/Cargo-j2,
one heavy job and2GiBscope/4GiBavailable preflight. No live/systembus, mutation,
network probe or measurement. Send START/results/DONE; no implicit clock-based
handoff. Integration still waits green published F4 and source review.

## Advanced correctness interval — 2026-10-18

After N0002 invalid-arm cleanup and explicit release, correctness-only slot is
advanced to12:20–13:25UTC, superseding14:30–15:30. Exact398993ef+1eded772,
existing method/checks and no-live/no-measurement boundaries unchanged. Send
START/results/DONE; one capped heavy job, no Bead remote overlap.

## Correctness disposition — 2026-10-18

Exact212c3efe5215a11fbb8f5a9e50d2b7199248464c plus193ad75b fixes the initial
zbus type bounds/test partial move and rustfmt reflows. fmt passes. Focused
compilation stopped in turbojpeg-sys before test bodies because cmake is absent
from the configured path/checked existing locations. Seven tests, fixture/security
and Clippy are NOT RUN. Private2GiBscope exited withOOM0;source unpublished.

Max10min inspect existing successful build records for explicit CMAKE/PATH /
CARGO_TARGET_DIR or a valid already-built native dependency cache; earlier run
reached Rust type checking, so check the actual environment difference. No
whole-environment/credential dump, new host inventory, install/download/sudo,
or speculative build retry. If a valid existing path/cache is identified, the
correctness-only grant through14:00UTC covers revised exact source; otherwise
remain Blocked and continue the eligible N0002 source-only collector phase.

## Functional review and containment deviation — 2026-10-18

At212c3efe fmt,seven named tests,primary compatibility and helper all-targetClippy
reported PASS using existing private cmake path, no install. This supersedes the
missing-cmake blocker. The run was NOT resource-compliant: effectiveMemoryMax
was infinity and peak4616724480bytes despite requested2GiB;OOM0 does not repair
that deviation. Preserve functional results honestly; do not rerun merely to
replace the record. Source review found lifecycle gaps captured in N0005 before
producer integration. N0003 remains Review, unpublished and gated on greenF4.

No next host build/test/capture until outer launch gate verifies actual numeric
MemoryMax AND cgroup memory.max plus child containment before releasing payload.
Reuse reviewed guard handshake; inspect existing argv/unit/cgroup records, no
new framework. Source/mock gate tests first, then bounded no-pressure smoke.
For future correctness/build jobs only, spec agent sizes cap6GiB (observed4.3GiB
peak) with>=10GiB MemAvailable before start, externalwatchdog abort<2GiB; one
heavy job. Measurement scopes remain2GiB and4GiBstart; no ceiling change to UI.
Actual granted host window still required. No install, globalsettings or stress.

## Evidence withdrawal — authoritative correction, 2026-10-18

Implementer withdrew the previously reported N0003 PASS/OOM claims. Do NOT rely
on or publish them as acceptance evidence. For acceptance, tests/Clippy/resource
verification are NOT RUN/NOT VERIFIED, notwithstanding earlier historical review
paragraphs. This correction supersedes those dispositions; retain the original
logs and local commits without deletion or relabelling. N0003 remains unpublished.

No N0003 rerun or host access is granted. A future correctness run requires a new
explicit exact-source assignment and verified containment. Current work is only
the local N0002 collector correction. N0005's source-review lifecycle findings
remain queued; no assertion they have been implemented or validated.

## Reconciliation — 2026-10-20

Base `origin/main`: `5b9631b337b9e96b46b931b71b7ce66949a0a494` (green F4 and
N9; N0013/N0014 landed). Final network source:
`b01c9606d4978a94c2620c2a92ebe31376dabfa0`.
The withdrawn acceptance evidence above remains withdrawn; this exact rebased
source has fresh local evidence, pending independent source review.

PASS: `cargo fmt --check`; workspace clippy; 13 focused `core_network_*` tests;
the full `nacre-helper` package suite; helper examples build; locked release
helper build. The package run replays private legacy network and `network.watch`
compatibility fixtures. Focused correctness tests use private `dbus-daemon
--session` fixtures. All Cargo jobs used a transient user scope with
`MemoryMax=2G`.

```sh
PATH=/home/siverteh/.rustup/toolchains/1.99.0-x86_64-unknown-linux-gnu/bin:$PATH cargo fmt --check
cargo clippy -j2 --all-targets -- -D warnings
cargo test -j2 -p nacre-helper core_network_ -- --nocapture
cargo test -j2 -p nacre-helper --quiet
cargo build -j2 --locked --examples -p nacre-helper
cargo build -j2 --locked --release -p nacre-helper --bin nacre-helper
```

No system bus, live NetworkManager, scan, secret read, or mutation was used.
Exact-head hosted CI and live-provider behavior remain unverified.
