# Nacre implementation queue

Current handoff: 2026-10-20 lead transfer. F4 and N9 are Done with green
exact-head CI. N10 is local-only and incomplete. The owner requested that this
lead finish the current Bead B2 arm, release the desktop, checkpoint both queues
and stop. The transfer receipt below supersedes older grants and results.
[Standing rules](../architecture/standing-rules.md) apply.

- Structure/licensing decision accepted 2026-10-21: [architecture note](../architecture/structure-and-licensing.md); Bead roadmap and ADR 0004 are in Bead.

| Order | Spec | Status | Authorized next action |
| --- | --- | --- | --- |
| 1 | [N0002 F4 acceptance](0002-f4-whole-group-acceptance.md) | Done | Original8a accepted under owner Niri-debt/Hyprland<=12 rule; all10Hypr valid. Runtime integrated dfcedea7; checker/test-fixture fixes through 9b37c663, CI39402420054 SUCCESS (desktop + fresh-install). |
| 2 | [N0009 installer handoff](0009-f4-switch-prerequisites.md) | Done | Source ab3bcae4, exact CI39431539570 SUCCESS (desktop + fresh-install); 459 shell-tool tests and 534 parity cases pass, frozen references unchanged. Cooperative destination handoff accepted; no live action. |
| 3 | [N0014 F4 switch-lite](0014-f4-switch-lite.md) | Integrated; exact-head CI pending | `nacre f4 status/enable/disable/rollback` on main. Owner desktop needs the full source shell install to deploy daemon units, then a supervised enable. |
| 3a | [N0013 vendor user units](0013-vendor-user-units.md) | Done | `c9e732a2`, Source checks CI success. |
| 4 | [N0016 daemon network controls](0016-daemon-network-controls.md) | Done | `961de0a3`, Source checks CI success. Admission-time `{changed}`, authoritative readback, single sanitized post-admission auth failure (core-services clarification). Private D-Bus fixtures only; live NetworkManager untested. |
| 5 | [N0017 daemon notifications contract](0017-daemon-notifications-contract.md) | Proposed: owner review | Design only (`e1bab666`): daemon as sole notification server/history writer, separate `legacy\|daemon` ownership mode (not tied to N0014), no forced name takeover. Open owner decisions listed in the spec. |
| 6 | [N0018 native keystroke launchers](0018-native-keystroke-launchers.md) | In progress | Luna; parity cases nacre-app/settings/welcome/startup-apps. |
| 6 | [N0019 weather/updates workers](0019-weather-updates-workers.md) | In progress | Luna; producer before consumer; outbound consent unchanged. |
| 6 | [N0020 Brain data layer](0020-brain-data-layer.md) | In progress | Sol; vault integrity, frozen merge references, one-rename rollback. |
| 7 | [N0021 app theming targets](0021-app-theming-targets.md) | Integrated; exact-head CI pending | GTK3 adw-gtk3, Chromium theme extension, Discord (Vencord), Spotify (Spicetify, live reload) with Settings toggles; user guide docs/app-theming.md. dev-1 Arch CI replica green except 6 Niri fixture tests inherited from 37fa77f0 (fix owned by the other lead). Deployed to the owner laptop on request. |
| 8 | [N0022 AI core library](0022-ai-core-library.md) | Implemented; pending lead review | Two phases implemented; both full Arch desktop CI replicas green, 534 frozen cases each, no reference changes. Existing credential stores preserved; no push or live install. |
| 9 | [N0023 Sidebar chat server and composer](0023-sidebar-chat-server.md) | Implemented; pending lead review | Native standalone composer and chat server/client; both full Arch desktop CI replicas green, 534 frozen cases each, 13 fake-provider protocol tests. No reference changes, push or live install; remaining Python boundaries documented; daemon hosting is N0028. |
| 10 | [N0025 desktop adapters A](0025-desktop-adapters.md) | In progress | Native desktop-actions, control-tools, display-recover, device-actions, device-location, screenshot; preserves frozen parity corpus. |
| — | [N0010 full package rollback](0010-f4-managed-opt-in-and-rollback.md) | Deferred: hardening before public release | Preserved on `task/n10-complete-20261020` (phases 1–3, about 22.5k lines, unpublished). Superseded for activation by N0014; resume only for crash-safe package downgrade. |
| — | [N0011 final QML attempt](0011-f4-qml-residency-investigation.md) | Closed: fixes rejected, evidence retained | No more fix hunt; original8a debt accepted. |
| — | [N0012 memory debt](0012-f4-qml-memory-debt.md) | Queued, low priority | No new work now; later phases may not add QML beyond recorded debt without fixing it. |
| — | [N0007 QML memory correction](0007-f4-qml-memory-fix.md) | Done under owner-approved memory-debt decision | Source8a correctness retained. Single-pair veto superseded by owner five-pair rule. No further investigation now. |
| — | [N0006 switch-on plan](0006-f4-owner-switch-on-plan.md) | Review: owner checklist | Updated for N0014 activation-only flow, source full-installer deployment, N0013 vendor units, TTY journal recovery, and manual package downgrade. N10 full rollback deferred. No live action. |
| — | [N0001 F4 source review](0001-f4-settings-completion-audit.md) | Done | Accepted N2 and exact-head CI39402420054 passed.
| — | [N0004 activation](0004-f4-activation-progress-and-audit.md) | Done | Exact1382 accepted. |
| — | [N0005 network lifecycle](0005-network-owner-lifecycle.md) | Review | Unpublished; integration after F4. |
| — | [N0003 network adapter](0003-core-network-state-adapter.md) | Blocked | Broader acceptance unclaimed; old scope evidence withdrawn. |

## New lead — 2026-10-20 evening

Claude Code (Opus) is now lead, with Codex workers. N10 phase 1 is assigned to
one Sol worker on branch `task/n10-complete-20261020`, created from `5a7d5cec`;
the old N10 branch is preserved. Phase 1 covers rebasing onto main with N9,
completing the controller and recovery, and an AC matrix. It has no push and no
privileged or live action, and its tests run on the dev-1 build host so the
desktop stays quiet for Bead B2 captures. Lead scope decisions:
source-staged HOME units stay refused, ALPM hooks stay blanket-refused, and
real-pacman gate 1 is a later phase.

### Lead update — 2026-10-20 night

- N10 phases 1–2 are local checkpoints on `task/n10-complete-20261020` (`41bdb0e0`):
  the branch is rebased onto main, the combined vendor-unit orchestration and
  fresh-process recovery work under a single lock, and 189 + 17 focused tests
  pass on dev-1. Public mutators remain fail-closed. Phase 3, the remaining
  acceptance matrix, is in progress.
- Finding: the Arch package stages core user units into HOME
  (`~/.config/systemd/user`), and N10 deliberately refuses HOME units. The lead
  decided to ship package-installed core units as vendor units in
  `/usr/lib/systemd/user`, migrating only exact package-deployed HOME copies;
  copies with local edits are kept and reported. Source installs are unchanged.
  This is new unit N0013, assigned to a Sol worker on
  `task/n0013-vendor-units-20261020`. It has no push and no live action.

## Lead-transfer receipt — 2026-10-20

- **Publication:** F4 source `9b37c663a35280f3fc66b836fed0940eac646aa1`,
  [CI39402420054](https://github.com/nacre-desktop/nacre/actions/runs/39402420054)
  green; N9 `ab3bcae4ff3e6c126bb8d5efb1636f5196c79c41`,
  [CI39431539570](https://github.com/nacre-desktop/nacre/actions/runs/39431539570)
  green. No package/tag/release/install/live unit or QML action occurred.
- **N10 local work:** worktree `luna-n10-integration-prep-20261020`, branch
  `task/n10-integration-prep-20261020`, clean at
  `5a7d5cecd9de5b1139d0a28898cffea394fcb7f6`; 62 task commits not published.
  Preserve the branch; do not reset/clean it. Local source milestones:
  `71cd82ab` observed pacman-policy confinement, `e63e443f` separate prepare and
  execute requests with durable caller intent before mutation, `4e263b6b`
  hash-pinned recovery import bundle and private package resume, `f8576de6`
  non-actionable rollback-package baseline, `78c2c040`/`d0e806b7` setup tree
  inventory/bound manifest, `ecea997e` fresh-process snapshot WAL reconciliation,
  `5a7d5cec` typed metadata for new successful setup records. Historical setup
  records without `nacre-setup-tree-v1` intentionally refuse; none were rewritten.
- **Evidence limits:** lead independently ran 167 tests through `4e263b6b`;
  subsequent units have worker-reported 180 six-module tests + 15 writer-lease
  tests, Ruff check, py_compile and diff-check passing. Full format-check reports
  differences in existing sections of `tools/releases.py` and its tests. The
  newer snapshot/metadata diffs still need root review and exact integrated CI;
  no complete N10 build, full repository pass or runnable switch is claimed.
- **N10 next:** review all local diffs; finish complete controller, setup/file,
  package, drop-in/unit/activation rollback and fresh-process crash recovery.
  Root helper remains preparation-only and public mutators fail-closed.
  Preserve exact-lock FD lifetime, pidfd, receipt and package-policy constraints;
  finish deployed pacman child lifetime and ALPM hook gates (hooks currently
  blanket-refused). Source-staged HOME units remain explicitly unsupported.
  Reconcile onto accepted N9/main without duplicating F4, run exact-head CI,
  and update N6 before any owner-controlled publication or live action.
- **Push safety:** EVERY new Nacre commit, code/tests included, must contain
  `[no-publish]`; audit `git log origin/main..HEAD --format=%B` before every
  push. Four preserved unpublished N10 messages currently lack it:
  `71cd82ab`, `0501b16b`, `4ec15902`, `b430736a`. Reword only those task-owned
  unpublished commits before any future push; never rewrite published history.
  Do not push the checkpoint branch as-is. This gate is standing and published.
- **Other preserved work:** N5 worktree `nacre-network-owner-lifecycle-20261018`
  clean at `ecd08dfe8a8449001ac499a14ae92a9620d93fc3`, still unpublished Review.
  N3 `nacre-network-state-adapter` clean at
  `212c3efe5215a11fbb8f5a9e50d2b7199248464c`; prior acceptance claims remain
  withdrawn/Blocked. No reruns assigned. Old F4 worktrees are preserved; use
  accepted main rather than replaying old diagnostic candidates.
- **Bead/desktop:** stopped after A4 completed17:37:36UTC. Eight Hyprland arms
  captured (A1–A4/B1–B4), seven controller acceptance markers plus A4 verified
  after the administrative boundary stop; twelve arms unrun. Four-pair partial
  opened-once medians total24.199219→24.709961MiB, anonymous5.253906→6.279297
  (+1.025391); no complete five-pair verdict. See Bead queue for all raw rows,
  archive hashes and stop-hook typo/reverification. Controller, own processes
  and scopes ended; bench-lock free. **Heavy slot RELEASED; all workers stopped.**
  No capture or build is armed; no continuation grant is held by this lead.
  B2 uses the owner's five-pair medians, not a single 24.5 MiB stop: below25 with
  explained categories passes; 25–27 only file-backed extra may use the one-time
  allowance; above27 or unexplained anonymous increase above1 stops. All quiet,
  swap, geometry, scope and watchdog rules remain. Hosted GitHub CI is not a
  desktop heavy job. N10 gets priority for the next desktop build; no old capture
  grant or controller is to be resumed automatically by the successor.
- **Conditional B2 GC work:** no optimization was implemented. Read-only mapping
  found the retired panel VM is dropped, but the retained bar VM receives
  `SceneSource::set_idle(true)` through the owning worker, currently a no-op in
  shell Runtime. That surviving VM is a plausible full-GC hook; malloc_trim is
  already called immediately and after100ms. Verify the mlua API and ordering,
  do not claim this proves the hypothesis. If full Hyprland medians confirm
  opened-once above25, owner allows only this targeted idle-GC attempt, then
  three control/three fixed opened-once arms; if below25 and anonymous growth
  below1, full B2 both compositors. Otherwise stop/report, no speculative fixes.

## Historical results and preserved work

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

## Profiling handoff — standard tool, not acceptance

Standalone A full workload and trace/drain/parse passed. Standard output runtime
833.90s, useful exported Qt stacks, private symbol limitations; Massif totals are
interval peaks and detailed trees are zero placeholders, not phase owner proof.
Plain B stopped before configuration completion/IPC60s; trace events stop1.46s,
not full wall-wait coverage. No paired allocation/memory inference. All cleanup
complete and partials retained.

One brief standard GDB startup snapshot captured main thread read/libunwind/
unw_backtrace/heaptrack_malloc during existing Qt.md5/OpenSSL allocation; IPC
then succeeded after detach. Exact mechanism UNKNOWN. Baseline/candidate have
16 identical Qt.md5 callsites; do not replace hashing or claim OpenSSL memory
cause. Standard alternative unwinder is build-time only; no rebuild granted.

Independent source ACCEPT inventory38ffa207 grants ONE full candidate profile
with the same20s read-only backtrace before settling. Debugger exit/detach and
credential absence gate all workload. Same original scenes/fixtures/geometry and
12GiB/swap0 guardian. External25min supervision; no retries/further profiler
iterations. A had no debugger, explicit diagnostic difference, no PSS acceptance.
Conditional standard parse after all parity/drain/cleanup gates, max15min, same
capped guardian; continuous trace coverage through phases must be verified.
No GUI/diff/source fix/new acceptance run granted by this note.

## Owner workflow update — 2026-10-19

Luna handles almost all subagent work; coordinator performs final reviews. Terra
routing removed; Sol workers only rare documented bounded escalations after Luna
plus coordinator review is insufficient. Astra requires explicit owner direction.
Personal lead-workflow, standing AGENTS and installed roles updated with backups.
Existing F4 profiler worker finishes its current active assignment; ordinary
analysis/execution/fixes thereafter go to Luna. One heavy job remains in force.

## Latest profiling checkpoint

See [heaptrack diagnostic](../research/f4-n0007-heaptrack.md). Full standalone A
and GDB-assisted B workload/trace/parse completed; exact-owned cleanup released.
Luna whole-run diff passed but no supported product fix. Standard Massif owner
trees are empty; phase timing origin uncertain about0.28s, no endpoint claim.
Luna prepares standard GUI prefix[0,T]/Leaked view in private Xvfb only, no new
capture. Extra installed tools xdotool/xclip only; xauth already present, no
system upgrade. No GUI payload grant until coordinator source review. F4 remains
unpublished and memory gate blocked. New Luna-first routing applies to all new
work; previous Sol profiler finished and exited.

Standard GUI prefix analysis source ACCEPTED; one30-minute private authenticated
Xvfb session now granted to Luna. Start with loaded baseline summary screenshot,
root review before any filter/B. Explicit owned display/window input only; original
12GiB/swap0 guardian and exact-owned cleanup, no live desktop. No new profiling
capture. F4 and switch-on remain blocked pending supported fix and acceptance.

## Authoritative handoff — owner decision 2026-10-19 afternoon

Profiling hunt STOPPED; all profiler jobs cleaned up. Baseline noise means a
single pair is no longer the F4 decision. N0002 Ready: unchanged v14, source8a
vs5f4, five alternating pairs each compositor,20 arms; overnight exclusive
20:00UTC Oct19–08:00UTC Oct20. Quiet-only retry once at end for at most2 arms;
other failures stop. New owner <5 / explained5–8 / >8 or QML>2 rule is in N2.
N0009 and N0010 Ready NOW for Luna local implementation, independent worktrees,
no push/live operations; builds/tests serialize before20:00. Integration after
green F4. Bead light source work only during overnight capture, no heavy CI.
Earlier dependency/profiling holds above are superseded by this handoff.

Owner timing clarification: desktop unused rest of Oct19; start acceptance as
soon as prerequisite tests/review finish, no20:00 wait. Exclusive12h from actual
CAPTURE_START, recorded by lead. No timing/gate compression.

Implementation checkpoint: N9 exact5962 compiled and lease tests passed, but
launcher migration regression failed at backup hard-link validation order. N9 also must
pin/lease actual config AND state roots; no source acceptance yet. N10 local
checkpoint67b4393f has38 focused tests/isolated recovery component; controller
mutators still refuse, full package/unit/activation rollback is NOT implemented.
All N9/N10 commits remain local. N2 prospective batch/mocks ready for final review;
no capture payload yet. No further profiling.

## Pre-capture correctness handoff — 2026-10-19 14:49 UTC

N9 local84497fa14328640abc3f06dbdd12bc1b81ba7e9d passed fmt/check,3 Rust
lease tests,15 settings tests,2 pinned publication tests and affected all-target
Clippy;9 Python lease/profile tests also passed. Scope6GiB/swap0 verified, OOM
counters zero, owned scopes inactive/lock free. No full N9 acceptance or push;
installer/release integration/barrier coverage remains for review. N10 local
67b4393f checkpoint38 focused checks passes, still refuses every mutator; full
package/unit activation/rollback remains unfinished. Neither is integrated.

N2 r3 standard20-arm outer schedule accepted after independent mock/report
review. Frozen v14 sampling/geometry/quiet/safety bytes unchanged. Root grants
exclusive12h from actual CAPTURE_START, starting immediately after the above
N9 release; worker records actualUTC/deadline, no historic peer-ACK chain.
All other workers are local light source only; no hostedCI/build/nested overlap.
Report source5f4 vs8a explicitly and per-phase group/component median results;
owner-approved end-only quiet retries/max2 and threshold rule remain mandatory.

## ACTIVE capture handoff — 2026-10-19 14:52 UTC

CAPTURE_START14:52:01.045668UTC; deadline2026-10-20 02:52:01.045668UTC.
Batch N0002-std-v14-20261019-145037Z, task-private controllerPID1611119.
Standard20arms source5f4/8a, unchangedv14; no accepted results at initial check.
Luna N2 worker owns all capture processes/lock/safety; no other heavy job/CI.
Authorized scheduleSHA5720ffe55d4e8a645fb4a9f8697f6b3e6d7cc90083232bef0e44413ee9bb5018;
bindings b569ca0e3f312aa6fcb4558e030f186b4849912fc126c3f5af9bd0e58402de07;
outerdriver d7f01656e2d795afef5899792b173770ea7bee0e88f52eb06566b688005fa57a;
classifier ea66e7ac2e6c73413ed1bff5ef16617a44eb10b2fc581fc4beb6ca1124dea622;
reportparser e3cde2da1d66d3bd44da4fcd5cbf9c4c156be7395ad4392bc0b3013c30935336.
These outer schedule/retry/report adapters do not change frozen samplers.
Original/invalid arm evidence is retained; only permitted quiet-only retry at
end/max2 applies. No source integration before all results and lead decision.

Light workers: N9 installer release hooks in separate local worktree; B15 daemon
audio view source review/fixes. Neither may access lab/startCI/build/capture.
N10 checkpoint is safe unavailable controller + regular-file recovery primitives,
NOT complete enable/rollback. Preserve all local source and tests until review.
If lead restarts, read actual N2 ledger and obtain worker ownership handoff; never
start a duplicate capture or kill another worker’s processes. No more profiling.

### First-arm quiet rejection and continuation review — 15:12 UTC

First niri-r1-A ran824.58s and is INVALID: whole-wrapper host swap-in
exceeded1MiB. No accepted arms; raw results are diagnostic only. Guardian
released=true, rc1,12GiB/swap0 and clean finalACK/OOM0; controller/scope/lock
released. The outer retry classifier incorrectly rejected the expected
RuntimeError:payload failed abort reason for the exact quiet-only payload exit.
Local classifier checkpoint4ab7d7dc fixes only this narrow classification;
coordinator mock independently PASS. Frozen v14 method remains unchanged.
Continuation adapter is under local source/mock review, NOT yet launched:
retain original ledger/raw arm, start next unattempted niri-r1-B and defer A
retry to batch end. Original A consumes one of max2 distinct quiet retries.
Original deadline02:52:01UTC remains; no other heavy work is eligible meanwhile.
N9 release-hook checkpointa4ce3368 has22 focused Python checks PASS; broader
barrier/security acceptance remains unverified. No N9/N10 product integration.

Continuation approved after root mock/source review at checkpoint91036a6d.
Actual continuation start2026-10-19T15:17:46.972UTC, same02:52:01UTC deadline.
Ledger N0002-std-v14-20261019-145037Z-continuation-2f31f17f7bfa838f.json
in original private batch directory. Session
n0002-standard-v14-continuation-20261019-151724Z; first arm niri-r1-B.
Original A failure/evidence/ledger unchanged; seeded retry consumes1/2 slots.
Classifier26d0ef02a041f412efd9c87a205cf76d39c6bebbebf79945517294b023a9d1f8;
continuation079bcca92a965dbd044006b2b49ee37461dae4f8411b1f4c8e70128fd83e203d.
Frozen v14 method/geometry/guardian/source/helper identities unchanged.

Continuation r1-B also INVALID for quiet-only whole-wrapper swap-in:
inner3,846,144B/outer4,067,328B; phase quiet checks otherwise passed, clean
finalACK/12GiB/swap0/OOM0. No accepted arms. Both r1 A/B are deferred at end,
using2/2 distinct quiet retries. niri-r2-A active next; any third distinct quiet
failure stops without retry or a gate change. Diagnostic PSS is not acceptance.
N9 additional local checkpoint3b747e4f extends refusal/nested-lease tests; focused
Python checks pass, native daemon startup/cross-endpoint barriers still pending.

### N2 terminal stop / N9 correctness handoff — 2026-10-19

Third distinct quiet failure niri-r2-A stops continuation. Zero accepted arms,
zero retries executed. Outer swap-in A1=15,962,112B, B1=4,067,328B,
A2=1,789,952B; all exceed strict1MiB. No F4 memory-budget conclusion.
A2 finalACK clean except expected quiet rc1,12GiB/swap0/OOM0; controller,
scopes absent and lockfree. Preserve allraw/ledgers; no new capture grant.
N9 exact local checkpointbf328db4 now owns sole correctness-heavy slot,90min
maximum after START: verified6GiB/swap0 heldchild,10GiB-start/2GiB-abort;
focused native startup/barrier tests+helperClippy/fmt and privatePython only.
No fullsuite/release/hostedCI/live/capture. Record actual workerSTART/result.

Owner fresh N2 authorization after N9 heavy release: same standard20arm5f4/8a
v14, reset quiet-only allowance2; batch/eacharm must see swapused<64MiB.
Scoped owner swapoff/swapon exception only noheavyjobs and available>swap+8GiB,
record free-h beforeafter; failedcondition/sudo unavailable stops for owner.
Outer source/mock prep is light only now; no reset/capture before N9DONE.

N9 focused desktop slot DONE/RELEASED at source8ed20957f02af870b9ad7fb410339e0203091d71,
clean tree6003c687d9277ddc79cfac399815adbe755cd89a. Rustfmt,lease/settings/pinned
regressions,normal/activated startup and native-vs-Python barriers,Clippy and
33 focusedPython checks PASS; broader N9 AC remain unverified.6GiB/swap0
same-child evidence/OOM0/scopesinactive/lockfree; failedfmt/socketassert retained.
Fresh N2 guard root-review PASS: guardbf2ff17fd34f692940f30d248baa5653e53941939d61ff96eb9cb0ee833a588c,
runner0dcb15db554f18ff886d103a8d0c6af2baa5242428f430ea4a5b388b932c4541;
originalschedulerd7f01656 and correctedclassifier26d0ef02 unchanged. Actualrunner
mock and directswapguard script PASS; unittest discovery found0tests, not evidence.
Bothrepos latestCI terminal verified. Fresh N2 soleheavy12hactualstart granted;
remote stage/preflight next, no arm until swap<64MiB and othergatesclose. If
swapresetneeded but sudo unavailable, stop/reportowner/noarm. No secret transfer.

### Restart preflight blocked — 2026-10-19 16:05 UTC

Fresh swap guard verified noactiveN2/build and MemAvailable13,926,928,384B
strictly>swapused644,202,496B+8GiB, but sudo swapoff returned1. No reset, arm,
ledger or accepted capture. Remote swap-audit preserved in
~/nacre-quiet/n0002-f4-restart-20261019T1610Z/swap-audit.jsonl. Launcher16:05:32
is NOT CAPTURE_START. Worker ended; tmuxgone/lockreleased/noN2scope. Owner
notified to run sudo swapoff-a && sudo swapon-a; waitforconfirmation/recheck,
no automatic retry. Soleheavy reservation remains N2; no otherworker running.

Owner requested lead run desktop reset directly. At16:39UTC, exacthost+noCI/
build/scope+lockfree+headroom confirmed. sudo via private credentialstdin only;
swapoff-a/swapon-a returned0 but zram was not restored by swapon-a. Lead restored
existing/dev/zram0 with generatedunit's priority100/discard; capacity15,569,252,352B,
used0, available~12GiB. Audits owner-swap-reset-20261019T163916Z{,-restore}.json
in private remoteartifacts. No cache/sysctl/config change. FreshN2preflight
reverifiedexactsource/staticpins and0/40outputs; narrow outercapacityinvariant
is underfastmockreview beforearm, frozenv14unchanged. Laterresetneededstopfor
lead to avoid disablingzram silently. No payload at this checkpoint.

### Actual fresh N2 payload START — 2026-10-19 16:46:44 UTC

Freshbatchrestart-b actuallyexecutesniri-r1-A; deadline2026-10-20 04:46:44UTC.
Session n2-standard-v14-restart-b-r3 controller1648255, heldpayload1648318,
scope nacre-n0002-standard-v14-restart-b-niri-r1-A-db3df6f0be7f41568f621b324d91d46f.scope.
ProofSHA2d675a576606e70698a0abdca21e0b1be84e585d2d9400c6f26502e33b4f8006.
Guardready/firstsample noerrors,12GiB/swap0, hostSwapTotal15,569,252,352B used0.
Fresh20arms/quietallowance0/2, 5f4vs8a unchangedv14method. Currentpackageroot
~/nacre-quiet/n0002-f4-restart-20261019T1646Z. Earlierrestart-a missing-inner
attemptand malformedproof rejection preserved separately, noacceptedmeasurement.
Outerbatchdf325d866f8266be799c9937e706dc88e5e008b72782d475a727aef744390982;
guarde559937bc1c9ac700290dbbbc08fddc3e438551c1c4f0204ffdf3f83b097a144;
runner0dcb15db554f18ff886d103a8d0c6af2baa5242428f430ea4a5b388b932c4541.
No otherheavyjob/CI. Workerownscontroller/scope/lock; donotduplicateorstopforeign
processes. Anycapacitychange/highswapstopsforlead; thirdquietstop/noextras.

First freshrestart-b baseline niri-r1-A ACCEPTED at17:00:29UTC: wrapperrc0,
acceptedreportvalid, outerpswpin/out0, all3phasewindowsvalid>=60s,
finalACKnoerrors/OOM0. Acceptedcount1/20; candidateB began17:00:29UTC.
Do notmisreadguardian released:true asarmcompletion: liveflagrecordsheldchild
release; requireterminalledger/report/finalACK. No F4memorydecision beforefullbatch.
N10 localcheckpoint62ea3863/privateengine47checksPASS; livemutatorsstilldisabled,
package/setup/recoveryACpending. N9readlinkAPI6f048e60/root15testsPASSnotintegrated.

Firstcandidate niri-r1-B ACCEPTED: outerpswpin/out0, allmandatoryfrozengatesPASS;
count2/20, niri-r2-Aactive. Task-switch windows2/3 incompletebecause7ownedtasks
exited: counter_valid/noerrors/durationsvalid, wholegrouptotalUNKNOWN and survivor
LOWER_BOUND perexistingcontract, notnewgate. Firstpairdiagnostic(groupB-A MiB):
fresh+14.759,closed+15.993,afterlatency+0.492; QML+12.055,+10.998,-2.287.
SinglepairisNOTdecision; finishownerfivepairs/compositor withoutprofilinghunt.
N10local9a522d3e source/fake50checksPASS; pinnedlinkauthoritativeundermask;
realprotocolhealthnotimplemented, productionCLIrefuses, N10notDone.

## Niri five-pair memory FAIL — 2026-10-19 19:10 UTC

10Niri arms valid/noquietfailures. Groupconditionmedian deltas fresh+9.499,
closed+10.098,afterlatency+6.679MiB; QML+5.643,+8.173,+4.799. Allphases fail
ownerbudget. ReportSHA226ec4f6fd14ac7f5faa7811da68a385c74b783ea9924f983980a47f059412e9;
rootindependent30rowrecomputePASS. N2worker directed exactownedcleanup of partial
Hyprlandr1A; no morearms/retries/F4publication. Awaitexplicitcleanuprelease.
N0011Ready read-only Sol escalation justified by prior Luna fixes+heap/source
review failing to explain now-validated QMLresidencygap. No heavyprobe yet.
N10localHEAD3694f4ec prototype/privatechecks only, actualCLIunavailable; HOMEunit
fence/packagehelpers/recoveryintegration remain blockers, no live switch grant.

OwnernewN0011fixgrant19:14UTC: LunaFIRST, Solonlyifstuck, deadline23:14UTC
conservative4hsource+correctness+2NiriABpairs. Trylazytopics→inplacecanonical
snapshot/noJSONroundtrips→inplacemodels. PriorSolread-onlycompletedUNKNOWN/noedit.
Niri5pairfailremains, N2released/lockfree, soleheavyavailableN0011checks/diagnostic.
Ownworktree luna-n0011-js-fix-20261019 base8a. No protocol/schema/nativechange;
all durableownedintent/boot/error/fallbackcorrectnessmandatory. QML<=2/group<=8
in2pairdiagnostic requiredbeforeowner-authorizedfull5pairspercompositor.
N9/N10local/sourcecontinue, Beadlight, nootherheavy/CI. No F4publication yet.

## Current owner final-attempt decision — Oct19 ~21:00UTC (supersedes23:14 cutoff)

Colors94866a37 is parked, not integrated: four valid quiet/safety-clean diagnostic
arms gave group+14.69/+15.40/+18.71MiB andQML+12.11/+14.83/+15.28 perphase.
Raw evidence preserved; no full acceptance qualification. Original8a remains
candidate for final category-led attempt. N0011 Luna owns one private Niri
session each5f4/8a atpanelsclosed: smaps/rollup/kernelbacking, ownedprivate
surfaceinventory, QV4_MM_STATS1/availableQtGCoutput. Guard12GiB/swap0/empty-swap
andsoleheavy remain. Current probe pair running with sharedbenchlock; no CI.

Owner final fix cutoff Oct20 **03:00Oslo=01:00UTC**. If no qualifying fix, owner
pre-approves original8a as tracked Niri memory debt, then requires five Hyprland
pairs on8a with groupincrease<=12MiB before acceptance/integration/exact-headCI.
Qualifying fix still requires2Niripairs(QML<=2/group<=8), then5pairs/compositor.
No live action. N9/N10 source-only/private work continues; N10HEAD795e39be8private
recoverytestsPASS independentlyrerun, CLIstillrefuses andswitchnotrunnable.
N11spec contains exact evidence/source/guard/deadline. Beadlightonly.

## Handoff/current final attempt — Oct19 22:04UTC

Category diagnostic5f4/8a completed withsame1QML64x48window+6layers. Explicit
JSGCHeap:QtQml resident81,616→87,056KiB; Qtallocatedcapacity82,384,896→87,890,176B;
liveafterGC18,399,648→18,395,264B. KernelPSSsinglepoint528,663→527,008KiB;
otheranonymous/THPvariationoffsetstheJSdelta. Bothmapjemalloc/no[heap]; no
blanketanon=JSormemfd=graphicsclaim. NaturalQtdebugGCobserved, literal
QV4_MM_STATSoutputabsentdespiteenv1. Onepairdiagnosticonly, notacceptance.
Fullmirrors underprivatevalidation/n0011-residency-probe-prep-20261019/remote-evidence.

Final narrow fix **67e0ef0ccd6b094d05b0ad78824d8d7d296ab749**, ownworktree
`luna-n0011-statefile-object-20261019`, baseORIGINAL8a, clean. Default-off
NacreStateFileobjectprojectionusedonlyControlPreferences+DashboardStateextras;
normalizeownedarrays/scalars, defaulttext/BatteryAlertsunchanged. No native/schema/
protocolchange/noColors. Local5testsPASS; desktop35daemon+5consumerQtPASS0skip;
3productionQMLformatmatches. Rootreviewedactual40testlogs andunchangedguardian
latestpre-releaseparent/effective12GiB/swap0/PIDproof, parentfinal0events,
finalACKsamePIDticksrc0/done. Summarywriterargbugpreserved; no functionaltestfailure.
Post-stopmissingcg rereadnotclaimedpass. TestsDONE/IDLE/RELEASED, benchlockfree.

**N2worker /root/luna_n2_colors_diagnostic_prep owns soleheavy grant NOW22:04:24Z
throughOct20 01:00:00Z** forfreshNiriN1A/B,N2A/B, baseline5f4/helper4a65,
B67e/helper439unchanged; exactOrient3efe andQSH7796116a...045f. Package:
`~/.cache/nacre/rust-migration-validation/n0011-niri-generic-2pair-prospective-20261019T2202Z`.
Rootindependentstatic/runner/rawreportmocksPASS. Fullsourceoverlaybase5f4→67e
86files+1deletion; manifestdbdd76be285100e5252a605f78e78cc2f6a3a370a71863d752d82e008277ce6c,
tar2ea51a9d5aec9b8f801052b863ba36de1c7e8dcc1fe48190bb77d3a3d144a80f.
Standardv14runtimeunchanged/noGCdebugshims;12GiBheldguard/swap0/start4G/abort2G,
originalSwapTotal15569252352B/used<64MiBbeforeeacharm, strictwholearmpswpin<1MiB,
processSwapPss0. Quiet-onlyretryonceatendmax2;third/otherfailurestops.
NeedallphaseconditionmedianQML<=2/group<=8 beforefull5pairsBOTHcompositors.
Nootherheavy/CI; Beadlight. AwaitactualSTART/PIDfromworker, then4armterminalreport.

Ifnoqualifyingfinalfixby03:00Oslo/01:00UTC, ownerpreapprovedORIGINAL8aNiri
memorydebtusing5pairreport226ec4...e9, then5Hyprpairs≤12MiBgroup everyphase.
UnarmedpreparedHyprfallback:
`~/.cache/nacre/rust-migration-validation/n0011-fallback-hypr-5pair-20261020T0100Z`,
prospective01–05UTC. RootMUSTreview/explicitgrantbeforetransfer/payload.
NeverintegrateColors94866a37; parkedfourarmfailedmemoryevidenceonly.
AfteracceptanceintegrateF4/exactheadCI thenN9/N10; no live switchapproval.
N10localHEADecaf0500 readonlystatus/recoveryprototype30switchtests+8recovery
PASS(rootindependentlyrerun), productionmutatorsstillrefuse/fullACnotDone.
N9local6f048e60 leasehooks/readlink/focusedchecksretainpreviouslimits; notDone.
Allstate localcommits; no productpush/package/install/liveunitactionauthorized.

Actual START for the final fix screen: 2026-10-19 22:07:28 UTC, batch
`N0011-diag-2pair-generic-20261019T2202Z`, controller PID 1820199 / start ticks 31727974.
B is frozen at 67e0ef0c; initial swap used 0 and capacity exact. N2 worker owns the sole
heavy slot and monitors four fresh arms; no GC instrumentation. Await all-four
report before decision; cutoff Oct20 01:00 UTC. The fallback remains unarmed.

N9 source-only review follow-up: `304d28fee60538dd2037760ed981829128688056`
now pins daemon startup and refresh reads to the acquired writer lease; two
private replaced-config-ancestry regression tests added. Root inspected the diff;
Cargo/fmt/Clippy NOT RUN on this delta, deferred until N2 releases heavy slot.
Final diagnostic first baseline arm valid at22:22Z (1/4); candidate arm active.
No memory conclusion until all four; no other heavy job authorized.

### Final fix screen terminal — Oct19 23:02:30 UTC

Candidate67e0ef0c completed4/4 valid Niri arms, zero retries. Root independently
recomputed per-arm sample medians then condition medians. Group/QML deltas MiB:
fresh +7.211/+5.434; closed +5.219/+1.595; after-latency +13.747/+9.795.
FAIL final diagnostic limits; park optimization as evidence, do not integrate.
Raw archive SHA175d8c5422aeec29326b67ecf30e5a1d73719d6defbe308bd90d70715785c944
at private validation n0011-niri-generic-2pair-prospective-20261019T2202Z.
All four scope/quiet guards passed, no source/helper changes.

Owner final-attempt allowance now used; stop fix/profiling hunt. Proceed ORIGINAL
8a as owner-approved tracked Niri memory debt, conditional on five Hyprland pairs
(group increase <=12MiB in each phase), then normal integration/exact-head CI.
Root advances prospective fallback window to Oct19 23:15–Oct20 04:30 UTC, ample
for ten uncompressed arms plus at most two quiet-only end retries. Worker doing
local schedule/source/mocks review; payload still UNARMED until root grant.
No Colors or67e in fallback. N9 new304d28fe local read fix remains untested;
N10 still source prototype, no live action authorized.

Root fallback ACTIVATION GRANT issued after independent validate_prep ALL PASS:
Oct19 23:15–Oct20 04:30UTC, sole heavy worker luna_n2_colors_diagnostic_prep.
ORIGINAL8a/helper439 vs5f4/helper4a, five Hyprland A/B pairs (10arms), unchanged
v14/geometry/safety/paging/swap gates and max2quiet-only end retries. Group<=12MiB
everyphase; QML/daemon/others reported. PeersIDLE/nootherheavy/CI. No new owner
approval chain. Await actualSTART/PID; previous67e capture cleaned.
Schedule030ac1bde532e4767365fc233637d48ae51dbaf9d92112e36a966cc915087e2e,
runner9990c478dc345be8d4016e8bde6e611ffbc5103ed94e827ddadd5a2be13fda91.
No morefixhunt; after this accepted result integrate+exactheadCI, debtfollowup,
N9/N10 source continuation. No liveaction.

Actual fallback START23:15:30UTC, controllerPID1861730/startticks32136159;
exactA5f4/4a vsORIGINAL8a/439, QSH7796116a, hostfb7ab. Preflight available
13240213504B, swapcapacity15569252352B/use19320832B, geometry/source/runtime
passed. Worker owns soleheavy10-armbatch. New queued low-priorityN0012 records
conditional Niri memory debt and no-later-QML-growth rule; NOT F4 accepted yet.

### Morning continuation — Oct20 05:40UTC

Overnight batch stopped before B due swapused>=64MiB; A valid. Root approved
reset completed23:31UTC, originalcapacity restored/use0; audit
owner-swap-reset-20261019T2332Z.json. Continuation review then stalled with lead;
04:30window expired, NO additional arms ran overnight. Do not claim otherwise.
Root renewed same-batch continuation05:40–10:30UTC. Originalledger/schedule/A
evidence immutable, Bfirst/importAonce, fresh continuationledger hardbound.
Adapter4733142e0454fbf872a250da664ec26bd4bdce5880fc3d2512a292bf129f5c6b,
resumeschedule946f6ecffd8d1810342e1e8b862df6aaa1d63262b900a88df83f4869f6b9c869.
Rootmocks/actualbatchstaticchecksPASS; only timeaccessorsoverriddenforexactnew
window, standardv14/runtimeunchanged. ActualSTART05:40:17UTC controller1895596,
Avalid1/10 andBactive; swapcapacity15569252352/use0, available13227134976B.
Soleheavyworker luna_n2_colors_diagnostic_prep, nootherbuild/CI. Finish9remaining
plusmax2quiet-onlyendretries, group<=12MiB allphases, finalrootreview.
No F4 acceptance/integration or live switch claimed.

### F4 memory acceptance PASS / source main — Oct20 08:30UTC

All10Hyprprimaries valid,0retries. Root rawauditPASS: original8a/sourcehelpers,
fullidleactualelapsed>=60, wrapperpswpin<1MiB/processSwapPss0,12GiBswap0,
zero max/OOM and allcleanup. Groupmedian deltas+8.265625/+6.868164/+0.712891
MiB<=12allphases. Ownerconditionaloriginal8aNirimemorydebt now accepted;
no rejected67e/Colors. Full report docs/research/f4-memory-acceptance-20261020.md
and lowpriorityN0012. Source22commits integratednormally main dfcedea76e968bd71c62d7c9ea8c1b81dd68e542,
exactheadSourceCI39393677394 running (notgreenclaimed). No packages/liveaction.
BeadB15audioDone published1e4a2b0 docs source dc8832f+e5fc3d8 CI39390219914green,
noBeaddesktopjob; nextdesktopB2dashboardmemorythenB3typingaftergrant.
N9local304d28fe readpinfix stillNOTRUN; N10productionmutatorsstillnotready.


## 2026-10-20 N10 bounded lock-ownership design review
F4 is Done (9b37c663 / CI39402420054 SUCCESS); N9 full CI continues after
fixture-only corrections. N10 source remains local, fail-closed. Luna identified
a concrete privileged rollback serialization gap: independent user-bundle and
root-helper release.lock acquisition can deadlock, while the root helper must
not trust an unverified caller claim of ownership. Lead authorizes one rare,
read-only Sol design review (no implementation/live/heavy work) to compare a
single lock-owner orchestrator with a verified fd handoff and choose the smallest
contract satisfying N10. This narrow escalation follows Luna analysis and lead
review; it is not a routine expensive review tier.


Owner decision2026-10-20: swap-reset exception is standing for ALL private
captures; see standing rules. Root reset at11:12:59UTC (425MiB→0, original
capacity restored; private audit retained). Bead B2 fresh Hyprland batch granted
11:15–16:30UTC subject to unchanged gates; no desktop heavy overlap. Nacre
N9 hosted CI may continue; N10 remains local source/private Python tests only.

## Lead update — 2026-10-21

- The owner's laptop is updated to main. The `niri-on-pc` experiment drop-ins
  are retired (N0015 `--retire-niri-experiment`), and the control-center
  branch, which had never been pushed, is restored on main (`90723ae1`, plus
  `fea5d143` pill polish and `2f7c223e` launcher focus/favourites). Live Niri
  binds were migrated to the native helper.
- F4: enable worked once the health check polled for readiness (`3c1039d7`),
  but `nacrectl ui` toggles were unreliable in daemon mode on the live laptop
  (not reproduced in nested sessions). The laptop is back in LEGACY mode
  through the documented manual rollback. Rollback scan fixed in `8ff6910c`.
  Next: a clean relogin (the experiment Niri still owns the session
  namespace), then a re-enable test against the exact visible shell PID.
- Testing rule (owner): all tests, nested sessions and builds run on the
  owner's desktop or dev-1, never the laptop. Codex is at its usage limit
  until Oct 27, so workers are Claude subagents meanwhile.

## N0014 activation-only switch — 2026-10-20

[N0014](0014-f4-switch-lite.md) separates a small F4 activation switch from the
full N10 package and setup rollback; the owner approved the split, and the full
N10 branch stays preserved as later hardening. The commands are
`nacre f4 status|enable|disable|rollback`, and the controller is about 800 lines.
It journals the exact prior drop-in and socket state and reverts automatically
on failed health. Rollback works from completed or interrupted transactions, and
every refusal happens before any write. Both package installs (vendor units,
N0013) and source installs (installer-owned HOME units with checked
content/ownership) are supported. Package downgrade remains a documented manual
step after `disable`. Tests: 26 switch tests and 16 release tests on dev-1. A
read-only lead probe against the owner's live user manager found clean startup
inputs; the only blocker is that the daemon units are not yet deployed (a full
`nacre/shell-tools/install.py` run deploys them). Enabling on the owner's
desktop remains a supervised owner step.


N0014 lead correction (`f45cc8b6`, local for review): installer-owned source
units require safe regular files
and bounded environment-content checks, without configuration receipts. Reverted
the configure file-map/adoption changes and their test. Full source install
(without --code-only) stages missing daemon templates plus the helper; diagnostics
and the owner procedure now name that exact installer path. No installer edits,
implementer live probe or push.

Correction validation: 26 switch tests and 16 release tests pass on the dedicated
SSH test host; configure and its test exactly match pre-extension 68d28945.
