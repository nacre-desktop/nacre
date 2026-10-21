# N0020 — Brain data layer: nacre-brain, brain-sync, brain-maintain

- Status: In progress (Codex worker, lead review)
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
