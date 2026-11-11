# N0173 — Timing-independent test acceptance

Status: validating on the shared build host. Product code unchanged.

Default tests check behavior rather than how often a loaded host schedules a
debug build. The workspace switch checks advancing frame deadlines, monotonic
capsule motion in both directions, and return to the original position. The
pinned Bead Session has no public controlled clock; its scheduler already has
deterministically stepped unit coverage. The integration test retains its strict
frame count/cadence checks behind `NACRE_TEST_PERF=1` for idle-host measurements.

The picker fake daemon stays runnable during synchronous scene compilation;
same-socket reply barriers replace sleeps around the press/release assertions.
The chat disconnect fixture parks reconnection until the disconnected projection
has been observed. Its previous failure and successful reruns were recorded by
the preceding chat unit. Neither change alters client timeouts or transport code.
The daemon session fixture waits for an actual `READY=1` notification with a
60-second hang bound, replacing the load-sensitive 20-second startup allowance.

The migration's 2,000-note content, privacy, idempotence and sequential/all tree
equivalence checks remain in the default suite. Its two 60-second limits move to
the ignored `synthetic_vault_migration_performance` test. Brain's browser-load
test keeps content, auth retry, visible timeout/Retry, unchanged persisted index,
and live vault-change checks; first-view/restart speed gates require
`BRAIN_TEST_PERF=1`. Real browser waits use generous hang bounds. Search-quality
coverage is already timing-independent and its latency gate is already ignored.

The historical picker failure was a shell test, despite its attribution to the
helper in the task. Reachable rollback history already excludes Python bytecode
from fingerprints and tests that exclusion. The later restore-failure traceback
was not retained; no second distinct 120 Hz count test could be identified in
the setup rollback history. No temporary duplicate implementations or evidence
files are introduced.

## Validation

Pending: matched repeated runs under CPU load, full desktop CI, integration.
