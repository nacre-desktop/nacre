# N0402 — Nacre-owned shell state

Status: implementation; focused and niri VM gates pending.

Panel state, surface recovery (`ui/`), UI health JSON/locks and widget window
geometry belong in `$XDG_STATE_HOME/nacre/shell` (default
`~/.local/state/nacre/shell`). One XDG helper also supplies presentation reads;
`nacre/presentation.json` remains owned by the daemon. Widget layout is config,
and font/wallpaper caches are outside this state-only unit.

At shell startup, rename the known legacy entries from `bead/` and the old
`nacre/widgets-window.json` without replacing any destination, including under
concurrent startup. Existing destinations leave both entries intact with one
conflict log line per entry. Failures preserve the source; interrupted migration
can resume. Sync the directory chain before moving and both parents afterward.
Remove `bead/` only when empty, preserving unrelated framework apps.
The migration is temporary: delete 30 days after release, planned release
2026-11-12, expiry **2026-12-12** (adjust with the actual release date).

Purge already covers `.local/state/nacre` in `tools/nacre_modules.py:292-297`;
uninstall is unchanged. No framework changes, services or UI changes.
At pinned Bead `ed03c8ee22e5`, Rust/Luau contains no fixed XDG state path:
`crates/bead-ui/src/session.rs:248-249` accepts Nacre's recovery path, and
`crates/bead-core/src/recovery.rs:33-37` writes below its supplied parent.

Tests use a subprocess with a temporary XDG_STATE_HOME, checking absent state,
legacy state, existing destinations and unrelated Bead data. Focused tests,
formatting and clippy run in one fresh dev-1 container; a fresh niri VM proves
purge leftovers without an allowlist change. Hosted CI remains the full gate.
