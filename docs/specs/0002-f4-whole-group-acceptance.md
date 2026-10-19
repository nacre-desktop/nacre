# N0002 — F4 whole-group acceptance and publication

- ID: N0002
- Repo: nacre-desktop/nacre
- Implementer: built-in Luna capture worker; lead reviews/integrates
- Status: Ready — owner-authorized overnight five-pair acceptance
- Depends on: accepted N0001/N0004 source review; reviewed prospective harness
- Date: 2026-10-18
- Size: one focused capture session, through the granted cutoff

## Goal

Validate the frozen F4 candidate as QML + daemon + all helpers, then publish only
if the whole-group and endpoint gates pass. F4 is the owner's first priority;
package publication, unit enablement and live opt-in remain separate owner work.

## Context and immutable bindings

Read N0001, N0004, architecture/standing-rules.md and development/rust-migration.md.
Baseline source5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57; release helper
4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa.
Candidate1382a28ab544743e32e906f7542bcecdfdfca083; manifest
ff415fd749aa343abbdd8e9745ece13fc48d68f98fa1dcc55d78d4d5ca365875;
release helper439dd355fca276334c2faccbd0992d1b23eac6c6caab311ca1c1c61d3672e92c.
Orient gitlink3efe8d1e7fb50200d74a3e0193ad63699cb8ebe8.
Frozen base collector c363ef877d025c0276de43a9b58ecc3b67de76b6353a5dc892a3e4a1348c3a6b.
Report exact executed collector patch separately, from reviewed plan.

Private host raw-file machine digest:
fb7ab717874d5e6493f6e30fadcc8b5c01ab65268a56c6256261a8df387a938a.
Hash raw /etc/machine-id bytes; stripping newline yields a different digest.
Use the existing trusted runner; never inspect/copy its credentials.

See [eight-arm diagnostic](../research/f4-n0002-eight-arm-diagnostic.md):
v12/v13 passed cell validators but used wrong geometry; no target-profile
acceptance. New capture requires prospective geometry review and a new grant.

Historical v25 partials and v3–v11 failed attempts remain preserved, not current
acceptance. Earlier grants and harness pins in Git history are superseded.
No accepted current eight-arm batch exists. Frozen Wave3 and pending physical
owner A/B/A/B remain separate and immutable.

## Scope

In: one fresh nested eight-arm batch, whole-group/endpoint analysis, report and
later normal integration after lead acceptance. Small harness corrections are
reviewed here. Out: runtime optimization, new services, physical tests, package
publication/install, live config/unit/keybinding/session changes.

## Current geometry-corrected harness and exclusive grant

**Heavy window ended; no active capture grant.** The former Oct18 22:50–Oct19
03:15 window is closed by owner morning guidance. All other workers IDLE;
B0011 exact source af61d18/CI39147491406 is green and heavy ownership released.
No other full suite, hosted heavy CI, release build or nested session overlaps.

Reviewed private package: `sol-n2-geometry-v14-20261019` in the validation cache.
Driver `a97c7b195a4f0dcb605adbbea5f4dbb068cb2a0737da268a733d1dae3935e42e`;
plan `45c1d0a1c684ba4081d409c4822526d56b2a2fffdfacbde1f2552509e51b55a2`;
geometry module `6e4987fc1cb98cb83591d253b91152f88b7d93bb7cc858f2a2e046d777cbcc9f`;
smoke `0794204b574f8dac46408e026a4597b2887f94dce66924729a2acbf26e536a5d`.
Read all other expected hashes/argv directly from the plan and SHA256SUMS; never
manually retype a comparison table. Sol+Terra ACCEPT 40 manifest/29 plan pins,
51 core/geometry, 7 smoke and 11 original tests. Existing sampling functions,
census, timing and quiet gates are unchanged; four geometry-only collector
patches and an independent raw-output readiness gate were added. Report the
executed collector patch separately from frozen base collector.

Geometry must be ACTUAL, not just requested: parent output-manager IPC, nested
Hyprland/Niri IPC and direct QML screen proof before readiness/warmup. Required
physical 2880×1800, scale 1.5, logical 1920×1200; Hyprland 120 Hz, niri 60 Hz.
Units, transforms and source/display identifiers are explicit. Parent/Hyprland
logical sizes may be transparently derived from observed mode/scale/transform;
niri logical size is reported directly. The cell validator reparses raw proof
and checks every phase's QML logical size. Missing/unsupported/malformed modes,
wrong scale or refresh and a 60 Hz Hyprland fallback reject. Actual tool/query
capability remains unverified until both real backend smokes pass.

All previous eight v12/v13 arms remain diagnostic only. Their geometry differs;
no continuation or reuse is permitted for the corrected target-profile gate.

## Steps and measurements

1. Verify local and remote reviewed hashes, raw machine digest, free lock, peers
   IDLE, no competing heavy job, fresh output paths and >=4GiB MemAvailable.
   Transfer only absent/identical reviewed files; never overwrite evidence.
2. Run ONE reviewed real readiness smoke PER BACKEND, sequentially
   (<=120s each, <=240s combined; payload45s/controller85s).
   Each requires actual parent/nested/QML geometry proof above, actual shell
   IPC, exact full-argv owned supervisor selection,
   seven critical roots, census/terminal checks and2s actual idle, then unwinds
   existing cleanup. No600s warmup/actions/performance acceptance. Failure stops
   without retry. Only on BOTH backend PASS plus cleanup, announce CAPTURE_START and proceed with
   the full batch without another permission request.
3. Use eight NEW arms: N1A/B,H1A/B,H2A/B,N2A/B. Two alternating pairs per
   compositor are an explicit exception to the default five. Hyprland virtual
   120 Hz and niri nested 60 Hz, both 2880×1800/scale 1.5. Match private HOME,
   catalog, palette, settings and actions. Every arm gets ONE 600s warmup BEFORE
   phase counters, then THREE full 60s idle windows. Total fixed waits 104min,
   plus startup/actions/validation/cleanup. Do not compress or reuse old arms;
   the first invalid cell stops the entire new batch without automatic retry.
4. Run all descendants in an exact private scope: MemoryMax12884901888 (12GiB),
   MemorySwapMax0. Owner sizing=max(recorded baseline peak×1.5,12GiB), superseding
   the incorrect sub1GiB expectation/2GiB cap. No low scope-peak abort threshold.
   Prove manager/effective limits and same held child PID/start/cgroup before
   release. Independent1s guardian aborts host MemAvailable<2GiB, OOM, identity or
   monitoring failure. Parent/guardian final evidence and bounded ACK precede
   teardown. Stop only owned exact scope/PIDs; never Codex/app-server or peers.
5. Sampling contract: sequential/non-atomic whole-group PSS/RSS at defined
   snapshots, with all owned helpers visible at each completed snapshot.
   Pin critical roots from launch hooks and check before/after/terminal.
   Within ONE snapshot, read initial inventory then only newly discovered
   PID/start identities, at most4 extra passes and1s TOTAL wall time including
   initial sampling. Retain first attempts; never reread for favorable values.
   Budget exhaustion invalidates. Record passes/timestamps/elapsed/unresolved.
   Every final-live identity needs its own valid read. Exact departed identities
   remain NOT_LIVE_AT_CUTOFF with historical memory or null/UNKNOWN, separate
   from concurrent PSS; never fabricate zero. New live unread/reused/unresolved
   identities, permission/read errors and critical-root loss reject. Any observed
   SwapPss>0 rejects even if the sampler later drops that exiting process.
   Original bounded exit-confirmation delays remain inside the1s budget;
   no additional completion sleeps, snapshot restarts or arm retries.
6. Context-switch survivor deltas are LOWER_BOUND; incomplete whole-group totals
   are UNKNOWN and support no optimization claim. Between-snapshot/action process
   lifetimes and PSS peaks are UNKNOWN. Whole-arm cgroup peak is distinct from
   sampled product PSS. Report cadence/durations/gaps and all limitations.
7. Quiet gates: process SwapPss0; host swap-in strictly<1MiB in collector AND
   whole-wrapper intervals including warmup/setup/cleanup. Report independent
   warmup and each idle paging interval, in/out pages, page size and byte deltas.
   No host pressure, cache drops, sysctl/kernel changes or physical intervention.
8. Report QML+daemon+ALLhelpers individual and group PSS, peak, anonymous/file/
   shared changes, CPU/call, process launches and per-action/cold CLI endpoints.
   Every raw run value, medians, min/max spreads and peaks; missing is UNKNOWN.
   Do not interpret invalid-arm PSS/latency as accepted performance evidence.
9. Send Review and DONE/IDLE/RELEASED: own unit/cgroup/held PID/controller/session
   gone, lock free, evidence preserved. Lead reviews gates before publication.
   On valid PASS only, integrate the reviewed local F4 candidate into current
   main preserving concurrent docs/collector changes. No rebase of others' work.
   Push normally and run all required exact published-head CI as sole heavy job.
   No release/install/live opt-in is authorized by passing this spec.

## Acceptance criteria

- [ ] Eight fresh valid arms, exact artifacts/full windows/containment/quiet.
- [ ] Whole group equal/lower, component increases explained; OR increase
  strictly<5MiB explained and tracked. Otherwise stop.
- [ ] No reproducible endpoint regression outside standing rules. Cold CLI
  repeats explicitly assessed against comparable controls; uncertainty reported.
- [ ] N0001/N0004 source correctness and all writer ownership reviewed.
- [ ] Normal main integration, exact capture-to-commit mapping and full green
  published-head CI. No package/live action.
- [ ] Raw evidence retained; concise report and explicit owned cleanup/release.

## Stop conditions and report

Hash/host mismatch, overlap, quiet/containment failure, missing live helper,
root loss, snapshot budget/error, repeatable regression or uncovered writer:
stop and preserve evidence; no automatic retry or changing gates mid-capture.
Send status, exact source/profile/binary/support hashes, commands/results,
controller/unit/PIDs, per-arm values and gates, report/raw paths, CI links,
limitations, cleanup and the precise blocker if any. No fabricated PASS.

## General rules

Commit as siverteh <92440237+Siverteh@users.noreply.github.com>; author+committer.
Isolated worktree; direct normal main integration, no force push/rebase of others.
Nacre commits [no-publish]. nice10/Cargo-j2; private/nested only. No live desktop,
installed Nacre/configs/keys/units or publication without owner approval. Preserve
AI permissions and paru command. Stop only PIDs started by this task. No host
memory pressure/cache drops/sysctl. No overbuilding; extras one line in report.

## Authoritative owner acceptance amendment — 2026-10-19 afternoon

This supersedes quick-screen/eight-arm requirements and all older windows. Stop
profiling: no further Heaptrack GUI or mapped/rendering investigation now.
Standard unchanged v14 method, source8a1df35c6fec44e660530643ffef367e6b57ef89
versus baseline5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57; release helpers remain
439dd355fca276334c2faccbd0992d1b23eac6c6caab311ca1c1c61d3672e92c and
4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa.
Five alternating A/B pairs per compositor (20 arms), correct2880×1800/1.5,
niri60Hz and Hyprland120Hz. Preserve v14 sampling/actions/timing:600s warmup,
three full60s idle phases. Only outer scheduling, immutable input selection,
and reporting change; no v15 tool or shortened windows.

Exclusive private lab availability: **immediately after prerequisite checks and
coordinator batch review on2026-10-19, for12hours from actual CAPTURE_START**.
Owner confirms desktop unused rest of day; no need to wait for night.
No other heavy work, including Bead CI, builds or nested sessions. Start after
coordinator review and explicit peer pauses; source readiness/preflight may be
prepared locally now. Existing 12GiB/swap0 held-child containment, independent
watchdog, >=4GiB start/<2GiB abort, geometry, SwapPss0 and whole-arm host swap-in
<1MiB gates remain. Owned cleanup only; no live desktop or package actions.
If only host swap-in quiet gate fails, preserve invalid arm and rerun that same
arm ONCE at batch end. At most TWO distinct arms may need this; third such arm,
failed retry or any other failure stops batch. No replacement of other evidence.

Report each arm and per-compositor A/B aggregate medians, spreads, and paired
deltas for whole group and QML/daemon/other helpers separately, each idle phase.
Compute component sums within each sample before its median; do not sum
independent medians. Apply thresholds to each reported phase; any ambiguity
returns to coordinator, not a worker-selected favorable summary.
Owner rule (MiB): group increase <5 on both compositors passes; 5–8 inclusive
passes ONLY if QML increase<=2 and remaining increase is accounted for in daemon
settings residency (not unexplained other helpers), with tracked follow-up spec.
Group>8 or QML>2 stops/report; only then resume buffer investigation. Equality5
uses explained-cost branch. No single-pair5MiB veto. Lead reviews before F4
integration, exact-head CI required; no release/live switch granted.
