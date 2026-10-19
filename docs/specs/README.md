# Nacre implementation queue

Current handoff: owner guidance, morning 2026-10-19; supersedes older thread grants.
[Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [N0007 QML memory correction](0007-f4-qml-memory-fix.md) | Ready: one Sol worker | Correct-size Niri +7.343/+10.530MiB; investigate subscriptions/double conversion and data parity; source fix now, no capture/tool changes. |
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

The heavy window has ended. No capture or heavy job is active. V14 proved both
screen profiles; Niri baseline/candidate passed, H baseline failed swap-in
2,592,768B and later five cells did not run. Archive3aae67b5...d84cd retained.
Niri fresh/closed/after deltas+7.343/+10.530/−3.520MiB at correct geometry.
Owner directs one Sol QML fix; no more captures now and no v15 tool. Then one
quick pair per compositor, then full eight-arm confirmation once. If measurement
tool execution prevents that, use standard five pairs per compositor instead,
with a separately granted window. Bead restarts independent LIGHT preparation.

Owner allows overnight. Capture cap is 12 GiB; host >=4 GiB start/abort <2 GiB,
held-child containment and independent guardian, strict whole-wrapper swap-in
<1 MiB and process SwapPss zero remain mandatory. No low-peak stop threshold.
No host pressure, cache drops, sysctl or physical-session changes.

Ownerpriority F4first, switch-onplan, BeadB2/B3, thencoreservices. B0011Done:
sourceaf61d18/CI39147491406green/docs5ad2551, heavyreleased. Ownerinstalled
wayland-utils; oldmissingdependencyblocker superseded. BeadB2frozenruntime1cff
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
