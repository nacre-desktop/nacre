# N0134 — Brain viewer load robustness (U12)

Status: source implementation complete; remote validation blocked.

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

The container is named `n0134`; builds run under tmux, nice 10, Cargo `-j2`.
The task's conflicting dev-1/dev-2 host requirements must be resolved before
heavy work. Raw command output is kept in task-private `.lead/` evidence.
The task branch alone is pushed; installation and main integration are lead-owned.

Source changes and the synthetic browser harness are committed, but none of
the runtime or timing acceptance checks has run for this revision. The later
standing rule restricts builds to dev-2, which is unreachable; using reachable
dev-1 requires the owner's resolution of that conflict. There are no accepted
performance measurements, and this unit is not ready for installation.
