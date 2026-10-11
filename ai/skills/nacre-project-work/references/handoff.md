# Cross-assistant task checkpoint

At a verified milestone, before a long wait, or before ending unfinished work,
save a compact checkpoint through `nacre-brain note --kind checkpoints`.
Search the project/task first and skip unchanged duplicate checkpoints.

Include only useful fields:
- Objective and current status: active, blocked, or completed.
- Project, host, branch/worktree and exact commit; distinguish uncommitted changes.
- What changed and why; keep unrelated work out.
- Exact checks run and outcomes; identify NOT VERIFIED claims.
- Relevant artifact/log paths, rollback details for deployments, and dated identity.
- Blockers, next concrete action and any outstanding user decision.
- Links to the previous checkpoint and canonical repository issue/spec.

Never store credentials, raw transcripts or speculative findings as verified.
On resume, read the latest relevant checkpoint, inspect actual git/service/device
state, and reconcile differences before acting. Either assistant can use the same
checkpoint; native chat histories and authentication remain separate.
