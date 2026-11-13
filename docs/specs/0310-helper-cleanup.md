# N0310: nacre-helper cleanup from audit B

Date: 2026-11-13. Scope: the non-daemon helper items 2, 3, 5, 7, 8 and 9 of the read-only
audit B (Nacre 5cb5e22a2). Out of scope: the Rust-to-Python bridges (item 4), the Python
originals (item 6), `bin/*.rollback` (item 1) and everything under `src/daemon/`.
Re-checked against origin/main e1022abd2 before applying; none of the items had been done
by N0309, N0311-N0319 or N0313.

## Per-item status

| Item | Status | What changed |
| --- | --- | --- |
| 3 fsync | Done | `classic_state::atomic_write` (desktop.json, compositor and lock publishers), its byte twin `atomic_write_bytes` (deleted) and `lock_prepare::atomic` call `files::atomic`, which fsyncs the file before the rename. The lock cache keeps its 0700 directory; `tempfile` already creates 0600 files. No parent-directory fsync was added: it would touch all ~50 `files::atomic` callers (daemon included) and only makes the rename itself durable, not the content. |
| 5 dead subcommands | Done | Removed `shell-environment` (module, test, i18n key), `niri-acceptance-monitor` (monitor + systemd modules, parity example, fixtures, help asset, two Python tests; install.py already lists it in `RETIRED_NIRI`, no unit installs it), `state-file` (`state::run` and its two daemon bridges), and the thin `input-preferences`, `kde-palette`, `terminal-font`, `file-icons`, `lock-dashboard` arms. `pearl-state` stays: `test_shared_helper_protocols.py` drives it. |
| 7 unused crates | Partly | `wayland-protocols-wlr` removed (only nacre-shell uses it). `gif` stays: the audit missed `wallpaper_media::animated_gif` (`gif::DecodeOptions`). |
| 8 duplicate versions | Partly | `toml` 0.9 -> 1 removes the duplicate `toml`, `toml_datetime` 0.7 and `winnow` 0.7. `nix` not bumped: `orient-decoder` (deps/orient, Orient repo) also requires 0.30, so a helper-only bump removes nothing. `bit-set`/`bit-vec` 0.8 not touched: the newest fancy-regex (0.20.2) still requires bit-set 0.8, and proptest 1.9 (nacre-protocol dev-dependency) pulls it too. |
| 9 regexes | Done | `brain_registry::valid_name` compiles its five regexes once (`LazyLock` statics). |
| 2 protocol compiled twice | Done upstream | Landed on main while this unit was open (nacre-shell uses the workspace path); this unit only re-merged Cargo.lock. See below. |

Callers were verified on origin/main across the whole repo (shell/, nacre/, tools/, bin/,
config/, packaging/, crates/nacre-shell, installers, units, desktop files, docs) and in the
Bead repo (`git -C ~/Projects/bead.git grep <name> origin/main`: no hits).

## Coverage kept

- `state-file`: the CLI-level tests in `tests/daemon.rs`, `test_settings_storage.py` and
  `test_versioned_state.py` drove only the removed client. The writer behaviour stays covered by
  the state.rs unit tests (concurrent fields, one-time migration backup, future-version refusal)
  and the remaining `test_versioned_state.py` desktop migration test.
- `kde-palette`: the frozen `kde-palette-reference.json` unit test in kde_palette.rs seeds custom
  kdeglobals content and compares the exact published files and modes.
- `input-preferences`: `test_settings_input.py` was the only check of the live (daemon-called)
  `software_input::switch_layouts` validation and rollback. It is replaced by the unit test
  `invalid_or_failed_layout_changes_leave_no_preference` (skips inside a niri session).
- `terminal-font`: the developer adapter `native_desktop_settings.py` only records the font
  event now; `test_desktop_settings.py` mocks it.

## Item 2: nacre-shell pinned nacre-protocol by git rev

`crates/nacre-shell/Cargo.toml` had `nacre-protocol = { git = ".../nacre", rev = "e147c8f1" }`
although both crates are workspace members, so every build compiled the crate twice. The pin came
in with 1fcdda9ff (2026-10-15), when nacre-shell lived in the Bead repo; B0214 moved the shell into
Nacre and the pin was only bumped by hand. It was switched to the workspace path on main after
e1022abd2, so this unit did not touch it.

## Line delta

Net across the five code commits (without this spec): +61 / -4,448 lines over 32 files
(`nacre-helper/src` +44 / -1,314; tests, examples, fixtures and Python tests the rest; Cargo.lock
+14 / -45).

Duplicate crate names in Cargo.lock (names with more than one version): 41 -> 38 for this unit alone (`toml`,
`toml_datetime`, `winnow`); 37 after the rebase onto main, which also dropped the git copy of
`nacre-protocol`. `cargo tree -d --workspace` before the rebase: 42 names.

## Checks

dev-1, `nacre-ci/arch-nacre:current`, offline cargo cache, on the pre-rebase tree:

- `cargo fmt --all -- --check`: rustfmt rewrapped the five regex statics in brain_registry.rs
  (folded into the regex commit); clean afterwards.
- `cargo clippy -p nacre-helper --all-targets --locked -- -D warnings`: clean.
- `cargo build -p nacre-helper --examples`: clean.
- `cargo test -p nacre-helper`: 1070 unit tests and every integration binary pass. In the first
  run `desktop_settings::direct_user_manager_call...` failed only because the job's target dir was
  not `<repo>/target`, where that test looks for its example binary; rerun with the target in the
  workspace: `desktop_settings` 13/13.
- Python shell tools (`test_file_manager_style`, `test_versioned_state`, `test_desktop_settings`,
  `test_niri_daily_driver`): 31 tests pass with the target in the workspace.
- Python: `test_translations` (21), ruff format and check (the unused `os`/`subprocess` imports
  left in `test_versioned_state.py` were removed), `tools/i18n/check.py` (1704 messages) and
  `tools/compositor_boundary.py`: pass.
