# Settings architecture and phase 2 contract

Audited 2026-10-14 against Nacre `ab5fbea2` (the worktree base). This is a
source audit, not an observation of personal configuration. Phase 1 records
current behavior and the destination of existing settings. Phase 2 is an opt-in
native prototype; phase 3 requires owner approval. No release, deployment, live
configuration edit or package-manager implementation is authorized by this task.

## Verified baseline

- `settings-catalog.js` contains 12 pages. Its search tokenizes the query and
  matches page label, detail and terms; it never indexes individual controls.
- `NacreSettingsView.qml` lazy-loads 12 hand-written page components. Language
  and edge visibility live in Appearance, activation/detachment in Desktop.
  Weather photos share `dashboard-extras.json.weatherPhotos` between Workflows
  and Dashboard Customize; this is duplicated UI over one value.
- AI is unconditional in the old catalog. `NacreProfile.ai` and `.brain` already
  derive installation from `profile.json.components`; the AI page starts
  SidebarChat on creation, making the missing gate consequential.
- `DesktopSettings` queues `python3 desktop-settings.py` for each desktop change
  (coalescing pending writes by key) and polls state every 3000 ms while Settings
  is visible. A desktop.json file watcher already exists. Other stores use
  native helpers, file views or their services; **not every setting in the
  desktop** invokes that Python script.
- Search has a surface background from NacreTextField but the rail uses the
  same surface color, so the field has no distinct resting boundary. Header and
  section titles use accent ink. Desktop uses `desktop_windows`, Displays
  `monitor`: distinct source names, they are rendered as separate Material Symbols ligatures. Their similar
  monitor silhouettes explain the reported visual ambiguity. Use `dashboard` for Panels and `monitor` for Displays.
- The Settings FloatingWindow is opaque, rectangular and has no rounded
  outer surface. The new window uses a transparent native background with a
  clipped `radii.large` surface, matching the shared tile kit.
- Size scale is already integrated in `widgets/size-scale.json` / NacreTokens.
  Foundation's versioned-state implementation is still an independent task at
  audit time; its proposed NacreStateFile delegates guarded writes to the native
  `state-file save` API. Do not copy that task's uncommitted code or introduce
  a competing migration. Settings' store interface delegates production writes
  to the owning provider; prototype-only state is isolated from user files.

## Final navigation

Group names organize a bounded rail; subpages do not add top-level tabs. Prefer
entities (panel, display, application) over a new category per feature. Use a
scrolling, keyboard-accessible rail on wide windows and a category chooser on
narrow windows. Keep at most 15 controls on a page; complex lists have their own
subpage. Advanced is allowed only for at least three infrequently used controls.

| Group | Page | Sections / subpages | Visibility |
| --- | --- | --- | --- |
| Look & feel | Wallpaper | Scene/picker; Collections; Rotation; Motion; Lock wallpaper | Always |
| Look & feel | Colors | Source; Mode/schedule; Palette style; Readability; App theming; Colors studio | Always |
| Look & feel | Appearance | Fonts/text size; Desktop frame; Corners; Motion | Always |
| Desktop | Panels | Screen diagram; edge subpages: Top bar, Dashboard, Control center, AI sidebar, Bottom panel; Lips & pearls | AI edge only with AI |
| Desktop | Launcher | Sources/prefixes; Web search; Favorites/hidden apps; History; Custom entries; Packages (reserved) | Packages only with packages capability |
| Desktop | Windows & workspaces | Windows/effects; Workspaces; Workflows/routes; Overview | Per-option compositor capabilities |
| Desktop | Notifications | DND/schedule; Popups; OSD; History; App rules (future) | Always |
| Devices | Displays | Arrangement; entity page: Mode/scale/VRR; Night light; Brightness | Each feature gated; transaction retained |
| Devices | Sound | Output; Input; application/entity pages | Device availability |
| Devices | Network | Wi-Fi; Ethernet; VPN; connection/entity pages | Wi-Fi only with adapter; Ethernet remains |
| Devices | Bluetooth | Adapter; device/entity pages | Only with adapter |
| Devices | Power & battery | AC/battery profiles; Battery saver; Idle dim/sleep; Button/lid; Alerts | Only with battery, per owner requirement |
| Devices | Keyboard & mouse | Layouts; Repeat; Touchpad; Mouse; Shortcuts (read-only initially) | Per input capability |
| System | Lock & login | Lock widgets/privacy; Idle lock; Login colors | Login feature availability |
| System | Apps | Defaults; Startup; Optional tools; Software (reserved) | Software gated by packages |
| System | Privacy | Retention; Online lookup links; Clear history | AI retention only with AI |
| System | Date, time & language | Zone; Clock format; Language; Units; Weather location | Location provider status |
| System | System | Release/updates; Health; Recovery/rollback; Desktop restart; Backup/restore; About | Always |
| Add-ons | Add-ons | Install cards; registered extras | When either AI or Brain is absent; future extra cards remain here |
| Add-ons | Nacre AI | Assistant; Accounts; Usage; Projects; Sidebar link | AI installed |
| Add-ons | Brain | Local knowledge; Search; Sync; Access links | Brain installed |

### Decisions refining the brief

Keep the proposed eighteen main destinations. Notifications stays under Desktop,
not System, because popup/OSD behavior is shell interaction. Power is hidden on
battery-less hosts as requested; keep session lock/suspend actions discoverable
through Lock & login and the launcher rather than strand them behind that gate.
Privacy links to Weather photos and Lyrics in Panels / Dashboard, and to weather
location in Date, time & language: one canonical control, multiple searchable
entry points. Idle lock is owned by Lock & login; Power links to that row.
AI sidebar's panel geometry belongs to Panels, while provider/accounts belong
to Nacre AI. Reduce motion belongs to Appearance, with a link from battery saver.
Favorites belong to Launcher, defaults/startup to Apps. Existing Colors remains
the studio for role overrides, saved palettes and exports; quick color choices
belong to Colors. Display brightness appears with its display, not a new page.

The fixed rail can grow through registered Add-ons and entity subpages, not one
tab per extra. Extensions need unique namespaced IDs, i18n IDs, owner metadata,
capability declaration and a declared store adapter. Do not render arbitrary
plugin code from a data fragment. Keep Bead out of visible labels; index `bead`,
`Orient`, `lips` and `pearls` as synonyms. Explain Nacre AI, Brain and Lips & pearls
in one sentence at their first visible occurrence. Credit Colors by Orient.

## Store inventory and destinations

Paths below are relative to `~/.config/nacre` unless marked state/system.
Readers are source references, not a claim that every reader owns the value.
Repeated entity values (per-display mode, per-app audio, saved workflows) are
listed by their parameterized key. Generated state, caches, credentials and
installation bookkeeping are distinguished from user preferences.

### Desktop values (complete desktop-defaults.json key inventory)

| Store / key | Current surface | Destination | Readers / owner |
| --- | --- | --- | --- |
| `desktop.json` / `animations` | Settings / Desktop | Appearance / Frame and motion | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacrePanelState.qml`, `nacre/shell/services/compositor/NiriBackend.qml` |
| `desktop.json` / `blur` | Settings / Desktop | Windows & workspaces / Windows and effects | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/compositor/NiriBackend.qml`, `crates/nacre-helper/src/compositor/backend.rs` |
| `desktop.json` / `shadow` | Settings / Desktop | Windows & workspaces / Windows and effects | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/compositor/NiriBackend.qml`, `crates/nacre-helper/src/compositor/backend.rs` |
| `desktop.json` / `followMouse` | Settings / Desktop | Windows & workspaces / Windows and effects | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/compositor/NiriBackend.qml`, `crates/nacre-helper/src/compositor/backend.rs`, `crates/nacre-helper/src/compositor/hyprland.rs` |
| `desktop.json` / `naturalScroll` | Settings / Desktop | Keyboard & mouse / Touchpad | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/compositor/NiriBackend.qml`, `crates/nacre-helper/src/niri_session_runtime.rs`, `crates/nacre-helper/src/compositor/backend.rs` |
| `desktop.json` / `gapsIn` | Settings / Desktop | Windows & workspaces / Windows and effects | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/compositor/NiriBackend.qml`, `crates/nacre-helper/src/compositor/backend.rs`, `crates/nacre-helper/src/compositor/hyprland.rs` |
| `desktop.json` / `gapsOut` | Settings / Desktop | Windows & workspaces / Windows and effects | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `crates/nacre-helper/src/niri_session_runtime.rs`, `crates/nacre-helper/src/compositor/backend.rs`, `crates/nacre-helper/src/compositor/hyprland.rs` |
| `desktop.json` / `borderSize` | Settings / Desktop | Windows & workspaces / Windows and effects | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/compositor/NiriBackend.qml`, `crates/nacre-helper/src/niri_session_runtime.rs`, `crates/nacre-helper/src/compositor/backend.rs` |
| `desktop.json` / `rounding` | Settings / Desktop | Windows & workspaces / Windows and effects | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/compositor/NiriBackend.qml`, `crates/nacre-helper/src/compositor/backend.rs`, `crates/nacre-helper/src/compositor/hyprland.rs` |
| `desktop.json` / `frameWidth` | Settings / Appearance | Appearance / Frame and motion | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacreHoverIntent.qml` |
| `desktop.json` / `frameRounding` | Settings / Appearance | Appearance / Frame and motion | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `frameSheen` | Settings / Appearance | Appearance / Frame and motion | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `workspaceWindowHints` | Settings / Appearance | Windows & workspaces / Overview | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `detachablePanels` | Settings / Desktop | Panels / Lips & pearls | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacrePearls.qml` |
| `desktop.json` / `dragPanels` | Settings / Desktop | Panels / Lips & pearls | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacrePanelDragRouting.qml`, `nacre/shell/services/NacrePearls.qml` |
| `desktop.json` / `topEdge` | Settings / Appearance | Panels / Edge subpages | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `clickEdgeMenus` | Settings / Desktop | Panels / Lips & pearls | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `leftEdge` | Settings / Appearance | Panels / Edge subpages | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `rightEdge` | Settings / Appearance | Panels / Edge subpages | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `bottomEdge` | Settings / Appearance | Panels / Edge subpages | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `leftDrawer` | Settings / Appearance and Nacre AI | Panels / Edge subpages | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacrePanelDragRouting.qml`, `nacre/shell/services/NacrePearls.qml` |
| `desktop.json` / `livePreviews` | Settings / Appearance | Windows & workspaces / Overview | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `nativePalette` | Settings / Appearance | Launcher / Search sources | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `nativeOverview` | Settings / Appearance | Windows & workspaces / Overview | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `nativeClipboard` | Settings / Appearance | Launcher / Search sources | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py` |
| `desktop.json` / `dnd` | Settings / Notifications; Control center; focus timer | Notifications / Do not disturb | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacreControlPreferences.qml`, `nacre/shell/services/NacreDashboardState.qml` |
| `desktop.json` / `lockMedia` | Settings / Lock screen | Lock & login / Widgets and privacy | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/LockWidgets.qml` |
| `desktop.json` / `lockWeather` | Settings / Lock screen | Lock & login / Widgets and privacy | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/LockWidgets.qml` |
| `desktop.json` / `lockNotifications` | Settings / Lock screen | Lock & login / Widgets and privacy | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/LockWidgets.qml` |
| `desktop.json` / `lockNotificationContents` | Settings / Lock screen | Lock & login / Widgets and privacy | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/LockWidgets.qml` |
| `desktop.json` / `weatherLocation` | Settings / Lock screen | Date, time & language / Weather and units | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacreWeather.qml` |
| `desktop.json` / `weatherFahrenheit` | Settings / Lock screen | Date, time & language / Weather and units | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacreWeather.qml` |
| `desktop.json` / `interfaceFont` | Launcher menu / Interface font | Appearance / Fonts | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacreMenuContext.qml` |
| `desktop.json` / `language` | Settings / Appearance | Date, time & language / Language | `nacre/shell/services/DesktopSettings.qml`, `nacre/shell-tools/desktop-settings.py`, `nacre/shell/services/NacreI18n.qml`, `crates/nacre-helper/src/i18n.rs`, `crates/nacre-helper/src/niri_session_stage.rs` |

### Wallpaper and Colors

| Store / key | Current surface | Destination | Readers / owner |
| --- | --- | --- | --- |
| `wallpaper-picker.json` / `kind` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/DesktopExtras.qml`, `nacre/shell/services/NacreAiViews.qml`, `nacre/shell/services/NacreControlTools.qml` |
| `wallpaper-picker.json` / `layout` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacrePearls.qml`, `crates/nacre-helper/src/niri_hover_probe.rs`, `crates/nacre-helper/src/niri_session_runtime.rs` |
| `wallpaper-picker.json` / `paused` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacreDashboardState.qml`, `nacre/shell/services/NacreNetwork.qml`, `nacre/shell/services/WallpaperPlayback.qml` |
| `wallpaper-picker.json` / `motionMode` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/WallpaperPlayback.qml` |
| `wallpaper-picker.json` / `pauseCovered` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/WallpaperPlayback.qml` |
| `wallpaper-picker.json` / `rotationEnabled` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py` |
| `wallpaper-picker.json` / `rotationMinutes` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py` |
| `wallpaper-picker.json` / `rotationKind` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py` |
| `wallpaper-picker.json` / `rotationShuffle` | Settings / Appearance; wallpaper picker | Wallpaper / Picker, rotation and motion | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py` |
| `wallpaper-picker.json` / `palettePreset` | Settings / Appearance; launcher color family; Colors apply | Colors / Source, mode and palette style | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacreWelcomeApp.qml` |
| `wallpaper-picker.json` / `paletteMode` | Settings / Appearance; launcher color family; Colors apply | Colors / Source, mode and palette style | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreMenuContext.qml`, `nacre/shell/services/NacreWelcomeApp.qml` |
| `wallpaper-picker.json` / `paletteAccent` | Settings / Appearance; launcher color family; Colors apply | Colors / Source, mode and palette style | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacreMenuContext.qml` |
| `wallpaper-picker.json` / `paletteHarmony` | Settings / Appearance; launcher color family; Colors apply | Colors / Source, mode and palette style | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacreMenuContext.qml` |
| `wallpaper-picker.json` / `palettePersonality` | Settings / Appearance; launcher color family; Colors apply | Colors / Source, mode and palette style | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreMenuContext.qml`, `nacre/shell/services/NacreWelcomeApp.qml` |
| `wallpaper-picker.json` / `paletteBackgroundFromWallpaper` | Settings / Appearance; launcher color family; Colors apply | Colors / Source, mode and palette style | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreMenuContext.qml` |
| `colors.json` / `wallpapers.{image}.mode` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.personality` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.backgroundFromWallpaper` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.overrides` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.brightness` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.saturation` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.hour` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.autoMode` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.tideAutomatic` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.timeTint` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.workspaceColors` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.accent` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `wallpapers.{image}.favoriteId` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `colors.json` / `favorites` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py`, `nacre/shell/services/LauncherPreferences.qml`, `nacre/shell/services/NacreMenuContext.qml`, `crates/nacre-helper/src/launcher_preferences.rs` |
| `colors.json` / `history` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacreChatSession.qml`, `nacre/shell/services/NacreNotifs.qml` |
| `colors.json` / `lastApplied` | Colors studio | Colors / Studio (mode/auto surfaced in quick settings via adapter) | `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell-tools/colors.py` |
| `cli.json` / `wallpaper` | CLI; managed templates | Wallpaper / Collections; Colors / App theming and studio advanced | `nacre/shell-cli/src/nacre_shell/cli.py`, `nacre/shell-cli/src/nacre_shell/template_data.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/LockWidgets.qml`, `nacre/shell/services/NacreColorsApp.qml` |
| `cli.json` / `templates` | CLI; managed templates | Wallpaper / Collections; Colors / App theming and studio advanced | `nacre/shell-cli/src/nacre_shell/cli.py`, `nacre/shell-cli/src/nacre_shell/template_data.py` |
| `cli.json` / `schemes` | CLI; managed templates | Wallpaper / Collections; Colors / App theming and studio advanced | `nacre/shell-cli/src/nacre_shell/cli.py`, `nacre/shell-cli/src/nacre_shell/template_data.py` |
| `cli.json` / `custom` | CLI; managed templates | Wallpaper / Collections; Colors / App theming and studio advanced | `nacre/shell-cli/src/nacre_shell/cli.py`, `nacre/shell-cli/src/nacre_shell/template_data.py`, `nacre/shell/services/compositor/HyprlandBackend.qml`, `crates/nacre-helper/src/paths.rs`, `crates/nacre-helper/src/window_hide.rs` |
| `cli.json` / `orient` | CLI; managed templates | Wallpaper / Collections; Colors / App theming and studio advanced | `nacre/shell-cli/src/nacre_shell/cli.py`, `nacre/shell-cli/src/nacre_shell/template_data.py`, `nacre/shell/services/DesktopActions.qml`, `crates/nacre-helper/src/dashboard_media.rs`, `crates/nacre-helper/src/lib.rs` |
| `state/scheme.json` / `name` | CLI scheme; Appearance color style/contrast | Colors / Palette style and readability (derived state; apply via publisher) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell-cli/src/nacre_shell/storage.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/DisplayRecovery.qml`, `nacre/shell/services/NacreApps.qml` |
| `state/scheme.json` / `mode` | CLI scheme; Appearance color style/contrast | Colors / Palette style and readability (derived state; apply via publisher) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell-cli/src/nacre_shell/storage.py`, `nacre/shell/services/Colours.qml`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacreColorsApp.qml` |
| `state/scheme.json` / `variant` | CLI scheme; Appearance color style/contrast | Colors / Palette style and readability (derived state; apply via publisher) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell-cli/src/nacre_shell/storage.py`, `crates/nacre-helper/src/niri_hover_install.rs`, `crates/nacre-helper/src/niri_session_runtime.rs`, `crates/nacre-helper/src/systemd.rs` |
| `state/scheme.json` / `flavour` | CLI scheme; Appearance color style/contrast | Colors / Palette style and readability (derived state; apply via publisher) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell-cli/src/nacre_shell/storage.py` |
| `state/scheme.json` / `wallpaper` | CLI scheme; Appearance color style/contrast | Colors / Palette style and readability (derived state; apply via publisher) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell-cli/src/nacre_shell/storage.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/LockWidgets.qml`, `nacre/shell/services/NacreColorsApp.qml` |
| `state/scheme.json` / `choices` | CLI scheme; Appearance color style/contrast | Colors / Palette style and readability (derived state; apply via publisher) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell-cli/src/nacre_shell/storage.py`, `crates/nacre-helper/src/arguments.rs`, `crates/nacre-helper/src/menu_cli.rs`, `crates/nacre-helper/src/compositor_config/niri.rs` |
| `state/presentation.json` / `colours` | Generated current presentation | Colors / Preview (read-only, not a second preference) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/LockWidgets.qml`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreColours.qml` |
| `state/presentation.json` / `mode` | Generated current presentation | Colors / Preview (read-only, not a second preference) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell/services/Colours.qml`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreColours.qml` |
| `state/presentation.json` / `poster` | Generated current presentation | Colors / Preview (read-only, not a second preference) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell/services/LockWidgets.qml`, `nacre/shell/services/NacreMenuContext.qml`, `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell/services/Wallpapers.qml` |
| `state/presentation.json` / `paletteOptions` | Generated current presentation | Colors / Preview (read-only, not a second preference) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell/services/NacreMenuContext.qml`, `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell/services/Wallpapers.qml` |
| `state/presentation.json` / `selectedAccent` | Generated current presentation | Colors / Preview (read-only, not a second preference) | `nacre/shell/services/NacrePresentation.qml`, `nacre/shell/services/NacreMenuContext.qml`, `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell/services/Wallpapers.qml` |
| `state/wallpaper/media.json` / `path` | Current wallpaper selection; Welcome demo | Wallpaper / Scene (derived; use media provider) | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/DesktopSettings.qml`, `nacre/shell/services/LauncherPreferences.qml`, `nacre/shell/services/NacreBatteryAlerts.qml` |
| `state/wallpaper/media.json` / `poster` | Current wallpaper selection; Welcome demo | Wallpaper / Scene (derived; use media provider) | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/LockWidgets.qml`, `nacre/shell/services/NacreMenuContext.qml`, `nacre/shell/services/NacrePresentation.qml` |
| `state/wallpaper/media.json` / `preview` | Current wallpaper selection; Welcome demo | Wallpaper / Scene (derived; use media provider) | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreI18n.qml` |
| `state/wallpaper/media.json` / `kind` | Current wallpaper selection; Welcome demo | Wallpaper / Scene (derived; use media provider) | `nacre/shell/services/NacreWallpapers.qml`, `nacre/shell-tools/wallpaper-media.py`, `nacre/shell/services/DesktopExtras.qml`, `nacre/shell/services/NacreAiViews.qml`, `nacre/shell/services/NacreControlTools.qml` |

### Panels and dashboard

| Store / key | Current surface | Destination | Readers / owner |
| --- | --- | --- | --- |
| `dashboard.json` / `tabs` | Dashboard Customize; media; focus | Panels / Dashboard: Pages, Media, Home time, Calendar, Focus | `nacre/shell/services/NacreDashboardState.qml`, `crates/nacre-helper/src/dashboard_data.rs` |
| `dashboard.json` / `lyrics` | Dashboard Customize; media; focus | Panels / Dashboard: Pages, Media, Home time, Calendar, Focus | `nacre/shell/services/NacreDashboardState.qml`, `crates/nacre-helper/src/dashboard_data.rs`, `nacre/shell/services/NacreMediaDetails.qml`, `crates/nacre-helper/src/dashboard_media.rs` |
| `dashboard.json` / `albumPalette` | Dashboard Customize; media; focus | Panels / Dashboard: Pages, Media, Home time, Calendar, Focus | `nacre/shell/services/NacreDashboardState.qml`, `crates/nacre-helper/src/dashboard_data.rs` |
| `dashboard.json` / `homeZone` | Dashboard Customize; media; focus | Panels / Dashboard: Pages, Media, Home time, Calendar, Focus | `nacre/shell/services/NacreDashboardState.qml`, `crates/nacre-helper/src/dashboard_data.rs` |
| `dashboard.json` / `homeCity` | Dashboard Customize; media; focus | Panels / Dashboard: Pages, Media, Home time, Calendar, Focus | `nacre/shell/services/NacreDashboardState.qml`, `crates/nacre-helper/src/dashboard_data.rs` |
| `dashboard.json` / `calendarFile` | Dashboard Customize; media; focus | Panels / Dashboard: Pages, Media, Home time, Calendar, Focus | `nacre/shell/services/NacreDashboardState.qml`, `crates/nacre-helper/src/dashboard_data.rs` |
| `dashboard.json` / `focusMinutes` | Dashboard Customize; media; focus | Panels / Dashboard: Pages, Media, Home time, Calendar, Focus | `nacre/shell/services/NacreDashboardState.qml`, `crates/nacre-helper/src/dashboard_data.rs` |
| `dashboard-extras.json` / `enabled` | Dashboard Customize; Workflows extras and weather photos | Panels / Dashboard: Extras and Weather | `nacre/shell/services/NacreDashboardState.qml`, `nacre/shell/modules/dashboard/NacreDashboardExtrasOptions.qml`, `nacre/shell/services/LauncherPreferences.qml`, `nacre/shell/services/NacreBluetooth.qml`, `nacre/shell/services/NacreColours.qml` |
| `dashboard-extras.json` / `order` | Dashboard Customize; Workflows extras and weather photos | Panels / Dashboard: Extras and Weather | `nacre/shell/services/NacreDashboardState.qml`, `nacre/shell/modules/dashboard/NacreDashboardExtrasOptions.qml`, `crates/nacre-helper/src/diagnostics.rs`, `crates/nacre-helper/src/menu_arguments.rs`, `crates/nacre-helper/src/niri_session_runtime.rs` |
| `dashboard-extras.json` / `weatherPhotos` | Dashboard Customize; Workflows extras and weather photos | Panels / Dashboard: Extras and Weather | `nacre/shell/services/NacreDashboardState.qml`, `nacre/shell/modules/dashboard/NacreDashboardExtrasOptions.qml`, `nacre/shell/services/NacreWeatherPhoto.qml` |
| `control-center.json` / `tiles` | Control center Edit | Panels / Control center: Tiles (embed editor) | `nacre/shell/services/NacreControlPreferences.qml`, `nacre/shell/modules/controlcenter/NacreControlCatalog.qml` |
| `state/shell/pearls.json` / `windows[].geometry` | Panel pearl drag/dock/window resizing | Panels / Per-edge geometry (restored state) | `nacre/shell/services/NacrePearls.qml`, `crates/nacre-helper/src/pearl_state.rs` |
| `state/shell/pearls.json` / `windows[].panel` | Panel pearl drag/dock/window resizing | Panels / Per-edge geometry (restored state) | `nacre/shell/services/NacrePearls.qml`, `crates/nacre-helper/src/pearl_state.rs` |
| `state/shell/pearls.json` / `windows[].edge` | Panel pearl drag/dock/window resizing | Panels / Per-edge geometry (restored state) | `nacre/shell/services/NacrePearls.qml`, `crates/nacre-helper/src/pearl_state.rs` |
| `state/shell/pearls.json` / `ai.edge` | Panel pearl drag/dock/window resizing | Panels / Per-edge geometry (restored state) | `nacre/shell/services/NacrePearls.qml`, `crates/nacre-helper/src/pearl_state.rs` |
| `state/shell/pearls.json` / `ai.channels` | Panel pearl drag/dock/window resizing | Panels / Per-edge geometry (restored state) | `nacre/shell/services/NacrePearls.qml`, `crates/nacre-helper/src/pearl_state.rs` |

### Launcher, applications, profile and add-ons

| Store / key | Current surface | Destination | Readers / owner |
| --- | --- | --- | --- |
| `launcher.json` / `favorites` | App context menu / favorite, hide and launches | Launcher / Favorites, hidden apps and history | `nacre/shell/services/LauncherPreferences.qml`, `crates/nacre-helper/src/launcher_preferences.rs`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreMenuContext.qml` |
| `launcher.json` / `hidden` | App context menu / favorite, hide and launches | Launcher / Favorites, hidden apps and history | `nacre/shell/services/LauncherPreferences.qml`, `crates/nacre-helper/src/launcher_preferences.rs`, `nacre/shell/services/NacreApps.qml`, `nacre/shell/services/NacreControlActions.qml`, `nacre/shell/services/NacreDashboardState.qml` |
| `launcher.json` / `recent` | App context menu / favorite, hide and launches | Launcher / Favorites, hidden apps and history | `nacre/shell/services/LauncherPreferences.qml`, `crates/nacre-helper/src/launcher_preferences.rs`, `nacre/shell/services/NacreAiViews.qml`, `nacre/shell/services/NacreDashboardState.qml`, `nacre/shell/services/NacreMenuContext.qml` |
| `launcher.json` / `usage` | App context menu / favorite, hide and launches | Launcher / Favorites, hidden apps and history | `nacre/shell/services/LauncherPreferences.qml`, `crates/nacre-helper/src/launcher_preferences.rs`, `nacre/shell/services/NacreDashboardRegistry.qml`, `nacre/shell/services/NacreDashboardSystem.qml`, `nacre/shell/services/NacreMenuContext.qml` |
| `launcher-menu.json` / `entries[].id` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-menu.json` / `entries[].parent` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-menu.json` / `entries[].label` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-menu.json` / `entries[].description` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-menu.json` / `entries[].icon` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-menu.json` / `entries[].operation` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-menu.json` / `entries[].arguments` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-menu.json` / `entries[].keywords` | User-edited menu / installed catalog | Launcher / Custom entries (list/entity editor later) | `nacre/shell/services/NacreMenu.qml`, `crates/nacre-helper/src/menu.rs` |
| `launcher-workflows.json` / `entries` | Launcher saved actions, snippets, searches, bookmarks, workspace routes and file roots | Launcher / Custom entries and Sources | `nacre/shell/services/NacreLauncherWorkflows.qml`, `crates/nacre-helper/src/launcher_workflows.rs`, `nacre/shell/services/NacreAiViews.qml`, `nacre/shell/services/NacreApps.qml`, `nacre/shell/services/NacreColorsApp.qml` |
| `launcher-workflows.json` / `roots` | Launcher saved actions, snippets, searches, bookmarks, workspace routes and file roots | Launcher / Custom entries and Sources | `nacre/shell/services/NacreLauncherWorkflows.qml`, `crates/nacre-helper/src/launcher_workflows.rs`, `crates/nacre-helper/src/menu.rs`, `crates/nacre-helper/src/paths.rs`, `crates/nacre-helper/src/niri_widths/x11.rs` |
| `apps.json` / `browser` | Launcher / Default apps; setup | Apps / Defaults | `crates/nacre-helper/src/menu_actions.rs`, `tools/setup.py`, `nacre/shell/services/NacreMediaDetails.qml`, `nacre/shell/services/NacreMenuContext.qml`, `crates/nacre-helper/src/menu.rs` |
| `apps.json` / `terminal` | Launcher / Default apps; setup | Apps / Defaults | `crates/nacre-helper/src/menu_actions.rs`, `tools/setup.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacreMenuContext.qml`, `crates/nacre-helper/src/launcher_workflows.rs` |
| `apps.json` / `files` | Launcher / Default apps; setup | Apps / Defaults | `crates/nacre-helper/src/menu_actions.rs`, `tools/setup.py`, `nacre/shell/services/DesktopActions.qml`, `nacre/shell/services/NacreChatSession.qml`, `nacre/shell/services/NacreMenuContext.qml` |
| `apps.json` / `mail` | Launcher / Default apps; setup | Apps / Defaults | `crates/nacre-helper/src/menu_actions.rs`, `tools/setup.py` |
| `apps.json` / `emoji` | Launcher / Default apps; setup | Apps / Defaults | `crates/nacre-helper/src/menu_actions.rs`, `tools/setup.py`, `nacre/shell/services/NacreLauncherPreview.qml`, `nacre/shell/services/NacreMenuContext.qml`, `crates/nacre-helper/src/launcher_workflows.rs` |
| `profile.json` / `startup` | CLI / startup-apps; workflow opt-in | Apps / Startup | `nacre/shell-tools/startup-apps.py`, `nacre/shell/services/DisplayRecovery.qml`, `nacre/shell/services/NacreWelcomeApp.qml`, `crates/nacre-helper/src/niri_hover_probe.rs`, `crates/nacre-helper/src/niri_session_runtime.rs` |
| `profile.json` / `workspaceCount` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/niri_session_runtime.rs`, `crates/nacre-helper/src/niri_session_stage.rs` |
| `profile.json` / `workspaceNames` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/niri_session_runtime.rs`, `crates/nacre-helper/src/niri_session_stage.rs` |
| `profile.json` / `workspaceIcons` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/niri_session_stage.rs` |
| `profile.json` / `components` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/dashboard_data.rs`, `crates/nacre-helper/src/launcher_workflows.rs` |
| `profile.json` / `aiFullAccess` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/niri_session_stage.rs` |
| `profile.json` / `windowTrashBindings` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/niri_session_stage.rs`, `crates/nacre-helper/src/compositor_config/hyprland.rs` |
| `profile.json` / `routes` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/menu_cli.rs`, `crates/nacre-helper/src/niri_session_stage.rs` |
| `profile.json` / `brainWorkspace` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/niri_session_stage.rs`, `crates/nacre-helper/src/compositor_config/mod.rs` |
| `profile.json` / `aiWorkspace` | Setup; CLI configuration | Windows & workspaces / Workspaces; Add-ons / Installed; Nacre AI / Permissions; System / Session | `nacre/shell/services/NacreProfile.qml`, `nacre/shell-tools/profile-policy.py`, `tools/setup.py`, `crates/nacre-helper/src/niri_session_stage.rs` |
| `workflows.json` / `{preset}.sink` | Settings / Workflows save and preset buttons | Windows & workspaces / Workflows | `nacre/shell-tools/workflow-profiles.py`, `nacre/shell-tools/desktop-settings.py` |
| `workflows.json` / `{preset}.source` | Settings / Workflows save and preset buttons | Windows & workspaces / Workflows | `nacre/shell-tools/workflow-profiles.py`, `nacre/shell-tools/desktop-settings.py` |
| `workflows.json` / `{preset}.displays` | Settings / Workflows save and preset buttons | Windows & workspaces / Workflows | `nacre/shell-tools/workflow-profiles.py`, `nacre/shell-tools/desktop-settings.py` |
| `workflows.json` / `{preset}.roles` | Settings / Workflows save and preset buttons | Windows & workspaces / Workflows | `nacre/shell-tools/workflow-profiles.py`, `nacre/shell-tools/desktop-settings.py` |
| `ai/settings.json` / `default_assistant` | Nacre AI workspace settings / CLI; Settings / AI default provider | Nacre AI / Assistant and accounts (no credential copies) | `bin/nacre-ai` |
| `ai/settings.json` / `accounts.codex` | Nacre AI workspace settings / CLI; Settings / AI default provider | Nacre AI / Assistant and accounts (no credential copies) | `bin/nacre-ai` |
| `ai/settings.json` / `accounts.claude` | Nacre AI workspace settings / CLI; Settings / AI default provider | Nacre AI / Assistant and accounts (no credential copies) | `bin/nacre-ai` |
| `ai/projects.json` / `projects[].id` | Nacre AI workspace registry / CLI | Nacre AI / Projects (link to existing owner) | `bin/nacre-ai` |
| `ai/projects.json` / `projects[].path` | Nacre AI workspace registry / CLI | Nacre AI / Projects (link to existing owner) | `bin/nacre-ai` |
| `ai/projects.json` / `projects[].build_host` | Nacre AI workspace registry / CLI | Nacre AI / Projects (link to existing owner) | `bin/nacre-ai` |
| `ai/projects.json` / `projects[].build_path` | Nacre AI workspace registry / CLI | Nacre AI / Projects (link to existing owner) | `bin/nacre-ai` |
| `brain-sync.json` / `host` | CLI / private sync configuration | Brain / Sync (existing owner; no credentials) | `bin/nacre-brain-sync`, `nacre/shell/services/NacrePanelDragRouting.qml`, `nacre/shell/services/NacrePearls.qml`, `crates/nacre-helper/src/diagnostics.rs`, `crates/nacre-helper/src/menu_registry.rs` |
| `welcome.json` / `showAtLogin` | Welcome / Show at login | System / Welcome link (bookkeeping, not a toggle) | `nacre/shell-tools/welcome.py`, `nacre/shell/services/NacreWelcomeApp.qml` |

### System-owned and operational values

| Store / key | Current surface | Destination | Readers / owner |
| --- | --- | --- | --- |
| `system:/etc/nacre/timezone.json` / `automatic` | Settings / Date and time; privileged timezone helper | Date, time & language / Time zone | `nacre/shell/services/TimezoneSettings.qml`, `nacre/shell-tools/timezone.py` |
| `system:/etc/nacre/timezone.json` / `confirmedTimezone` | Settings / Date and time; privileged timezone helper | Date, time & language / Time zone | `nacre/shell/services/TimezoneSettings.qml`, `nacre/shell-tools/timezone.py`, `crates/nacre-helper/src/dashboard_data.rs` |
| `hypridle.local.conf` / `listener.timeout` | CLI / private idle policy; Settings lock policy link | Power & battery / Dim and sleep; Lock & login / Idle lock (editor later) | `nacre/shell-tools/idle-policy.py`, `nacre/shell/modules/settings/pages/NacreLockPage.qml` |
| `hypridle.local.conf` / `listener.on-timeout` | CLI / private idle policy; Settings lock policy link | Power & battery / Dim and sleep; Lock & login / Idle lock (editor later) | `nacre/shell-tools/idle-policy.py`, `nacre/shell/modules/settings/pages/NacreLockPage.qml` |
| `hypridle.local.conf` / `listener.on-resume` | CLI / private idle policy; Settings lock policy link | Power & battery / Dim and sleep; Lock & login / Idle lock (editor later) | `nacre/shell-tools/idle-policy.py`, `nacre/shell/modules/settings/pages/NacreLockPage.qml` |
| `system:compositor config` / `outputs.{name}.mode` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py` |
| `system:compositor config` / `outputs.{name}.position` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py` |
| `system:compositor config` / `outputs.{name}.scale` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py` |
| `system:compositor config` / `outputs.{name}.refresh` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py` |
| `system:compositor config` / `input.keyboard.layout` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py` |
| `system:compositor config` / `input.keyboard.repeat` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py` |
| `system:compositor config` / `input.touchpad` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py`, `crates/nacre-helper/src/compositor/hyprland.rs` |
| `system:compositor config` / `workspace.appRoutes` | Settings / Displays; CLI config and shortcut definitions | Displays / Display entity (confirm/revert); Keyboard & mouse / Input; Windows & workspaces / Routes | `nacre/shell/services/NacreCompositor.qml`, `nacre/shell-tools/desktop-settings.py`, `tools/configure.py` |
| `system:PipeWire/WirePlumber` / `defaultSink` | Settings / Sound; Control center; bar | Sound / Output, Input and Apps (owning service) | `nacre/shell/services/NacreAudio.qml`, `nacre/shell/modules/settings/pages/NacreSoundPage.qml` |
| `system:PipeWire/WirePlumber` / `defaultSource` | Settings / Sound; Control center; bar | Sound / Output, Input and Apps (owning service) | `nacre/shell/services/NacreAudio.qml`, `nacre/shell/modules/settings/pages/NacreSoundPage.qml` |
| `system:PipeWire/WirePlumber` / `nodes.{id}.volume` | Settings / Sound; Control center; bar | Sound / Output, Input and Apps (owning service) | `nacre/shell/services/NacreAudio.qml`, `nacre/shell/modules/settings/pages/NacreSoundPage.qml` |
| `system:PipeWire/WirePlumber` / `nodes.{id}.muted` | Settings / Sound; Control center; bar | Sound / Output, Input and Apps (owning service) | `nacre/shell/services/NacreAudio.qml`, `nacre/shell/modules/settings/pages/NacreSoundPage.qml` |
| `system:NetworkManager` / `wifi.radio` | Settings / Network; Control center; bar | Network / Wi-Fi, Ethernet, VPN entity | `nacre/shell/services/NacreNetwork.qml`, `nacre/shell/services/NacreVpn.qml` |
| `system:NetworkManager` / `connections.{id}` | Settings / Network; Control center; bar | Network / Wi-Fi, Ethernet, VPN entity | `nacre/shell/services/NacreNetwork.qml`, `nacre/shell/services/NacreVpn.qml` |
| `system:NetworkManager` / `vpn.{id}` | Settings / Network; Control center; bar | Network / Wi-Fi, Ethernet, VPN entity | `nacre/shell/services/NacreNetwork.qml`, `nacre/shell/services/NacreVpn.qml` |
| `system:BlueZ` / `adapter.powered` | Settings / Bluetooth; Control center; bar | Bluetooth / Adapter and Device entity | `nacre/shell/services/NacreBluetooth.qml`, `nacre/shell/services/DeviceActions.qml` |
| `system:BlueZ` / `adapter.discoverable` | Settings / Bluetooth; Control center; bar | Bluetooth / Adapter and Device entity | `nacre/shell/services/NacreBluetooth.qml`, `nacre/shell/services/DeviceActions.qml` |
| `system:BlueZ` / `devices.{address}.connected` | Settings / Bluetooth; Control center; bar | Bluetooth / Adapter and Device entity | `nacre/shell/services/NacreBluetooth.qml`, `nacre/shell/services/DeviceActions.qml` |
| `system:BlueZ` / `devices.{address}.paired` | Settings / Bluetooth; Control center; bar | Bluetooth / Adapter and Device entity | `nacre/shell/services/NacreBluetooth.qml`, `nacre/shell/services/DeviceActions.qml` |
| `system:BlueZ` / `devices.{address}.trusted` | Settings / Bluetooth; Control center; bar | Bluetooth / Adapter and Device entity | `nacre/shell/services/NacreBluetooth.qml`, `nacre/shell/services/DeviceActions.qml` |
| `system:power-profiles-daemon` / `activeProfile` | Control center / Power profile; launcher actions | Power & battery / Power profile (current value; separate AC/battery policies future) | `nacre/shell/services/NacreControlTools.qml`, `nacre/shell-tools/control-tools.py` |
| `state/control-tools.json (or -niri)` / `nightLightEnabled` | Control center / Night light | Displays / Night light (supported provider only) | `nacre/shell/services/NacreControlTools.qml`, `nacre/shell-tools/control-tools.py`, `nacre/shell/services/NacreMenuContext.qml` |
| `system:backlight` / `brightness` | Control center; brightness keys | Displays / Brightness; Keyboard & mouse / Keyboard backlight | `nacre/shell/services/NacreBrightness.qml`, `nacre/shell/services/NacreKeyboardLight.qml`, `nacre/shell/services/NacreColorsApp.qml`, `nacre/shell/services/NacreControlPreferences.qml`, `nacre/shell/services/NacreLightChannel.qml` |
| `system:backlight` / `keyboardBrightness` | Control center; brightness keys | Displays / Brightness; Keyboard & mouse / Keyboard backlight | `nacre/shell/services/NacreBrightness.qml`, `nacre/shell/services/NacreKeyboardLight.qml` |
| `state/shell/notifications/history.json` / `items` | Notifications history; Dashboard optional page | Notifications / History; Privacy / Clear history (history is content, not schema settings) | `nacre/shell/services/NacreNotifs.qml`, `crates/nacre-helper/src/notification_history.rs`, `nacre/shell/services/NacreDashboardState.qml`, `nacre/shell/services/NacreMenuContext.qml`, `crates/nacre-helper/src/dashboard_data.rs` |
| `state/menu/stay-awake.json` / `enabled` | Control center awake; launcher actions | Power & battery / Sleep inhibition (session state) | `crates/nacre-helper/src/menu_actions.rs`, `nacre/shell/services/LauncherPreferences.qml`, `nacre/shell/services/NacreBluetooth.qml`, `nacre/shell/services/NacreColours.qml`, `nacre/shell/services/NacreCompositor.qml` |
| `state/shell/extras/pins.json` / `items` | Clipboard pins; launcher | Privacy / Clipboard clear (content; retention future) | `nacre/shell/services/DesktopExtras.qml`, `nacre/shell-tools/desktop-extras.py`, `nacre/shell/services/NacreDashboardState.qml`, `nacre/shell/services/NacreMenuContext.qml`, `crates/nacre-helper/src/dashboard_data.rs` |

### Action-only surfaces and fixed policy

Welcome offers demonstration colors/wallpapers, font preview, installation choices
and setup links; these target the same providers above. Launcher optional tools
use install-extras and package operations, not a new preferences store. The
maintenance page offers checks, release/update status, rollback, restart and
recovery through Maintenance; these are actions/status, destination System.
Clock formats are fixed at the NacreTime callers (no saved choice yet); workspace symbols from profile; battery alerts
have fixed 20%/15% thresholds in NacreBatteryAlertPolicy (not preferences yet).
Lock/login templates and color export targets are generated by their existing
owners. UI transient state (selected tab, scroll position, pending display rollback,
recording, focus timer deadline) is not user preference. Tokens/account auth,
Bluetooth secrets and NetworkManager passwords never enter the schema or gallery.

### App theming target inventory

`classic-state.py` currently publishes GTK 3/4 styles and icons, KDE/Qt color
schemes (including Dolphin and the shared Qt app environment), Kitty, Rofi,
Hyprlock and Nacre's own shell/compositor colors; Thunar and Dolphin integrations
consume that publication. `cli.json.templates` supports additional Orient
exports. App theming should enumerate the publisher's actual target manifest and
installed status, including custom exports, rather than freeze a separate list
of application names in QML. Per-target opt-out preferences do not yet exist.
The prototype registers the target subpage and demonstrates rows/status with a
small synthetic target manifest; phase 3 must obtain the complete runtime manifest
from the owning publisher before enabling these controls.

## Duplicates, orphans and missing settings

| Finding | Evidence / resolution |
| --- | --- |
| Edge visibility/activation split | Appearance vs Desktop; merge in Panels |
| AI sidebar repeated | `leftDrawer` in Appearance and AI; Panels owns, AI links |
| Weather photos repeated | Workflows and Customize share exact key; embed Customize |
| Wallpaper/colors repeated | Settings, Colors studio, launcher, Welcome and CLI; use existing publisher adapters |
| Dashboard optional pages repeated | Workflows uses ExtrasOptions, Customize also uses it; Settings embeds Customize |
| Do not disturb multiple entry points | Same DesktopSettings key plus focus timer restore; retain quick entry points with one provider |
| Profile values not identified in UI | Add provenance badge; never silently hide overridden values |
| `reduceMotion` read but not in desktop-defaults | NacreTokens reads it, desktop-settings rejects unknown keys; future registered/backend field, no writable pretend toggle |
| `albumPalette` in dashboard defaults | Reader exists but Customize has no switch; expose under Dashboard / Media in phase 3 |
| Whole-store keys without Settings entry | launcher custom entries/roots, favorites/hidden/reset, font, default apps/startup, keyboard/layouts, profile workspaces, collections, Brain sync |
| No top bar preferences owner yet | Modules/order/tray/clock settings need provider/schema from owning panel task; don't invent files |
| Requested but absent backend | Mono font/text size, lip size/delay, automatic theme schedule, contrast preference, per-app theming switches/status, notification popup/OSD/retention policy/schedule, AC/battery profiles, battery-saver bundle, idle UI/lid/button/battery-alert preferences, VRR GUI, lock wallpaper selection, login follows colors toggle, settings backup/restore GUI |
| Package manager | Future Packages and Software sections gated by `packages`; typed schema reservations only, no install/repository/AUR implementation |

Missing settings above are **future**, not audited working controls. Phase 2
shows unsupported controls with a specific reason. A battery-saver bundle must
later apply/restore exactly its own overrides (still wallpaper, 60 Hz through
confirm/revert, reduced effects), retain prior values and tolerate disconnected
outputs. Do not write a display mode directly from a generic switch.

## Research, primary sources and decisions

Checkouts live in ignored `.research/`; no upstream source is vendored. Snapshot
hashes below make observations reproducible. Adoption/avoidance is design judgment,
not a claim that the upstream does or does not support every desired feature.

| Reference / inspected source | Adopt | Avoid in Nacre |
| --- | --- | --- |
| [dms](https://github.com/AvengeMedia/DankMaterialShell/blob/699eb237751bc5d61fdf90e1709fb6cabe9896a4/quickshell/Common/SettingsTabs.qml) (`699eb237751b`) | Hubs, hidden child routes, aliases and stable IDs; current DMS is already consolidating many tabs | Numeric tab-index coupling, a separate search vocabulary per page and growing advanced leaves |
| [noctalia](https://github.com/noctalia-dev/noctalia-shell/blob/83f21859fcfda13fe98e3c796e8d4322230b9a71/src/shell/settings/settings_registry.h) (`83f21859fcfd`) | Typed setting variants, path, searchText, advanced flag, RegistryEnvironment capabilities | C++ callbacks and executable visibility predicates in portable schema; numerous custom variants |
| [caelestia](https://github.com/caelestia-dots/shell/blob/a3a8fe59aba2c0048ab02dc817748678f96076ff/modules/nexus/PageRegistry.qml) (`a3a8fe59aba2`) | Category registry, panel entity pages and Language & region placement | A flat page component registry without a complete setting search index |
| [end4](https://github.com/end-4/dots-hyprland/blob/2f2a376fdb2ad738bf55f693dd0b474b64bb0ab8/dots/.config/quickshell/ii/settings.qml) (`2f2a376fdb2a`) | Separate Settings window, practical previews, grouping shell controls | Quick/General/Advanced catch-all navigation and ad hoc page/component arrays |
| [hyprpanel](https://github.com/Jas-SinghFSU/HyprPanel/blob/1961ba86ad5ab880beb639e5454054b2b5037e0d/src/components/settings/shared/Option/index.tsx) (`1961ba86ad5a`) | One label/description/input/reset row, typed enum/number inputs | Splitting config and theme into parallel trees for the same entities; exposing JSON editors as normal controls |
| [ax-shell](https://github.com/Axenide/Ax-Shell/blob/1a3551fb6d1d5354c99f31880a39f7c7149da78e/config/settings_gui.py) (`1a3551fb6d1d`) | Simple vertical stack navigation and distinct shortcuts/appearance/system areas | Large creation functions mixing filesystem/apply logic and layout; wholesale config edits |
| [cosmic](https://github.com/pop-os/cosmic-settings/blob/f3b1b60aebcc5319f2344254019c77fdf3d33bf8/cosmic-settings/src/app.rs) (`f3b1b60aebcc`) | Task/device pages, build feature gating, reusable page title/search header | Duplicating their Rust widget model in Nacre; desktop-specific coupling in portable data |
| [gnome](https://github.com/GNOME/gnome-control-center/blob/385e2d501b8a08a27d1f1ac46b9313a7860f9b5b/docs/SIDEBAR_PANEL_LIST.md) (`385e2d501b8a`) | Explicit stable sidebar ordering/separators, panel-specific entity views | Assuming GNOME services exist in Nacre; search limited to panel metadata |
| [kde](https://github.com/KDE/systemsettings/blob/e20a31c7c1cae898c34cab400064b7b44e853f8f/app/MenuModel.cpp) (`e20a31c7c1ca`) | Module metadata, hierarchical entity pages, searchable keywords | Unbounded module/category proliferation and Apply semantics for instantly applied Nacre preferences |
| [macOS](https://support.apple.com/en-gb/guide/mac-help/mchl8d10839d/mac) | Search from a persistent rail and direct jump to matching settings | Narrow fixed content and a long undifferentiated list |
| [Windows 11](https://support.microsoft.com/en-us/accessibility/windows/understand-and-explore-windows-settings), [Microsoft cards/expanders](https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings), [Settings URI](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings) | Familiar label/control rows, grouped cards, direct routes and capability-specific destinations | Recommendation/promotional home content and fragmentation into legacy dialogs |
| [Android settings guidelines](https://source.android.com/docs/core/settings) (requested legacy URL now maps here) | Consistent components, information architecture and indexing every setting for universal search | Promotional suggestions; opaque hidden controls without status/reason |

Web sources inspected 2026-10-14. No user testing has yet established an optimal
number of categories: eighteen is the owner's task map, not a research-derived
magic number. Measure owner findability at phase 2 before migrating all pages.

## Portable schema, registry and store API

Fragments are JSON data conforming to a versioned JSON Schema. A manifest lists
fragments explicitly; do not infer them by import or execute extension code.
`version: 1`; namespace IDs; pages carry `id`, `group`, `icon`, `labelId`,
`descriptionId`, `order`, `visibleWhen`, parent route and aliases. Sections carry
`id`, `page`, `labelId`, `order` and optional `customType`. Settings carry
`id: page.section.key`, `page`, `section`, label/description i18n IDs, `type`,
`store: {file, key}` (key is a path array, never an executable expression),
`default`, options/range/unit, `visibleWhen`, `enabledWhen`, `advanced`, synonyms,
`apply: instant|confirm|restart`, optional dependency ID, and backend status.
Descriptions/options also have i18n IDs. Defaults use the actual stored type;
enum options must share it. Actions are declared separately and require an
allowlisted adapter action ID, not an arbitrary command string.

Conditions are a small portable grammar: `{capability: "battery"}`, `{all: [...]}`,
`{any: [...]}`, `{not: ...}`; no JS, function names, eval or shell commands.
An absent capability evaluates false. Providers expose compositor features,
battery, Wi-Fi/Bluetooth, night light, AI, Brain, package helpers (`paru`, `yay`,
`flatpak`) and `packages` (the installed future manager, not merely a helper).
Update capability snapshots through service events. Search and routes use the
same gates as rendering, so hidden capabilities cannot be bypassed by deep links.

The adapter interface is `value(setting)`, `write(setting, value)`,
`reset(settings)`, `provenance(setting)` and `changed`/failure notifications.
Validate type/range/options and ownership before handing writes to providers.
The production store adapter uses DesktopSettings, NacreWallpapers,
NacreColorsApp, LauncherPreferences, DashboardState and ControlPreferences;
no store consolidation or Python-to-Rust port here. Persistent providers use
foundation's versioned-state reader/writer: preserve unknown keys, back up
migration input, atomic writes, reject newer versions, surface failures. Don't
blindly overwrite an entire stale store snapshot. Prototype preview storage
is explicitly separate and cannot authorize production/hardware writes.

The short custom-type allowlist is `panels-diagram`, `colors-preview`,
`wallpaper-picker`, `display-arrangement`, `wifi-list`, `shared-editor`.
Shared editor targets are only Dashboard Customize and Control Center Edit.
Lists otherwise use generic entity navigation. Custom sections receive route,
store and capability interfaces, and emit navigation/actions; no new persistence
owner. Add a custom type only when generic rows/entity pages cannot express it.

Search indexes each visible setting label, description, options, synonyms,
section/page and group breadcrumb. Results identify the setting and breadcrumb;
choosing one opens its owning route, scrolls to the row and highlights it.
Keyboard: type to search except within an editable control; Up/Down/Enter in
results; rail arrows/Home/End/Enter; Escape clears a query before closing.
Use visible focus rings, wrapped descriptions, responsive control rows and an
accessible overflow with Reset to defaults on each prototype page.

Deep-link grammar is `page[/subpage][#setting]`; e.g.
`nacre-settings panels/dashboard#pages` and
`settingsApp.open("panels/dashboard#pages")`. IDs/aliases are schema metadata.
Resolve a fragment only in the destination page; reject unknown/hidden routes,
never execute a command. Aliases: desktop → windows-workspaces, workflows →
windows-workspaces/workflows, lock → lock-login, time → date-time-language,
ai → nacre-ai (or Add-ons installation destination), maintenance → system;
appearance remains Appearance. Legacy callers/CLI remain unchanged until phase 3.

## Validation, boundaries and review

Phase 1: audit/document commit. Phase 2: registry, generic renderer, search,
Panels overview, Colors, Launcher and Power & battery; search is the fifth
review surface. Embed existing editors on the two Panels subpages without
editing their implementation. Commit a gallery for 1100 px and 620 px windows
using synthetic public state in a private nested compositor. Include source
revision, dimensions and fixture/capability scope, and visually inspect images.
Demonstrate persistence in an isolated fixture directory, navigation/keyboard,
row highlight, profile badge, feature dependencies, capability gating and reset.
This is a prototype, not a claim of integrated new backend functionality.

Schema tests: duplicate IDs; valid store and typed defaults; option/range types;
all i18n IDs; section/page ownership; aliases and every setting deep link;
search reaches each visible setting; capability matrices; invalid routes/types;
no more than fifteen generic controls per page; Packages/Software remain hidden
without packages. QML tests prove actual row interaction and narrow layout.
Run applicable owner source/QML checks, then integrate only task commits and
check CI after each push. Use `[no-publish]` commits to avoid release workflows.
Do not touch live desktop/config or publish a package/site/release.

The legacy three-second poll is retained during the prototype because the old
window remains installed and display pending/monitors are not driven exclusively
by preference file changes. Phase 3 can remove the timer after event coverage;
Wave 4 owns native desktop-settings and hardware state subscription. Prototype
store itself is event-driven; there is no periodic preference polling.

**Stop after phase 2.** Owner review should choose navigation/group labels,
row density, diagram behavior, normal/narrow layout, Colors studio boundary and
unavailable-feature treatment. Only approval authorizes remaining pages, CLI/IPC
migration, removing the old catalog and the final docs/usage.md/gallery update.
