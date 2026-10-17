# N0004 — F4 activation progress and ownership corrections

- ID: N0004
- Repo: nacre-desktop/nacre
- Implementer: NACRE
- Status: In progress
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

