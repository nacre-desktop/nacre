# N0002 — F4 whole-group acceptance and publication

- ID: N0002
- Repo: nacre-desktop/nacre
- Implementer: NACRE
- Status: In progress (built-in Luna; active grant below)
- Depends on: N0001/N0004 source-ready review; explicit slot confirmation; final Done after correctness
- Date: 2026-10-17
- Size: one focused session, maximum 4 hours

## Goal

Finish the existing F4 whole-group comparison and integrate only a candidate
that meets the owner's regression rule. Preserve already valid captures and
passing source checks.

## Context

Published baseline5f4c0bf3; existing F4 receipt44ba7afa/helperabdfc528 is unpublished.
Reported current gate uses fixed600s warmup +60s idle and has a first niri A/B
pair; these reports require artifact inspection, not assumed acceptance. Read
N0001, architecture/standing-rules.md and development/rust-migration.md. Existing
frozen Wave3 and pending physical desktop A/B/A/B are entirely separate.

## Scope

In: current private F4 captures, whole-group/per-action analysis, docs/report and
normal integration once correctness/gates pass. Out: product optimization during
capture, new tooling layers, live deployment, daemon-unit enablement, new services.

## Steps

1. Return exact baseline/candidate source and binary hashes, capture host scope,
   active controller PID/session, raw evidence path and estimated remaining time.
   Preserve ongoing valid runs. The spec agent grants exclusive continuation
   naming these hashes; no new capture can infer approval from this document.
2. This spec explicitly uses the existing matched A/B/A/B sequence (two A/B
   pairs per compositor), rather than default five pairs, to preserve the fixed
   noisy-baseline plan already in progress. Every arm gets600s warmup and a full
   60s idle window. Complete both Hyprland virtual120Hz and niri nested60Hz at
   2880×1800/1.5 with identical HOME/catalog/palette/settings script per arm.
   Do not replace a valid arm merely because its result is disappointing.
3. Measure QML + daemon + every helper as one group with stable PID membership.
   Record individual PSS and group sum, peaks, anonymous/file/shared changes,
   CPU/call and process launches. Include startup/read/set and cold CLI latency
   as separate paths; do not hide a repeatable cold-launch regression behind
   lower helper CPU. Last niri553.52→552.46MiB and395.79→429.06ms are preliminary.
4. Validate every arm: process SwapPss0, host swap-in<1MiB, page size/counter
   provenance, full windows and exact artifacts, no overlapping build/test job.
   Exclude invalid runs explicitly and request time for replacements. Report all
   run values, medians, min/max spreads and peaks, not only deltas.
5. If cold CLI or another endpoint repeats worse beyond control variation, stop
   publication and send evidence for a bounded fix spec; no new warmup/threshold
   trick. If baseline noise remains inconclusive, label unknown and ask spec agent.
6. On N0001 correctness + this gate PASS, commit as approved identity with
   [no-publish], integrate directly onto current main using only own task commits,
   preserve concurrent docs changes, push normally, inspect every applicable CI
   job. Update report with exact source/artifact/capture-to-commit mapping.
   Return Review; no live opt-in or install. Release measurement slot explicitly.

## Measurements

Only the above fixed600s+60s two-pair exception. No host pressure/cache drops,
physical tests or altered immutable baseline. Missing evidence is unknown.
Restart≤1s private evidence and existing CPU receipts may be reused when their
source bytes match; no rerun of passing builds without a reason.

## Acceptance criteria

- [ ] Four valid alternating arms/compositor with exact hashes/windows and quiet.
- [ ] Whole-group total equal/lower and component increases explained, OR total
  increase strictly<5MiB explained and tracked. Otherwise STOP.
- [ ] No reproducible endpoint regression outside standing rules; cold CLI repeats
  explicitly assessed using comparable controls. No performance claim if unknown.
- [ ] Full F4 correctness/source evidence reviewed, all migrated writers accounted.
- [ ] Normal main integration; complete published-head CI green; no release/install.
- [ ] Raw artifacts retained, report concise and slot released with cleanup proof.

## Stop conditions

Incorrect hash, overlapping measurement, quiet failure, repeatable regression,
missing group member or uncovered Settings writer: stop and report. Do not expand
sample count/window/scope unilaterally or spend a memory allowance on speculation.

## General rules

Apply the linked standing-rules.md in full. Author and committer use the approved
GitHub noreply identity; no private paths or inventory. Isolated worktree, direct
normal push to main, inspect outgoing history, check CI after every push; rebase
only own commits. Commands nice -n 10; Cargo -j2; private/nested tests only.
No live desktop/install/config/keybinding/autostart/unit changes, package/tag /
release publication, permission-policy changes, or paru skipreview changes.
Stop only owned PIDs; no host pressure/cache drops/kernel changes. Nacre commits
include [no-publish]. Framework remains permissive, publish=false, backends frozen
except fixes; no launcher engines in Bead. Preserve immutable fixtures. Extras
are report-only. Any new security advisory goes to the spec agent; vulnerabilities
go to the owner. Measurement requires an explicit exclusive slot grant.

## Report format

Send spec ID/status, start and final full commit IDs, changed files, each acceptance
criterion with PASS/FAIL/NOT RUN and command evidence, published-head CI URLs,
protocol/artifact hashes where relevant, deviations/blockers and raw private report
locations (never commit private paths). For captures include per-run numbers,
medians/min/max/peaks, quiet counters, attribution and owned-PID cleanup. Missing
is unknown. Ask the spec thread directly; do not improvise or ask the owner for
routine design. Submit Review, then start the next eligible Ready spec.

## Revised-candidate clarification — 2026-10-18

The v25 candidate and its four valid partial cells are historical evidence only.
N0001v7 changes runtime and N0004 fixes remain required: NO measurement grant for
either. The next approved candidate requires a FRESH complete eight-cell batch
(A/B/A/B per compositor), each arm600s+60s as already specified. Do not combine
v25 cells with a revised candidate or treat old partials as acceptance.

Retain runtime baseline5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57 and candidate
based on it for now: current upstream delta at review is docs-only. No dirty-tree
rebase solely to add docs. The exact revised manifest/helper and baseline helper
hashes plus private host scope/controller/time window are still required before
explicit grant. If upstream gains runtime changes before final review/integration,
report them and obtain a compatibility/measurement ruling; no silent substitution.

## Capture safety — restart rule, 2026-10-18

This section supersedes earlier capture windows. No capture starts until incident
review is reported to the owner and the spec agent issues the replacement grant.
An unexplained simultaneous agent shutdown is not proof of OOM or a safe retry.

- “Stop other jobs” means pausing your own builds and asking the other implementer
  to pause theirs. Never kill a process you did not start. Never signal or restart
  Codex/app-server processes. An expired window is not proof of host idleness;
  require explicit DONE/IDLE/RELEASED before another task starts.
- Before each arm: no builds running, peer pause acknowledged, and MemAvailable
  at least 4 GiB (4,194,304 KiB in /proc/meminfo). Record the value. During the
  entire warmup and capture sample MemAvailable at least once per second; abort
  and invalidate the arm if below 2 GiB (2,097,152 KiB), or if monitoring fails.
- Run the harness and all measured descendants (private compositor, UI, daemon,
  helpers and input driver) inside one new private systemd user scope. Set an
  explicit MemoryMax well above expected peak: 2 GiB for the existing nested
  lanes, expected below 1 GiB. Verify the effective cgroup limit before any arm;
  no unlimited fallback. If expected peak cannot fit with generous headroom,
  stop for spec-agent sizing instead of silently raising it. Scope setup is
  transient and task-private, not a live unit/configuration change.
- Keep the lightweight memory watchdog outside that measured scope, but task-owned.
  On low memory, scope OOM, unexpected exit or lost watchdog, invalidate the arm
  and stop only the recorded owned scope/descendants. Preserve partial logs,
  cgroup memory.events/peak and the reason; never retry automatically or count
  aborted samples as a valid cell. Do not include Codex/app-server in the scope.
- Verify containment and watchdog behavior with a bounded mocked check before
  measurement; do not deliberately exhaust RAM or induce host memory pressure.
  Fold the small guard and regression tests into the measurement spec, not a new
  tooling project. Report scope name, limit, start/minimum MemAvailable, guard
  result, exact artifact identities and owned cleanup with ordinary gate evidence.
- Inspect kernel and user journals from 2026-10-18 02:30 with explicit timezone
  and retained-boot coverage. Report OOM/kills, agent/app-server exits and the last
  owned job without touching those processes. If evidence points to another
  cause, fix it and tell the owner before re-granting. Missing old journals mean
  unknown, not no OOM. Never change global OOM policy, swap, sysctl or caches.

## Active final-candidate grant — 2026-10-18

N0004 Done at1382a28ab544743e32e906f7542bcecdfdfca083. Bead explicitly released
the private lab due to an installation-permission prerequisite. Prepare only
the release helper from this exact commit+Orient3efe8d1 on the existing isolated
build tree, nice10/Cargo-j2,2GiB private scope and >=4GiB available. Record peak
and OOM counters; report cap failure instead of automatic retry/limit increase.
No broad correctness rerun. Never reuse eee973 or another old candidate helper.

Exclusive N0002 private nested capture grant Oct18 12:30–14:30UTC on the same
previously verified lab. Baseline source5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57,
release helper4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa;
candidate1382a28 release binary whose freshly built SHA256 must be recorded and
sent with CAPTURE_START before first arm. No additional permission wait once
these prerequisites pass. This grant binds that exact source/profile and actual
reported artifact; it never permits unrecorded substitution or mutable builds.

Fresh8arms, A/B/A/B per compositor,600s warmup+60s idle each;2880x1800/1.5,
Hyprlandvirtual120/nirinested60. Preserve mandatory private2GiBscope/verified
containment, externalwatchdog,4GiBstart/abortbelow2GiB,quiet/OOMcounter gates
and peer pause. Baseline/payload/method unchanged, no historical partial reuse.
No builds while measuring. Stop if remaining time cannot fit whole batch; tell
spec agent BEFORE first arm for immediate adjusted window, never compress it.
Explicit DONE/IDLE/RELEASED transfers host; Bead cannot preempt a valid arm.

N0001/F4 publication stays held pending this gate and source review; do not push
local runtime commits yet. If gate fails, return exact evidence for bounded
fix; independent N0003 source remains eligible.

The revised preparation window through12:30UTC is a full build-only grant, not
the remainder of N0004's expired correctness slot. Candidate1382a28, existing
toolchain, private2GiBscope and OOM/memory checks remain mandatory. Return exact
release hash before CAPTURE_START. If blocked, report immediately; N0003 can
advance after explicit release instead of waiting idle.

## Gate review — blocked, 2026-10-18

The first fresh Niri baseline arm ran12:07:34–12:19:34UTC and was INVALID.
Controller stopped before arm2, no retry/substitution and no valid comparison.
Do not interpret its latency/PSS as accepted performance. Final candidate
1382a28 release helper439dd355fca276334c2faccbd0992d1b23eac6c6caab311ca1c1c61d3672e92c
and5f4 baseline helper4a65f4ec were correctly bound. Scope2GiB,peak1687920640B,
OOM/maxevents0, available memory start/min13640980/12530024KiB. Safety held.

Host-global pswpin invalidated quiet: internal1281pages*4096=5246976bytes;
outer start-to-cleanup1428pages=5849088bytes; pswpout0. Both exceed strict<1MiB.
No inference that the measured group caused the host reads. High available RAM
does not rule out retrieval of earlier swapped pages. Raw artifacts preserved.
Controller/session/scope ended and lock free at12:19:54UTC.

All prior N0002 capture grants are stopped; NO retry grant. Before considering
a new full batch, review EXISTING evidence/scripts for exact paging-counter
window boundaries relative to warmup/idle and any already-recorded process
fault timeline, max20minutes. If attribution is absent, report UNKNOWN. Do not
change the window/quiet threshold, drop caches, change swap/sysctl, stop other
processes, or launch another measurement to reconstruct this arm. Return concise
finding/quiet-readiness proposal; spec agent decides next gate.

N0003 correctness is the eligible independent work now; no publication of F4
until N0002 passes. Preserve final release artifact for future same-source gate.

## Eligible source-only collector correction — 2026-10-18

This phase is Ready now while captures are Blocked; maximum90minutes. Existing
saved evidence shows internal pswpin spans before harness main through scenarios
and cleanup, including1063pages during600s warmup. fresh_static and
all_panels_closed collect1s; static_after_latency collects60s. Attribution beyond
host-global counters is UNKNOWN; existing rows lack per-process major faults.

1. Preserve all v3/v4 scripts, reports and invalid arms. Make a separately named
   prospective collector revision from the existing private N0002 cell scripts,
   or the existing repository collector if that is their actual source. Report
   exact touched files; no product/helper/protocol changes, new tooling framework
   or frozen artifact mutation. Final helper439dd355 remains unchanged.
2. Retain600s warmup. Collect full60s uninterrupted idle in EACH existing
   fresh_static, all_panels_closed and static_after_latency phase; preserve
   actions/latency ordering outside those idle windows. No one-second substitute
   or marker-only wait. Collect /proc/vmstat pswpin/pswpout immediately before and
   after EACH idle observation; record page size, elapsed duration, counter values
   and deltas. Keep warmup and whole-arm start-to-cleanup totals separately.
3. Preserve strict CURRENT rejection on whole-arm hostswap-in>=1MiB and process
   SwapPss>0. New per-window data is additional evidence, NOT permission to waive
   paging, retrospectively accept old arms, change thresholds or run a new batch.
   Keep guard4GiBstart/2GiBabort,scope2GiB,OOM checks and exact artifact binding.
4. Add deterministic stdlib fake-clock/counter tests alongside existing collector
   tests: all_three_idle_phases_observe_full_sixty_seconds;
   paging_windows_keep_warmup_and_idle_separate;
   paging_delta_uses_recorded_page_size;
   whole_arm_paging_failure_remains_invalid;
   incomplete_idle_or_counter_reset_is_invalid. No real sleep, subprocess,
   compositor, /proc sampling or memory pressure in tests. Preserve counter
   validation and cleanup/ownership assertions.
5. Run those mocked tests,py_compile and diff-check locally. Report source diff /
   hash, tests and revised COMPLETE8arm duration including all new full windows
   and transitions; do not assume two hours still fits. Return source Review.
   This is folded into F4 gate work, no separate tooling spec or runtime rebuild.

Acceptance: correct explicit full60s windows and separately reported counters,
strict existing rejection preserved, mocks pass, no measurement. Stop if current
collector cannot be changed in90min without a newframework; report exact gap.
After review spec agent decides quiet-window interpretation/readiness and a
future full-batch window; no capture grant is implied here. No cache/swap/sysctl
changes or stopping unrelated/live processes.

## Current owner-priority execution grant — supersedes all older windows

Fresh worker owns this spec only, with its own isolated task worktree. Baseline
5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57/helper
4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa;
candidate1382a28ab544743e32e906f7542bcecdfdfca083/helper
439dd355fca276334c2faccbd0992d1b23eac6c6caab311ca1c1c61d3672e92c,
Orient3efe8d1e7fb50200d74a3e0193ad63699cb8ebe8. Do not rebuild or substitute.
Coordinator grants2026-10-18 15:30–18:30 UTC exclusive private-lab capture window,
recorded machine digest fb7ab717874d5e6493f6e30fadcc8b5c01ab65268a56c6256261a8df387a938a.
No arm before source/mock readiness, peer IDLE acknowledgment and no other heavy
job. Verify collector38118f43 exact-head CI39136680743 conclusion first; task-caused
failures require correction before activation. Driver v5 remains source-only
until worker verifies corrected actual-wrapper mock proves ONE600s warmup per
arm and THREE60s idle phases; minimum104min for8arms before transitions/actions.
Use fresh distinct output paths, no historical cells. Order N1A/B,H1A/B,H2A/B,N2A/B.
Hash-pin the exact collector and driver/support files, preserve old scripts.
Record separate warmup, each idle phase and whole-wrapper paging boundaries;
whole-arm and start-to-cleanup swap-in>=1MiB both reject. No counter/window waiver.

Each arm runs in verified private2GiB user scope/swap0, with parent effective
limit/held-child containment proof before same-PID payload release; external1s
monitor,>=4GiB MemAvailable before release, abort below2GiB. Stop only owned scope/
PIDs. No other build/test/nested job during capture. Invalid first arm stops batch;
no automatic retry or baseline laundering. Report promptly and retain evidence.
If ready too late to finish uncompressed inside window, report before starting.
Report CAPTURE_START with source/profile/binary/driver hashes, session/controller
PIDs, UTC deadline. At completion assess whole-group and endpoint rules; send
Review to coordinator immediately. Publication review follows; no live install.
Private source/mock fixes to prospective driver allowed within this spec; no
product optimization during measurement, no new tooling framework or dependency.

## Active built-in worker grant —2026-10-18 16:15–19:00 UTC

This section supersedes the15:30–18:30 separate-chat grant; that worker is retired
and may not start a payload. N0002 is In progress. Source/mock v6 guard accepted
by built-in Sol reviewer AND Terra second opinion: launcher
0cc8dd37b614073be5ce46c76ffe1d98682ef16207ba15aff0ff44b1d427d00b;
tests b66ca2a2e5818281669e06b9a9a1c352f8b3111ffd1e447699e25a39fc0786df;
plan f18f82b0519f9aeebc68ca69ecb7967152433e3925e2bb0074360bee0f036c2c.
All24manifest entries,23actual-launcher/guardian mocks, original11driver mocks,
Python syntax and18shell syntax checks passed. This is source/mock acceptance,
not proof of real systemd behavior. No runtime/helper rebuild requested.

A fresh BUILT-IN Luna worker owns one heavy slot for private transfer/preflight,
ONE small no-pressure containment smoke, then the fresh eight-cell batch within
16:15–19:00 UTC, only while no other heavy job runs. Both retiring peers explicitly
ACKed IDLE; Bead39142524705 finished FAILURE, not acceptance. No new Bead heavy CI
until N0002 explicit release. Collector38118f43/CI39136680743 SUCCESS unchanged.
Use the same recorded private lab machine digest and frozen baseline/candidate /
helper/Orient identities above. Verify every transferred source/support pin.
Do not overwrite old evidence or frozen v3/v4/v5 scripts; v6 uses distinct paths.

Read reviewed REPORT/SHA256SUMS from the owned private v6 directory. Scope smoke
must exercise held-child gate, final-ready/ACK and guardian teardown using a tiny
shell payload, not a compositor or memory pressure. Require numeric2GiB/swap0,
parent/child membership proof before release, final zero OOM/valid events and
peak, clean owned cleanup. Failure stops; report before another attempt. Smoke
success is not a measured arm and cannot replace quiet/performance evidence.

On successful smoke, announce exact CAPTURE_START and run all8fresh arms in the
frozen order, each ONE600s warmup plus THREE60s idle phases.104min fixed waits
plus transitions/actions; do not compress. Both processSwapPss0 and hostswap-in
<1MiB over collector AND whole-wrapper intervals remain mandatory. First invalid
arm stops entire batch with no automatic retry. No accepted latency/PSS claim
from invalid arms. Keep independent guardian1s memory/scope/OOM/owner checks;
>=4GiB available before release/abort<2GiB. Parent and guardian record independent
final events/peak before bounded5s finalhold ACK and before teardown. Guardian
abort including at normal completion invalidates; evidence-write failure must
never prevent own-scope cleanup. No other process/cgroup may be stopped.

After complete valid evidence, send Review and DONE/IDLE/RELEASED with artifact
bindings, per-run values/medians/spreads/peaks and whole-group/endpoint gates.
No runtime push until lead review; no live install/package/unit/config action.
No payload beyond19:00; if not ready with enough full-batch margin, report rather
than begin an unfinishable partial. Do not add tooling/product changes midcapture.


## Owner-sized replacement grant — 2026-10-18 18:00–22:00 UTC

Supersedes the expired v6 grant. The v6 first baseline arm was stopped during
warmup for a planning-headroom concern, not OOM or a measured paging failure.
Zero cells accepted; preserve its raw evidence. Scope peak 1,566,932,992 bytes
was below the already recorded baseline peak 1,687,920,640 bytes. The previous
below-1-GiB expectation was incorrect for the complete nested scope.

Owner directs MemoryMax=max(recorded baseline peak × 1.5, 12 GiB) on this
30 GiB host. Therefore prospective v7 uses **12,884,901,888 bytes**, swap zero.
No separate low scope-peak abort threshold applies. Keep host start >=4 GiB
and abort below2 GiB, independent guardian, effective parent/held-child scope
proof, OOM and paging gates unchanged. Product source, binaries, collector and
600s+three60s per arm remain frozen. No cap-induced retry or shortened windows.

Fresh built-in capture worker receives the sole heavy slot 18:00–22:00 UTC
after the distinct v7 source/pin/mock review and tiny containment smoke pass.
B0011 source af61d18/CI39147491406 is green, its worker explicitly released
the heavy slot. All other heavy work remains paused for this grant. Lead sends
the final reviewed v7 hashes directly to the worker before any payload. Use
new evidence/session paths and all eight fresh arms; never reuse invalid cells.
If readiness slips, require enough room for 104min fixed waits plus measured
overhead and cleanup; a fresh full four-hour window may be assigned without
compressing. Report CAPTURE_START, then Review and DONE/IDLE/RELEASED.


## Reviewed replacement grant — 2026-10-18 19:00–23:00 UTC

Supersedes the18:00–22:00 grant. V7 stopped after its first baseline cell due
to incomplete task-counter coverage; zero cells accepted. It is preserved and
will not be reinterpreted. Its scope and quiet gates held. Saved evidence also
showed warmup was included in the first phase paging bracket; v8 corrects this.

Built-in Sol and Terra independently ACCEPT the prospective v8 review2 package:
`sol-n2-boundary-v8-review2-20261018` in the private validation cache.
Driver ca9c557a6e5b41a499f5d9c9fdd4ebbfa184535c1f4ca1653bcf6d71175ed040;
census d34c6b5a1fb4904b5738cc6b32f652de95731e41fe77eb19ba769fbc6528f5f2;
validator 0cdfcfbde8564789d08dd428d7bd01aebe014f8b3c92eccae1ae1e1463d3ead4;
plan a986827ed9f6da3d180f94e286cf10920be086e28e4b1df21fdbd2042a8e44af.
All34 manifest entries,17 new actual-driver/validator mocks plus11 original
mocks, Python syntax and18 shell syntax checks passed independently.

Measurement contract: whole-group PSS/RSS covers every owned process visible
at each defined phase sampling snapshot. Pin critical process identities from
launch hooks and verify PID/start identity before/after snapshots and terminal
scan. Enumerated helpers require measured memory or reject UNKNOWN; confirmed
departures retain identity and last historical memory separately from concurrent
PSS. Read errors/unconfirmed departures reject. Do not equate exiting threads
with lost critical roots. Context-switch survivor deltas are lower bounds;
whole-group totals with incomplete lifetime coverage are UNKNOWN, never zero
and never optimization evidence. Between-snapshot/action process lifetimes
remain unobserved/UNKNOWN; no exhaustive-lifetime PSS peak claim. Cgroup peak
covers the complete arm and is reported separately from PSS.

Exactly one600s warmup completes before first phase paging brackets, followed
by three full60s idle windows. Whole-wrapper paging still includes setup,
warmup, all scenarios and cleanup; strict<1MiB and processSwapPss0 unchanged.
Frozen runtime/helper/Orient/base collector identities remain unchanged; report
executed collector patch b226050fc607347495d96414039bd26113bff1ed6cba0c9a15919c670d5b29fe
separately from frozen base collector. The reviewed12GiB scope,swap0,host4GiB
start/2GiB abort, independent guardian and final ACK are unchanged.

Luna capture worker owns the sole heavy slot19:00–23:00UTC: hash-verified
private transfer, one tiny containment smoke, then all8fresh arms. All other
workers are IDLE; B0011 exact CI is green and released. No retry after an
invalid cell, no old-cell reuse, no compression or runtime rebuild. Announce
CAPTURE_START then complete Review and DONE/IDLE/RELEASED. F4 remains unpublished
until valid N2 evidence and lead integration approval, followed by exact-head CI.


## Current grant — 2026-10-18 19:27–23:30 UTC

Supersedes the19:00–23:00 grant. V8 stopped before measurement/report because
a token selector matched both the D-Bus wrapper and actual supervisor child.
No accepted cells; preserve its evidence. Xwayland warning is not established
as cause. Reviewed v9 selects full exact collector launch argv inside the owned
branch and rechecks PID/start/argv; ambiguity/missing/reuse reject.

Sol+Terra ACCEPT v9 source/mock: driver
ab1ba863d850e7ffa369f8915dad1ed7c7134f7b7061435c2c1183c3dee4d97d; plan
a88a6f0458c58080a423e4291c31772fa6e7c9b4ab7a0b8ab4c2d015e413acfd.
21new+11original mocks,syntax and30 manifest entries pass. All other v8
contracts and frozen runtime/artifact identities remain unchanged. Executed
collector patch04f87e5d9d5ec531c0d070a554adfaf88d4094e421fbabfff9de7a9fe4fc20b7.

Before any fresh batch, run ONE separately reviewed real nested readiness smoke
(max120s) from private sol-n2-v9-readiness-smoke-20261018 REPORT. Controller
a5e68bfc82ecb63ddae1fd0c644a0b4eb32a5b1ee1bddaa7de1e477be6b44fc2; inner
ab8b432c86b53335aace240cd636b972f4a00a2e00d2aaead3754ec4ff5fc11e.
Source review ACCEPT;6mocks and5manifest entries pass. It checks actual shell
IPC,all7pinned roots,census snapshots and terminal roots, then unwinds existing
cleanup BEFORE600s warmup/idle/actions. No performance result. Payload45s,
controller85s and total120s bounds; same12GiBguard and exact owned cleanup.
Failure stops and reports without retry. Correct machine digest hashes raw
/etc/machine-id bytes, consistent with original scripts; never strip newline.

On smoke PASS and confirmed owned cleanup, announce CAPTURE_START and execute
all8fresh v9 arms without further permission, within19:27–23:30UTC. No other
heavy jobs; peersIDLE. No runtime rebuild or old-cell reuse; existing guard,
whole-wrapper quiet and full600+3x60 timing gates unchanged. First invalid cell
stops batch; Review and DONE/IDLE/RELEASED required before another heavy job.


## Current v10 grant — 2026-10-18 19:50 through Oct19 00:00 UTC

Supersedes earlier windows. V9 readiness stopped for a real QML startup Python
child that exited before its first memory read; no batch or accepted evidence.
Exact saved command was the NacreIcons platform-name query, not root failure.
V10 uses an explicitly sequential, completed snapshot round: enumerate owned
identities, read memory, reconcile final live inventory, and verify critical
roots before/after and terminal. Every final LIVE identity requires its own
valid PID/start memory read. Exact-confirmed departed identities are retained
as NOT_LIVE_AT_CUTOFF, with historical memory or null/UNKNOWN if never sampled;
they are not concurrent live PSS and are never assigned fabricated zero.
New/unread final live identities, unresolved exits, read errors and identity
reuse without the new identity's read reject. Any observed positive SwapPss
rejects even if that process exits or the original sampler drops its row.
No sampling or arm retries, sleeps, process exclusions or shortened windows.
Sampling is non-atomic; inter-snapshot/action lifetimes and PSS peaks UNKNOWN.

Sol+Terra independently ACCEPT v10 package
sol-n2-completed-snapshot-v10-20261018: driver
129ab4ed05b895662d69e238ae84e15836b5c9ccd4e059acfb213fc9329b5785; census
49ab5d728a3bee590a5fde9e4e85a27c98cace1a5524c9247ee2d4a6c7ff8aa8; validator
31c793049336c25aad4d9298ab5817c97f19107f246fa4e750f45b3524172ba0.
All37 manifest/27plan pins,28snapshot+6smoke+11original mocks pass.
Read exact remaining identities and argv directly from that reviewed manifest
and plan; do not manually transcribe expected hash tables. Frozen runtime,
helper,Orient and base collector unchanged; report executed patch from plan.

Luna soleheavy grant effective19:50UTC through2026-10-19 00:00UTC. First ONE
reviewed <=120s actual readiness smoke (same45s/85s bounds), then on PASS and
confirmed cleanup announce CAPTURE_START and start all8fresh arms without
further permission. TwelveGiB cap/swap0,host4GiB start/2GiB abort, independent
guard, strict whole-wrapper paging and full600+3x60 windows unchanged.
Failure stops without retry. Previous evidence remains invalid and preserved.
All peersIDLE. No runtime rebuild/push/install; final Review/DONE release.
