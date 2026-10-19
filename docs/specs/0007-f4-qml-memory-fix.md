# N0007 — Remove the F4 QML memory increase

- ID: N0007
- Repo: nacre-desktop/nacre
- Implementer: one built-in Sol worker
- Status: In progress (correctness accepted; memory screen failed, source follow-up)
- Depends on: N0001/N0004 accepted source; v14 correct-profile diagnostic pair
- Date: 2026-10-19
- Size: one focused source/fix session, then separately granted checks

## Goal

Find and fix the F4 candidate's QML memory increase without weakening its
single-writer or durable-success behavior. The owner directs product correction
now: no more capture-tool versions, then one quick pair per compositor and one
full eight-arm confirmation after the quick check succeeds.

## Context

Start at local candidate1382a28ab544743e32e906f7542bcecdfdfca083, in a NEW isolated
worktree, preserving that candidate and all existing small commits. Baseline
5f4c0bf3, helper4a65f4ec; current candidate release helper439dd355. Protocol and
immutable settings validation/controller/rollback corpora remain unchanged.
Read N0001/N0004, standing-rules, v14 package REPORT, and the private archive
n0002-v14-geometry-capture-evidence-20261019 (SHA3aae67b5...d84cd).

Both real smokes proved2880×1800/scale1.5/logical1920×1200, H120/N60. Only Niri
A/B passed; H1 baseline failed host swap-in2,592,768B. No eight-arm gate PASS.
Whole-group median MiB fresh/closed/after-latency: baseline536.593/561.902/567.708,
candidate543.936/572.432/564.188. Deltas+7.343/+10.530/−3.520. Initial saved QML
favorites also differed12baseline/0candidate; investigate whether a candidate
behavior defect or fixture/input mismatch before claiming a comparable fix.
Earlier1920×1080 runs are diagnostics only, not target acceptance.

## Scope

In: focused QML state/subscription/data-flow correction and meaningful regression
checks. Start DesktopSettings.qml, NacreStateFile.qml, shared client topic/cache
lifecycle and direct consumers only when needed. Small native corrections only
if source evidence identifies the same F4 state-routing cause; explain design.
Out: changing protocol/schema, deleting required live updates, changing data or
favorites to reduce memory, renderer/allocator tuning, unrelated UI features,
worker/pacman optimization, benchmark-tool changes or live deployment.

## Steps

1. Create own worktree from1382. Read exact v14 reports and saved topic payloads.
   Attribute the correct-size delta to QML/daemon/helpers and anon/file/shared
   categories using matching phases. Separate endpoint snapshots from medians.
   Resolve favorites12→0 and verify input/catalog/visibility equivalence.
2. Test source hypotheses in order: (a) 11 startup subscriptions vs3 and eager/
   duplicate retained topic/model copies; (b) DesktopSettings and NacreStateFile
   stringify/parse cycles plus subscribe-and-get duplicate initialization and
   reconnect reads; (c) resulting binding/high-water/shared-buffer retention.
   Payloads were only~2.3KB in earlier diagnostics: do not assume huge JSON.
   Time-box initial source diagnosis to60min, then report evidence/remaining
   uncertainty to lead rather than inventing a cause or broad rewrite.
3. Implement the smallest supported correction. Preserve complete state, unknown
   fields and operation-owned deletion semantics. Every save acknowledgement
   still requires same store/boot/revision, clean/error-free matching intent;
   cancellation, supersession, disconnect/new boot and lost replies never replay
   writes or create fallback writers. Keep consumers reactive and compatibility
   string/text interfaces where currently promised; avoid duplicate retained data.
4. Add focused behavior tests named for the corrected path, with exact assertions
   in the report. Required cases as applicable: one initial projection despite
   snapshot+reply delivery, correct data after late snapshot/reconnect/new boot,
   unrelated edit vs same-field supersession, and favorites round-trip/equivalent
   seeded count. Existing saved() fake-client and native failure/ownership tests
   must remain. Never write tests that merely mirror a helper implementation.
5. Commit small logical local [no-publish] commits immediately. No push until lead
   review and memory gate. Run bounded light checks available locally; request a
   lead heavy slot for private QML/correctness/release work, no installation.
   Report exact source/profile/binary hashes; if Rust unchanged reuse its proven
   helper only with explicit source mapping. No full suite without grant.

## Measurements and acceptance

No capture is granted by Ready status. After source review, lead grants ONE quick
A/B pair per compositor at correct v14 geometry, preserving existing collector,
600s+three60s windows/quiet/scope gates (4arms,52min fixed+overhead). This is a
screening check, not publication. If it improves and meets limits with data
parity, lead grants the full existing eight-arm method ONCE. No v15 tool or
measuring-tool redesign; if existing tool prevents execution use owner's standard
five-pair whole-group method with a separate explicit slot. No automatic retry.

- [ ] Focused correction explained by source and matched component evidence.
- [ ] No lost state/update, functional reduction, fallback writer or durable-ack regression.
- [ ] Identical fixture inputs/required favorites/catalog; missing data isn't zero.
- [ ] Focused tests pass, source review ACCEPT, exact artifact provenance retained.
- [ ] Quick pairs pass standing memory rules; pure optimization cannot raise memory.
- [ ] Full eight-arm confirmation passes whole-group/endpoint/quiet/profile gates.
- [ ] Only then normal main integration and exact published-head CI; no release/live action.

## Stop conditions and report

Stop for scope/protocol expansion, unexplained anonymous growth>1MiB, correctness
loss, pure optimization raising memory, missing tool/cap/quiet or noncomparable
inputs. Ask lead precise questions; do not ask owner for ordinary source choices.
Report status, root cause/evidence, chosen change, commits/files, exact checks/
results, artifact bindings, matched measurements with limitations, cleanup and
next blocker. No unsupported performance PASS. Preserve failed evidence.

## Short rules

siverteh <92440237+Siverteh@users.noreply.github.com> author+committer; own worktree,
normal push only when authorized, never force/rebase others; [no-publish]. nice10,
Cargo-j2, one granted heavy job. Private/nested only; capture cap12GiB/swap0,
host>=4GiB start/abort<2GiB, verified held-child scope and watchdog. No live desktop,
installed Nacre/configs/keys/units, publication, AI-policy/paru changes, other
people's PIDs, memory pressure/cache drops/sysctl. Extras go in report, not code.

## Source review — 2026-10-19

Accepted local source `0df0bc145968f51ce156075caf380cf64619f00d` after
independent Sol review. It removes duplicate initialization/projection, preserves
owned mutable copies and durable-intent checks, and fixes the seeded favorites
initialization race. All fourteen selected correctness checks passed: twelve
local private Qt cases and two exact-helper wire cases in the verified scope.
Rust/protocol trees are unchanged from1382; helper439dd355 is reusable without a
rebuild. Source acceptance is not a memory claim or publication approval.

Next: bind the unchanged v14 method to fresh outputs and this exact source.
Quick screening compares published baseline5f4 against fixed0df, one A/B pair
per compositor, to assess the absolute standing memory budget. The old1382 pair
is diagnostic context. No capture is armed until lead reviews those bindings and
records a fresh exclusive window; no new measurement-tool version is allowed.

## Quick-screen grant — 2026-10-19 07:10–10:10 UTC

Lead grants the private lab exclusively to four arms: Niri baseline/fixed, then
Hyprland baseline/fixed. Baseline5f4/helper4a65; fixed source0df0bc14 with
unchanged helper439dd355. Source/input review ACCEPT. Manifest3b2302c9,
overlay8d071d08 and immutable v14 method retained. Bindings only changed; no
v15 tool. Each arm retains600s warmup plus three60s phases and actual
2880×1800/1.5 (1920×1200 logical), N60/H120 proofs. One batch lock, fail first,
no retry or automatic full-eight run. Equal seeded inputs and prescribed action
outcomes must be verified before interpreting memory. All existing12GiB/swap0,
held-child/guardian/final-ACK, host4GiB start/2GiB abort and strict whole-arm
quiet gates apply. No build/install/live action. Bead is local light source only.
Full-eight confirmation is separately granted only after this screen passes.

Reviewed staging correction: the existing source checker pins candidate identity
as a literal. A private copy substitutes only1382→0df (SHAab2a4225); every other
byte/check is identical. This is source input binding, not a new measuring tool.
The unchanged v14 method, root-level launch copies and manifest passed staged
inventory/source checks. No arm ran during staging corrections.

## Failed first screen and reopened source review — 2026-10-19

Source0df Niri quick pair passed geometry/quiet but whole-group deltas were
+13.413/+15.101/+11.047MiB. Stop: no Hyprland/full-eight. Only wayland-utils
was installed08:08:50UTC during B warmup; retain that comparability caveat.
Archive62ce9f233bc5dfde70e596ae2411b9fa4a2e8ce94fd7c3e9612335ba228a9a72.

Reopened local source8a1df35c6fec44e660530643ffef367e6b57ef89 is independently
ACCEPTED with21 local fake-QML and2 exact-helper wire cases PASS. It shares the
stateless JS library, removes duplicate storage lifecycle reads and rejects old
client/generation/boot callbacks without replay. Native source/helper unchanged.
Wire log26f41c40b43a2dc31af59a4c7a6b83ef69ec2c2632bca00bb6e525823ac1c27e;
receipt archivea592e8ee29d8e84c03f32111b44cfa4a45443309fb62e784525b07ffdb5cfc55.
The initial tar-mode preflight stopped before payload; sole actual execution
verified all145 source files using exact bytes plus Git executable-bit semantics
(archive0664 extracted0644). Scope12GiB/swap0, no OOM; cleanup complete.
SCHED_IDLE was preserved, numeric nice was not inferred from ps NI='-'.
Memory improvement is UNKNOWN. Fresh bindings retain the frozen v14 method,
Niri pair first and separate Hyprland resume grant; no full-eight activation.

## Fresh Niri-only grant — 2026-10-19 09:00–11:00 UTC

Independent bindings ACCEPT: inventory6f117c58c1d5d57a4ce57d4ec6c412c2cb6c1ea08c42073ab0b940d1bc8649f4,
manifest24c9b63108c3bca905d54b0de9c739182fb60abf7e8b6894b14805aae35c5c2b,
overlay5a908c1f26d253f495d29250feb6f456c7c83b78d8f8d09045a148c5166dd344.
Private lab identity and baseline/helper pins remain as above; B source8a uses
unchanged1382/helper439. New source/output roots, frozen v14 algorithm. All peers
released; sole heavy slot assigned to sol_n7_quick_capture. Stage before09:00,
measure only in window. Entry runs ONLY Niri A/B and exits normally. No retry,
no automatic Hyprland or full-eight. Both arms are after wayland-utils install.
Require exact source, geometry, seeded-input parity and existing scope/quiet
checks. Lead reviews per-phase group/QML results and unknowns after cleanup.
If whole-group increase>=5MiB, unexplained anonymous growth>1MiB or other
standing-rule failure: STOP. Hyprland needs separate explicit resume grant.

## Source8a quick result — STOP 2026-10-19 09:28 UTC

Both Niri cells valid. Fresh/closed/after whole-group MiB A541.409/573.119/581.385,
B547.165/579.240/581.873: +5.756/+6.121/+0.488 blocks H/full-eight. QML phase
medians A495.099/520.727/530.434, B496.172/529.528/529.334. Independent component
medians are not additive to group medians. Existing mapping endpoints are not
phase medians; no causal explanation inferred. Phase favorites12 and closed
visible12, final six configs/catalog match after exact private-path normalization.
Early B geometry0 vs A12 recovers before measured phases; readiness remains UNKNOWN.
Wrapper swap-in4096/20480B; SwapPss0.12GiB/swap0 scopes, no OOM; normal exit,
all owned identities/cgroups/tmux absent and lock free. No H/retry/full-eight.
Archive940f5f88c57c947fab2c1c20a3c3f4ebac50621325c6ba52ccacd13dfd319a35.

Next bounded assignment: same Sol source worker,60min source-only diagnosis of
matched residual, with evidence and smallest fix proposal before code. No runtime
probe/build/CI/capture; native scope expansion needs lead decision. Preserve all
accepted fixes and23-case correctness evidence; no claim full acceptance. Root
may schedule Bead hosted CI while this investigation remains local/read-only.

## Residual diagnosis and diagnostic-preparation boundary

Sol source review and one bounded Astra low read-only escalation found no
supported small residual fix. Astra was used only after Sol could not identify
cause following accepted source correction plus a fresh valid failed screen.
No product edits, host probes or new acceptance runs by either investigator.
Closed QML private-memory drops appear in USS/RSS as well as PSS, with no sampled
process-set change; medians20–30s and30–40s still show excess. Native fresh cost
is separate. Saved mappings/endpoints cannot establish object/GC/allocator/render
causation. Source8a remains clean and all speculative tuning is rejected.

Installed QML profiler is not a safe route here: Quickshell0.3.2 calls Qt debug
server without a hostname, which listens on all interfaces; supported CLI has
no host option. Do not start it, change firewall or invent an environment bypass.
Sources: [Quickshell launch](https://github.com/quickshell-mirror/quickshell/blob/v0.3.2/src/launch/launch.cpp#L275-L279),
[Qt debug server](https://doc.qt.io/qt-6/qqmldebuggingenabler.html#startTcpDebugServer).

Lead authorizes SOURCE PREPARATION ONLY of a private logging/owned-memory diagnostic:
identical Qt GC log rules on both arms, pinned owned QML/daemon rollups, no debug
listener, forced GC, tuning, install, protocol/data changes or collector rewrite.
Frozen v14 modules remain byte-identical and acceptance blocked. Profiled values
would be diagnostic only, never substitute for an acceptance arm. Independent
review requires exact launch/probe pass-through, executed identity/descendant
checks, bounded observer progress and startup evidence, exact owned cleanup and
meaningful failure mocks. Initial draft failed that review and is on one bounded
source-only correction pass. No runtime/profiler slot is granted by this note.

## Logging-only diagnostic gated grant — 2026-10-19 10:00–10:50 UTC

Private package2440a5cfb46e475dc41fc751c1f0657a51e824d9dac642df66b8063fd5ce9fca
independently ACCEPTED after one bounded source-fix pass; six initial review
failures preserved,119 source pins checked. Correct health-command pass-through,
actual executed identities/argv/descendant edges, attributable numeric GC details,
3s sampling/90s startup/30s cleanup bounds, independent observer heartbeat and
first-reason-preserving aborts. These are diagnostic support only; seven frozen
acceptance modules remain unchanged. Mocks are not runtime proof.

Lead assigns sole heavy private lab slot to sol_n7_quick_capture, baseline5f4/
helper4a65 vs source8a/helper439, actual Niri geometry/fixtures unchanged. Read-only
preflight/staging allowed before10:00. Before payload, verify/review installed
Quickshell digest/options/Qt versions and supervisor environment preservation;
no listener/profiler install/build/source patch. Fresh private artifacts/output.
Strict90s real startup gate must pass or stop UNKNOWN, not retry/repair. If it
passes, only one sequential diagnostic pair with600s+three60s each, normal owned
cleanup; no H/full-eight. Existing held-child12GiB/swap0/independentguardian,
host>=4GiB start/<2GiB abort, strict paging/SwapPss gates remain. No other heavy
job/CI may overlap. GC/rollup/logging overhead makes all output DIAGNOSTIC ONLY;
existing report.accepted filenames denote structural validity, never landing.
Grant no forcedGC, allocator/render tuning, data reduction or acceptance waiver.
