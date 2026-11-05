# N0134 — Brain viewer load robustness (U12)

Status: complete; synthetic acceptance and desktop CI validated.

The viewer previously waited for `/api/brain` to run discovery synchronously.
The overhaul audit measured cold discovery at 145 seconds with vectors and
21 minutes without them for approximately 1,900 notes. The UI supplied neither
a request deadline nor a Retry button. This is the supported explanation for
the reported post-deploy loading hang; the actual laptop failure was not
captured by this unit. Authentication readiness remains a regression boundary.

## Scope

Load counts, subjects and recent notes through a light `/api/summary`, with
note bodies fetched only when selected. Build connections in the background
at server startup and after changes to the vault. Persist the completed graph
and its input fingerprint under `.brain-state/index/`; reuse it on restart.
Publish a building state while discovery runs and update the viewer without
a reload. All API loading failures must become visible with a Retry action.
Retry the first forbidden API response once after authentication readiness.

No category redesign, ranking changes, graph UI redesign or versioning changes.
`brain_data.rs` belongs to the parallel U1 writer. No live state or owner notes
may be accessed by tests. No temporary alternate product implementation is
introduced; synchronous CLI graph generation remains a supported CLI path.

## Evidence required

- Headless Chromium through the real launcher: synthetic 2,000-note cold vault
  displays subjects and recent notes in under five seconds; connections arrive
  without a reload after background discovery completes.
- Stall the summary/API beyond the viewer deadline: visible error and Retry;
  release the stall and verify Retry succeeds.
- Restart with the saved graph: useful data in under two seconds, no rebuild.
- Cherry-picked launch-sequence tests plus the first-launch regression pass.
- Full desktop job from `.github/workflows/check.yml` in a disposable Arch
  container, including formatting, clippy, Rust/Brain tests, catalog checks,
  product proofs, tools checks and native gallery/memory checks.

The dev-1 container is named `n0134`, with 8 CPUs and 16 GiB; builds run under
tmux, nice 10, Cargo `-j2`. The lead confirmed that the dev-2-only rule was stale
and authorized dev-1. `/root` is mode 711 for private parity sandboxes.
Raw command output is kept in task-private `.lead/` evidence.
The task branch alone is pushed; installation and main integration are lead-owned.

## Validation

All 18 shell steps from the desktop CI job passed, including workspace Rust
tests, clippy with warnings denied, product proofs, cargo-deny, `tools/check.py`,
534 helper parity cases, English/pseudo galleries, and raster memory regression.
The gallery covered 244 states per language and 185 per language with a wide font.
Focused checks passed 89 Rust Brain tests, the Brain CLI integration, 52 Python
Brain tests, 23 discovery parity runs, public graph parity, and semantic-worker
cache/protocol/idle checks. Formatting, browser syntax, Prettier, catalog coverage
(4,199 messages) and generated assets passed.

Headless Chromium through the real launcher with a private mock compositor:

| Synthetic acceptance | Result |
| --- | --- |
| Cold 2,000-note vault, no index: useful subjects/recent notes | 1,477 ms (< 5 s) |
| Stalled summary: visible error and Retry; Retry succeeds | 8,039 ms to error |
| Connections finish without reloading the page | Pass |
| Restart: data/connections, persisted index unchanged | 1,460 ms (< 2 s) |
| Add note 2,001: connections refresh without reloading | Pass |
| First API 403 retries after auth readiness | Pass |
| Browser authentication/content boundaries | Pass; zero browser exceptions |

The initial source/UI run rejected an uncommitted source overlay; syncing the
committed candidate fixed provenance. Chromium, installed for browser acceptance,
made an optional publisher appear in frozen wallpaper fixtures; hiding its launcher
while running desktop CI restored the prescribed package environment. No frozen
expectations were changed. The semantic fallback fixture now removes its synthetic
persisted graph before testing a malformed worker response, so cache reuse cannot
mask that protocol check. The older browser security harness uses the current
same-origin bootstrap in place of the removed file handoff.

Raw CI, focused-check and Chromium results plus gallery artifacts are retained in
task-private `.lead/evidence/`. Only synthetic notes and private homes were used;
the laptop failure was not reproduced and no live deployment was performed.
