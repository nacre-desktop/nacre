# Versioned settings and state

Nacre-owned persistent JSON uses the shared
[`nacre-state-files.json`](../../nacre/shell-tools/nacre-state-files.json) registry.
Rust embeds this registry; the independent Python guard reads it. QML-owned stores
publish through the restricted native `state-file` command. Source tools, installed
helpers and the palette wheel share the same guard without requiring Orient to
load during installation.

A missing version is legacy version **0**. Reads remain compatible and do not
rewrite legacy files. The next save validates its input, locks the document,
checks the **existing on-disk version again**, and creates an exact original-byte
`FILE.pre-vOLD-to-vNEW.bak` before atomic replacement. Each migration backup is
created once and never replaced. A malformed document recovers from defaults on
read; a save retains `FILE.corrupt.bak` first. Unsupported version metadata and
unknown newer versions fail closed, including reset/default writers. Failed
backups prevent replacement. Backups and locks are 0600; linked destinations or
backup files are refused. Recovery does not imply secure erasure or provider-side
deletion.

Object documents carry `version` beside their existing fields. Legacy top-level
lists become `{ "version": 1, "items": [...] }` on disk; helper callers still
receive lists. Existing public version fields are retained. Control center v1
`optionalTiles` migrates to v2 `tiles` by appending them to the original default
order. Legacy permissive boolean/float version-1 markers are treated as v0 and
canonicalized with a backup. AI settings/project path overrides use explicit
schema identities rather than depending on their filenames.

## Persistent JSON inventory

Paths below `nacre/` are relative to the config, state or share root identified by
the owning writer; Rust/QML accept absolute XDG roots. Python helpers which still
use HOME paths retain that behavior until their port. `*` covers per-item records.
The registry is the machine-readable inventory. Every listed schema uses the
migration, backup, recovery and downgrade guard above.

| Store | Path / family | Owner | Current version | Shape |
| --- | --- | --- | --- | --- |
| profile | `nacre/profile.json` | profile-policy / installer | 1 | object |
| cli | `nacre/cli.json` | shell-cli | 1 | object |
| desktop | `nacre/desktop.json` | desktop-settings | 1 | object |
| launcher | `nacre/launcher.json` | launcher-preferences | 1 | object |
| launcher-menu | `nacre/launcher-menu.json` | installer seeds; user-edited entries | 1 | object |
| launcher-workflows | `nacre/launcher-workflows.json` | launcher-workflows | 1 | object |
| dashboard | `nacre/dashboard.json` | dashboard-data | 1 | object |
| dashboard-extras | `nacre/dashboard-extras.json` | NacreDashboardState | 1 | object |
| wallpaper-picker | `nacre/wallpaper-picker.json` | wallpaper-media / shell-cli | 1 | object |
| colors | `nacre/colors.json` | colors / shell-cli | 1 | object |
| welcome | `nacre/welcome.json` | welcome | 1 | object |
| workflows | `nacre/workflows.json` | workflow-profiles | 1 | object |
| ai-settings | `nacre/ai/settings.json` | nacre-ai | 1 | object |
| ai-projects | `nacre/ai/projects.json` | nacre-ai / AI installer | 1 | object |
| brain-sync-config | `nacre/brain-sync.json` | user-edited sync configuration | 1 | object |
| control-center | `nacre/control-center.json` | NacreControlPreferences | 2 | object |
| scheme | `nacre/scheme.json` | shell-cli / classic-state | 1 | object |
| presentation | `nacre/presentation.json` | classic-state / package runtime | 1 | object |
| wallpaper-media | `nacre/wallpaper/media.json` | wallpaper-media | 1 | object |
| dashboard-focus | `nacre/dashboard-focus.json` | dashboard-data | 1 | object |
| pearls | `nacre/shell/pearls.json` | pearl-state | 1 | object |
| notifications | `nacre/shell/notifications/history.json` | notification-history | 1 | array |
| chat-list | `nacre/shell/extras/chats.json` | desktop-extras / sidebar-chat | 1 | object |
| clipboard-pins | `nacre/shell/extras/pins.json` | desktop-extras | 1 | array |
| brain-focus | `nacre/shell/extras/brain-focus.json` | desktop-extras | 1 | object |
| drafts | `nacre/shell/drafts/*.json` | composer | 1 | object |
| sidebar | `nacre/shell/sidebar/*.json` | sidebar-chat | 1 | object |
| control-tools | `nacre/control-tools.json`<br>`nacre/control-tools-niri.json` | control-tools | 1 | object |
| display-pending | `nacre/display-pending.json`<br>`nacre/display-pending-niri.json` | desktop-settings | 1 | object |
| battery-alerts | `nacre/battery-alert-state.json` | NacreBatteryAlerts | 1 | object |
| welcome-session | `nacre/welcome/session.json` | welcome | 1 | object |
| welcome-demo | `nacre/welcome/demo/*.json` | wallpaper-media | 1 | object |
| recording | `nacre/menu/recording.json` | menu actions / launcher-workflows | 1 | object |
| recording-audio | `nacre/menu/recording-audio.json` | launcher-workflows | 1 | object |
| stay-awake | `nacre/menu/stay-awake.json` | menu actions | 1 | object |
| webapps | `nacre/menu/webapps.json` | menu registry | 1 | array |
| reminders | `nacre/menu/reminders.json` | menu registry | 1 | array |
| configuration | `nacre/configuration.json` | configure | 1 | object |
| modules | `nacre/modules.json` | configure / nacre module | 1 | object |
| package-links | `nacre/package-links.json` | package runtime | 1 | object |
| defaults | `nacre/defaults.json` | package seed | 1 | object |
| release-current | `nacre/releases/current.json` | releases | 1 | object |
| release-record | `nacre/releases/*/release.json` | releases | 1 | object |
| retired-native | `nacre/releases/*/retired-native.json` | releases | 1 | object |
| session-latest | `nacre/session/latest.json` | session-watch | 1 | object |
| performance-latest | `nacre/performance/latest.json` | maintenance | 1 | object |
| brain-sync-status | `.local/state/nacre/brain-sync.json` | nacre-brain-sync | 1 | object |
| chat-titles | `nacre/ai/chat-titles/*.json` | nacre-ai-chat | 1 | object |
| chat-directories | `nacre/ai/chat-titles/directories/*.json` | nacre-ai-chat | 1 | object |
| fonts | `nacre/fonts/installed.json` | font-setup | 1 | object |
| font-transaction | `nacre/fonts/*/transaction.json` | font-setup | 1 | object |
| dolphin-default | `nacre/dolphin-default.json` | dolphin-files | 1 | object |
| thunar-default | `nacre/thunar-default.json` | install-thunar | 1 | object |
| thunar-style | `nacre/thunar-style.json` | thunar-files | 1 | object |
| timezone-config | `etc/nacre/timezone.json` | timezone | 1 | object |
| timezone-status | `nacre/timezone/status.json` | timezone | 1 | object |
| brain-auth | `nacre/brain/auth/session.json`<br>`nacre/brain/auth/bootstrap.json`<br>`nacre/brain/auth/browser.json` | Brain auth | 1 | object |
| brain-overrides | `.brain-state/discovery/overrides.json` | Brain discovery | 1 | object |
| brain-subjects | `.brain-state/discovery/subjects.json` | Brain discovery | 1 | object |
| brain-vectors | `.brain-state/discovery/vectors.json` | Brain semantic client | 1 | object |
| brain-peers | `.brain-state/peers/*.json` | nacre-brain-sync | 1 | object |
| brain-conflicts | `.brain-state/conflicts/*.json` | nacre-brain-sync | 1 | object |
| brain-captures | `.brain-state/capture-*.json` | nacre-ai-memory | 1 | object |
| brain-backups | `.brain-state/backups/*.json` | nacre-brain-sync | 1 | object |
| install-backups | `nacre/backups/*/manifest.json`<br>`nacre/shell/backups/*/manifest.json`<br>`nacre/config-backups/*/manifest.json` | install / configure | 1 | array |

| apps | `nacre/apps.json` | menu actions / user editor | 1 | object |

## Other written formats and derived data

These are inventoried separately because changing their fields would break their
owning format, or because they are disposable derived caches rather than durable
preferences/history. They are not converted to persistent JSON by this pass.

| Family | Written files | Format / owner |
| --- | --- | --- |
| Provider accounts and conversation records | Codex/Claude account auth, TOML/JSON provider settings, session JSONL/SQLite databases and browser profiles | Provider-owned schemas and migrations. Nacre's own settings, drafts, sidebar session pointers and title metadata are in the registry above. Never copy provider authentication. |
| Brain knowledge | Vault Markdown/text/Canvas/Base notes and curated wiki pages | User content; wiki expected-hash updates and append-only evidence rules remain authoritative. Discovery/vectors, sync peers/conflicts, capture deduplication and backup JSON are versioned above. |
| Append-only session evidence | `state/nacre/session/events.jsonl` | Each newly appended record has `version: 1`. Old lines remain readable, are not rewritten, and are not migrated in place. |
| Generated desktop and system configuration | Hyprland Lua/config, Niri KDL, Fish/Kitty/GTK/Dolphin/Thunar config, app launchers, systemd/D-Bus/polkit files, SDDM/login themes, timezone/GeoClue config and generated profile Lua | Native format owned by its component. Configuration/release/font transaction JSON records track copies, checksums and rollback; `profile.json` and timezone JSON are versioned separately. Source apply/rollback policy still controls native files. |
| Palette and media products | Generated QML/JS palettes, color/theme templates, thumbnails, icons, artwork, posters, lock images, terminal image packets, user screenshots/recordings and exported color palettes | Generated or user-kept assets, not JSON preference documents. Orient's `formatVersion` remains the palette protocol authority. |
| Shell caches | `cache/nacre/weather.json`, `weather-photo.json`, `updates.json`, `lock-widgets.json`, `lock-art-source.json`, `lock-outputs.json`, media/album palettes, `colors/*.json`, `wallpaper-media/*.palette.json`, menu catalog cache, file-icon indexes and terminal-logo receipts/raw packets | Rebuildable caches; existing format/version/expiry behavior retained. Persistent selections and wallpaper/media records are in the inventory above. |
| Short-lived session IPC and recovery | Runtime sockets, locks, selector requests/responses, display fallback requests, prepared lock snapshots/dashboard files, minimized/trash window records, PID/source-revision/latest-backup/last-wallpaper markers | Process/session protocols or short-lived caches. Do not treat these as durable preferences. |
| Diagnostic experiment evidence | Exported doctor reports, `acceptance-observed.json`, `acceptance-events.jsonl`, `baseline-hashes.json` and performance measurement outputs | Diagnostic/report protocols, not persistent preferences; existing recorder/provenance rules remain authoritative. |
| Build/install receipts | Prepared `installation.json`, package metadata/seed maps, Orient/niri build manifests, verified wallpaper receipts, runtime fingerprints and staged package contents | Generated immutable build artifacts; their existing schema/provenance and package checks remain authoritative. Installed configuration/release records and private rollback manifests are versioned above. |

## Validation and port contract

Rust and Python tests exercise every registered schema for version-0 migration,
exact one-time private backups, corrupt recovery and newer-version refusal.
Additional tests cover control-center v1 conversion, linked backup refusal,
concurrent saves, path overrides, source/package module loading and the real native
QML writer. Frozen parity files remain unchanged; the harness explicitly compares
legacy payloads across the new version envelope/lock/backup representation and
tracks notification/profile corrupt recovery and JSONL record versions as intended
differences. Diagnostics uses strict reads so corrupt configuration still reports
maintenance unavailable. It still
compares all non-state files, action effects, IPC and payload fields.

New persistent state must be registered and use these boundaries. Rust ports must
retain the guard, original-byte backup, explicit future-version refusal and existing
caller payload shapes. Adding a future migration requires a new schema version and
fixtures for the old and new data; do not widen a downgrade guard or replace a
backup to make a test pass. No live files are migrated by developing or testing
this code; migration occurs only when installed code later saves a document.
