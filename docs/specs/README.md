# Nacre implementation queue

Updated2026-10-18. [Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Next action |
| --- | --- | --- | --- |
| 1 | [N0004 activation progress](0004-f4-activation-progress-and-audit.md) | In progress | Finish deterministic actor-consumption test and focused checks; no repeated audit |
| 2 | [N0001 F4 review](0001-f4-settings-completion-audit.md) | Review | Runtime/audit accepted; N0004 test proof remains |
| 3 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Ready, gated | Fresh8cell600s+60s ABAB after source review and slot |
| 4 | [N0003 network state](0003-core-network-state-adapter.md) | In progress source phase | Independent worktree source/tests while waiting; integration after green F4 |

Restart recovery2026-10-18: published source at recovery is ccc19e3; preserved
F4 has84 dirty entries and network source has4, no outgoing commits. No reset,
clean, deletion or process termination performed by the spec agent. N0003 source
phase is In progress in its isolated worktree; integration remains gated on F4.

All former capture windows are expired/superseded. Incident review and the new
[capture safety](../architecture/standing-rules.md#capture-safety--restart-rule-2026-10-18)
are prerequisites. Bead B0002 then B0003 has priority once prerequisites pass;
Nacre N0002 has no grant. Only explicit peer release transfers the host.
No live session/install/package/tag/release/unit changes are authorized.

Use approved commit identity, docs(spec): for spec-agent commits, [no-publish]
on all Nacre commits. Status edits at start/review/Done, batch notes/clarifications.
