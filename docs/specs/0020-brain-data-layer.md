# N0020 — Brain data layer: nacre-brain, brain-sync, brain-maintain

- Status: Implemented (pending lead review)
- Date: 2026-10-21
- Plan: AI/Brain/Wave-4 migration plan 2026-10-21

Goal: native `nacre-brain` CLI (init/path/search/note/secret refs/run; `open|capture` keeps delegating to the UI
launcher), `nacre-brain-sync` (timer every 2 minutes) and `nacre-brain-maintain` (daily backup/check, restore,
resolve), sharing one Rust vault module (note-path rules, atomic write, lock, digest).
Scope: 233 + 373 + 162 LOC, and `nacre-ai-memory` vault writes stay Python until N0021 but call the new CLI.
Preserve exit codes (75 for nothing to do), status-file schema read by Settings, diagnostics.rs and
maintenance.py, three-way merge, conflict files, no delete propagation, backups. Non-goals: web server,
semantic search, systemd unit placement (stay in HOME), new sync features.
Dependencies: none. Parity: `nacre-brain`, `nacre-brain-sync`, `nacre-brain-maintain` frozen cases plus
`ai/tests/test_brain_sync.py` assertions; private SSH fake peer; disposable vault copy only.
Risk: high (user data; every AI session calls the CLI). Rollback: keep the Python entrypoints renamed `.rollback`
for one soak, swap by a single rename. Size: M-L. Parallel: yes (disjoint from N0018/N0019).
Stop: any divergence in merge output against the frozen references.

## Common rules

Common to every unit (not repeated): base is latest main; `[no-publish]` on every commit; private/nested
sessions only; nice 10, cargo -j2; frozen parity references under `tests/parity/helpers/<name>` are replayed
with `run.py --command 'nacre-helper {name}'` and never regenerated; historical Python test assertions are
kept as Rust tests; security additions are explicit new cases with intended-difference notes; no live
desktop, installed Nacre, units, keybindings or publication; existing AI permissions and the paru command
unchanged; standalone native command ships first, daemon hosting is a separate unit. Status for all: Proposed.
Numbers are proposals; N0017 is the last used.

## Evidence

- The three entrypoints dispatch to standalone Rust commands sharing `brain_data`;
  UI delegation and HOME unit placement stay intact. Python `.py.rollback` sources
  are retained, including memory's raw-metadata validator until N0021.
- The full `desktop` job from `.github/workflows/check.yml` passed in the Arch CI
  container, at nice 10 with Cargo `-j2`: formatting, Clippy, builds, Rust tests,
  protocol consumer, `tools/check.py`, translations and both galleries. Python
  suites: tools 441 (one opt-in Wayland soak skipped), AI 116, Brain 45, shell-tools 478.
  Raw log: `.lead/desktop7.log`; tested source hashes match `.lead/code.sha256`.
- Explicit native replay passed all 10 frozen Brain cases (4 CLI, 3 sync, 3 maintain);
  the full corpus passed 534/534. No frozen reference was regenerated. Historical
  sync assertions are retained in Rust, alongside concurrent CLI privacy and
  non-reading-child deadline regressions.
- `verify_brain_native.py` passed five Python/native exchange comparisons, including
  Unicode/backslash names, conflict evidence and backup digests/content. Disposable
  fake SSH/keyring/terminal checks cover bidirectional sync, no deletes, exit 75,
  backoff, error status, secret whitespace, child environment and prompt cancellation.
- Explicit security cases reject linked state/lock targets and malformed peer
  replies; overflowed retry counters saturate. Status follows the standard absolute-XDG
  rule. No merge divergence, daemon hosting or live deployment occurred. CPU/RSS
  benchmarks and a live soak were not run.
