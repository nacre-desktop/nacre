# Nacre implementation queue

Current state2026-10-18. [Standing rules](../architecture/standing-rules.md) apply.
This summary supersedes older schedules/dispositions inside historical spec
sections and thread messages. Status changes are batched at start/review/Done.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Ready: fresh Luna worker | Priority capture15:30–18:30UTC after source/mock/CI readiness; exact frozen candidate1382/helper439dd355. One heavy job only. |
| 2 | [N0006 switch-on plan](0006-f4-owner-switch-on-plan.md) | Ready: fresh Luna docs worker | Draft source-verified owner commands/rollback in parallel; no execution or heavy job. |
| 3 | [N0005 network lifecycle](0005-network-owner-lifecycle.md) | Review | Scoped evidence accepted; runtime integration waits F4 and later core-service priority. |
| 4 | [N0003 network state](0003-core-network-state-adapter.md) | Blocked | Broader acceptance unclaimed; old evidence withdrawn. No new core implementation before owner priority. |
| 5 | [N0001 F4 review](0001-f4-settings-completion-audit.md) | Review | Source-ready; Done after N0002/published-head CI. |
| 6 | [N0004 activation](0004-f4-activation-progress-and-audit.md) | Done |1382a28 accepted. |

## Current execution limits

**N0002 exclusively holds15:30–18:30UTC subject to the current spec readiness gate. All older grants are expired.** Do not restart from older thread grants.
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

## Orchestrator handoff2026-10-18

Fresh visible Luna chats own one spec each; maximum3 active workers, one heavy job. Existing Nacre implementer finishes N2 prospective-driver handoff/CI monitoring, existing Bead implementer finishes B0011, then both retire without new specs. Owner reports wayland-info installed. Priority F4 publication, owner switch-on plan, Bead B2/B3 captures, then core services. Worker thread/worktree IDs will be recorded after launch; coordinator retains review/publication gate.
