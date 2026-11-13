# Keyboard shortcuts

Super is usually the Windows key. These are the default keys on every
compositor unless a note says otherwise. Super+K (or Super+F1) shows the
shortcut list inside Nacre; it is also under **Learn → Keyboard shortcuts** in
the launcher and **Settings → Keyboard & mouse → Keyboard shortcuts**. The full
generated tables, including Hyprland's resize mode, are in
[usage](usage.md#shortcut-reference).

## Everyday

| Key | Action |
| --- | --- |
| Super+Space | Launcher |
| Super+Enter | Terminal |
| Super+E | File manager |
| Super+Shift+B | Browser |
| Super+I | Settings |
| Super+W | Wallpaper picker |
| Super+V | Clipboard history |
| Super+. or Super+Ctrl+E | Emoji picker |
| Super+Ctrl+Space | Command palette |
| Super+K or Super+F1 | Shortcut list |
| Super+L | Lock |
| Super+X or Super+Escape | Power menu |

## Windows

| Key | Action |
| --- | --- |
| Super+Q | Close |
| Super+F | Fullscreen |
| Super+M | Maximize |
| Super+T | Float or tile |
| Super+arrow | Focus in that direction |
| Super+Shift+arrow | Move the window |
| Super+Alt+arrow | Resize |
| Alt+Tab / Alt+Shift+Tab | Cycle windows |
| Super+G | Column tabs (niri) or window group (Hyprland) |
| Super+Ctrl+Page Up / Page Down | Previous or next window in the tabs or group |

## Workspaces

| Key | Action |
| --- | --- |
| Super+1 … 9, Super+0 | Workspace 1 to 10 |
| Super+Shift+1 … 9, Super+Shift+0 | Move the window to that workspace |
| Super+Ctrl+Left / Right, Super+scroll | Previous or next workspace |
| Super+Ctrl+Shift+Left / Right | Move the window to the previous or next workspace |
| Super+Tab | Overview |

On niri the numbers select existing workspaces on the focused output, so their
numbers can change. Super+Alt+V brings windows visiting from another monitor
onto the current one; see [monitor workspaces](configuration.md#monitor-workspaces).

## Shell and panels

| Key | Action |
| --- | --- |
| Super+O | Dashboard |
| Super+Z | Hide or show the shell frame and panels |
| Super+Shift+W | Widgets |
| Super+Ctrl+D | [Detach the open panel](detachable-panels.md) into a window |
| Super+Ctrl+C | Capture menu |
| Print Screen or Super+Shift+S | Capture an area |
| Super+Print Screen | Save a screenshot |
| Super+Shift+R | Reload the compositor configuration |

## Add-ons

| Key | Action |
| --- | --- |
| Super+A | AI sidebar (when the AI add-on is installed) |
| Super+B | Brain (when Brain is installed) |
| Super+N | Capture a note in Brain |

## Hardware keys

Volume, mute, microphone mute, display brightness and keyboard backlight keys
work when the device supports them. The power key locks the session.

## In the launcher

Arrow keys move between the category rail on the left, the results and the
quick controls. Typing always goes back to search. While you have typed text,
Left and Right move the cursor; Left at the start of the text moves into the
rail. Enter opens the selected item, Tab shows its actions, and Escape closes
the launcher. Clicking the logo in the bar opens the same Home view as
Super+Space.

## Compositor extras

On **nacre-niri** and **Hyprland** (not stock niri):

- Super+H hides the window, Super+Shift+H brings back the last hidden one and
  Super+Alt+H brings back the hidden windows on this workspace.
- Super+P keeps the window on all workspaces.

On **nacre-niri** only: Super+Ctrl+Tab and Super+Ctrl+Shift+Tab switch desks,
Super+Ctrl+N creates one, and Super+Ctrl+Shift+Page Down / Page Up move the
window to the next or previous desk. Super+Ctrl+Shift+Q force closes the
window; it also closes that app's other windows.

On **Hyprland** only: Super+S toggles the scratchpad and Super+Ctrl+S sends the
window there, Super+J toggles the split direction, Super+Shift+P toggles pseudo
tiling, Super+R enters resize mode, and Super with the left or right mouse
button drags or resizes a window. The put-away/undo keys (Super+Ctrl+Q,
Super+Shift+Q) are off unless you
[opt in](configuration.md#personal-trash-bindings-opt-in).

## Change a key

- **niri**: add your binds to `~/.config/nacre/niri/custom.kdl` in niri's
  `binds { }` syntax. Nacre loads it last and drops its own default on the same
  key. If the file makes the configuration invalid, the session starts without
  it. Log out and back in to apply a new file.
- **Hyprland**: launcher → **Setup → Edit personal shortcuts** opens
  `~/.config/nacre/shortcuts.lua`. Unbind the combination first, assign the new
  action, then reload with Super+Shift+R. See
  [default apps and shortcuts](configuration.md#default-apps-and-shortcuts).
