# N0011 — Explain the validated F4 QML residency regression

- Status: In progress — owner-authorized Luna fix time box
- Owner: Luna first; Sol only if Luna is stuck within the same time box
- Source: baseline5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57 vs candidate8a1df35c6fec44e660530643ffef367e6b57ef89

## Evidence and routing

Five correct-geometry Niri pairs are valid with zero host paging and clean
containment. Their phase group deltas are+9.499,+10.098,+6.679MiB;
QML+5.643,+8.173,+4.799MiB. These fail the owner group>8/QML>2 rule.
Local report: ~/.cache/nacre/rust-migration-validation/niri-interim-report-10arm-20261019.json,
SHA256226ec4f6fd14ac7f5faa7811da68a385c74b783ea9924f983980a47f059412e9.
Lead independently recomputed all component/paired medians from30rows.
N2 stops with10valid Niri arms; partial Hyprland is excluded, no F4 publication.

Rare Sol is justified: Luna startup subscription/conversion fixes already produced
8a; coordinator/source review and prior heap profiling found no heap explanation.
Now the full five-pair result establishes a residual QML residency increase.
Owner explicitly permits mapped/rendering investigation only after this failure.
No Terra/Astra, no GUI Heaptrack, allocator/GC/cache/sysctl tuning or new acceptance tool.

## Bounded task

Read existing accepted raw reports, prior heap conclusions and exact source delta.
Separate kernel Pss_Anon/File/Shmem and mappings from invented bucket labels;
never infer a heap or graphics-buffer cause from an unexplained category name.
Check retained QML/scene/window/texture resources, settings snapshot object lifetime
and geometry/window proofs. Do not repeat the already-completed11-to3 subscription
and conversion fix blindly. Produce one causal hypothesis supported by evidence,
or state UNKNOWN and propose one discriminating bounded private probe.
Return the smallest concrete Luna fix task with file:line references, failure mode,
correctness checks and a diagnostic/acceptance validation plan.

Initial grant is local/source/saved-evidence inspection only. No remote payload,
new captures, profiles, builds, CI, source edit, live process/config/unit/package
change or foreign process signal. Wait until N2 explicitly releases the lab before
requesting any necessary heavy probe; lead grants it separately. Own private notes
may be written outside repo; do not alter frozen inputs/raw reports.

Report: cause or UNKNOWN, exact evidence and limitations, proposed bounded fix/probe,
files/lines, checks actually run, and no inference of F4 acceptance. Stop within
one focused investigation turn if no causal evidence rather than broad profiling.

## Superseding owner fix grant — 2026-10-19 19:14 UTC

The initial Sol evidence-only review ended: cause UNKNOWN, no source changes.
Owner now directs one Luna-first QML JS allocation fix attempt, maximum about4h.
Conservative lead wall deadline2026-10-19T23:14:00UTC includes source work,
focused correctness and diagnostic2pair check. Stop/report best validated numbers
if it cannot reach the target by then. No automatic extension or second hunt.
Final full acceptance after a qualifying diagnostic is separately authorized by
owner and may run past the fix deadline. All other capture/heavy work stays paused.

Ownworktree: luna-n0011-js-fix-20261019, base8a1df35c. Local logical commits only,
required author+committer, [no-publish], no push. Native/protocol/schema unchanged.
Try in order:
1. Inventory actual startup topics/callers after8a, subscribe only for always-on
   needs and acquire/release other topic leases with owning panel lifetime.
   Prior11-to3 fix already exists; prove any further lazy opportunity, don't
   invent a claim. Hidden eager components must not create unneeded subscribers.
2. Avoid whole-document stringify/parse roundtrips and dual string/object copies;
   apply valid patches in place to one canonical cache. Preserve existing API
   semantics, error/boot/revision/store ownership and observable notifications.
3. Reuse/update arrays/models in place instead of rebuilding when safe. Avoid
   unrelated UI/renderer changes, explicit GC/allocator/cache tweaks or broad APIs.

Correctness guardrails: operation-owned expected values MUST remain immutable
while a cached snapshot is patched in place. Deep-copy only accepted owned fields
where required; do not share an intent expectation with the live mutable object.
Keep absence-vs-null, queued save durability, clean+error failure, supersession,
new boot/disconnect cancellation, structural equality and no replay/fallback.
Test in-place same-object supersession and notifications, not just replacement
snapshots. Preserve unknown JSON fields and valid patch/array/delete semantics;
no prototype-key/security regression or unbounded retention.

Run existing focused fake-client/QML/store/patch tests and add meaningful regression
coverage. Required existing correctness must remain green; frozen validation/
controller/rollback corpora must not be regenerated. Request sole heavy slot
before remote/private checks; existing desktop route and verified held-child
scope only, no live owner bus/config. Native unchanged helper hash439dd355 remains
unless a new source change requires a separately justified artifact.

Target source/correctness freeze by21:45UTC; don't start diagnostic so late that
four unchanged13min-arm windows plus setup/cleanup cannot fit the deadline.
After coordinator source/evidence review, diagnostic is2 alternating Niri A/B
pairs at exactv14geometry/timing/quiet/safety with empty-swap precheck. Compare
baseline5f4 vs exact corrected candidate; fresh output identities. Require QML
median increase<=2MiB AND group<=8MiB for every phase before full acceptance.
If diagnostic qualifies, run standard5alternatingpairs on each compositor,
unchangedv14/gates. No diagnostic constitutes F4 acceptance. Full acceptance
still uses owner perphase thresholds/daemon explained-cost review/exact-headCI.
Scope12GiB/swap0, host4GiB-start/2GiB-abort, originalswapcapacity and usage<64MiB
beforeeveryarm. If resetneeded use lead's owner-authorized zram-safe reset, no
worker-secret access/config/sysctl/cache changes. N9/N10 may do local light work,
Bead remains light; no heavy/CI overlap. Report exact commit/tests/limitations,
perphase group/QML/daemon/other numbers, time budget used and remaining.

### Bounded attempt checkpoint — Oct 19 19:35 UTC

Luna candidate `94866a374388cd3796a96ed2eb73a2b04d56b492` (parent
`caaf74d85010bb01285aef839cd0c3c98195b8c1`) makes Colors subscriptions lazy
while closed and non-tide, gates opening on fresh valid state and releases its
retained projection. Global in-place cache changes were rejected during review
because retained compositor record aliases require immutable update semantics;
they are absent from this candidate. Native/protocol/schema remain unchanged.

Focused private Qt/helper run on caaf74d8 passed 36/36, zero skips with frozen
helper439dd355. Final94866a37 changes formatting only; local lease test passes.
Logs mirrored in private validation cache `n0011-colors-focused-caaf74d8-local-evidence`.
No memory improvement or F4 acceptance is claimed. Two alternating Niri pairs
against5f4 remain required; prospective outer orchestration is being reviewed.
The four-hour deadline remains23:14UTC; source-only N9/N10 continue independently.

Operational limitation: the focused correctness process verified its effective6GiB/swap0 cgroup, but the worker did not record the required same-held-PID parent proof before release. That launch-gate proof is UNKNOWN and this run is not claimed compliant with that operational requirement. The36/36 functional outputs remain separate. Future diagnostic must use the existing reviewed held-child guardian and retain the complete pre-release proof.

### Diagnostic active — Oct19 20:00UTC

Exact corrected source94866a374388cd3796a96ed2eb73a2b04d56b492, nativehelper439dd355
unchanged, baseline5f4/helper4a65. The final focused repeat passed36/36 through
the unchanged reviewed held-child guardian, with parent/effective12GiB/swap0,
exactPID membership pre-release and finalACK preserved. This closes the earlier
missing launch proof; two temporary harness final-handshake failures remain
preserved and excluded from operational-pass claims.

Fresh diagnosticN0011-diag-2pair-20261019 actualstart20:00:15.6Z; controller
PID1750522/startticks30964700, tmuxn0011-diagnostic-2pair-20261019. Four arms
Niri1A/B then2A/B, unchangedv14, no builds/CI overlap. Originalswapcapacity
15569252352bytes andused0 verified beforebatch/arm1. Hardcutoff23:14Z.
Candidate fullarchive67d5b8dff08043883b99094fb1c4360b1a1e7daaee9acbc0aee2255c94d51f64;
manifestfa079ccd6059c510089c1c1e64e5168a96c74ea66e705fbbd9d074d74d17c946.
No accepted-memory conclusion yet.

The20:00diagnostic stopped after validA1 (hostpaging0/0) and B1 setup failure:
an unresolved product-directory placeholder in the outer invocation reached
unchangedv14 and caused FileNotFoundError before measurement. Neither a memory
regression nor a quiet-only retry. Evidence preserved; no laterarms ran.
Root-reviewed fresh outer package rejects placeholders in every selected
primary/retry script and verifies candidate B argv. Its scheduler/static/report
mocks passed independently. Runtime measuring/geometry/guardian remain unchanged.
Fresh batchN0011-diag-2pair-rerun-20261019T2015Z actualstart20:19:06.9UTC,
controller1760818/startticks31077831, tmuxn0011-diagnostic-rerun-2015.
Allfourarms are fresh; priorA1 is not reused. Initialswapused0 andcapacityexact.
Hardcutoff23:14UTC remains. Resultpending.

Escalation rationale20:55UTC: Luna completed step(b) bounded audit without a safe persistent-copy removal; coordinator review found immutable-projection and durable-intent contracts prevent blindly removing copies. Owner explicitly permits Sol only when Luna gets stuck. One20minute Sol source-only escalation may identify a narrow safe allocation fix or conclude none; no profiling/tool changes/heavy overlap and deadline23:14UTC unchanged.

## Owner superseding final-attempt decision — Oct19 ~21:00UTC

Finish current2Niripairs first. If Colors optimization is worse, park it as
evidence only and revert to original8a for next diagnostic. If it meets every
phase's group<=8MiB/QML<=2MiB limits, proceed directly to full5pairs/compositor.

Otherwise measure first: about1hour, one private correct-geometry Niri session
each baseline5f4 and original8a, at panels-closed compare complete owned-QML
smaps/rollup, [heap]/anonymous/shared-memfd-dmabuf/file-backed categories, exact
private windows/layer-surface identities/sizes, QV4_MM_STATS=1 output atGC where
available. Anonymous/kernelbacking alone does not establish JS/graphics ownership;
retain unknowns and absent statistics. No GUIheaptrack. Fix only the category
that grew; if unclear bisect QML commits5f4..8a with3panels-closed repeats/step.
Smallest safe change, correctness green, no protocol/schema change, Luna first.
Check2alternatingNiripairs, then full5pairs/compositor ifgroup<=8/QML<=2 perphase.

New fix cutoff is Oct20 **03:00Europe/Oslo=01:00UTC**, superseding23:14UTC.
If no qualifying fix by cutoff, owner pre-approves original8a as tracked F4
memory debt using validated Niri5pair evidence: QML+4.8..8.2MiB, group+6.7..10.1.
Then run5Hyprlandpairs on8a; accept only ifHyprlandgroupincrease<=12MiB.
Otherwise stop/report. After acceptance integrate normally, exact-headCI, then
N9/N10. Recorddebt/lowpriorityfollowup; laterphases may notincreaseQML beyond
recordeddebtwithoutfix. No live action authorized. Existingempty-swap/safety
andsoleheavy gates stay; Beadlight. Currentdiagnosticcontroller's23:14cutoff
neednotchangebecauseits fourarms finishbeforethen.

### Final Colors diagnostic outcome and category-probe grant

All4fresharms accepted withhostpaging0/0 andclean12GiB/swap0 guardian, no
retries. LocalaggregateparserhadNameErrorresultinfinalgatingfield; local-only
repairpreservedrawreports. ReportSHA16e41937eaa6389fdbdfa729274b626ddb21f31944da54038bd2ecf2569cc03a.
ConditionmedianB-A MiB(group/QML/daemon/other): fresh14.6865/12.1147/2.0952/.4229;
closed15.3999/14.8291/.4346/.4165; afterlatency18.7090/15.2781/-1.1826/.4316.
Everyphasefailsgroup<=8/QML<=2. Colors94866a37 isparkedevidence-only, not
integrated; differingbatchbaselinesmeanthisdoesnotbyitselfproveColorsthecause.
Rootverifiedreportarithmeticandreviewedthefourarmraw/guardmirror.

N0011categoryprobe grant21:25–22:25UTC, A5f4 thenoriginal8a, separateprivate
Niri sessions, sameQSH/usr/bin/quickshell SHA7796116aa1bf0139df37d023feba9e25ac02650eca678dc2bd4d23e1f09a045f.
Diagnostic-onlycopiedcollectorinsertsoneexternalhookafterclosed60ssampleand
beforenextaction; acceptancev14unchanged. QV4_MM_STATS1 plusQt6GClogging,
completeQMLsmaps/rollup/fdinfoandprivateNiriWindows/Layers; absentstats/sizes
remainUNKNOWN, kernelbackingisnotallocator/GPUattribution. Rootindependent9
unitmocktests+5generatedpathassertionspassed. Ownerempty-swap/strictpaging/
processSwapPss0/12GiBheldguard/swap0/host4Gstart2Gabort/soleheavy preserved.
No payloadbeforeverifiedsource/binary/cgroup/memberidentity.
