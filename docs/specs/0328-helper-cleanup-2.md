# N0328: helper cleanup pass 2

Date: 2026-11-14. Platform-owned dead or redundant code in `crates/nacre-helper`,
`crates/nacre-protocol` and Platform scripts, after N0310 (audit B) and N0313 (audit C batch 1).
Behaviour-preserving: every item below has no runtime caller. Out of scope: nacre-shell, shell/
Luau, installers, config defaults, the files N0323/N0324/N0325 are editing tonight, and the lock
pre-render path.

## Method

- Every CLI arm in `main.rs` was grepped as a word and as a quoted string across the whole repo
  (shell/, config/, bin/, tools/, nacre/, ai/, brain/, packaging/, .github/, crates/nacre-shell,
  Python adapters under `nacre/shell-tools/tests` and `tools/tests`, docs). Candidates were then
  read at each hit: `install.py`'s `RETIRED_HELPERS` list only removes stale installed Python copies,
  so it is not a caller. Two grep misses were caught this way and kept (see below).
- Examples, `native_*.py` adapters and fixtures were matched by file name (`target/debug/examples/<name>`
  and `include_str!` paths are literal everywhere).
- nacre-protocol: every `pub` fn/const/type was grepped as a word outside `nacre-protocol/src`.
- Compiler scan: a scratch copy turns every `pub` in the helper library into `pub(crate)` and folds
  `main.rs` in as the only public root, so rustc's `dead_code` lint reports what the binary cannot
  reach (`cargo rustc --lib --profile check -- -W unused_crate_dependencies`, and again with
  `--profile test`). See "Compiler scan" for what it found.

## Removed

| Item | Evidence | Lines |
| --- | --- | --- |
| `pearl-state` command: `pearl_state.rs`, `pearl-state-reference.json`, `native_pearl_state.py`, 2 i18n ids | Only driver was `test_shared_helper_protocols.py`, deleted in N0313; the adapter is imported by nothing; the shell restores pearls itself (`pearls.rs`). The `nacre-state-files.json` owner string stays (descriptive; nothing reads it). | -421 |
| `light-devices` CLI (`run`, the argparse emulation, `execute`, `native_run`), 2 i18n ids | No caller (help text still says "native QML owns reads"); the daemon calls `light_devices::{discover, write, sys_root, keyboard_cached}` in-process. | -101 |
| `dashboard-system` arm, `execute`, `terminate`, `require_identity`, their 2 tests, 4 i18n ids | No caller of the arm (shell/Luau `dashboard-system` is a view name); the daemon's `dashboard_metrics` uses `dashboard_system::snapshot` only. The end-process path had no other caller. `core-services.md` sentence updated. | -110 |
| `window-chat-title`, `resource-state` arms and their `execute` | No caller; the daemon uses `window_chat_title::resolve_info` and `resource_state::sensors`/`gpu_inventory` directly. | -26 |
| `diagnostics report` arm | No caller; `nacre-maintenance --report` and the menu's report action call `diagnostics_native::report` directly. | -7 |
| `compositor-backends-{hyprland,niri}-config` aliases | Names of the retired Python modules; `nacre_native_config.py` calls `hyprland_config`/`niri_config` (kept). | 0 (2 lines shortened) |
| 12 examples: `colors-parity`, `compositor-config-parity`, `compositor-reference`, `desktop-settings-parity`, `diagnostics-parity`, `diagnostics-production`, `menu-profile`, `service-fixture`, `systemd-report-parity`, `wallpaper-media-parity`, `pearl-window-parity`, `pearl-window-reference` | No test, adapter, doc, spec or CI reference by name (the Python drivers went in N0310/N0313; `pearl-window-parity`'s only driver was the orphan `native_pearl_window.py`). CI's `cargo build --examples` compiled them on every push. | -1,146 |
| `parity-api.rs` trimmed to `demo-wallpapers collection` | Its only driver, `native_demo_wallpapers.py`, calls that one operation; the other ~30 operations served deleted Python tests. | -269 |
| `desktop-settings-state-schema-input.json` | Read only by the deleted `desktop-settings-parity` example. | -17 |
| `native_pearl_window.py`, `native_shell_supervisor.py`, `orient-roles.json` | Imported or read by nothing (`orient-roles.json` never, since the initial commit). | -231 |
| nacre-protocol `framing::Reader::{trim_idle, retained_bytes}` + 2 tests | No caller outside their own tests since the shell left the Bead repo (`git log -S trim_idle`); reading is unchanged. Not part of the exported schema (`export --check` unchanged). | -86 |
| `config/uwsm/sessions/niri-plain.kdl` (lead request from Clasp's C0029: binds Ctrl+Alt+Delete to quit) | Nothing loads it: `env-niri` sets `NIRI_CONFIG` to the generated configs, the session entries start `niri.desktop`, `nacre session niri` or `nacre-niri-session`, and no file in nacre, Bead or nacre-niri names it. Added in 71f8cda29 as a manual phase-1 bootstrap config; the wholesale `config/uwsm` installs copied it as an inert file. No test or fixture names it. | -14 |

## Kept after checking

- `generate-theme-bundles` example: regenerates `theme-bundles/catalog.json`, which a `theme_bundles` test pins.
- `hyprland_config` / `niri_config` arms: `nacre_native_config.py` builds the names (`backend + "_config"`).
- `compositor-compat`, `window-hide`, `software-tui`, `idle-brightness`, `session-watch`: called by
  `config/hypr/scripts/*.sh`, generated `.desktop` files, the hypridle config the daemon writes, and `startup-apps.py`.
- `serve` / `helper-service` and `daemon/serve_bridge.rs`: documented as the legacy JSON-lines
  interface "for one release"; no in-tree client, but retiring it is a compatibility decision (lead).
- `bin/nacre-brain-queue.py`: no importer since the rollbacks went (N0313), but `ai/install.py` and
  `tools/configure.py` install it, so removing it needs an installer edit (lead).
- `bin/nacre-{control-tools,desktop-actions,device-actions,device-location,display-recover,screenshot}`:
  3-line shims with no caller; `install.py` copies `bin/` wholesale and `control.sh` puts that dir on
  its PATH, so they stay until the installed command set is reviewed.
- QML-era `qs ipc` fallbacks in `session_watch.rs`, `lock_info.rs`, `display_recover.rs` and the niri
  `NACRE_QS_*` launcher: they still run (spawn, fail, log; `display_recover` polls 15 s), so removing
  them changes runtime behaviour. Follow-up.
- Fixture provenance notes (`menu-*-reference.md`, `lock-artwork-provenance.json`) and
  `shell-tools/tests/fixtures/hardware/laptop.json` (named by `docs/development/hardware-testing.md`).
- `brain/tests/verify_native.py`, `verify_semantic_native.py`, `browser-*.mjs`: manual Brain checks
  named in spec 0024 / audit C; not run by CI.
- Atomic writers (`branding`, `login_appearance`, `brain_data`, `wallpaper_media`, `sidebar_chat`,
  `launchers`): they differ in file mode, temp prefix and link checks, so collapsing them is not
  behaviour-preserving.

## Compiler scan

The scan runs as the last step of the acceptance job (after this unit's commits were final), so its
findings are follow-up input for the next pass, not part of this diff; the unit report lists them.
`cargo tree -d` was not re-run for this unit: no dependency changed (all helper dependencies still
have a use).

## Line delta

Four code commits: +25 / -2,461, net -2,436 lines over 32 files (`nacre-helper/src` +8 / -427,
`nacre-helper/examples` +15 / -1,430, fixtures -215, Python adapters and test data -271,
nacre-protocol -86, i18n -16, config -14). With this spec and its README row the unit is still
net negative by about 2,350 lines. No temporary duplicates were added.

## Checks

One dev-1 container, `nacre-ci/arch-nacre:current`, offline cargo cache, cargo -j2, base 86433e4a4
(the four commits rebased onto main afterwards without conflicts; no upstream commit uses a removed item):

- `cargo fmt --all -- --check`: one blank line in `window_chat_title.rs`, fixed in the first commit.
- `cargo clippy -p nacre-helper -p nacre-protocol --all-targets --locked -- -D warnings`: clean.
- `cargo build -p nacre-helper --examples`: clean.
- `cargo test -p nacre-helper --no-fail-fast`: 1,307 passed, 3 failed, 6 ignored over 45 binaries.
  The failures are outside this diff: `lock_dashboard` `complete_cold_panels...` and `text_masks...`
  download FreeType for their frozen renderer, which `--network none` blocks; `dashboard_extras`
  `clipboard_topic_copy_and_clear...` timed out under load and passes alone 2/2.
- `cargo test -p nacre-protocol`: 124 passed. `cargo run -p nacre-protocol --example export -- --check`
  and the minimal consumer: pass.
- `cargo build --release -p nacre-helper`, then the ignored "Daemon raster memory regression"
  (`--test daemon memory:: -- --ignored`): passes when it starts (settled RSS 52-53 MiB, gate 60);
  with dev-1 at load average 114 it failed 3 of 5 runs, each time at the daemon READY wait (~17 s,
  before the first phase), which this diff does not touch. Hosted CI runs it on its own runner.
- ruff 0.16.10 format/check on the 256-file `tools/check.py` Python set, `tools/compositor_boundary.py`,
  `tools/i18n/check.py` (1,702 messages): pass. `tools/i18n/generate.py` needs `npm ci` (no network);
  its outputs only carry `login.*`, `ui.brand-logo.*` and `brain.ui.*` ids, none of which changed.
- `nacre/shell-tools/tests` `test_welcome_demo.py` (drives the trimmed `parity-api`): 5/5 OK, run as
  `tools/check.py` does (discover from the repo root, its `test_environment()`).
- `niri-plain.kdl`: config-only, no test or fixture names it; covered by the grep evidence above.
