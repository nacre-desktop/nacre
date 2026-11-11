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
| Hyprland | `windowCapture` | **Decision pending runtime evidence.** The explicit `backend.windowCapture` action is unsupported (`crates/nacre-shell/src/backends/hyprland.rs:284-295`), while overview previews use the shell capture hub (`crates/nacre-shell/src/extras.rs:351,468-489`) and Settings hides `windows-workspaces.overview.livePreviews` when this bit is false (`shell/settings/registry/settings.json:2471-2492`). The screenshot check could not run because the desktop TEST host is unreachable. Keep the current false value until the nested overview capture is observed. |

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

Status: not run. The desktop TEST host did not accept SSH (`ssh -F ~/.ssh/config -o ConnectTimeout=5 desktop hostname` timed out on 2026-11-12). No private session was started and no screenshots are committed. The N0300 harness remains on dev-1; copy the needed files into `~/nacre-lead/n0301` on the desktop when reachable, and preserve N0300 unchanged.

| Check | Stock niri | Hyprland |
| --- | --- | --- |
| Bar idle; workspace switch | BLOCKED: desktop unreachable | BLOCKED: desktop unreachable |
| Launcher, app placement; dashboard | BLOCKED | BLOCKED |
| Overview focus/workspace controls and preview fallback | BLOCKED | BLOCKED |
| Settings dead controls; wallpaper collection hidden | BLOCKED | BLOCKED |
| Collections/workspace-stack UI absent | BLOCKED | BLOCKED |
| Widgets add/drag and desktop layer | BLOCKED | BLOCKED |
| Detach and panel drag behavior/hidden | BLOCKED | BLOCKED |
| Keybind hints; OSD/notification | BLOCKED | BLOCKED |
| Lock/unlock | SKIPPED: no safe nested session | SKIPPED: no safe nested session |
| UI_ERROR / RPC errors / Unsupported log counts | BLOCKED | BLOCKED |

No screenshots or raw logs are included. Shell-lead UI checks to carry into the
runtime pass: hide unsupported scratchpad controls with `scratchpad`; hide trash
controls with `windowTrash`; gate native mirror settings with `monitorMirroring`;
settle the Settings live-preview gate with the Hyprland overview screenshot. The
explicit `backend.windowCapture` route is registered by `crates/nacre-shell/src/reload.rs:4613-4618`
and dispatched at `:4653`; no Luau callsite was found, so there is no exact UI
entry to name yet. If previews render on Hyprland, keep `windowCapture` true for
the capture-hub setting and ask Shell to hide the unsupported `backend.windowCapture`
action where offered. Keep the single capability bit; this unit does not add a
second flag because the action currently has no discovered UI caller. Collections
should continue to use `workspaceCollections`.

## Validation and scope

Regression tests enumerate every advertised-true boolean for stock niri and
Hyprland against the implemented-path set. Nacre-niri uses the stock test plus
the probe-gated fork overlay; its runtime coverage is pending the desktop host.
The Hyprland test catches the base-commit mismatch because four unsupported
actions were previously advertised true.

No UI/Luau files changed. Runtime evidence, full CI and publication status are
recorded in the unit report.
