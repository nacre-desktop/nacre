# N0048: `nacre` CLI and keybinds on one provider-neutral route

Status: **Implemented; pending lead review** (CI and the nested QML check are recorded under Evidence).
Inventory unit N-E1 (`qml-retirement-inventory-20261023.md`, section 8). Consumer: Bead B0026/B0045/B0052.

## The one path

```
keybind / nacre CMD / nacre-settings / nacre-welcome / nacre-colors / nacre-menu
  -> nacrectl ui TARGET METHOD [ARGS...]          (control.sh, generated binds, native helpers)
  -> daemon call ui.dispatch {target, method, args}
  -> the registered `ui` provider                  (QML NacreShellIpc today, Bead ui_provider after the switch)
```

Before this unit the generated binds and most `control.sh` commands already used `nacrectl ui`. The remaining
direct `quickshell ipc` callers for user-facing commands were the app launchers and two helpers; they now use the same path:

| Caller | Old target | New route |
| --- | --- | --- |
| `nacre-settings [page]` (Super+I, Rust `launchers.rs` and the Python reference `bin/nacre-settings`) | `quickshell ipc call settingsApp open PAGE` | `nacrectl ui nacre settings [PAGE]` |
| `nacre-welcome [page]`, `--enable/--disable` | `... welcomeApp open PAGE` / `refresh` | `nacrectl ui welcomeApp open PAGE` / `refresh` |
| `nacre-colors [image]` | `... colorsApp open IMAGE` | `nacrectl ui colorsApp open IMAGE` |
| `nacre-menu ROUTE` (`menu_cli.rs`) | `qs -c nacre ipc call nacre menu ROUTE QUERY` | `nacrectl ui nacre menu ROUTE QUERY` |
| `nacre-menu pick/input` selector prompt | `qs ... menuPrompt open/cancel ID` | `nacrectl ui menuPrompt open/cancel ID` |
| `desktop-actions brightness N` | `qs ... brightness set X` | `nacrectl ui brightness set X` |
| `nacre settings [page]` in `control.sh` | `nacrectl ui nacre settings` (no page) | `nacrectl ui nacre settings [PAGE]` |

QML side (so the current session keeps working): `colorsApp` (`open`, `close`), `welcomeApp` (`open`, `close`, `refresh`) and
`menuPrompt` (`open`, `cancel`) are now registered with the daemon dispatcher (`NacreDaemon.registerIpc`, same pattern as
`pearls`); `nacre settings PAGE` (one argument) is answered in `NacreShellIpc.daemonCall` by `NacrePanelState.openSettings(PAGE)`.
The legacy `quickshell ipc` handlers are unchanged, so any old caller still works.

`nacre settings [page]` is the neutral Settings route because Bead B0045/B0052 already implement exactly it
(`commands::map_views` -> `app::route(settings, open, page)`). The other app routes use the catalog names Bead already lists
(`colorsApp`, `welcomeApp`, `menuPrompt`), so Bead only has to add handlers (see Bead gaps).

## Command map

The machine-readable map is `nacre/shell-tools/shell-routes.json` (tests read it). Note: `nacre/shell-tools/cli.json` is the
palette CLI's user-config seed (`~/.config/nacre/cli.json`), not a command registry; it is unchanged.

### `nacre CMD` (`control.sh`, installed as `~/.local/bin/nacre-shell`)

| Command | Route | Bead |
| --- | --- | --- |
| `launcher`, `wallpaper` (no args) | `nacre launcher ""` / `">wallpaper "` | done (B0026) |
| `menu ROUTE [Q]`, `keys` | `nacre menu ROUTE Q` | done (B0026) |
| `clipboard` | `nacre clipboard` | done (B0026) |
| `palette`, `overview` | `nacre mode X` | partial: menu aliases (B0026) |
| `hide`, `close` | `nacre hide` / `nacre close` | done (B0028, B0026) |
| `settings [page]` | `nacre settings [PAGE]` | done (B0045, B0052) |
| `left` | `nacre left` | done (B0052) |
| `controls [section]` | `nacre controls SECTION` | partial: home, edit, calendar, network, bluetooth, battery, sound |
| `session` | `nacre session` | **Bead gap** |
| `toggle` | `drawers toggle dashboard` | done (B0026) |
| `brightness up\|down` | `brightness step DIR` | done (B0026) |
| `keyboard-light up\|down` | `keyboardLight step DIR` | **Bead gap** |
| `detach`, `pearl [kind]` | `pearls detach` / `pearls open KIND` | done (B0048); `pearl controls` refused |
| `lock-preview` | `qs -p lock-preview.qml` (QML-only, expiry Group F) | retire (lock is hyprlock) |
| `dolphin thunar start scheme-mode wallpaper-image wallpaper ARGS doctor profile check rollback apps updates wifi bluetooth tasks new resume lock files terminal network audio volume focus workspace play next previous mute brain capture` | native helpers, no shell route | n/a |

### Compositor binds (generated from `compositor-contract.json`; niri and Hyprland identical)

| Keys | Command | Route |
| --- | --- | --- |
| Super+Space / W / V | launcher / wallpaper / clipboard | `nacre launcher ""`, `nacre launcher ">wallpaper "`, `nacre clipboard` |
| Super+. , Super+Ctrl+E / Ctrl+C / X , Esc / K , F1 | emoji / capture / power / shortcuts | `nacre menu actions.emoji\|actions.capture\|power\|learn.shortcuts ""` |
| Super+Ctrl+Space, Super+Tab (Hyprland) | palette, overview | `nacre mode palette\|overview` (niri Super+Tab is niri's own overview) |
| Super+O | dashboard | `drawers toggle dashboard` |
| Super+Z | hide | `nacre hide` |
| Super+A | AI sidebar | `nacre left` |
| Super+I | Settings | `nacre-settings` -> `nacre settings` |
| Super+Ctrl+D | detach panel | `nacre-shell detach` -> `pearls detach` |
| XF86MonBrightness Up/Down | brightness | `brightness step up\|down` |
| XF86KbdBrightness Up/Down | keyboard light | `keyboardLight step up\|down` (**Bead gap**) |
| Escape (Hyprland, non-consuming) | dismiss hover edges | Hyprland global shortcut `nacre_shell:dismissHoverEdges`, answered by QML; not an IPC call; Bead has no frame lips yet (B0047) |

Volume, mute, microphone, lock, screenshots, workspace/window and terminal/browser/files binds do not touch the shell.

## Bead gap list (units to do)

1. `nacre session`: map to `bead.panel toggle session` (Bead currently only reaches the session view from launcher power rows).
2. `keyboardLight step up|down`: consume daemon `keyboard_light.adjust` (N0036 is published; B0026 blocker B).
3. `brightness set X`: map to daemon `brightness.set` on the focused output (B0049 tiles already call it; the CLI route is unmapped).
4. `colorsApp open [image]`: Colors app (B0052 on-demand app pattern).
5. `welcomeApp open [page]`, `welcomeApp refresh`: Welcome app (B0052 pattern).
6. `menuPrompt open|cancel ID`: selector prompt view, needed by `nacre-menu pick|input`.
7. Partial rows: `nacre mode overview|palette` (menu aliases only), `nacre controls` other sections, `drawers toggle` other drawers, `pearls open controls`.

## Temporary fallbacks (all expire in the Group F QML deletion pass)

- `legacy_ui` in `crates/nacre-helper/src/daemon/client.rs`: `nacrectl ui` runs `quickshell ipc -c nacre call` only when the
  daemon is absent or no provider is registered (provably undispatched); it maps `nacre settings PAGE` to `settingsApp open PAGE`.
  With Bead as the provider this branch is never taken; delete it when QML is deleted.
- niri generator `shell.shortcut` action (`compositor_config/niri.rs`) still emits `qs ipc call nacreCompositor shortcut`;
  the shipped contract has no such niri binding (test asserts the generated outputs contain no `ipc call`).
- QML-internal probes that read or restore QML view state and are not user commands: `shell_supervisor.rs`, `session_watch.rs`,
  `daemon/session_policy.rs` (`nacre state`), `display_recover.rs` (`restoreViews`, `settingsView`), `lock_info.rs` (`lockWidgets state`),
  `niri_hover_probe.rs`, `tools/install.py validate_live`, and the Python reference `display-recover.py`. They belong to the N-E2
  supervisor unit; they are deleted with QML.
- Compositor window rules still match the Quickshell class `org.quickshell` and the QML window titles (N-E2 scope).

## Tests

- `tools/tests/test_shell_routes.py`: every `control.sh` case is in the registry and vice versa; every registry route is answered
  by a QML `registerIpc` signature with the same argument counts; both rendered backends and the committed bind files contain no
  `quickshell ipc`, `qs ipc`, `ipc call` or `bin/qs`, and every `nacrectl ui` call they contain is a registered route; fallbacks and
  gaps carry an expiry or a Bead unit.
- `crates/nacre-helper/tests/ui_routes.rs`: a fake daemon that stands in for the UI provider receives exactly
  `{target, method, args}` for every example of every registry route through the real `nacrectl ui` binary entry.
- `crates/nacre-helper/tests/daemon.rs`: the legacy fallback still runs only for a proven undispatched call and maps the Settings route.
- Updated: `launchers.rs` unit tests, `test_settings_launcher.py`, `test_welcome.py`, parity cases `nacre-settings`, `nacre-welcome`,
  `menu-cli/route` (declared exception in `menu-cli-reference.md`).
