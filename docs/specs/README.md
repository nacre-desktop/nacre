# Nacre implementation queue

Current state2026-10-18. [Standing rules](../architecture/standing-rules.md) apply.
This summary supersedes older schedules/dispositions inside historical spec
sections and thread messages. Status changes are batched at start/review/Done.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Capture Blocked; collector source accepted | Collector integrated5f50d7f8; fix task-caused Ruff B023 in test closure from CI39134154257/head6c1551c0, normal push and exact-head CI. No runtime push/capture. |
| 2 | [N0005 network lifecycle](0005-network-owner-lifecycle.md) | Review; scoped correctness accepted | ecd08dfe/c992fc58 lifecycle and seven core-network tests/fmt/Clippy accepted with verified payload containment; integration remains gated after F4. |
| 3 | [N0003 network state](0003-core-network-state-adapter.md) | Blocked | Prior PASS/OOM claims withdrawn; no acceptance or rerun. Preserve local commits/logs. |
| 4 | [N0001 F4 review](0001-f4-settings-completion-audit.md) | Review | Source-ready; final Done waits N0002 and published-head CI. |
| 5 | [N0004 activation progress](0004-f4-activation-progress-and-audit.md) | Done | Exact1382a28 accepted; do not repeat tests or build old candidates. |

## Current execution limits

**No host build/test/capture slot is active. All earlier UTC capture/correctness
windows are withdrawn or expired.** Do not restart from older thread grants.
A future job requires an explicit exact-source/host/window assignment and verified
payload containment before release. No deadline automatically transfers a host.

N0002 invalid baseline arms remain invalid, with zero accepted comparisons.
Host-global swap-in exceeded1MiB; do not infer a product regression or process
attribution. Final prepared1382a28 helper439dd355 remains preserved; no rebuild
is requested. Whole-arm paging rejection remains unchanged. Future prospective
entry must pass --full-idle-windows, use the external driver's existing600s
warmup boundary, record separate warmup/whole-arm/three60s idle counters, and
receive a NEW capture grant after readiness review. No old partial reuse.

N0003/N0005 runtime commits remain local/unpublished until source review and green
published F4. The TWO approved N0002 collector Python files may integrate now;
that approval does not authorize any F4/network runtime push. Missing local Ruff
is handled by hosted CI, not a package/toolchain install or another approval wait.

Incident CLOSED by owner: likely lost Codex connection, no OOM/oomd or relevant
kernel-crash attribution. Keep capture safeguards: own jobs only, peer pause,
4GiB available start/abort below2GiB, verified private2GiB capture scope and
external watchdog. Separately sized build caps require an actual job grant.
No live desktop/install/config/unit/package-release/AI-policy changes.

Bead has independent B0010 source/private tests/hosted CI; B0002 remains blocked
by missing wayland-info and denied sudo. Neither implementer should probe the
shared host while no job is authorized. Ask spec agent only for actual scope /
resource/conflict decisions, not routine local corrections already specified.

Author/committer approved siverteh noreply; [no-publish] on every Nacre commit.
Spec-agent commits use docs(spec):. Isolated worktrees, preserve others, no force
push, inspect own outgoing commits, check CI after each normal push. Keep private
raw artifacts outside Git. Historical evidence is retained in specs/reports.
