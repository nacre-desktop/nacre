# Nacre implementation queue

Current handoff: owner guidance, morning 2026-10-19; supersedes older thread grants.
[Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [N0007 QML memory correction](0007-f4-qml-memory-fix.md) | In progress: memory correction | Source8a independently accepted;21 local+2 wire checks pass. Fresh bindings ACCEPT; Niri-only capture granted09:00–11:00UTC. Hyprland/full-eight require separate grant. |
| 2 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Blocked: product correction | Both v14 geometry checks passed; only Niri A/B valid. H baseline failed whole-wrapper swap-in; no eight-run PASS. |
| 2 | [N0006 switch-on plan](0006-f4-owner-switch-on-plan.md) | Review: prerequisites | Corrected plan retained; F4/package readiness, exclusive installer handoff and managed persistent opt-in/rollback missing. No live action. |
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

**N0007 source8a ready for a fresh quick screen.** Earlier source0df Niri
A/B validators passed, but memory screen failed +13.413/+15.101/+11.047MiB;
Hyprland never ran. Owner installed ONLY wayland-utils on the desktop at08:08:50UTC
during B warmup; preserve the installation caveat, not a clean causal regression
claim. No other packages changed. Desktop prerequisite is now resolved.

Source8a1df35c shares stateless JS imports, avoids duplicate store initialization
reads and rejects stale callbacks across daemon boots. Independent source review
ACCEPT;21 local and2 exact-helper wire checks PASS. Rust/protocol unchanged;
existing helper439 reused, no rebuild. Native-wire scope released after checks;
memory effect remains UNKNOWN. Bead focused correctness cdd9fb2 completed and
released08:39UTC; no other heavy worker is authorized. Fresh source8a quick
bindings independently ACCEPTED. Sole heavy lab grant09:00–11:00UTC Oct19:
Niri A/B entry ONLY, staging allowed before09:00. Worker sol_n7_quick_capture
uses manifest24c9b631 / inventory6f117c58, baseline5f4/helper4a65 versus
source8a/helper439 at unchanged1382 artifact path. All other workers released;
no build/CI overlap. Separate Hyprland resume grant after Niri review; no full-eight.
Stop at invalid quiet/geometry/input gates or out-of-budget growth, no retry.

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
