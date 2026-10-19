# Nacre implementation queue

Current handoff: owner guidance, morning 2026-10-19; supersedes older thread grants.
[Standing rules](../architecture/standing-rules.md) apply.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [N0007 QML memory correction](0007-f4-qml-memory-fix.md) | In progress: memory correction | Source8a independently accepted;21 local+2 wire checks pass. Source8a quick screen failed +5.756/+6.121/+0.488MiB; no H/full-eight. Sol/Astra cause unknown; reviewed logging-only diagnostic gated grant10:00–10:50UTC, no acceptance run. |
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
Logging-only diagnostic package2440a5cf independently ACCEPTED after one bounded
source correction; no acceptance rerun or forcedGC/tuning. Sole heavy private
Niri A/B diagnostic grant10:00–10:50UTC to sol_n7_quick_capture. Installed binary
pin/options/env proof must be reviewed before payload; strict90s real GC/startup
evidence gate, existing12GiB/swap0/guardian and quiet rules, no retry. Missing
evidence stops UNKNOWN. Output is diagnostic only; H/full-eight remain blocked. Full QML debugger route is rejected
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
