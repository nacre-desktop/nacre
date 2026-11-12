# N0173 — Timing-independent test acceptance

Status: complete; repeated load checks and the full desktop CI passed. Product code unchanged.

Default tests check behavior rather than how often a loaded host schedules a
debug build. The workspace switch checks advancing frame deadlines, monotonic
capsule motion in both directions, and return to the original position. The
pinned Bead Session has no public controlled clock; its scheduler already has
deterministically stepped unit coverage. The integration test retains its strict
frame count/cadence checks behind `NACRE_TEST_PERF=1` for idle-host measurements.

The picker fake daemon stays runnable during synchronous scene compilation;
same-socket reply barriers replace sleeps around the press/release assertions.
The chat disconnect fixture parks reconnection until the disconnected projection
has been observed. Its previous failure and successful reruns were recorded by
the preceding chat unit. Neither change alters client timeouts or transport code.
The daemon session fixture waits for an actual `READY=1` notification with a
60-second hang bound, replacing the load-sensitive 20-second startup allowance.

## N0303 — daemon READY with held backends

The two readiness integration tests now always verify READY with silent D-Bus
fixtures; a named 30-second hang guard prevents stalled CI. The two-second READY
and socket latency budgets run only with `NACRE_TEST_PERF=1`, matching the
opt-in pattern above. Lock-content and all-backend tests retain their
service/socket behavior checks.

Phase-1 dev-1 comparison (`951278f51` before N0209 → `a0aaf12ae`; 20 runs each;
min/median/max READY ms; debug pinned to four CPUs; shared-host load present):
- Lock-content release: 16/20, 101/293/1646 → 19/20, 99/105/1799; debug: 5/20, 836/1269/1924 → 17/20, 634/696/1906.
- Every-backend-hangs release: 13/20, 75/960/1851 → 15/20, 75/83/1897; debug: 16/20, 450/550/1871 → 20/20, 460/474/487.
- Both revisions flaked under the two-second bound, so hosted CI checks ordering/behavior while strict latency stays opt-in; no production readiness ordering changed.

The migration's 2,000-note content, privacy, idempotence and sequential/all tree
equivalence checks remain in the default suite. Its two 60-second limits move to
the ignored `synthetic_vault_migration_performance` test. Brain's browser-load
test keeps content, auth retry, visible timeout/Retry, unchanged persisted index,
and live vault-change checks; first-view/restart speed gates require
`BRAIN_TEST_PERF=1`. Real browser waits use generous hang bounds. Search-quality
coverage is already timing-independent and its latency gate is already ignored.

The historical picker failure was a shell test, despite its attribution to the
helper in the task. Reachable rollback history already excludes Python bytecode
from fingerprints and tests that exclusion. The later restore-failure traceback
was not retained; no second distinct 120 Hz count test could be identified in
the setup rollback history. No temporary duplicate implementations or evidence
files are introduced.

## Validation

Repeated fresh test processes in the same disposable Arch container (8 CPUs,
16 GiB), with `nice -n 10 stress-ng --cpu N --cpu-method matrixprod` running
concurrently. Rust binaries used an exact filter and `--exact --nocapture`;
the runner rejected filters selecting zero tests. Cargo used `--locked -j2`.

| Test | CPU workers | Before failures/runs | After failures/runs |
| --- | --- | --- | --- |
| `workspace_switch_ticks_every_refresh_at_120_hz_in_both_directions` | 10 | 41/50 | 0/50 |
| Same workspace test | 6 | 0/50 | 0/50 |
| `current_scene_picker_dispatches_once_on_pointer_release` | 6 | 0/50 | 0/50 |
| `a_dropped_daemon_link_reopens_the_worker_and_resyncs` | 6 | 0/50 | 0/50 |
| `daemon_owned_session_completes_notifies_and_restores_dnd_after_a_restart` | 6 | 0/50 | 0/50 |
| `search_quality_gate_on_2000_synthetic_notes` (unchanged) | 6 | 0/50 | 0/50 |
| `synthetic_vault_dry_run_then_all_is_idempotent_and_preserves_content` | 6 | 0/10 | 0/10 |
| `brain/tests/browser-load.mjs` | 6 | 0/10 | 0/10 |

All 41 failures at ten workers were frame-count assertions, with no VM guard
failures. An exploratory sixteen-worker baseline produced 48 frame-count failures
and two Bead 50 ms VM safety-budget failures in 50 runs; that overload is separate
from the matched proof and the product safety budget remains unchanged. Other
historically reported failures were not reproduced in the six-worker baseline.

The baseline was `238f4f76c`; the test patch was rebased unchanged onto the
published AI launch fixes before final CI (`ad2a83994`). Every command from the
`desktop` job in `.github/workflows/check.yml` passed in the Arch container:
formatting, Clippy, workspace tests (helper 1,004 passed/3 ignored; shell 871
passed/4 ignored), feature builds, Luau/product checks, cargo-deny, translations,
`tools/check.py`, AUR gallery and the explicit ignored raster-memory regression.
The Python suites passed 406 tools tests (one skip), 146 AI, 54 Brain and 377
shell-tools tests. The unchanged package restore fixture additionally passed
50/50 runs with six CPU stress workers. Raw evidence remains outside tracked files.

Product lines: +0/-0. Test lines: +111/-23 (net +88); no duplicate implementation remains.
