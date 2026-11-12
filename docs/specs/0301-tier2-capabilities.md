# N0301: Tier 2 capabilities and runtime smoke

Owner decision (2026-11-11): tier 1 is nacre-niri; tier 2 is stock niri and
Hyprland. The core shell runs on both tier-2 backends. Optional fork behavior
must be hidden when its capability is absent.

## Capability implementation map

The compositor topic is assembled in `crates/nacre-helper/src/daemon/compositor.rs:132-156`;
the Settings-facing `capabilities` topic maps selected flags in
`crates/nacre-helper/src/daemon/capabilities.rs:118-173`. The shell adapter
advertises supported actions through its `Snapshot.capabilities` and encodes
actions in `crates/nacre-shell/src/backends/{niri,hyprland}.rs`.

| Backend | Advertised true bits | Supporting path |
| --- | --- | --- |
| Stock niri | `perOutputWorkspaces`, `scrollingLayout`, `floatingPlacement` | Native niri snapshot and action mapping: `crates/nacre-shell/src/backends/niri.rs:140-161,223-297`; workspace projection: `crates/nacre-shell/src/workspace_rows.rs:390-415`. |
| Stock niri | `nativeWindowScreenshot` | Helper `screenshot_window` sends niri `ScreenshotWindow`: `crates/nacre-helper/src/compositor.rs:156-175`; action route: `crates/nacre-helper/src/compositor/niri_actions.rs:318-332`. |
| Stock niri | `windowGroups` | Group/collection stream negotiation defaults off on stock; this bit denotes native window grouping in the niri IPC contract. Nacre workspace collections are separately probed and projected at `crates/nacre-helper/src/daemon/compositor.rs:391-402`; do not infer collection support from this bit. |
| Stock niri | `monitorConfiguration` | Helper monitor config operations: `crates/nacre-helper/src/compositor/niri.rs:316-321`; shared monitor settings route: `crates/nacre-helper/src/desktop_settings.rs:620-697`. |
| Stock niri | `nightLightController` | Niri adapter uses the shared night-light controller: `crates/nacre-helper/src/compositor/niri.rs:30-48`; daemon device route in `crates/nacre-helper/src/daemon/`. |
| nacre-niri | All stock niri bits above | Same standard niri IPC paths. |
| nacre-niri | `workspaceCollections` | Native collection capability probe and event negotiation: `crates/nacre-helper/src/compositor/niri_transport.rs:42-104,321-331`; daemon collection rows/actions: `crates/nacre-helper/src/daemon/compositor.rs:390-402` and `crates/nacre-helper/src/daemon/shell_actions.rs:56-80,175-205`. |
| nacre-niri | `pinWindow`, `windowHide` | Native probe overlays `pinWindow`/`windowHide`: `crates/nacre-helper/src/compositor/niri.rs:30-48`; action helpers: `crates/nacre-helper/src/window_hide.rs:20-29` and `crates/nacre-helper/src/compositor/niri_actions.rs:476-490`. |
| nacre-niri | `windowForceCloseWayland`, `windowRestore` | Only published from native `force_close`/`window_restore` probe: `crates/nacre-helper/src/compositor/niri.rs:34-44`; native action boundary in `crates/nacre-helper/src/compositor/niri_actions.rs`. |
| nacre-niri | `monitorWorkspaceActions`, `launchWorkspace` | Published from the matching native probe: `crates/nacre-helper/src/compositor/niri.rs:42-48`; action calls are dispatched through the native niri action transport. |
| Hyprland | `stableWorkspaceIndices`, `globalWorkspaces`, `floatingPlacement`, `fullscreenState` | Snapshot normalization and action encoders: `crates/nacre-shell/src/backends/hyprland.rs:83-104,204-282`. |
| Hyprland | `pinWindow`, `windowHide` | Helper command routes and capabilities: `crates/nacre-helper/src/compositor/hyprland.rs:289-290,654`; window hide/restore adapter helpers. |
| Hyprland | `windowGroups`, `windowForceKill`, `pseudoTiling`, `layoutSplit` | Hyprland helper command dispatcher: `crates/nacre-helper/src/compositor/hyprland.rs:600-654`; shell capabilities/action handling in `crates/nacre-shell/src/backends/hyprland.rs`. |
| Hyprland | `keymaps`, `nonConsumingShortcuts`, `bindingIntrospection` | Binding inventory, parsing, and action query: `crates/nacre-helper/src/compositor/hyprland.rs:314-369,654`; Settings capability publication uses the compositor flags. |
| Hyprland | `windowTrash`, `scratchpad`, `monitorMirroring` | **Removed from advertised true bits.** The shell backend returns `Unsupported` for these actions (`crates/nacre-shell/src/backends/hyprland.rs:284-295`). |
| Hyprland | `windowCapture` | Retained `true` from main because the Hyprland overview could not be started in the authorized headless smoke. Overview previews use the shell capture hub (`crates/nacre-shell/src/extras.rs:351,468-489`) and Settings hides `windows-workspaces.overview.livePreviews` when this bit is false (`shell/settings/registry/settings.json:2471-2492`). The explicit `backend.windowCapture` action remains unsupported (`crates/nacre-shell/src/backends/hyprland.rs:284-295`); this capability does not establish that action's support. Preview behavior is undecided pending a Hyprland screenshot. |

The helper's stock-niri capability map is `crates/nacre-helper/src/compositor/niri.rs:27-28`;
native Nacre flags are selectively overlaid at lines 30-48. Missing or unsupported
native-probe fields default false (`niri_transport.rs:67-90`). The shell action
encoder rejects generic Niri `windowTrash`, `scratchpad`, `windowCapture`, and
`monitorMirroring` at `crates/nacre-shell/src/backends/niri.rs:298-310`; native
hide/restore use separate helper routes.

### Follow-ups

- Hyprland scratchpad: consider special workspaces, after matching Nacre's hide/restore semantics.
- Hyprland mirroring: implement through the monitor configuration route only if semantics match the advertised flag.
- Hyprland trash: wire to the existing window-trash route or leave disabled.
- Hyprland direct window capture: use geometry-based capture only if ownership and stale-window behavior are defined; overview already has the independent capture-hub fallback.

## Tier-2 runtime smoke

Status: headless, software-rendered first pass on dev-1. The desktop TEST host
pass follows when that host is back. Sessions used private HOME/XDG/runtime and
activation-free D-Bus. Product fonts were copied from the read-only N0300 cache
to each private HOME after matching the `font-assets.json` sizes and SHA-256
hashes; `fc-cache` ran. Screenshots and raw logs stay under `.lead/smoke/` and
are not committed. The Niri renderer probe did not find `llvmpipe` or `swrast`
in process mappings, so software-GL renderer confirmation is a failure despite
the Mesa software-rendering environment being set.

| Check | Stock niri | Hyprland |
| --- | --- | --- |
| Bar idle; workspace switch | PASS: `bar-idle.png`, `workspace-switching.png`; compositor dispatches acknowledged | BLOCKED: nested compositor did not start after two attempts |
| Launcher/app placement; dashboard | PASS: launcher and dashboard captured; FAIL: Widgets app route reported zero compositor clients | BLOCKED |
| Overview controls and preview fallback | PASS: overview opened; focus/workspace controls were not pointer-driven; no live preview conclusion | BLOCKED: no overview screenshot |
| Settings pages; Wallpaper collection controls | Pages opened; FAIL: Wallpaper still shows the Collections section while `workspaceCollections=false` | BLOCKED |
| Collections/workspace-stack UI | PASS: overview screenshot has workspaces and no collections/stack UI | BLOCKED |
| Widgets app, desktop widget and drag | FAIL: Widgets app had zero clients and UI add/drag was not completed; PASS: private layout write rendered the desktop clock in `desktop-widget.png`, with `desktop-widgets.*` layer-bake log entries and `bead-dev` background surface | BLOCKED |
| Detachable panels and panel drag | FAIL: Settings > Panels shows “Your desktop edges — This section is not available in Bead yet”; drag not attempted without an input seat | BLOCKED |
| Key hints, OSD and notification | Key hints opened but showed “No matching shortcuts”; OSD skipped (no hardware trigger); notification bubble visible in `notification.png` | BLOCKED |
| Lock/unlock | SKIPPED: no safe private credential/input path configured | SKIPPED: compositor did not start |
| UI_ERROR / RPC errors / Unsupported | shell 0 / 0 / 0; daemon 0 / 0 / 0 | No shell/daemon started |

Hyprland exited before creating its IPC socket on both attempts. Exact output:
`Hyprland was launched with superuser privileges, but the privileges check is not omitted. Hint: Use the --i-am-really-stupid flag to omit that check.`
The second attempt also timed out waiting for the nested Hyprland IPC socket.
No further Hyprland attempt was made. `windowCapture` remains true as on main;
preview behavior is undecided until the overview can be captured.

### For the Shell lead

- Hide Wallpaper collection management (`wallpaper.collection`) when `workspaceCollections` is false; the Niri screenshot still shows that section.
- Hide “Your desktop edges” detachable-panel controls when `nativePanelDrag` is false.
- Hide empty keybind hints when `keymaps`/`bindingIntrospection` is false; Niri showed “No matching shortcuts”.
- Keep scratchpad, trash and monitor-mirroring controls gated by `scratchpad`, `windowTrash` and `monitorMirroring` respectively; all three are false on Hyprland.
- Keep the overview live-preview setting visible under Hyprland's current `windowCapture=true`; runtime preview behavior remains undecided.
- Hide the explicit `backend.windowCapture` action where offered: the Hyprland backend rejects it. The route is registered in `crates/nacre-shell/src/reload.rs:4613-4618` and dispatched at `:4653`; no Luau callsite was found to name a specific menu entry.
- Widgets app launch returned zero clients; the private settings write rendered a desktop clock, but UI add and drag remain unverified without pointer input.
- Verified product font files were installed, yet screenshots still display truncated ligature text in several icon slots; verify the shell's icon-font selection.

## Validation and scope

Regression tests enumerate every advertised-true boolean for stock niri and
Hyprland as review snapshots. When a bit changes, verify its handler in
`crates/nacre-shell/src/backends/{niri,hyprland}.rs`. Nacre-niri uses the stock
snapshot plus the probe-gated fork overlay. The Hyprland snapshot test fails on
the base commit because four unsupported actions were advertised true.

No UI/Luau files changed. Runtime evidence, full CI and publication status are
recorded in the unit report.
