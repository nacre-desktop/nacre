# N0001 — F4 settings with one writer: completion audit

- ID: N0001
- Repo: nacre-desktop/nacre
- Implementer: NACRE
- Status: In progress
- Depends on: published5f4c0bf3; preserve current unpublished F4 and active captures
- Date: 2026-10-17
- Size: one focused session, maximum 4 hours

## Goal

Complete the already implemented F4 single-writer work without duplicating its
contract or discarding validated changes. Establish exact ownership, parity and
failure semantics so the current candidate is reviewable before publication.

## Context

Published starting commit5f4c0bf3. Existing unpublished F4 has settings.rs,
settings_controller.rs, settings_validation.rs in crates/nacre-helper/src/daemon,
desktop_settings.rs, owned_settings.rs, protocol settings.rs, QML/CLI routes,
fixtures and exporter artifacts. The current reported source receipt is44ba7afa
and helper SHA prefixabdfc528 (full hashes must come from the agent's receipt).
Reported423Rust/954repo/534immutable parity/15Qt/50controller are evidence to
inspect, not new tests for the spec agent to rerun. Existing measurements continue
only under N0002's grant; do not alter candidate bytes during that capture.
Read architecture/{foundation,protocol,standing-rules}.md,
development/{settings-architecture,security-review,rust-migration}.md.
Live registry at5f4 has72 records and19 with settings metadata; charter65 is a
historical count. Do not demand generic RPC authority over all72 records.

## Scope

In: audit/complete existing F4 settings writer, native desktop controller, owned
provider clients, opt-in QML routing, schemas/transcripts and source evidence.
Out: new contract redesign, other core services, live/default opt-in, privileged
ownership expansion, measurements except separately granted N0002 continuation.

## Steps

1. Send a concise existing-work report first: parent commit, full source/binary
   receipt hashes, active job/capture and frozen150validation/50controller fixture
   paths and hashes. Keep all passing checks; rerun only if source changes,
   failures, missing evidence or an unmet criterion requires it. Never reset an
   active checkout or overwrite another agent's work.
2. Keep the existing additive API: settings.get {store}; settings.set
   {store,key:[segments],value,rev}; settings.store.<id> snapshot SettingsStore
   {store,rev,exists,data,dirty,error?}; error {file,line,message} with registered
   relative file only. settings.desktop {arguments,rev?} preserves controller
   transactions, mutation requires revision. settings.publish permits only
   control-center/dashboard-extras/battery-alerts. settings.provider permits only
   colors/wallpaper-picker/dashboard/cli/apps/launcher-workflows and exact owned
   fields. Preserve bounds in current protocol settings.rs; boot+rev is required
   for mutation. Publish producer artifacts before any Bead adoption.
3. Audit all72 registry rows in one compact table in the F4 report: metadata/access,
   current writers, daemon route or preserved provider boundary, and remaining
   in-scope gap. Explicitly identify the19 settings entries, battery-alerts owner,
   launcher frecency and six native provider routes. Settings ownership must be
   complete for this registered Settings surface; root/provider/credential and
   later-wave records remain explicitly outside F4 generic-write authority.
4. Confirm writer semantics: validate before mutation; stale rev cannot commit;
   one serialized owner;150ms quiet atomic persistence with existing backups;
   valid external edits observed through parent-directory events; invalid edits
   preserve last-good memory and original bytes. Keep current disk-wins conflict
   rule: queued writes cannot overwrite an unobserved external edit; report the
   conflict and require explicit re-read/new action. No automatic retry of a
   mutation or fallback disk writer after dispatch/boot change/lost reply.
5. Check publication failure rigorously: dirty:false alone is NOT success when
   error exists. A durable client success requires correct boot/rev, dirty:false,
   no error and intended data still present. IO/conflict can clear pending work
   only with visible error; never claim requested value persisted. Add/extend
   named test settings_failed_publication_never_acknowledges_success to inject
   IO failure and external conflict. No new global retry policy or error topic.
6. Reuse existing tests proving concurrent_writers_use_memory_revision_and_one_quiet_publication,
   invalid_hand_edit_keeps_last_good_and_reports_line_without_clobbering,
   unobserved_edit_wins_over_a_queued_write,
   providers_share_revision_preserve_unknown_fields_and_never_gain_other_stores,
   provider_boot_change_and_lost_mutation_reply_never_start_a_disk_writer,
   hardlinks_and_future_versions_never_replace_an_original and
   inotify_observes_atomic_replacement_without_a_polling_timer. Map each proof to
   its exact file/test name in the report; equivalent existing tests need no copy.
7. Verify opted-in DesktopSettings/NacreSettingsStorage/NacreStateFile and other
   migrated publisher services never start the old helper/file writer. Preserve
   one-release standalone mode only when selected before dispatch. Preserve
   preview/confirm/revert, root privilege boundaries, unknown fields, normal
   language enum and developer pseudo-language opt-in. No Python in ported path.
8. Update protocol.md and rust-migration.md to exact supported ownership and
   outstanding gaps; do not publish runtime changes until N0002 passes. If a
   correctness fix changes captured bytes, retain old evidence as diagnostic and
   ask spec agent to reissue the capture revision; never relabel old results.

## Clarification: durable success checks operation-owned values

Added 2026-10-17 after wait_store audit. This is an authorized bounded correctness
fix in state.rs, owned_settings.rs, daemon/settings_controller.rs, daemon/menu.rs
and their existing tests; it changes the frozen candidate and requires fresh
source/binary receipts. Preserve v25 source and measurements as historical evidence.

- Extend wait_store with a caller-supplied expectation of key paths and expected
  JSON values (including explicit absence). Keep boot/connection, target revision,
  error, timeout and dirty checks. A clean revision >= target is successful only
  when every expected path still matches. Compare JSON structurally; absent and
  null differ, arrays and an explicitly set object subtree compare in full.
  A mismatching clean snapshot returns a visible conflict/superseded error;
  never retry/replay or fall back. Compare the initial clean accepted snapshot too.
- settings.set: expectation is the exact requested key path, with its value from
  the accepted snapshot after validation/normalization. Include no-op requests;
  do not derive intent only from changed fields.
- settings.publish: check all authorized fields supplied by this operation,
  including supplied unchanged values. Do not check unrelated retained fields.
- settings.provider: check its authorized fields using the existing provider-key /
  descriptor allowlist. Include owned fields omitted by the replacement request
  as expected absence, because omission can delete them. Retained unknown fields
  are not owned. For colors.appThemes, check only its eight known theme keys;
  preserved unknown members must not cause false conflicts. Reuse existing
  validation/ownership metadata, not a competing general diff/merge framework.
- menu.preferences mutators: result already contains the four fields this actor
  writes: favorites, hidden, recent, usage. Capture expectations from that result,
  never from a later settings.get that might describe another mutation.
- desktop controller: the result's rev identifies its admitted store revision.
  If a follow-up get is needed for accepted data, require get.rev == result.rev;
  otherwise fail conservatively with conflict rather than treating a later
  mutation as the caller's acceptance. For explicit set, check the requested key
  even if unchanged. For other controller actions, check every changed/deleted
  desktop path between the pre-call store and that exact accepted snapshot;
  preserve the existing action/transaction semantics. If expected paths cannot
  be determined safely, stop and report that action before widening the wire API.
- The wait establishes that the caller's owned values are present at the observed
  durable snapshot. It cannot promise values will remain unchanged afterwards,
  or prove intermediate revisions were individually flushed. Do not claim that.

Extend settings_failed_publication_never_acknowledges_success and add focused
cases (equivalent existing tests may be extended):

1. settings_wait_same_key_superseded_is_conflict: accepted key A is replaced
   before flush; newer clean/error-free revision must NOT report A as successful.
2. settings_wait_unrelated_key_change_still_succeeds: later key B changes while A
   remains; newer durable snapshot succeeds for A. Whole-document equality fails
   this requirement and must not be used as the general check.
3. settings_wait_absent_is_not_null: an owned provider deletion is restored or
   changed to null; neither matches expected absence. Unknown preserved fields
   and unknown appThemes keys can change without a false conflict.
4. settings_wait_noop_and_fast_clean_check_intent: no-op and initially clean paths
   use the same expectation checks; existing IO/error tests still fail safely.
5. settings_controller_receipt_race_is_conflict: another mutation between desktop
   call and get cannot supply the earlier operation's accepted snapshot.

Run focused relevant tests after the fix; rerun broader required gates only for
this changed candidate. No frozen fixture regeneration and no capture grant.

## Measurements

None newly authorized here. N0002 covers existing fixed-endpoint captures.

## Acceptance criteria

- [ ] Exact receipt/hashes, active-job state and150/50 frozen source cases located.
- [ ]72-row ownership audit; all19 Settings stores and extra owned routes accounted
  for, no unresolved competing writer on F4-owned fields.
- [ ] Original150validation/50controller and immutable parity unchanged; applicable
  existing source checks, schemas/Luau/freshness/Qt proofs PASS with real receipts.
- [ ] Named ownership/conflict/security proofs plus failed-publication proof PASS.
- [ ] No lost-reply replay, post-dispatch fallback, arbitrary path/privileged grant.
- [ ] N0002 performance gate passes before integration; full published-head CI green
  before final Done. Send Review at source-ready without duplicating capture work.

## Stop conditions

Missing evidence, writer gap beyond4h, changed privilege/legacy semantics,
required breaking API, data loss or invalid measured revision: stop and send the
exact gap for a follow-up. Preserve active captures, frozen fixtures and WIP.

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
