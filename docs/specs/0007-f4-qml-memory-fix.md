# N0007 — Remove the F4 QML memory increase

- ID: N0007
- Repo: nacre-desktop/nacre
- Implementer: one built-in Sol worker
- Status: Review (source accepted; memory screening pending)
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

## Quick-screen grant — 2026-10-19 07:10–09:10 UTC

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
