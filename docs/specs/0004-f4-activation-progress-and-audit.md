# N0004 — F4 activation progress and ownership corrections

- ID: N0004
- Repo: nacre-desktop/nacre
- Implementer: NACRE
- Status: Review
- Depends on: N0001 v7 source-ready submission; preserve unpublished candidate
- Date: 2026-10-18
- Size: one focused session, maximum3hours

## Goal

Fix the demonstrated activation wait inside the serial menu actor and correct
ownership-report claims. Passing v7 tests do not prove the missing progress case.

## Context

Base5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57; v7 manifest
3187e8cb71fac78c5cfb6fe8a4bb6d1026c409a1b354403be5d06f79d7c9cab2;
helper2f11566a9da56a90b9b7b46438c4bc323919b69645b8a28eef0297859edbdc2d.
These are reported receipts, not a measurement-approved revision. Frozen150/50/
rollback fixture hashes match independent local read-only review. Read N0001's
2c4a2fc0 internal-activation clarification and standing rules. Keep all source,
fixtures, private evidence and other tasks intact. Do not rebase/reset dirty tree.

Review found menu.rs run loop menu.activate→preference(...record...).await,
which calls settings.launcher_preferences and waits until flush; the serial menu
actor cannot serve another query meanwhile. Existing preference JoinSet branch
covers public preferences, not this activation branch. Required activation tests
are absent. Runtime paths need the bounded fix already authorized by N0001.

## Scope

In: daemon/menu.rs, settings.rs only as needed to split admission from persistence
observation, focused existing private tests, ownership/report corrections. Out:
other settings redesign, launcher ranking/provider/catalog behavior, QML changes,
new transport/dependencies/framework, live actions or measurements.

## Steps

1. Preserve current activation order: record admission before launch, exactly one
   dispatch, existing reply shape/dispatch uncertainty/fallback semantics. Separate
   short settings record admission (accepted rev and four fields captured under
   settings lock) from waiting for persistence. menu.preferences keeps its existing
   durable wait for all callers. Do not duplicate persistence owners or values.
2. In the activation branch, do not await150ms publication inside the serial menu
   loop. Put the pending chain/observation into the existing bounded JoinSet flow;
   leave settings lock/Hub borrows released. Reuse its16-job bound and existing
   completion messages, distinguishing activation work from provider refresh.
   Keep the actor serving queries during any persistence wait. No detached task,
   self-RPC, polling or new background process. If retained admission-before-launch
   requires waiting before dispatch, that wait must occur in the bounded job,
   not the actor; do not silently relax existing ordering to get a passing test.
3. Successful menu.activate is activation outcome, not a promise frecency remains
   durable. IO/supersession after dispatch must be visible without replay or a
   false not-dispatched error. Cancellation/deadline/bound failure must not launch
   a second action, leak a waiter or exceed concurrency. Preserve prior admitted
   mutation semantics and existing5s/10s/method deadlines; report any conflict.
4. Add activation_frecency_wait_does_not_block_menu_query: hold the real settings
   actor's flush pending using private controlled state, submit activation through
   the actual menu run-loop path with fake launch provider, submit another query
   before releasing flush, and prove query completes correctly. A settings mutex
   test alone is insufficient. Add activation_frecency_failure_never_replays_activation:
   injected publication failure/supersession and cancellation exercise that same
   path, assert exact dispatch count and correct uncertainty/error reporting.
   Preserve no-lost-wakeup/actor-progress tests for public preferences.
5. Correct ownership report: launcher-menu has write metadata but no enabled
   descriptor. The two cited NacreMenuContext FileViews set/read/watch paths and
   have no write call; classify them as readers, disabled in shared mode. Do not
   label an editor/current writer without exact executable call evidence. If a
   true runtime writer is found, report exact file/function/line before expanding
   authority; no new generic setter/descriptor. Standalone externally user-edited
   files do not establish a competing daemon-mode application writer.
6. Audit active-descriptor counts row by row using backend != planned, matching
   settings_validation::descriptor. Profile has5 total descriptors,4planned,
   1enabled aurReview; its report currently says5active. Recompute totals and
   affected rows; distinguish metadata access, enabled descriptors and actual
   writer routes. Preserve offline installer/deployment boundary from N0001.
7. On the authorized preserved isolated correctness host, run focused tests first,
   then required affected broader gates for changed source. No rerun of frozen
   output generation; no claim v7 receipts cover new bytes. Update private review
   report with new full manifest/overlay/helper/receipt hashes and commands.
   Continue to preserve base5f4: latest main at review differs only in docs. Before
   final integration fetch again; runtime advances require new review, not a
   silent baseline substitution.

## Measurements

None. Source review/correctness only. N0002 requires this review plus a separate
exact-hash/host/window grant. Old v25 cells are historical, not reusable acceptance
for the new candidate. Do not measure v7 or the fix without that grant.

## Acceptance criteria

- [ ] No serial menu actor wait for activation frecency persistence; queries progress.
- [ ] Two required tests cover actual activation run-loop path and one-dispatch /
  no-replay behavior; existing durable public preference tests still pass.
- [ ] Ownership table accurately distinguishes FileView reads/actual writes and
  planned/enabled profile descriptors; no widened setting/permission authority.
- [ ] Frozen150/50/rollback and other parity outputs unchanged; required source /
  fmt/Clippy/tests pass on exact revised bytes; final full hashes/receipts supplied.
- [ ] No runtime publication/capture/live action; submit N0004 and N0001 Review.

## Stop conditions

Need to relax launch ordering, broaden API, modify security/AI policy, add a worker,
lose cancellation/dispatch certainty, identify a real competing writer, or exceed
3hours: report exact issue; do not improvise. Preserve dirty candidate and evidence.

## Report format

N0004 Review; exact base/final manifest/overlay/helper hashes, changed files,
criterion PASS/FAIL/NOT RUN with test commands and counts; activation trace proving
query progress and dispatch count; corrected ownership counts; private report /
receipt locations and cleanup state. No measurement claim. Include candidate
readiness for a separately granted fresh eight-cell N0002 capture.

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


## Failure visibility and evidence clarification — 2026-10-18

- For ancillary frecency IO failure, retain the settings actor's existing error
  snapshot/publication path and emit an existing tracing warning carrying only
  sanitized operation/error code. For supersession, emit a sanitized warning
  for that operation's failed durability expectation; do not mark a newer valid
  store revision erroneous or overwrite it. No new wire topic/schema/UI surface.
- Do not silently discard a failed JoinSet result. Test its handled error outcome
  and exact dispatch count; existing log capture may assert the warning when
  available, but do not add a logging framework only for the test. No raw app ID,
  command line, document payload, private path or credential in the warning.
- Once activation dispatched, its eventual reply retains the real launch/dispatch
  result and existing uncertainty flags. Frecency warning is not an undispatched
  or retryable activation failure; no replay/fallback. If a failure prevents
  dispatch under existing ordering, report that actual pre-dispatch state safely.
- Reports must distinguish v25 historical capture44ba7afa/abdfc528, v9 correctness
  manifest5495c030/helper2f11566a, and newer dirty N0004 source with no validated
  receipt. Freeze/hash final revised source before its next correctness gate;
  do not relabel either old receipt as proof for the new edits.
- The reported SSH host-key rejection blocks remote writes/builds until trusted
  identity/key provenance is reconciled. Do not disable strict checking, remove
  the conflicting key, accept a replacement from ssh-keyscan alone, or change
  credentials/host configuration as part of this spec. Local source work may
  continue; return the exact non-secret endpoint/expected fingerprint provenance
  and rejection category for review if the existing trusted route cannot be
  recovered. No repeated speculative SSH retries or measurement host probes.

## v10 review disposition — 2026-10-18

Source activation admission/JoinSet split and corrected ownership evidence are
accepted as reviewed. Local report SHA8d41af15 and full rerun4 log SHA3057003e
match supplied receipts;438Rust and complete repo results remain scoped to v10.
Status remains In progress for ONE proof gap; no capture grant.

activation_frecency_failure_never_replays_activation currently calls flush,
checks store and try_recv, then exits LocalSet. That does not ensure the actor
consumed the completed JoinSet result/warning branch before teardown cancelled it.

- Add a private cfg(test)-only completion channel at the ACTUAL post-dispatch
  activation-frecency JoinSet handling branch (after result handling). Pass the
  handled outcome/error code to the fixture; never emit from the settings flush
  or background waiter alone, which would miss the actor-consumption proof.
- For IO, supersession and caller-cancellation cases, await this signal with the
  existing bounded test deadline, assert expected handled outcome, then check
  exact one-dispatch/no-replay. No wall-clock sleeps as a substitute for this
  synchronization. Warning-string capture/framework remains unnecessary.
- Keep production behavior unchanged; no new wire field/topic/normal-build API.
  Prove cfg(test) isolation in the diff. Retain source-review acceptance for the
  bounded activation runtime fix unless new production changes require review.
- Run focused activation/public-preference tests, fmt and affected test-target
  Clippy; do not rerun all tools/AI/Brain/parity suites solely for this test hook.
  Update source/overlay hashes and the Review report, explicitly map unchanged
  production/helper bytes to existing v10 broader receipts, and state the new
  focused results separately. If production bytes change, report that scope and
  rerun affected gates; never relabel v10 tests as new-revision results.

Return N0004 Review after deterministic completion proof. No N0002/grant until
it passes; the fixed600s+60s fresh8cell plan/baseline5f4 remain unchanged.


## Restart safety

No measurements are authorized by this spec. Any later measurement must follow
the standing capture-safety rules: peer pause, MemAvailable >=4GiB before start,
abort below2GiB, private memory-limited scope, and explicit spec-agent grant.

## Recovery preservation — owner decision, 2026-10-18

Immediately protect the current preserved F4 changes with small logical LOCAL
commits, approved identity and [no-publish]. Include intended tracked/untracked
files and deletion, inspect each staged diff, and record remaining dirty paths.
Do not push until reviewed. Do not reset, clean, discard or rewrite others' work.
These commits are explicitly unvalidated until focused checks pass. Continue
recover-and-continue activation JoinError handling/tests and N0003 source work;
no build/test overlap with active Bead captures on the same private host.

## Focused source review — 2026-10-18, candidate87911aa1

Reviewed local commits through87911aa1f9c12d8a0c000e68e4d754376ab3ea45
over baseline5f4c0bf3. Source inspection accepts short admission, bounded task
wait, actor-consumed completion, pending replies keyed by task ID, conservative
uncertainty on activation JoinError, no replay, and continued actor service.
Successful and failed completions remove activation metadata. Public preference
durable behavior remains intact. Production JoinError recovery is explicitly
authorized here, superseding earlier test-hook-only wording.

Reported exact-source focused results accepted: activation_frecency_*5/5,
menu_preferences_*5/5, rustfmt and helper test-target Clippy-Dwarnings pass.
Log SHA ac20bdeced3454032d3f4318f348e0f220fa06839f3dcb609170d0413b71413d.
No new release helper built; old helper hashes do not identify this candidate.
No publication or measurement accepted.

One bounded test fix before Done: parameterize the existing waiter-cancellation
test for before AND after launch reply. The latter must send launch success,
await the real reply while controlled flush is pending, abort the waiter, await
actor-handled JoinError{replied:true}, then prove no replay and subsequent query
success. Before-reply case keeps conservative Internal/dispatched=true/retry=false.
Assert metadata release through existing private test access if available; do
not add a production API or framework. No production change requested. Run only
changed test,fmt,affected test-targetClippy; no broad-suite repetition.

Keep local commits unpublished. After this proof, prepare an exact final release
artifact/hash on an explicitly free correctness/build host; N0002 still requires
its separate safe capture grant and fresh8cells. N0003 source remains eligible.
