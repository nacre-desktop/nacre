# N0402 — Nacre-owned shell state

Status: complete on branch `release/n0402-state`.

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
Migration errors are logged and do not prevent shell startup.
The migration is temporary: delete 30 days after release, planned release
2026-11-12, expiry **2026-12-12** (adjust with the actual release date).

Purge already covers `.local/state/nacre` in `tools/nacre_modules.py:292-297`;
uninstall is unchanged. At pinned Bead `ed03c8ee22e5`, UI recovery accepts the
caller path (`crates/bead-ui/src/session.rs:248-249`, saved at `:275-277`) and
crash history uses the path passed to `Run::begin`
(`crates/bead-core/src/recovery.rs:180-190,218`). Nacre supplies both paths:
`crates/nacre-shell/src/reload.rs:33` for `ui/`, and
`crates/nacre-shell/src/main.rs:576-577` for `ui-health*`. Bead has no fixed
state path writer at this revision.

The migration test uses a subprocess with temporary `XDG_STATE_HOME`; it covers
empty state, old-only state, conflicts, symlinks, interrupted-write temps and
unrelated Bead data. A main-binary test injects a migration error and confirms
startup proceeds. Focused state/widget tests, formatting and clippy passed on
f8cf445a; the follow-up state and startup-error tests passed on 81af639a.

The VM-only Nacre packages used `CARGO_PROFILE_RELEASE_LTO=false`,
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `CARGO_BUILD_JOBS=4`,
`RUST_MIN_STACK=16777216`, and `MAKEFLAGS=-j4`. For the helper compile, the
temporary PKGBUILD used `-j1` after a `-j2` lean-profile attempt aborted under
the 12 GiB container cap. The repository release profile is unchanged.

The niri VM stages were: prepare PASS (88.8s), inventory PASS (0.8s),
repository PASS (1.9s), bootstrap-plan PASS (0.1s), bootstrap-install PASS
(236.2s), setup PASS (114.5s), session PASS (30.7s), update SKIP (0.2s),
uninstall PASS (4.8s), package-removal PASS (5.0s), leftovers PASS (0.3s).
The test package archives, VM evidence and logs remain under the task's ignored
`.lead/evidence/`; temporary dev-1 files and containers were removed.

## N0402b — upgraded pearl state and purge

Migration also moves `pearls.json`, `chat-pearls.json`, their `.unreadable`
side files, and `<name>.<pid>.tmp` saves. As before, `RENAME_NOREPLACE`
preserves both copies on conflict. `uninstall --purge` removes only the known
legacy shell entries (including the owned `ui/` tree) under `state/bead`, lists
them in its plan, and removes that directory only when empty; unknown entries
remain. The migration remains temporary and expires **2026-12-12** with N0402.
