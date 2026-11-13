# N0203 — Settings completeness

## Goal

For v0.1, every visible setting must have a reader that matches its label. Dead
controls are removed; planned records assigned to upcoming feature units remain hidden
until those readers land. The shell registry is canonical at `shell/settings/registry/`.

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
| Planned rows with no reader: `panels.lips.lipSize`, `panels/bottom.panel.enabled`, `panels/top-bar.panel.modules`, `keyboard-mouse.mouse.shakeCursor`, `privacy.history.{notification,clipboard,screenshot,aiChat}Retention`, `power.saver.batterySaver`, `power.idle.{dim,sleep}`, `power.hardware.{lid,button}`, `wallpaper.motion.lockWallpaper`, `colors.palette.automatic` | Confirmed no v0.1 reader. Per owner decision, restored all 15 rows and all label, description, option and unavailable translations with `backend: planned`; assigned them to N0211–N0213 below. |
| Dashboard preferences: `lyrics`, `albumPalette`, `homeZone`, `homeCity`, `calendarFile`, `focusMinutes` | The five settings before `focusMinutes` have readers in media metadata, dashboard home-time preferences and calendar loading, so restored them. `albumPalette` had a stored value but media metadata hard-coded false; now the existing reader honours the stored boolean. `focusMinutes` has no settings reader: the dashboard focus card and launcher timer use their own fixed choices, and reminders do not consume this preference. Removed it and its stale navigation aliases; legacy `dashboard.json` values are ignored. |
| `launcher/entries.editor`, `system.backup`, `system.restore` | No matching action handler exists; removed the three broken action rows and their translations. `lock-login.preview`, `system.updates`, and `appearance.fonts.installed` are hidden by the existing `None` dispatch; `lock-login.idle-policy` remains planned and filtered, so none renders as a dead row. |

## Planned, being built

These settings stay hidden with `backend: planned` until their assigned readers land:

- **N0211 — power:** `power.saver.batterySaver`, `power.idle.dim`, `power.idle.sleep`, `power.hardware.lid`, `power.hardware.button`
- **N0212 — history retention:** `privacy.history.notificationRetention`, `clipboardRetention`, `screenshotRetention`, `aiChatRetention`
- **N0213 — appearance, panels and startup:** `colors.palette.automatic`, `panels.lips.lipSize`, `panels/bottom.panel.enabled`, `panels/top-bar.panel.modules`, `keyboard-mouse.mouse.shakeCursor`, `wallpaper.motion.lockWallpaper`, `apps.startup.startup`

## Candidate features (not in v0.1)

- `panels/dashboard/calendar-focus.editor.focusMinutes` (the dashboard card and launcher use fixed timer choices; reminders have no reader for this preference).

## Verdict counts

- Settings: 138 unique rows after copy merge; 117 active reader-backed rows, 16 planned rows (including `apps.startup.startup`), and 5 removed rows (4 dead visible and 1 dashboard preference without a reader). The other five dashboard controls are restored or wired. No known partial/mislabeled findings remain unresolved.
- Actions: 32 audited; 25 live, 3 intentionally hidden unavailable actions, 1 planned hidden action, and 3 broken rows removed.

## Checks

- In one fresh `n0200-build:latest` container: `cargo fmt --check`, `ruff check bin/nacre-settings nacre/shell-tools/tests/test_settings_launcher.py`, and `python3 tools/i18n/check.py` passed (1,763 messages). The temporary-HOME active-source snapshot test passed.
- The N0221a every-page untranslated-text guard passed. Settings goldens updated and rechecked: `settings_tests::` passed 18 tests in both runs; only the three new dashboard page images were added. Reviewed all six existing changed settings goldens; the differences are the requested mono-font, launcher-source, active-workspace, top-bar reader, battery-alert, and dead-clipboard changes.
- Helper checks passed: media tests (2), registry/validation tests (5), hosted preference and CLI tests (1 each), and the whole helper library (1,026 passed; 3 ignored). All six `test_software_*.py` suites passed (16 tests).
- Both focused app-spawn tests passed. The whole shell binary passed (884 tests; 4 ignored). `cargo build` for helper and shell, focused clippy for both crates, and the Luau analyzer/generated SDK check passed (`LUAU_ANALYZE_OK 189 modules`).
- Peak Cargo target use was 19,980,392 KiB, below the 25 GiB cap. No desktop TEST host or live desktop was used.
