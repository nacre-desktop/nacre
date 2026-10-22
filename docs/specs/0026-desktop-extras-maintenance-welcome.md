# N0026 — Desktop extras, maintenance, and welcome

- Status: In progress
- Date: 2026-10-22
- Size: M-L (draft estimate: about 1,538 LOC across the three source helpers)
- Parallel: no

**Goal.** Port `desktop-extras` (clips, pins, bindings, chat list, Brain
search, and menu routes), `maintenance` (state, doctor, profile, check, and
rollback through `diagnostics.rs`), and the `welcome` preferences/login-claim
helper to native `nacre-helper` commands. Preserve standalone argv0 dispatch
and `bin/` shims, following N0018–N0025.

**Dependencies.** N0020 (Brain search) and N0021 (chat list and memory). Reuse
their native APIs; do not add competing storage or chat discovery paths.

**Frozen parity.** Replay `desktop-extras`, `maintenance`, and `welcome` under
`tests/parity/helpers/` with
`run.py --command 'nacre-helper {name}'`. Never regenerate references. Keep
historical Python assertions as Rust tests and add explicit security cases with
intended-difference notes. Preserve the locked Welcome state-file behavior.

**Behavior and bounds.** Removing a clipboard clip also removes its thumbnail
without requiring the view to reopen. Retain clipboard history for seven days
while preserving pins. The native clipboard watcher stores only cliphist IDs
and capture times in private state because the installed cliphist interface
does not expose per-entry timestamps. Existing rows without timestamps receive
a seven-day grace period from first native observation. This timestamp index
contains no copied text or image bytes and ages out entries when their clips are
deleted. Leave the existing 30-day notification retention unchanged. Keep
maintenance recovery explicitly user-requested and route its existing
diagnostics actions through `diagnostics.rs`.

**Intended security difference.** Native thumbnail files are created exclusively
with mode `0600` before bytes are written, and deletion removes the matching
thumbnail immediately. This removes the Python implementation's umask-dependent
write window and stale-thumbnail window. Rust regressions assert the private
mode, byte contents, and immediate removal. Brain search continues to reject
hidden files and symlinks; path traversal remains refused.

**Non-goals.** Dashboard redesign, Welcome UI changes, notification retention
changes, and daemon hosting (a separate unit).

**Temporary duplicates and expiry.** During the trust period, the Python
originals `nacre/shell-tools/desktop-extras.py`,
`nacre/shell-tools/maintenance.py`, and `nacre/shell-tools/welcome.py` remain
beside their native replacements for historical parity/source assertions and
fallback callers. Delete each in clean-up pass (a), 7 days after trust, once
callers and parity checks no longer depend on it. This unit creates no
`.rollback` copies; no rollback duplicate is justified because the originals
remain available through the same trust period.

**Acceptance.** Exact frozen parity for all three helpers; historical Python
assertions retained as Rust tests; focused shell-tools suite and the full
`desktop` job in `.github/workflows/check.yml` pass in the dev-1 Arch CI
container. Report parity counts, security differences, any remaining Python
callers, exact commands/results, and exact-head CI evidence. No push or live
installation.
