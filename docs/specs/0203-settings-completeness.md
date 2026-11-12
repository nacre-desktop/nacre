# N0203 — Settings completeness

## Goal

For v0.1, every visible setting must have a reader that matches its label. Dead
controls are removed; planned records assigned to upcoming feature units remain hidden
until those readers land. The shell
registry is now canonical at `shell/settings/registry/`.

## Audit findings and fixes

| Problem rows | Code finding and fix |
| --- | --- |
| Registry copies: all settings/navigation | The UI already loaded `shell/settings/registry`; the helper embedded a drifting copy. Merged helper-only `wallpaper.collection.collectionWallpapers` (gated by `workspaceCollections`), pointed helper validation/route lookup, `bin/nacre-settings`, i18n coverage and tests at the shell registry, and removed `nacre/shell-tools/settings-registry/`. `nacre-settings` now resolves the active source-installed Bead snapshot through `UiShell.active_launcher()`, with checkout and packaged paths as fallbacks. The UI bundle is staged from `shell/`. Formatting and key order differences for `scrollSpeed` had identical meaning. |
| `colors.palette.mode`; `windows-workspaces.overview.nativeOverview`; `launcher.interface.nativePalette`; `privacy.history.nativeClipboard` | Rechecked each ID/store key: no reader exists. Removed rows and translations. Removed stale `dependsOn` references from launcher sources and `livePreviews`, leaving those existing controls usable. |
| `keyboard-mouse.scrolling.scrollSpeed`, `.mouseScrollSpeed`, `.touchpad.touchpadSpeed` | Hyprland has direct mouse and touchpad scroll factors, so added both option mappings; touchpad pointer speed remains Niri-gated. Hyprland's [input options](https://wiki.hypr.land/configuring/core/config-options/) document `input.scroll_factor` and a separate touchpad factor. Rechecked `niri_config.rs`: Nacre's `scrollSpeed` emits Niri `touchpad.scroll-factor`, matching the setting label; Niri's [input reference](https://github.com/niri-wm/niri/wiki/Configuration%3A-Input) specifies independent mouse and touchpad scroll factors. |
| `windows-workspaces.windows.gapsOut`, `.rounding` | Rechecked compositor option lists and mappings: `gapsOut` was already hidden where unsupported (`desktop.gapsOut` is absent for Niri); rounding is mapped for both Niri and Hyprland. No gate changes needed. |
| `appearance.fonts.monoFont`; `date-time-language.locale.weatherLocation`; `panels/top-bar.panel.workspaceWindowHints`; `panels/dashboard.pages.editor.pages` | Narrowed the descriptions to Kitty terminals, city lookup, the active workspace, and core dashboard pages respectively. The active weather description ID is the city-only hint; the reader does not parse coordinates. |
| `windows-workspaces.workspaces.workspaceCount`, `workspaceNames`, `workspaceIcons`; `panels/top-bar.panel.clockFormat`, `.tray`; `power.hardware.alerts` | Verified the Niri profile policy, workspace row renderer, bar readers and battery-alert behavior. Enabled these; gated workspace profile controls on `niriWorkspaces`. |
| `apps.startup.startup` | Profile validation and startup execution work, but the Settings editor marks startup lists unavailable (`settings.rs` entity type guard). Kept it planned until a supported editor exists. |
| Planned rows with no reader: `panels.lips.lipSize`, `panels/bottom.panel.enabled`, `panels/top-bar.panel.modules`, `keyboard-mouse.mouse.shakeCursor`, `privacy.history.{notification,clipboard,screenshot,aiChat}Retention`, `power.saver.batterySaver`, `power.idle.{dim,sleep}`, `power.hardware.{lid,button}`, `wallpaper.motion.lockWallpaper`, `colors.palette.automatic` | Confirmed no v0.1 reader. Per owner decision, restored all 15 registry rows and translations with `backend: planned`; they remain hidden while N0211–N0214 build their readers. |
| Dashboard preferences: `lyrics`, `albumPalette`, `homeZone`, `homeCity`, `calendarFile`, `focusMinutes` | The five settings before `focusMinutes` have readers in media metadata, dashboard home-time preferences and calendar loading, so restored them. `albumPalette` had a stored value but media metadata hard-coded false; now the existing reader honours the stored boolean. `focusMinutes` has no settings reader: the dashboard focus card and launcher timer use their own fixed choices, and reminders do not consume this preference. Removed it and its stale navigation aliases; legacy `dashboard.json` values are ignored. |
| `launcher/entries.editor`, `system.backup`, `system.restore` | No matching action handler exists; removed the three broken action rows and their translations. `lock-login.preview`, `system.updates`, and `appearance.fonts.installed` are hidden by the existing `None` dispatch; `lock-login.idle-policy` remains planned and filtered, so none renders as a dead row. |

## Planned, being built (N0211–N0214)

These records are hidden with `backend: planned` until their readers land:

- `panels.lips.lipSize`, `panels/bottom.panel.enabled`, `panels/top-bar.panel.modules`
- `keyboard-mouse.mouse.shakeCursor`
- `privacy.history.notificationRetention`, `clipboardRetention`, `screenshotRetention`, `aiChatRetention`
- `power.saver.batterySaver`, `power.idle.dim`, `power.idle.sleep`, `power.hardware.lid`, `power.hardware.button`
- `wallpaper.motion.lockWallpaper`, `colors.palette.automatic`

## Candidate features (not in v0.1)

- `panels/dashboard/calendar-focus.editor.focusMinutes` (the dashboard card and launcher use fixed timer choices; reminders have no reader for this preference).

## Verdict counts

- Settings: 138 unique rows after copy merge; 117 active reader-backed rows, 16 planned rows (including `apps.startup.startup`), and 5 removed rows (4 dead visible and 1 dashboard preference without a reader). The other five dashboard controls are restored or wired. No known partial/mislabeled findings remain unresolved.
- Actions: 32 audited; 25 live, 3 intentionally hidden unavailable actions, 1 planned hidden action, and 3 broken rows removed.

## Checks

- Passed on dev-1 in one fresh N0203 container; `cargo fmt --check` and `cargo clippy -p nacre-helper -p nacre-shell --all-targets --locked -j2 -- -D warnings` succeeded.
- `cargo test -p nacre-shell --bin nacre-shell settings_tests:: --locked -j2 -- --test-threads=2`: 17 passed with golden updates and 17 passed without updates. `settings_pages_tests::custom_page_goldens` updated 1 golden test; the full `settings_pages_tests::` suite passed 27 tests; the colors registry test passed 1.
- `cargo test -p nacre-helper --lib daemon::settings_validation::tests:: --locked -j2 -- --test-threads=2`: 5 passed, including the UI/helper source assertion and frozen validator cases. The Hyprland scroll mapping test passed 1.
- `cargo build -p nacre-shell --bin nacre-shell --locked -j2` passed. Pinned Luau 0.742 nonstrict analysis passed (`LUAU_ANALYZE_OK 189 modules`; existing advisory diagnostics remain); `python3 tools/i18n/check.py` passed with 1,710 owned messages.
- Target directory stayed below 25 GiB (12 GiB at completion). No desktop TEST host or live desktop was used; no temporary duplicate source or harness remains.
