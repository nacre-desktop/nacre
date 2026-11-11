# N0200 — Launcher cleanup

## Goal

Keep launcher destinations usable on each compositor tier and with optional AI/Brain components absent; simplify the category rail without changing saved destination IDs.

## What changed

- Removed the redundant `actions.apps` and `style.wallpapers.picker`; `style.wallpapers` remains the single picker destination.
- Row `backend` metadata gates compositor-specific actions/help. AI/Brain brands use the existing capability flags; static groups disappear when all their children are hidden. Prefix hints follow the same capability topic through the existing Quick adapter. Brain results require both their source preference and capability.
- Added Niri help; the personal Hyprland shortcuts editor is Hyprland-only. All AI/Brain assistant rows carry their brand; the AI Settings shortcut now does too. External AI software offers remain ordinary Software entries, independent of the optional Nacre assistant.

## Non-working targets found

| ID | Problem | Fix |
| --- | --- | --- |
| `actions.dictation.setup` | Chose Kitty directly instead of the configured terminal. | Existing `nacre-terminal -- voxtype configure`. |
| Generated `software-open` | No UI handler for installed software rows. | Reuse the daemon's indexed desktop-app launch and hidden-app check via `desktopId`. |
| Generated Brain notes | Relative note paths fail the absolute-file target check. | Pass the search provider's existing absolute `file` path. |
| `setup.shortcuts` on Niri | Edits a Hyprland-only override that Niri never reads. | Hide using row backend metadata. |
| Compositor-gated rows | The menu adapter discarded the published backend identity. | Retain `backendName` and invalidate prepared candidates when it changes. |

Target audit: **103/103 current static actions and 43/43 groups verified**, plus **87 software and 12 font templates** (245 definitions total). All 26 provider groups were traced through their emitted operation contracts, including the software-open and Brain-note fixes above. This checks shipped wrappers/helper dispatch, declared dependencies, native routes/forms/tools, settings IDs and aliases, package-operation targets, and URL dispatch; it does not install each offered third-party package. The QML-era names `colorsApp`/`welcomeApp` have native handlers; AI verbs route through the shipped `control.sh` wrapper; files/terminal use shipped `nacre-app`.

## Compatibility snapshots

`command-catalog.json` stays: production command parsing and the CLI embed it. `command-fixtures.json` and `shell/shell-routes.json` are test-only snapshots and are removed together with their snapshot-only tests; native route behavior tests and the canonical `nacre/shell-tools/shell-routes.json` stay.

## Checks

Validation in progress on dev-1; final test, Luau, golden and full desktop CI results will be recorded before completion. No live changes or pushes.
