# N0005 — Network owner replacement and watcher ownership

- ID: N0005
- Repo: nacre-desktop/nacre
- Implementer: NACRE
- Status: Review (source fixes required; unvalidated)
- Depends on: N0003 source212c3efe; integration after green publishedF4
- Date: 2026-10-18
- Size: one focused session, maximum3hours

## Goal

Never reuse old NetworkManager identities after its bus owner is replaced, even
if the new owner exports identical object paths. Ensure a failed/cancelled network
service cannot leave its signal watcher and connection alive across supervision.

## Context

N0003 at212c3efe adds complete typed readback and private tests. Source review:
identity generation changes only when network::run restarts; watch_events merges
NameOwnerChanged into ordinary refresh and keeps running. New owner with reused
paths can therefore inherit old IDs. The spawned watcher also lacks guaranteed
abort/join on early publication error or parent cancellation. Read network.rs,
network_state.rs::watch_events,daemon/supervisor.rs and standing rules.

## Scope

Only daemon/network.rs,network_state.rs private event helper if needed,focused
private tests. Preserve existing network.watch generation/changed byte contracts.
No new schema/method/dependency/secondbus/polling/mutator/Bead edit or live action.

## Steps

1. Extend the private event implementation to distinguish initial/ordinary refresh
   from NetworkManager NameOwnerChanged. Keep existing watch_events callback/API
   semantics as a compatibility wrapper; do not alter its consumers' changed-line
   behavior. Daemon reader uses tagged events on the SAME zbus connection.
2. A daemon owner-change event is sticky, never lost behind a full refresh channel.
   Stop that service generation, mark retained last-good health unavailable/stale,
   and let existing supervisor restart it with next identity generation. IDs for
   replacement objects must be new even for identical paths. Never publish a
   healthy snapshot from an old generation after that owner boundary.
3. Pin expected unique NM bus owner at service start with D-Bus GetNameOwner,
   private backend identity only. Check owner before/after each event-triggered
   read and before publication; mismatch/absence rejects that snapshot and exits
   generation. Do not publish unique bus name/path, guess by ordering, or treat
   owner failure as empty healthy collections. In-flight read must be cancellable
   when watcher ends/owner changes (select on actual owned watcher completion),
   not allowed to commit mixed-owner data. This is bounded event-driven readback,
   not a polling loop. Clone candidate IDs only on successful same-owner commit.
4. Own watcher with abort-on-drop cancellation guard. On normal/error loop exit
   explicitly abort and await watcher before returning to supervisor; on parent
   cancellation guard aborts it. No detached task, orphaned connection, background
   retry or new framework. Preserve existing coalescing and method deadlines.
5. Add core_network_owner_replacement_retires_same_path_ids and
   core_network_old_owner_snapshot_never_publishes_after_replacement using actual
   private D-Bus fixture owners and controlled snapshot barriers. Release owner,
   await stale state, replace with identical paths, assert new stable IDs and valid
   references. Delayed old read must never publish healthy current data. Add
   core_network_service_exit_drops_signal_watcher with a private drop/connection
   lease witness for both error and parent cancellation; providers stay alive,
   no callbacks or extra service connection after task teardown. Use oneshots,
   no wall-clock sleeps. Keep N0003 seven tests and compatibility fixtures passing.
6. Commit source/tests locally as approved identity/[no-publish], no push before
   source review/F4 integration gate. Correctness in an explicitly granted private
   slot ONLY after verified resource launch gate; otherwise source/mock work stays
   eligible. Run affected network/private-watch tests,fmt,Clippy; no broad rerun
   without reason, no measurements. Report exact revision and limitations.

## Measurements

None. Future capture requires separate slot,peerpause,4GiBstart/abort<2GiB and
verified private2GiBscope. Source work does not authorize host build/test access.

## Acceptance criteria

- [ ] Same-path replacement uses new IDs; stale prior generation never becomes fresh.
- [ ] Initial/property events work, compatibility watch output unchanged.
- [ ] Watcher abort/join on exit and parent cancellation; no orphaned connection.
- [ ] Three named private tests plus N0003 regressions/fixtures/fmt/Clippy pass.
- [ ] No live/systembus/mutation/schema/dependency change; commits unpublished.

## Stop conditions

Need another connection,change watch wire shape,relax ownership,or3hours exceeded:
report exact gap. No untrusted live probe or implementation by guessing owner paths.

## General rules

Standing-rules apply: approved siverteh noreply author+committer,[no-publish],
no private paths/inventory, isolated worktree/preserve others. Local commits only
until review; later direct normal main push/checkCI,no forcepush,own rebase only.
nice10/Cargo-j2/private tests. No live desktop/install/config/keybinding/autostart/
units/publishing/AI policy/paru flag changes. Only ownedPIDs,no patternkill or
Codex/app-server changes. Extras report-only; vulnerability/unsoundness to owner.
No implicit measurement or resource-scope waiver.

## Report format

N0005 Review: exact base/final commits,files,named test evidence,resource-gate
verification if run,old/new owner ID proof,watcher teardown proof,blockers and
cleanup. Private rawlogs outside Git; no measurement claim.

## Source review — e5c4a3a2, 2026-10-18

Reviewed sourcee5c4a3a21133068b0ea80a6de326725b57e79761; no build/test claim.
Owner tagging/pinning, staged IDs and owned watcher direction accepted. Required
fixes before correctness assignment:

1. Final prepublication select must prioritize owner/watcher completion (biased),
   then check sticky owner_boundary immediately before synchronous Hub publish,
   with no intervening await. Current unbiased select can choose owner checkOK
   while the owner-change signal is also ready. Preserve no-old-generation commit.
2. Existing tests manually create generation1/2 tables, publish stale state and
   construct a separate watcher. They do not test actual observe restart/commit /
   cleanup. Factor private observe_on_connection with the SAME production loop;
   run creates its system connection then delegates. Private fake-bus supervisor
   factory drives real owner loss/stale/replacement with identical paths. Delayed
   snapshot barrier must guard actual read/publication; teardown exercises real
   observe error and parent cancellation. Private cfg(test) publication/drop
   witness channels at actual production paths are allowed; do not substitute
   helper tests or sleeps for these lifecycle proofs. Keep existing test names.
3. New test calls network_manager_owner(...).await? need CoreError conversion;
   map private test errors to static sanitized crateError instead of widening
   wire/public API. Mark read_snapshot cfg(test) if now unused outside tests.

Bounded source corrections/local commits remain eligible now. Formatting and
compiler fixes within scope need no extra design approval. No Rust checks claimed
or host grant until verified launcher proves PAYLOAD containment before release:
record exact argv,unit,systemctl ControlGroup/MemoryMax,child PID and child cgroup,
and effective memory.max. A preflight process leaf/limit alone is insufficient.
Use reviewed handshake and mocked gate tests, no new tooling framework. Future
correctness cap6GiB/start>=10GiB may be used only in a newly assigned slot;
capture cap stays2GiB. Existing N3 acceptance claims remain withdrawn.
