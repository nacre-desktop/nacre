# N0313: Repo bloat deletion batch 1

## Scope

Delete the audit C (2026-11-13 read-only repo audit) items that are replaced
code, dead tools or zero-reference leftovers, re-verifying each one first with
a repo-wide `git grep` (installers, systemd units, desktop files, keybinding
configs, CI, docs) and `git -C ~/Projects/bead.git grep … origin/main` (no
Bead reference to any of them). Item 2 was added after the owner approved
deleting the rollback copies ("yeah you can delete them", 2026-11-13).

Out of scope: item 7 (`writer_lease.py`, installer imports), 8 (`f4_switch`,
live-switch path), 10 (brain service `ExecStart`), 13/14 (screenshots,
evidence logs; coordinator-owned), `tools/bench` and `docs/research` (N0308).

## Result (base `737acfd81`, 115 files outside docs/specs, +219 / −21,116, net −20,897 lines)

| Item | What | Files | Lines | Decision |
| --- | --- | --- | --- | --- |
| 1 | `crates/nacre-helper/src/niri_config.rs` | 1 | −660 | Deleted: never declared by any `mod`; the live renderer is `compositor/niri_config.rs`. |
| 2 | 14 `bin/*.rollback` copies and their consumers | 31 | +136 / −7,025 | Deleted (owner approval). Also removed: the three Brain rollback entries in `ai/install.py` and `tools/configure.py`, the `nacre-ai.py.rollback` scan in `tools/i18n/check.py`, the packaging skip in `stage.py`, the Python-parity tests (`ai/tests/test_brain_sync.py`, `test_shared_skills.py`, `test_translations.py`, `test_chat_titles.py`, `test_shared_chats.py`, the rollback cases of `test_workflow.py`, `test_claude_workflow.py` (the title-hook cases stay), `test_codex_startup.py` and `test_conversation.py` (the memory cases stay), the `python` mode of `test_native_launch.py`, the Python writer parity in `brain_cli.rs`/`brain_taxonomy.rs`, the rollback curses-menu test in `test_terminal_branding.py`, the controller-path test in `test_public_names.py`) and 21 `ai.ui.*` ids. The installer and `nacre-ai*`/`nacre-brain*` tests that exercise live code stay. On existing installs `tools/configure.py` removes its three copies through its file manifest; `ai/install.py` links into the checkout, so its three `~/.local/bin/nacre-brain*.py.rollback` links dangle (nothing calls them). No user doc described the rollback copies; the retirement is recorded here (specs 0022, 0028, 0029, 0135, 0156 said "delete after trusted"). |
| 3 | `sidebar-chat.py`, `composer.py`, `nacre_ai_controller.py` + tests | 7 | +2 / −1,712 | Deleted; `packaging/arch/test-installed.py` no longer imports the Python controller (it could only load a Python `bin/nacre-ai`, which is a shell shim now). The N0407 optional-AI proof keeps its intent: with `nacre-ai` installed it runs the native `nacre-ai --help`; without it, setup must create no `~/.local/bin/nacre-ai`. |
| 4 | `desktop-extras.py` + test | 5 | +16 / −1,080 | Deleted. The last caller, the sidebar workspace handoff (`sidebar_chat/actions.rs`), now calls `desktop_extras::resume(key, terminal=false)` in-process instead of spawning `python3 desktop-extras.py resume`; same `chats.json` record, same kitty `resume-terminal` launch. Behaviour note: the Python resume first focused an existing `nacre-ai-task` window for the same thread; the native port (already what `nacre-desktop-extras resume` does) always opens a new terminal. The integration test fakes `systemd-run`/`kitty` instead of the Python script. The sidebar now reads and writes `chats.json` under `Paths::environment().state` (honours an absolute `XDG_STATE_HOME`), the same file the native chat list and resume use; before, it hard-coded `~/.local/state`, which only matched the Python script. |
| 5 | `welcome.py`, Python `bin/nacre-welcome`, `bin/nacre-settings` + tests | 7 | +35 / −1,302 | Deleted; both bin files are the 3-line helper shim. `test_welcome_demo.py`'s private-scene case now sets the scenes through the native `nacre-helper welcome set-demo-wallpapers` instead of `welcome.py`. Flag parity checked: native `launchers::welcome` accepts `--login/--enable/--disable` and the six page names; native `settings` validates `page[/subpage][#setting]` routes. |
| 6 | `control-tools`, `screenshot`, `desktop-actions`, `device-actions`, `device-location`, `display-recover`, `maintenance`, `weather`, `timezone` `.py` + tests | 24 | +28 / −3,193 | Deleted, with `test_pearl_dashboard_data.py` (weather only) and the two `maintenance.py` cases of `test_compositor_helper_migration.py`. `control_tools.rs` error envelope names `nacre-screenshot`; `tools/releases.py` runs `nacre-helper maintenance`; `install.py` `RETIRED_HELPERS` removes the stale installed copies on upgrade; `test_niri_daily_driver.py` keeps its native desktop-settings cases. |
| 9 | `tools/tests` manual scripts, `plain-close-keyboard.c`, `fixtures/keybindings-contract.json` | 8 | −3,339 | Deleted (not discovered by unittest, zero references). |
| 11 | QML-era benchmark/performance tools, `helper_runtime.py`, `fetch-wallpapers.py`, `i18n/browser-audit.cjs` + tests | 19 | −2,245 | Deleted; `tools/measure-power.sh` kept (benchmark tool). |
| 12 | `tools/capture-aur-review.py`, CI steps "Native AUR review gallery" and "Native gallery evidence", `assets/screenshots/software-phase-two` | 5 | −314 (+210 KB images) | Deleted (`8f41732b4` had already removed two of its four PNGs). |
| 15 | `ai/SSH-PRESENTATION.md`, `ai/fix-fish-presentation.py`, `ai/projects.example.json`, `ai/install-claude.py` | 6 | +2 / −68 | Deleted; `ai/install.py` installs none of them. `shell/layout-example/` went with the Shell lead's `c1f149222` first. **Kept**: `nacre_native_palette.py` (imported by `tools/generate_branding.py` and the `native_*` parity harness), `niri_session_refresh.py` (imported by `install.py` and `releases.py`), the five `*-label.svg.in` (read by `branding.rs` `APPLICATIONS`). The audit's zero-ref claim was wrong for those three. |
| i18n | `en.json` / `context.json` | 2 | −178 | 89 ids whose only source was deleted Python (`tools/i18n/check.py` lists them as unused). |
| **Total** | | 115 | +219 / −21,116 (net −20,897) | With this spec and its README row (+54): 117 files, +273 / −21,116, net −20,843. About 210 KB of PNGs (item 12) removed as well. |

Temporary duplicates: none left by this unit.

## Checks

- Local: `ruff 0.16.10 format --check` and `ruff check` on the 258-file
  `tools/check.py` Python set (pass); `python3 tools/i18n/check.py` (1,674
  owned messages); `sh -n` on the two shims.
- dev-2 Arch container on base `62c41a23c`: `cargo fmt --check`,
  `cargo clippy -p nacre-helper --all-targets --locked -- -D warnings`,
  `cargo test -p nacre-helper -j2` (1,290 passed, 0 failed, 7 ignored, 48
  binaries, including `native_workspace_handoff_closes_provider_before_resume_and_blocks_reconnect`).
- dev-2 Arch container on base `cbc12bd95`: `tools/i18n/check.py`,
  `tools/i18n/generate.py` and the static part of `tools/check.py` (ruff,
  syntax, retired trees, window rules, Lua, greeter, compositor boundary) pass.
- dev-1 Arch container on base `737acfd81`: `cargo build -p nacre-helper
  --examples`, then the four `tools/check.py` unittest suites with the
  candidate helper and engine: `tools/tests` 400 OK (1 skipped), `ai/tests`
  33 OK, `brain/tests` 54 OK, `nacre/shell-tools/tests` 283 OK. The first
  suite run found five `ai/tests` modules and three `shell-tools` tests that
  still loaded the deleted Python; they are removed or ported above.
- Hosted CI on the pushed commit: see the README row.
