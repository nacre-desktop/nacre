# N0114: Nacre niri scratchpad and pinned windows

Status: Implemented across Nacre and nacre-niri; CI pending.

This contract depends on NN0008 providing the actions, rules and `scratchpad`
and `window_pinning` capability fields, and on N0113's `(nacre-niri)` version
identity check. Older capability replies remain readable, but the fork overlay
is validated against the selected native binary before launch.

Nacre keeps the stock niri session compatible and selects native actions only in
the `nacre-niri` session. The generated `nacre-niri.kdl` overlay contains:

- Super+H: move the focused window to the hidden scratchpad collection.
- Super+Shift+H: toggle the most recent scratchpad window; repeated presses cycle.
- Super+P: toggle pinning for the focused window.

The overlay is generated offline and is selected only for the fork. Its bindings
use `Nacre:hide`, `Nacre:scratchpad-windows`, and `Nacre:pin` descriptions so
the localized shortcut catalog can name them without changing Hyprland's
existing workspace scratchpad entry. Super+Alt+H keeps its
Hyprland destructive meaning and remains unsupported on niri.

Nacre reads `scratchpad` and `window_pinning` from the native capability IPC.
Older complete N0107 capability replies remain valid, with the new flags false;
additive future flags are accepted while malformed known flags are rejected.
Fork-specific unsupported metadata is generated beside the overlay and removes
only the three actions now supported by nacre-niri. Stock `unsupported.json`
continues to describe stock niri.

Fork-only window rules carry `open-pinned` for Nacre windows previously pinned
by Hyprland rules. These rules and bindings must not enter stock `config.kdl` or
other files read by stock niri. Both the setup generator and session refresh
must produce the same overlay and audit metadata.

Validation is the dev-1 Nacre desktop CI job in a task-named Arch container,
limited to 8 CPUs and 16 GiB; this worktree does not run local tests or builds.
