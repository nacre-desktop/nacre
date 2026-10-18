# Nacre implementation queue

Updated2026-10-18. [Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Next action |
| --- | --- | --- | --- |
| 1 | [N0004 activation progress](0004-f4-activation-progress-and-audit.md) | In progress | Finish deterministic actor-consumption test and focused checks; no repeated audit |
| 2 | [N0001 F4 review](0001-f4-settings-completion-audit.md) | Review | Runtime/audit accepted; N0004 test proof remains |
| 3 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Ready, gated | Fresh8cell600s+60s ABAB after source review and slot |
| 4 | [N0003 network state](0003-core-network-state-adapter.md) | Ready source phase | Independent worktree source/tests while waiting; integration after green F4 |

Measurement priority: Bead B0002 then B0003 first, per owner. Bead holds the
private nested lab window2026-10-18 00:55–10:55UTC (release early on completion).
Nacre has no capture grant. No build/test overlap on Bead's active capture host;
finish local source work and then independent N0003 source while waiting.
Spec agent assigns Nacre N0002 immediately when source-ready and host released.
No live session, installation, package/tag/release or unit enablement authorized.

Use approved commit identity, docs(spec): for spec-agent commits, [no-publish]
on all Nacre commits. Status edits at start/review/Done, batch notes/clarifications.
