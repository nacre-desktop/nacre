# N0221 Launcher overhaul

This spec is extended by later launcher phases.

## N0221a owner fixes

- Add the missing English labels for Codex and Claude Code and render every Settings navigation page in the existing harness, failing on visible untranslated-key-shaped text.
- Read active Niri bindings through the generated `nacre-niri.kdl` overlay, its included `config.kdl`, generated binding includes and final user `custom.kdl`; verify that complete layout in a temporary HOME. Hyprland retains its existing provider, which reads custom and Nacre Lua shortcut sources.
- Launcher hand-offs use the shared scoped app launcher, which passes the compositor activation token when available. When focus cannot be confirmed, display an “Opened <label>” notification through the desktop notification service.
- Validation on dev-1: active overlay/include fixture, token-argument test, Settings all-pages render test, hand-off argv/toast test, and custom page goldens passed. Full shell binary: 879 passed, 4 ignored. Helper library: 1,017 passed, 3 ignored; one colors-preview timing assertion failed in the full run and passed in isolation. Helper and shell clippy, fmt, Ruff 0.16.10, compositor boundary, translation coverage and Python software suite (16 passed) passed.
- Known out-of-scope test issue: `wallpaper_latest_activation_survives_view_retirement` failed once in isolation, while the final full shell binary passed. No wallpaper code changed.

## N0221b keyboard shortcuts

- Learn > Keyboard shortcuts opens the existing `keybindings` Extras view, which consumes the daemon `shortcuts` topic's ordered `{items: {items, order}}` data. It no longer opens the empty launcher menu route.
- Shortcut descriptions prefer Niri's `hotkey-overlay-title`; when absent, the helper derives a readable label from the action and arguments. The active `nacre-niri` process argv is authoritative for config selection, ahead of environment fallback.
- Verification fixture: the live topic shape with 91 ordered synthetic bindings renders its first key and description in the opened keybindings view. No temporary duplicate implementation remains.
- Validation on dev-1: `cargo fmt --check` and touched-crate clippy passed; helper library passed (1,024 passed, 3 ignored). Shell binary: 881 passed, 1 Wayland motion wire test hit its 8-second child timeout under load, and 4 were ignored; the failing test passed alone in 10.81 seconds. Both wallpaper activation tests also passed alone after timing-related failures in an earlier loaded run.
