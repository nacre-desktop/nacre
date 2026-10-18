# Nacre implementation queue

Updated2026-10-18. [Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Next action |
| --- | --- | --- | --- |
| 1 | [N0004 activation progress](0004-f4-activation-progress-and-audit.md) | Done | Exact1382a28 focused proof accepted; release artifact under N0002 |
| 2 | [N0001 F4 review](0001-f4-settings-completion-audit.md) | Review | Source-ready; final Done after N0002 gate and published CI |
| 3 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Blocked | First baseline invalid on host paging; no retry grant |
| 4 | [N0003 network state](0003-core-network-state-adapter.md) | Blocked | Prior PASS/OOM claims withdrawn; tests not accepted; no rerun grant |

Restart recovery2026-10-18: published source at recovery is ccc19e3; preserved
F4 has84 dirty entries and network source has4, no outgoing commits. No reset,
clean, deletion or process termination performed by the spec agent. N0003 source
phase is In progress in its isolated worktree; integration remains gated on F4.

Incident CLOSED by owner2026-10-18: likely lost Codex connection; no OOM/oomd
kills or relevant kernel crash attribution. Keep the capture-safety rules.

Bead B0002 ACTIVE Oct18 11:00–19:00UTC, then B0003 19:00–21:00UTC after
B0002 PASS, private nested lab only. Nacre N0002 takes the first released slot
once source/correctness ready; spec agent issues exact final-artifact grant.
No overlapping heavy jobs on that host. Ask the peer to pause; never kill others.

Nacre must immediately commit preserved F4 work locally in small logical commits
with approved identity/[no-publish], without pushing until spec-agent review.
Unvalidated local commits protect work and do not claim a passing gate. Continue
N0004 and independent N0003 source now; no waiting for further preservation
approval. Preserve the separate worktrees and record remaining uncommitted files.
No live session/install/package release/unit changes are authorized.

Use approved commit identity, docs(spec): for spec-agent commits, [no-publish]
on all Nacre commits. Status edits at start/review/Done, batch notes/clarifications.

Latest handoff: Bead explicitly released the lab. N0004 Done at1382a28;
Nacre owns N0002 release preparation now and safe capture12:30–14:30UTC.
This specific handoff supersedes the general Bead window while its dependency
is blocked. Bead has independent B0007 source/tests; no overlapping lab job.
Record final helper hash before CAPTURE_START; no second approval chain.

Current sequential schedule: final1382a28 release artifact preparation now–12:30UTC;
N0002 capture12:30–14:30UTC; N0003 correctness14:30–15:30UTC, only after explicit
N0002 release. Bead continues independent B0007 source/hosted CI. One heavy job
at a time; no automatic takeover at a deadline.

LATEST gate disposition: N0002 stopped12:19:34UTC after first baseline exceeded
host swap-in quiet gate; zero valid comparisons. Guard/cap held, artifacts
preserved, no regressions inferred. Former capture windows are withdrawn.
N0003 correctness owns released lab12:20–13:25UTC; Bead has B0008 source/CI.
No active capture grant. N0002 existing-evidence diagnosis is bounded20min and
never authorizes swap/cache changes or another capture.

LATEST eligible work: N0003 fmt passed at212c3efe but native compilation needs
cmake; tests/Clippy NOT RUN. Bounded10min existing-build-record check, then stop
if absent. N0002 source-only collector correction/mocktests is Ready now (90min),
with full60s each idle phase and separately recorded paging windows. Existing
paging rejection remains; NO capture/retry grant. Bead has B0009 source/CI.

LATEST: N0003 functional checks pass with existingcmake; run was uncapped, so
operational containment is NOT accepted. No host run until verified launchgate.
Current source work N0002 collector correction, then Ready [N0005 network owner
lifecycle](0005-network-owner-lifecycle.md). N0003 Review/unpublished pending
that source follow-up and greenF4. No capture grant; Bead has B0009 source/CI.

AUTHORITATIVE correction: N0003 implementer withdrew prior PASS/OOM reports;
tests are NOT RUN for acceptance. Preserve commits/logs, do not publish claimed
results and do not rerun without a new assignment. Current NACRE work is local
N0002 collector correction only; no host/capture. N0005 stays queued.

N0005 sourcee5c4a3a2 is Review with exact follow-ups in its spec: final owner
priority/commit check, actual observe/supervisor lifecycle tests and bounded
compile fixes. Source corrections eligible now; no host validation grant yet.
N0002 collector source reviewed for integration of d7c9bc2+b2103ee only; preserve
latest queue/docs rather than replay stale3276a0a. HostedCI pending integration.
