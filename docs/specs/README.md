# Nacre implementation queue

Current handoff: owner guidance, morning 2026-10-19; supersedes older thread grants.
[Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [N0007 QML memory correction](0007-f4-qml-memory-fix.md) | In progress: memory correction | Source8a independently accepted;21 local+2 wire checks pass. Source8a quick screen failed +5.756/+6.121/+0.488MiB; no H/full-eight. Sol/Astra cause unknown; GC diagnostic finished; no supported fix. Standalone private heaptrack baseline profile independently reviewed/granted; candidate B unarmed, acceptance blocked. |
| 2 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Blocked: product correction | Both v14 geometry checks passed; only Niri A/B valid. H baseline failed whole-wrapper swap-in; no eight-run PASS. |
| 2 | [N0006 switch-on plan](0006-f4-owner-switch-on-plan.md) | Review: prerequisites | Corrected plan retained; F4/package readiness, exclusive installer handoff and managed persistent opt-in/rollback missing. No live action. |
| — | [N0009 exclusive destination writer handoff](0009-f4-switch-prerequisites.md) | Blocked: after green F4 | Draft design reviewed; no implementation/live grant. |
| — | [N0010 managed opt-in and rollback](0010-f4-managed-opt-in-and-rollback.md) | Blocked: N0009 + green F4 | Draft reviewed for verified maskable package units only; source-staged units refuse, recovery boundaries explicit. |
| 3 | [N0001 F4 source review](0001-f4-settings-completion-audit.md) | Review | Source accepted; Done after correct N2 and published-head CI. |
| 4 | [N0004 activation](0004-f4-activation-progress-and-audit.md) | Done | Exact1382 accepted. |
| 5 | [N0005 network lifecycle](0005-network-owner-lifecycle.md) | Review | Scoped evidence retained; unpublished, integration after F4. |
| 6 | [N0003 network adapter](0003-core-network-state-adapter.md) | Blocked | Broader acceptance unclaimed; old scope evidence withdrawn; no unassigned rerun. |

## Current result and preserved work

Read [eight-arm diagnostic](../research/f4-n0002-eight-arm-diagnostic.md).
All8v12/v13 arms passed quiet/scope/windows, but actual1920×1080scale1/60Hz
violates required2880×1800/1.5 H120/N60. No publicationPASS. Repeateddiagnostic
niri fresh+10.277/+9.798MiB predominantlyQML needs attention atcorrecttarget.
Sol+Terra sourcecomparabilityacceptedfirst4v12+last4v13; oldinvalidcellsseparate.
Rawarchive/hash inreport, no physical-session claims or productoptimization.

Frozen1382a28ab544743e32e906f7542bcecdfdfca083/helper439dd355 remains LOCAL.
Reviewed localmerge d3622384 prepared, mustNOTpushbeforevalidN2/leadapproval.
Baseline5f4/helper4a65 andimmutablecorpora retained. Collector38118f43 exact
SourceCI39136680743SUCCESS. No rebuild withoutcause.

## Ownership and next units

**N0007 source8a Niri pair STOP/RELEASED09:28UTC.** Both cells passed geometry,
quiet and containment; whole-group medians fresh/closed/after A541.409/573.119/581.385,
B547.165/579.240/581.873MiB, deltas+5.756/+6.121/+0.488. QML A495.099/520.727/530.434,
B496.172/529.528/529.334. Standing memory screen FAIL: no Hyprland/full-eight,
no retry, F4 unpublished. Source8a remains correctness-accepted (21 local+2 wire).
Phase favorites12, closed visible12, normalized final six configs/catalog match;
early B geometry favorites0 vs A12 recovered later, startup readiness UNKNOWN.
Wrapper swap-in A4096/B20480B, SwapPss0; no OOM, cleanup18 exact identities,
both cgroups/tmux gone, lock free. Evidence archive940f5f88c57c947fab2c1c20a3c3f4ebac50621325c6ba52ccacd13dfd319a35.

Sol residual review and one bounded Astra escalation found no supported fix.
Escalation reason: Sol corrected source but could not identify residual cause from
matched saved evidence; no speculative product change. Source8a remains clean.
QML construction/GC/native allocation lifetimes are missing discriminators.
First diagnostic package2440a5cf failed first A after2.004s on supplemental
observer identity check; offending PID UNKNOWN, no rollups/gate/B. No app-crash
inference. Cleanup verified; archive57445658849cc72cef311cf511430ffb5fbfc7567af78064b20a95853530815b.
Actual startup Qt GC numeric records were present. No observer repair/retry.
Independent feasibility found the existing frozen census already records PSS/USS,
PID/start and read brackets; supplemental observer is unnecessary for first question.
Logging-only packagea949c63e completed10:38:54UTC; both structural/quiet/safety
checks and input parity passed, no acceptance claim. Lab released. Offline
analysis finds65A/61B complete GC groups, one per phase. Managed use falls at GC
while adjacent USS rises; large private drops have no overlapping GC, delayed
page return unknown. No causal fix supported. See [diagnostic review](../research/f4-n0007-gc-diagnostic.md).
No more captures or source speculation; heaptrack installation requested from
owner for allocation stacks. Keep private heavy jobs paused until install decision;
light Bead B0014 fixture completion/review continues. Source8a remains clean.
Full QML debugger route is rejected
because Quickshell0.3.2 exposes all interfaces with its supported debug CLI.
The Niri pair used frozen v14 unchanged and both arms after owner-installed
ONLY wayland-utils08:08:50UTC. Earlier source0df failed quick +13.413/+15.101/+11.047;
retain its installation-during-B caveat and archive, no cross-pair causal claim.
Bead Wi-Fi integrated021fdaad; exact-headCI39230443828 completed SUCCESS09:37:56UTC,
B0013 Done/docs85f89e9. Its heavy slot released; no lab job. Nacre local integration b003aa8f structurally ACCEPTED,
contains exact8a plus preserved current-main terminal/collector/docs changes;
no source publication until memory gates and exact-head CI. Old merge d362 remains
preserved and superseded for prospective integration only.

V14 previously proved both screen profiles; Niri deltas
+7.343/+10.530/−3.520MiB at correct geometry. H baseline failed swap-in
2,592,768B and later five cells did not run. Archive3aae67b5...d84cd retained.
If measuring-tool execution blocks the owner-prescribed screen, use standard
five pairs per compositor with a separate window, not a v15 tool.


Owner allows overnight. Capture cap is 12 GiB; host >=4 GiB start/abort <2 GiB,
held-child containment and independent guardian, strict whole-wrapper swap-in
<1 MiB and process SwapPss zero remain mandatory. No low-peak stop threshold.
No host pressure, cache drops, sysctl or physical-session changes.

Ownerpriority F4first, switch-onplan, BeadB2/B3, thencoreservices. B0011Done:
sourceaf61d18/CI39147491406green/docs5ad2551, heavyreleased. Owner clarified Oct19
that wayland-utils was first installed on the laptop, then on the desktop; only
wayland-utils changed. Desktop command availability was verified after release. BeadB2frozenruntime1cff
is separatefromnewershellfeatures. No package/tag/release/liveunit/optin granted.

Read-onlypacmansourcefinding: software_ai::snapshot invokes pacman-Sichatgpt on
software-libraryload; standaloneQML15sactive refresh is aprobesource, notproof
ofownerphysicalspikes. Saveforlaterboundedworker; frozenruntimeunchanged.

## Workflow and visibility

Useinstalledlead-workflow/BUILT-INagents; separatechatsretired/noassignments.
Atmost3subagents/oneheavyjob inclhostedfullCI. DefaultLuna; unclear/failedwork
Sol; nontrivialreviewSol/highriskTerra. OneAstrainvestigation diagnosedbounded
within-snapshotcompletion afterrepeatedrealstartupfailures. Sourcefixesreviewed
beforehostuse. LeadstaysWorkingwhileworkersrun, announcesstarts/finishes,
about30minheartbeats. Visibilitypermanentlysavedglobalskill. Specsnotesbatched.

Commit siverteh <92440237+Siverteh@users.noreply.github.com> author+committer.
Docs docs(spec): andNacre[no-publish]. Normalmainpush/no-force/rebaseonlyown;
preserveAIpermissions/parucommand. Docs-only[skip ci] avoidcompetingheavyjobs,
skippednotgreen. RawprivateartifactsoutsideGit. No credentials/personalpaths/
machineinventoryincommits; no killingpeers/Codex/app-server. IncidentCLOSED.
CorrectedN6plan docs/development/f4-switch-on.md, no live execution.

## Morning execution checkpoint — 2026-10-19 07:47 UTC

N0007 source0df0bc14 is accepted, all14 selected correctness checks pass. The
quick-only reservation now ends10:10UTC; unchanged four-arm method, no compression
or automatic retry/full-eight. Staged source verifier passes84files+1deletion.
The private verifier copy changes only its candidate head literal1382→0df;
reviewed SHAab2a4225. All other validation logic and v14 measuring bytes unchanged.
Exact input inventory30326ea0 includes root launch copies; helper439dd355 is a
byte-identical reuse, not rebuilt. Sol owns final launch audit/execution after
Luna staging handoff. No capture arm has begun at this checkpoint: unrelated
SourceCI39213360756 (terminal fix7b18f3ce) passed native regression and is running
gallery; one-heavy rule holds payload until terminal. No peer-ACK wait chain.

Bead B0012Done; B0013local26702487 source accepted after Sol completion/review,
Rust formatting/compilation/tests/Clippy NOT RUN and no source push. It awaits a
separate correctness slot after F4. No Bead heavy job/capture is active. A Luna
read-only explorer maps existing switch-on lock/opt-in hooks only; no live probe.
Pacman probe follow-up N0008 stays parked; do not implement it now.

## Future switch prerequisites — reviewed blocked drafts

N0009/N0010 drafts from16b6e867 independently ACCEPTED for specification only.
Pinned destination leases, safe private anchors under sticky /tmp, ownership-safe
disable preflight, durable write-ahead rollback and downgrade-surviving recovery
are required. Runtime/install roots must be resolved and leased explicitly.
Current bounded fence supports only verified maskable package units; source-staged
user units refuse. Fail-stopped guarantee ends at boot/user-manager restart or
activation. These limitations keep N6 non-runnable until implemented and validated.
No implementation, package publication, install, opt-in or unit action is granted.

## Lead handoff — 2026-10-19 allocation-stack prerequisite

Authoritative current state is this queue, not older grant paragraphs. All Nacre
sessions/scopes/locks released. N7source8a is correctness accepted but valid quick
fresh/closed exceeds5MiB; F4 is NOT published, H/full-eight NOT run. Logging-only
diagnostic and interpretation complete, no supported code fix. Owner approved heaptrack installation and profiling; installed tool verified. Baseline-only profiling capability grant active; no acceptance capture grant.
Local integrationb003aa8 remains conditional and needs latest docs before eventual
publication; never publish without acceptance/exact-head CI. BeadB13 Done at
021fdaad/CI39230443828SUCCESS/docs85f89e9. B14 source8c9e9e1 has two fixture
review fixes assigned to Sol in a fresh worktree; no test/build/CI execution yet.
N9/N10 blocked drafts reviewed; no switch/live/package action. N8pacman remains
parked. Relevant private archives/reports and immutable corpus identities retained.

## Allocation profiling restart — 2026-10-19

Owner authorized heaptrack installation and profiling. Heaptrack1.5.0 verified;
only heaptrack plus kdiagram/threadweaver dependencies installed, no system upgrade.
All prior jobs and hosted workflows are terminal. Sol prepares a standard-tool
allocation profile plan against baseline5f4 and source8a; independent review before
payload. Existing correct geometry, 12GiB/swap0 containment, host4GiB/2GiB guardian
and exact-owned cleanup remain. Instrumented values are diagnostic only. No
acceptance retry, product speculation, live changes or package publication.
Bead B0014 Done: source dcf541a9, exact-sourceCI39240289443 SUCCESS; docs0759bca9.
Duplicate exact-source main CI39241346554 deliberately cancelled, not passed.

Independent heaptrack launch review ACCEPTED. At11:09UTC lead granted Sol one
full frozen baseline A capability run, external25min deadline from actual start.
No candidate B/retry/repair. Verify compression integrity after cleanup; standard
parser/GUI analysis needs separate grant. Trace drain remains unproven because
normal whole-group cleanup may terminate compressor. Instrumented data diagnostic.

## Heaptrack capability outcome — 2026-10-19

First launch stopped in terminal job control introduced by external timeout; no QS
or trace. Removed timeout after independent review, original explicit Bash/tmux
launch restored. Sole corrected baseline reached real QS but original10s IPC
health timed out; no geometry/phase PASS. Trace zstd integrity failed (premature
end), confirming drainage prerequisite unmet. No allocation or acceptance claim.
Exact recorded task identities/scope/tmux/FIFO removed; lock free11:15:12UTC.
No candidate B or retry. Sol source-only reviews standard heaptrack attach to
validated owned QS outside collector killpg, within original capped scope; startup
allocations would be absent. No frozen health/cleanup changes or payload grant.

## Standalone standard profiling grant — 2026-10-19

Attach feasibility was not safe within frozen collector scope teardown. Root
authorizes minimal PRIVATE PROFILE launch lifecycle, leaving acceptance modules
unchanged. Package inventory62c1ae7f independently ACCEPTED after identity/drain
and serialization mock fixes. Reuses exact fixtures, geometry and guardian;
standard heaptrack from startup, same timed actions, diagnostic readiness60s.
Native supervisor is replaced by private lifecycle, an explicit comparability
limitation. No acceptance/PSS inference. One baseline A only,25min externally
supervised; QS-only pidfd TERM, then profiler drain<=30s and compression integrity
before remaining fixture/scope cleanup. B and analysis need lead review.
