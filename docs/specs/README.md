# Nacre implementation queue

Updated2026-10-18. [Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Next action |
| --- | --- | --- | --- |
| 1 | [N0004 activation progress](0004-f4-activation-progress-and-audit.md) | Review | Focused checks accepted at87911aa1; add after-reply waiter-cancellation variant |
| 2 | [N0001 F4 review](0001-f4-settings-completion-audit.md) | Review | Runtime/audit accepted; N0004 test proof remains |
| 3 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Ready, gated | Fresh8cell600s+60s ABAB after source review and slot |
| 4 | [N0003 network state](0003-core-network-state-adapter.md) | In progress source phase | Independent worktree source/tests while waiting; integration after green F4 |

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
