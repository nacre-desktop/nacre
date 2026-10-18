# N0002 — F4 whole-group acceptance and publication

- ID: N0002
- Repo: nacre-desktop/nacre
- Implementer: built-in Luna capture worker; lead reviews/integrates
- Status: In progress
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

Historical v25 partials and v3–v11 failed attempts remain preserved, not current
acceptance. Earlier grants and harness pins in Git history are superseded.
No accepted current eight-arm batch exists. Frozen Wave3 and pending physical
owner A/B/A/B remain separate and immutable.

## Scope

In: one fresh nested eight-arm batch, whole-group/endpoint analysis, report and
later normal integration after lead acceptance. Small harness corrections are
reviewed here. Out: runtime optimization, new services, physical tests, package
publication/install, live config/unit/keybinding/session changes.

## Current reviewed harness and exclusive grant

Sole heavy slot: **Oct18 21:33 through Oct19 00:00 UTC**. All other workers IDLE;
B0011 exact source af61d18/CI39147491406 is green and heavy ownership released.
No other full suite, hosted heavy CI, release build or nested session overlaps.

Reviewed private package: sol-n2-provenance-v13-20261018 in the validation cache.
Driver 5a1ab9fa82ac08fb981a6ab3ea59fb12ef8d271f1a767cc37b049336be40839b;
plana110611b237f419b9ff5a76f0dfc17f659f4175eab840c44970b77e153e40d88;
smoke13ef450f744f66ce0a2af3ec4ff02cbba5646fc9eb0b2d73459556cf02c8284c.
Read every other expected hash and exact argv directly from reviewed plan and
SHA256SUMS, not a manually retyped table. Sol+Terra ACCEPT all40 manifest/27plan
pins,46core+6smoke+11original tests. The source investigation escalated once to
Astra after repeated real startup census failures; it identified missing bounded
new-child discovery/read completion. No runtime/application changes were made.

V12 restores byte argv at the frozen sampler interface and exercises actual
idle. V13 fixes lossy diagnostic metadata merging: a current unavailable exe
remains UNKNOWN, while a successful SAME-round PID/start-anchored executable
observation is retained with timestamps/provenance. No previous-round/history
borrowing, PID reuse or invented errno/current path. Memory primitive, inclusion,
arithmetic, collector transforms and windows are unchanged. Sol+Terra verified
this comparability and the immutable four report/method/PASS log hashes.

Continuation entry package sol-n2-v13-continuation-v2-20261018 is independently
ACCEPT: entry6434e18e119692242e35f392d09e3cb1b21bfada589451878e66b4745b99a556;
contractc8260a0fa11f91ba5890f5a76b010dfce8fe2b9ddaa7a49849e0e4b7ae6faa15.
Seven actual-shell mocks/eight manifest entries pass. It checks pinned old
reports/method/PASS provenance, exact source and new absent outputs; acquires
one FD9 bench lock, uses a new noclobber log and invokes ONLY H2A/B,N2A/B,
fail-fast without retry. Do not run the full v13 eight-arm entry. Candidate
checkout HEAD is manifest.base5f4; manifest.head identifies candidate1382.
The failed v12 H2-before stays invalid and gets a fresh v13 replacement.
Record v12 first-four/v13 last-four support provenance separately. No memory
threshold is waived and no failed historical arm is retrospectively accepted.

## Steps and measurements

1. Verify local and remote reviewed hashes, raw machine digest, free lock, peers
   IDLE, no competing heavy job, fresh output paths and >=4GiB MemAvailable.
   Transfer only absent/identical reviewed files; never overwrite evidence.
2. Run ONE reviewed real readiness smoke (<=120s total; payload45s/controller85s).
   It verifies actual shell IPC, exact full-argv owned supervisor selection,
   seven critical roots, census/terminal checks and2s actual idle, then unwinds
   existing cleanup. No600s warmup/actions/performance acceptance. Failure stops
   without retry. On PASS plus cleanup, announce CAPTURE_START and proceed with
   the full batch without another permission request.
3. Retain the four reviewed immutable valid v12 N1A/B,H1A/B arms. Run ONLY
   fresh v13 H2A/B,N2A/B via the reviewed continuation below. The completed
   comparison remains two alternating pairs per compositor
   (explicit exception to default five). Hyprland virtual120Hz, niri nested60Hz,
   2880×1800/scale1.5. Match private HOME/catalog/palette/settings/actions.
   Every arm gets ONE600s warmup BEFORE phase counters, then THREE full60s idle
   windows.104min fixed waits total, plus startup/actions/validation/cleanup.
   Do not compress or reuse invalid arms; first invalid cell stops continuation.
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
