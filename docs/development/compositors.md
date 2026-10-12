# Compositor interface

Nacre currently supports Hyprland only. The shell and helpers consume a shared
compositor contract so a second backend can be added without copying the desktop.
This refactor changes transport ownership, not the layout, shortcuts, window
rules, profile choices or optional assistant permissions.

## Ownership

- `nacre/shell/services/NacreCompositor.qml` is the QML singleton. Runtime widgets
  use it for state, actions, events and shortcut registration.
- `nacre/shell-tools/compositor.py` is the matching Python module and CLI. Helpers
  import it directly; entrypoints outside that directory locate the source or
  installed module lazily. Importing either Python facade starts no processes.
- `nacre/shell/services/compositor/` owns native Quickshell types, reactive
  toplevels, capture handles, global shortcuts and compositor event translation.
- `nacre/shell-tools/compositor_backends/` owns native commands, socket framing,
  validation, configuration lowering, declaration capture and session variables.
- `hypr/` is the Hyprland backend configuration tree. The six generated files
  below are backend output, not a second editable source of bindings and rules.

`tools/compositor_boundary.py`, run by `tools/check.py` in CI, rejects native
command construction, Quickshell.Hyprland imports and native expressions outside
these boundaries. Test fixtures may describe native input/output. The boundary
itself necessarily contains the patterns it checks.

## State and actions

| Concept | QML | Python |
|---|---|---|
| Outputs | `monitors.values`, `focusedMonitor` | `query("monitors")` / `query("all_monitors")` |
| Workspaces | `workspaces.values`, `activeWorkspaceId` | `query("workspaces")`, `query("active_workspace")` |
| Windows | `windows`, `activeWindow` | `query("windows")`, `query("active_window")` |
| Focus | `focusWindow(id)` | `focus_window(id)` |
| Switch workspace | `switchWorkspace(ref)` | `switch_workspace(ref)` |
| Move window between workspaces | `moveWindowToWorkspace(id, ref, follow)` | `move_window(id, workspace=ref, follow=False)` |
| Move workspace between outputs | `moveWorkspaceToMonitor(ref, output)` | `move_workspace(ref, output)` |
| Close / float / pin | `closeWindow`, `floatWindow`, `pinWindow` | `close_window`, `float_window`, `pin_window` |
| Floating geometry | `moveWindow`, `resizeWindow`, `placeWindow` | `move_window`, `resize_window`, `place_window` |
| Configuration reload | `reload()` / `reloadConfiguration()` | `reload()` |
| Metadata refresh | `refreshState()` / `refreshWindows()` | explicit queries |
| Events | `event(value)` plus targeted signals | `events()` or `event_socket()` + `parse_event()` |
| Shortcut registration | `createShortcut(owner, params)` | generated binding declarations |

Window IDs are opaque. Current Hyprland IDs happen to look like addresses;
callers must delegate validation and selector encoding to the backend. Canonical
window fields include `windowId`, `appId`, `workspaceId`, `outputId`, `geometry`,
`hidden`, `pinned` and `fullscreenMode`. The QML window also exposes a Wayland
`captureHandle`; capture consumers do not look up native Hyprland toplevels.

Compatibility snapshot keys (`address`, `class`, `at`, `size`, `workspace`, etc.)
remain for current helper data contracts. A new backend must provide compatible
views until those consumers migrate; the keys no longer imply native address
syntax. Likewise `NacreHyprland` is a forwarding compatibility shim;
`activeWsId` remains the current numeric display alias. New logic should use
`activeWorkspaceId` and explicit workspace references.

A workspace reference distinguishes stable identity, a name, and an index on an
output. Hyprland has global, stable numeric workspace indices; its Python adapter
rejects an unsupported output-relative index rather than silently focusing the
wrong global workspace. Niri will resolve an output-relative index at action time.
Never use a changing display index as a window/workspace object's identity.

Event types include `window-focus`, `window-urgent`, `window-open`, `window-close`,
`window-workspace`, `window-title`, `window-floating`, `window-fullscreen`,
`window-pin`, `workspace-monitor-change`, `outputs-changed` and
`configuration-reloaded`. A `windowId` is included when the native event supplies
one. Unknown events are harmless. Python's optional stream starts with a state
snapshot; no stream or poller is started merely by importing the module. QML keeps
the existing event-coalescing delay and cold-start focus/cursor queries.

Capabilities distinguish `globalWorkspaces`, `stableWorkspaceIndices`,
`perOutputWorkspaces`, `scrollingLayout`, `floatingPlacement`, `pinWindow` and
`windowCapture`. Unsupported operations must fail explicitly; adapters must not
pretend that a tiled column position is an absolute floating coordinate.

Placement retains its sequence: verify ownership where applicable, enable floating,
resize when requested, then move. Existing bounded retries and timeout budgets
remain in the caller. `pearl-window.py` still requires the owned title and PID;
only native selection and action encoding moved into the interface.

## One editable configuration source

Edit `nacre/shell-tools/compositor-contract.json`, then run:

```sh
python3 tools/generate_compositor_config.py
python3 tools/generate_compositor_config.py --check
```

The source describes semantic actions, workspace targets, selectors, effects,
options, submaps and backend extension roles. The Hyprland compiler emits:

- `hypr/conf/keybinding.lua`
- `hypr/conf/windowrule.lua`
- `hypr/conf/nacre.lua`
- `hypr/conf/brain.lua`
- `hypr/conf/cursor.lua`
- `nacre/shell-tools/shortcuts.lua`

CI checks those files against deterministic output. Immutable baseline fixtures
also compare the complete ordered registrations: actions, arguments, flags,
submaps, rules, layer rules, config calls, unbinds and event subscriptions. All
94 main bindings retain their previous contract. Generated formatting is different;
registered behavior matches the pre-refactor configuration. Callback bodies for
session cursor setup and native drag handoff are explicit backend fragments,
rather than executable Lua embedded in the neutral source.

Profile-generated routes use the same rule compiler. General profiles still emit
no workspace assignments; custom profiles remain explicit opt-ins. The generator
never changes an installed profile, starts apps, signs into accounts or changes
assistant access. Existing private overrides still load after managed config.

## Hyprland to niri mapping

The future backend belongs in this repository behind the same facade. These are
implementation requirements, not claims that niri is supported today.

| Current Hyprland concept | Niri implementation requirement |
|---|---|
| Global numbered workspaces | Dynamic workspaces per output; keep stable IDs separate from changing indices. |
| Native toplevel addresses | Convert niri numeric window IDs into opaque interface IDs; publish compatible window records. |
| Reactive native metadata | Maintain an event-driven cache from niri's JSON event stream, including its initial state. |
| Directional tiled focus/move | Map to columns, windows within columns, and scrolling layout operations according to context. |
| Floating placement | Use floating-mode, width/height and floating-position actions; verify output-relative/logical-coordinate conversion. |
| Fullscreen / maximize | Distinguish full-width columns, maximize-to-edges and true fullscreen. |
| Pin/sticky windows | Advertise only after a supported implementation exists; do not silently substitute floating. |
| Native global shortcuts | Compile niri bindings into shell IPC triggers; define press/release behavior where required. |
| Layer-shell panels | Retain top/overlay/background roles; test fullscreen, overview and exclusive keyboard focus. |
| Lua config and rules | Add a KDL compiler for the same semantic source; report unsupported effects or backend extensions. |
| Monitor settings | Translate current connected-output records and mode/scale/transform transactions; preserve rollback. |
| Native drag hook | Implement equivalent panel handoff or report the unsupported backend extension explicitly. |

Niri's [IPC documentation](https://niri-wm.github.io/niri/IPC.html) describes JSON
requests and a stream that supplies initial state followed by updates. Its
[workspace model](https://niri-wm.github.io/niri/Workspaces.html) requires per-output
indices. Floating behavior and available actions are described in the
[floating-window guide](https://niri-wm.github.io/niri/Floating-Windows.html) and
[IPC action reference](https://niri-wm.github.io/niri/niri_ipc/enum.Action.html).

Before enabling a backend, test multiple outputs, output removal, stale IDs,
workspace reordering, native app focus, fullscreen/overview layering, detached
panel drag/docking, screen capture, keyboard registration and suspend/resume.
Then add session startup, portals, dependency selection and backend-specific
settings capabilities. A backend must not read another backend's private state
or start a second shell that competes for notification/clipboard ownership.

## Remaining ecosystem tools

These tools remain installed and configured as before. A Hyprland-branded tool
is not necessarily restricted to that compositor: check its Wayland protocols
and Nacre's integration separately before replacing it.

| Tool | Current coupling | Future neutral candidate / work |
|---|---|---|
| `hyprlock` | Generated lock config, output sizing, snapshot/theme and login1 launch policy. | `swaylock` uses the standard session-lock protocol. A replacement needs its own config/theme renderer and lock-ready acknowledgement; retaining hyprlock is also an option if protocol compatibility is verified. |
| `hypridle` | Private idle config and DPMS/lock callbacks. | `swayidle` uses idle-notify and login1; route DPMS through the compositor interface and preserve inhibitor/before-sleep behavior. |
| `hyprsunset` | Owned service, native temperature command/socket and external-controller protection. | `wlsunset` or `gammastep` through gamma-control; preserve ownership checks, enabled state and external-controller protection. |
| `hyprpicker` | Current color-picker tool selection. | Use backend-native color picking or a compatible Wayland picker after checking available protocols. |

The primary [swaylock](https://github.com/swaywm/swaylock) and
[swayidle](https://github.com/swaywm/swayidle) documentation describes their standard
Wayland interfaces. Niri's [gamma-control announcement](https://github.com/niri-wm/niri/discussions/272)
identifies wlsunset/gammastep compatibility. No replacements are made by this task.
Legacy helper script names remain compatibility entrypoints that delegate to the
Python interface; they do not contain native transport or selector logic.

Support diagnostics obtain bounded native version/output/config-error probes and
required executable names from the backend, retaining the report collector's
redaction and timeout policy. Panel dragging declares an explicit, modifier-free
left-button release callback in the neutral contract so release cleanup also runs
after Super is released. Its Lua extension preserves the native drag lifecycle.
