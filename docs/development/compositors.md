# Compositor interface

The shell and helpers consume a shared compositor contract. Hyprland retains its
existing behavior; the niri backend targets niri 26.04 using JSON IPC. Backend
selection does not alter layout, profile choices or optional assistant permissions.
Niri's differences and source-session validation steps are documented below;
packaged niri setup remains gated separately from backend implementation.

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
- `niri/` contains generated niri bindings, rules and an unsupported-declaration
  report. Its palette template is backend-owned and uses Orient roles.
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
wrong global workspace. Niri resolves an output-relative index at action time, then dispatches the stable
native ID. The necklace shows only the focused output's dynamic workspaces,
including its empty trailing workspace. Display numbers never identify windows.
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

### Proportional widths and application constraints

The prepared Niri session opens normal columns at `proportion 0.5`, so the width
follows the output's working area and scale. Native Wayland applications can
still enforce a larger minimum size; Niri's `min-width` rule only increases the
application's minimum. An application that renders a wider surface has the final
say, even when the compositor asks for a smaller size.

`nacre-niri-widths.service` adds compatibility for ordinary tiled X11 applications.
It listens to Niri window events and X11 property events, and relaxes only the
advisory minimum width. Newly opened constrained windows get one half-width
correction after the hints reach the bridge, handling apps that already committed
an oversized initial surface. Existing windows and subsequent manual geometry are
not reset. The opening percentage can be changed with `--opening-width`; it defaults
to 50 to match Nacre's standard Niri layout. App zoom stays unchanged. Later app hint
changes are handled without polling or changing manual column widths.

Floating windows, transient dialogs, non-normal window types and explicitly fixed
widths retain their constraints. Switching to floating restores the latest app
minimum, as does stopping the helper. The observer skips Hyprland and sessions
without a usable Niri/X11 context. Its code and service ship with core installation,
including the Arch package and rollback snapshots.

For applications with an enforced native minimum, their X11 mode can be more
flexible. This is a compatibility choice with X11's scaling behavior, rather than
a universal guarantee that every application's content can fit every viewport.
No monitor-specific scale factor is required. Avoid changing a native app's
platform unless its minimum-size behavior has actually been verified.

The standalone native check uses stock Niri, a separate nested compositor and
synthetic X11 clients, without real app accounts or a session activation import:

```sh
python3 tools/tests/verify_niri_widths.py --output /tmp/niri-width-proof.json
```

It verifies proportional widths while resizing the nested output, renewed app
minimum hints, manual resize ownership, floating/fixed/dialog exclusions and
restoration on service shutdown. Disabling the helper restores advisory hints;
it does not reset existing manual window sizes.

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

CI checks those files against deterministic output. Historical baseline fixtures
compare unchanged rules, config calls and event subscriptions; current default
fixtures compare ordered bindings, actions, arguments, flags, submaps, layer rules
and unbinds. The general desktop now has 97 main bindings, including workspace
1–10 and familiar app, overview, lock and screenshot shortcuts. Personal app
launches are absent from the shared declarations. Callback bodies for
session cursor setup and native drag handoff are explicit backend fragments,
rather than executable Lua embedded in the neutral source.

Profile-generated routes use the same rule compiler. General profiles still emit
no workspace assignments; custom profiles remain explicit opt-ins. The generator
never changes an installed profile, starts apps, signs into accounts or changes
assistant access. Existing private overrides still load after managed config.

Super+Left and Super+Right repeat while held. Niri lowers horizontal focus bindings
to native column-focus actions with a 120 ms cooldown, avoiding a helper process
for every repeated step without changing the keyboard's typing repeat rate.

## Hyprland to niri mapping

`NiriBackend.qml` consumes the Python backend's canonical stream through one
persistent process. The Python reducer waits for both initial window/workspace
snapshots before publishing state, then applies native deltas. Output metadata is
queried initially and on workspace/configuration changes because niri does not
provide an output event variant. No periodic poller starts on import or while idle.
Disconnects block QML actions and trigger bounded-backoff reconnection. Requests
are JSON-framed on the local `NIRI_SOCKET`; niri errors remain explicit failures.

Niri u64 IDs become `niri-window:<decimal>` and `niri-workspace:<decimal>` strings
before reaching JavaScript, preserving values beyond its exact-integer range.
The opaque ID validator accepts native zero and rejects malformed/out-of-range
IDs. The fixture suite exercises multiple outputs, focus clearing, urgency,
workspace reordering/removal, stale windows and incremental geometry changes.

| Concept | Niri behavior / capability |
|---|---|
| Workspaces | Per-output dynamic indices, stable opaque IDs; `perOutputWorkspaces=true`, `stableWorkspaceIndices=false`. Numeric shortcuts resolve on the focused output. Named or `{id}` and `{output,index}` references are explicit. |
| Directional focus/move | Left/right use columns, up/down use windows within columns; niri owns floating-layout interpretation. |
| Floating pearls | Keep title+PID ownership checks. Enable floating, resize, then use measured global geometry and relative movement; cross-output moves requery geometry. Unknown geometry fails rather than assuming a coordinate. |
| Floating coordinates | Native fixed positions include working-area offsets; measured relative deltas avoid guessing reservations, border offsets or scale. All geometry is in logical pixels. |
| Docking | Existing application-owned lift/release and `pearls detach/dock` IPC remain. Niri-native Mod+mouse moves do not emit Hyprland's drag-end hook; `nativePanelDrag=false`. Use the existing detach toggle on a focused pearl to dock. |
| Shortcuts | Generated binds invoke the facade/helper or shell IPC. Registration activation emits a pulse, never a claim of physical key holding; `shortcutRelease=false`. Non-consuming and Hyprland submap/pointer hooks are not generated. |
| Fullscreen/maximize | Fullscreen and maximize-to-edges are distinct actions. The 26.04 window record has no fullscreen bit; `fullscreenState=false` and `fullscreenKnown=false` make that limitation explicit. |
| Pin / hide / force-kill | Unsupported and fail explicitly; `pinWindow=false`, `windowHide=false`. Do not substitute floating for sticky windows or signals for owned force-kill. |
| Cursor / per-window capture | No global cursor-position query or unambiguous niri-ID-to-Quickshell capture mapping; `cursorPosition=false`, `windowCapture=false`. No fabricated native handle. |
| Settings / night light | The niri backend does not expose runtime desktop-option writes, monitor transactions or a night-light controller; capabilities advertise these limits. Session/controller integration belongs to its own task. |

## Niri configuration and source testing

Select `NACRE_COMPOSITOR=niri` explicitly, or let `XDG_CURRENT_DESKTOP=niri`
select it. Otherwise an existing Hyprland session signature wins over a leftover
niri socket; a lone `NIRI_SOCKET` selects niri. An unknown explicit backend is an
error. Both QML and Python use the same precedence. Source checks can override
`NACRE_COMPOSITOR_TOOLS` and `NACRE_PEARL_TOOLS` with this checkout's
`nacre/shell-tools` path; installed paths remain untouched. The PC/session agent
owns package installation, UWSM/session files and live acceptance; do not deploy
this branch into a daily session as a test shortcut.

```sh
python3 tools/generate_compositor_config.py --backend niri
python3 tools/generate_compositor_config.py --check
NACRE_COMPOSITOR=niri python3 nacre/shell-tools/compositor.py capabilities
NACRE_COMPOSITOR=niri python3 nacre/shell-tools/compositor.py query workspaces
```

The generator writes `niri/bindings.kdl`, `niri/window-rules.kdl` and
`niri/unsupported.json`, never the user-owned `niri/config.kdl`. Includes are
supported since niri 25.11; home-directory include paths require 26.04. Add
reviewed top-level includes **before** user settings so later user bindings and
properties win. Included files contain full top-level sections; includes inside
`binds` or `layout` are invalid. See the upstream
[include merge rules](https://niri-wm.github.io/niri/Configuration:-Include.html).
Validate without a session or a user-config write:

```sh
test_config_dir=$(mktemp -d)
cp niri/bindings.kdl niri/window-rules.kdl "$test_config_dir/"
printf 'include "bindings.kdl"\ninclude "window-rules.kdl"\n' > "$test_config_dir/config.kdl"
niri validate -c "$test_config_dir/config.kdl"
```

The generator, validator and `capabilities` command above are offline. The
`query workspaces` command requires the intended session's live `NIRI_SOCKET`.
Generated compositor-action commands honor `NACRE_COMPOSITOR_TOOLS`; shortcut
commands can target a source instance through `NACRE_QS_CONFIG` and optional
`NACRE_QS_BIN`. With the PC agent's isolated nested Wayland display and DBus
session already selected, source-shell invocation is:

```sh
export NACRE_COMPOSITOR=niri
export NACRE_COMPOSITOR_TOOLS="$PWD/nacre/shell-tools"
export NACRE_PEARL_TOOLS="$NACRE_COMPOSITOR_TOOLS"
export NACRE_QS_CONFIG="$PWD/nacre/shell/shell.qml"
export NACRE_QS_BIN=$(command -v quickshell)
"$NACRE_QS_BIN" -p "$NACRE_QS_CONFIG"
```

Do not run that full shell alongside the daily shell on its display/DBus session:
notification/clipboard ownership must remain isolated. Inherited application
launch commands still target installed app/controller scripts; helper overrides
alone do not make those application bindings source-isolated. Source acceptance
can test facade/shortcut IPC directly; full application-bind acceptance needs the
PC/packaging agent's matching isolated runtime. Generated `spawn-sh` bindings
inherit **niri's** environment, not the shell's. For generated-key testing, set
these overrides before the PC agent starts the isolated nested niri process;
exporting them only before launching Quickshell tests direct facade/IPC calls,
not the environment of an already-running compositor. No installed-script rewrite
or session file edit is performed by the generator.

`compositor_config.supported_backends()`, `backend_capabilities(name)`,
`render(contract, backend=...)`, `render_keyboard(values, backend=...)` and
`render_profile(profile, backend=..., workspace_names=...)` are the public setup
API. Hyprland rendering is unchanged. Niri custom initial workspace routes require
an explicit number-to-name map because a config rule cannot pin a changing
per-output index. Default profiles still emit no routes. The independent
`setup_ready` gate is false for niri until packaged offline deployment is complete;
the backend alone does not promise installer/session support.

The palette template sets the active focus-ring and border gradient from Orient
`primary` to `secondary` at 45°, matching the Hyprland palette's role order and
angle. It retains solid primary fallback, inactive `outlineVariant` and urgent
`error` colors, leaving enabled state and widths to the user. Niri gives gradients
precedence over solid colors; an explicit user gradient can be included afterward.
The unsupported report lists pin/center/size effects, native layer extensions,
submaps and actions without exact niri equivalents. Unknown declaration kinds
fail generation; unsupported known semantics are reported rather than silently
converted. Floating pearls are emitted as floating window rules, with final
placement owned by IPC and the verified title+PID helper.

Python socket/event/action fixtures and native Qt tests exercise the actual QML
backend. Generator tests enforce unchanged Hyprland bytes and validate include parsing,
keyboard/profile APIs and unchanged user wrapper files. The effective later-user
binding/color precedence follows upstream documented merge semantics; the parse
test does not inspect niri's resolved configuration. The native KDL parse test runs when
`niri` is installed; CI always runs deterministic source/output checks and the
fixture tests. Full real-session multi-output, native pointer movement, fullscreen,
layer-shell focus, capture and suspend/resume remain separate acceptance checks.

Niri's [IPC documentation](https://niri-wm.github.io/niri/IPC.html) describes JSON
requests and a stream that supplies initial state followed by updates. Its
[workspace model](https://niri-wm.github.io/niri/Workspaces.html) requires per-output
indices. Floating behavior and available actions are described in the
[floating-window guide](https://niri-wm.github.io/niri/Floating-Windows.html) and
[IPC action reference](https://niri-wm.github.io/niri/niri_ipc/enum.Action.html).

Before enabling a backend, test multiple outputs, output removal, stale IDs,
workspace reordering, native app focus, fullscreen/overview layering, detached
panel drag/docking, screen capture, keyboard registration and suspend/resume.
Session startup, portals, dependency selection and backend-specific settings
capabilities remain separately owned integration work. A backend must not read another backend's private state
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
