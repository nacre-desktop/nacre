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

## Lead handoff — 2026-10-18, after standing workflow installation

**Latest owner direction supersedes separate-chat orchestration:** use installed
lead-workflow with built-in subagents; at most3, writers isolated, one heavy job
including hosted full CI. Separate chats finish current tasks then retire. Use
Luna for clear work; after one clearer failed retry use Sol. Nontrivial review
uses reviewer; high-risk work also Terra. Existing permissions unchanged.

### Current state and work in flight

- F4 runtime candidate1382a28ab544743e32e906f7542bcecdfdfca083 remains LOCAL,
  unpublished; helper439dd355fca276334c2faccbd0992d1b23eac6c6caab311ca1c1c61d3672e92c.
  Frozen baseline5f4c0bf3/helper4a65f4ec unchanged. N0004 Done; N0001 Review.
- N0002 collector38118f43c1f2a450df406c6a047ca0e90a9d9cd4 exact CI39136680743
  SUCCESS. Prospective private v5 mocks11 pass, one600s warmup+three60s windows
  per arm, whole-wrapper and collector paging gates unchanged. Minimum104min
  fixed waits/8arms before overhead. Existing captures invalid; no accepted
  fresh comparison. Do not relabel partials or waive host paging rejection.
- N0002 separate worker01a14f98-71e5-7131-98cc-f9dbd3ff4561, branch
  task/luna-n0002-f4-20261018, owns current source/mock readiness. Both retiring
  peers explicitly ACKed IDLE. Capture window15:30–18:30UTC is CONDITIONAL;
  no new CAPTURE_START/accepted arm has been reported at this handoff. The private
  launch-before-verification watchdog DOES NOT meet the held-child gate. Worker
  was assigned bounded source correction and actual-launcher mocks; verify its
  latest output before any payload. Parent systemctl/effective2GiB/swap0 AND
  exact held-child cgroup membership must pass before samePID exec; external
 4GiBstart/abort2GiB monitor. Never infer grant from idle lock. If time insufficient,
  complete source fix, report release; do not start an unfinishable partial batch.
- Existing Nacre implementer01a13507-3311-7061-b8a5-38034082b372 completed driver
  handoff and retired. Sources/private evidence preserved; no owned job.
- N0006 source-cited plan draft6afd1d1 plus reviewed correction18882109 are
  incorporated here. Review remains OPEN: durable managed QML opt-in and exclusive
  installer/activation handoff with restoration are missing; no runnable complete
  one-command switch rollback yet. No package/live action authorized/executed.
  Docs worker01a14f98-760b-74b0-a42f-5b576163a702 finished IDLE; no new assignments.
- N0005 ecd08dfe/c992fc58 scoped lifecycle/regression evidence accepted, runtime
  integration gated after F4; N0003 broader acceptance unclaimed. Old N3 resource
  evidence withdrawn. No core-service implementation priority until after Bead gates.
- Bead B0011 current source72c43f6; CI39141528958 was running at last check.
  Earlier39140928302/bf02b13 FAILED. No Bead lab capture; wait exact CI conclusion
  before releasing any N0002 heavy payload. No further Bead CI during capture.

### Next units for a fresh built-in lead

1. Explorer reads latest worker output/CI and current queue, not older chat status.
   If N2 held-child fix still absent after the clearer retry, obtain old worker
   IDLE/source handoff, assign a built-in Sol worker to that bounded concurrency
   launcher fix; no simultaneous writers. Reviewer checks source/mock proof.
2. Finish N2 full valid batch, assess whole-group/endpoints, review source-to-capture
   mapping, integrate F4 normally and require green exact published-head CI.
   No runtime push before accepted gate; no rebuild of frozen release artifact.
3. After F4, implement bounded persistent opt-in/guard/rollback prerequisites and
   finish owner switch-on plan. Package/unit/live actions need reviewed owner plan.
   Then schedule Bead B2 and B3, then core services. wayland-info prerequisite is
   owner-reported resolved; no reinstall or repeated idle probes.

Superseded documentation-only full CI39139624556/39139965930/39140394830 canceled
intentionally to preserve heavy slot, not passed. This docs-only handoff uses
[skip ci] to avoid launching a full suite during the reservation; it does not
waive code validation. No status-only or private config files enter this repo.

### Current execution update —16:10UTC

Separate N0002 chat grant withdrawn. Built-in Sol implemented private v6 guard;
Sol reviewer and Terra both ACCEPT final0cc8dd37 source/mock package,23tests and
24manifest hashes. Built-in Luna gets16:15–19:00UTC only, ONE containment smoke
then fresh8arms on success, no retry/quiet waiver. See N0002 active grant for
exact bindings. No capture has started yet; no other heavy job may overlap.

B0011 source7975382/CI39142524705 FAILED. Retiring implementer ceased edits;
built-in Sol fixed the actual synchronous-initial-callback race at localfb81b387
in task/sol-b0011-lifecycle-20261018. Built-in reviewer ACCEPTed source pending
fmt/compiler/exact-head CI; no push/heavy test grant until N0002 release. This
supersedes earlier speculative fake-peer ordering diagnoses. No new core service
implementation or live switching now. F4 publication remains first priority.


### Execution handoff — 2026-10-18 17:38 UTC

N0002 v6 stopped during first baseline warmup, zero accepted cells; lab released.
Owner supersedes the sizing assumption: capture cap max(baseline peak×1.5,12GiB),
currently12GiB; host abort below2GiB unchanged. Distinct v7 source/mock update
and review in progress. Fresh full-batch grant18:00–22:00UTC, conditional only
on reviewed v7 pins and containment smoke; no other heavy work during it.
B0011 is Done: source af61d18 full CI39147491406 green, docs5ad2551; heavy slot
released. F4 local integration d3622384 remains reviewed and unpublished pending
valid N2 evidence, then exact published-head CI. N6 switch-on blockers remain;
no package/live action. Retired separate chats have no assignments or grants.


### Current execution handoff — 2026-10-18 18:55 UTC

N2 v7 first baseline invalid for task coverage; raw preserved,zero accepted.
Source-only v8 review2 fixes warmup boundaries and uses explicit sampled
whole-group census/critical-root stability; unknown lifetime counters stay
unknown. Sol+Terra ACCEPT, all34 pins/28 mocks pass. Sole built-in Luna heavy
grant19:00–23:00UTC for smoke+fresh8arms,12GiB cap/hostabort2GiB unchanged.
All other workers IDLE. F4 reviewed integration d3622384 remains local pending
valid capture then published exact-head CI. B0011Done/green; N6 blockers remain.


### Active execution — 2026-10-18 20:20 UTC

N2 priorfailedcells/readiness preserved,zeroaccepted. Sol+Terra ACCEPTv12:
57tests/37manifest/27planpins; argv-byte interface fixed and readiness exercises
actual2sidle. Built-in Luna soleheavy nowthroughOct19 00:45UTC: ONE<=120s smoke,
ifPASSfresh8arms full600+3x60,strictquiet,12GiBcap/hostabort2GiB. No otherheavyCI.
Astra investigated bounded snapshot completion after real startup race failures;
allhelper samplecoverage/unknown lifetimes are explicit in consolidated N2spec.
F4 reviewed localmerge d3622384 waitsvalidN2thenpublishedexact-headCI.
B0011Donegreen; N6switch-on blockersremain. No package/live action.
Visibility permanently saved; lead waits active workers and posts outcomes.
