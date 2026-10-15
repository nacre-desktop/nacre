# Configuration and profiles

Use Settings (Super+I) for everyday appearance, device and desktop preferences.
Profiles choose installation components, workspace labels/icons, startup commands,
window routes and optional app workspaces. They do not contain account data,
wallpapers, conversations or notes.

## Profiles

The [default profile](../profiles/default.json) has `version: 1`, six workspace
pearls, no startup apps and no app-to-workspace assignments. AI and Brain open on
the current workspace unless `aiWorkspace` or `brainWorkspace` is set. The bar
keeps extra occupied workspaces visible; scroll over the pearls to switch, or
click a window symbol to focus it. Turn off **Show window symbols** in
**Settings → Appearance → Workspace bar** for a quieter bar.

Pass `--profile default` or `--profile /path/to/profile.json` to `nacre setup`.
Source installs accept the same choices through `./install.sh --profile`.
Omitting the option preserves an existing profile; explicitly choosing `default`
replaces its startup/routing choices. Updates preserve private overrides or refuse
conflicting edits. Startup entries are argument arrays, not shell snippets.

## Monitor workspaces

**Settings → Windows & workspaces** selects Niri's monitor policy. The default is
**Primary fixed, other monitors dynamic**. The primary keeps `workspaceCount`
named workspaces (six for new profiles). Other monitors start with Niri's single
unnamed workspace and grow when used; unused workspaces disappear. Nacre never
creates additional empty workspaces. Niri itself keeps its native trailing spare.
Existing profiles retain their explicit workspace count.

**Primary workspace monitor** accepts a connector such as `DP-1`. If it is absent,
the laptop display (`eDP`/`LVDS`) is preferred, followed by the first connected
connector in name order. Leaving it blank uses the same fallback. The fixed set
follows the selected primary, including when undocking. Startup app routes keep
using workspace names rather than unstable display numbers.

Opt into **Fixed sets on all monitors** to keep an equal named set on every
connected output. On disconnect, occupied workspaces visit a remaining monitor.
Empty named visitors lose their names and are removed by Niri. Reconnect recreates
the full set with `open-on-output`; occupied visitors retain their identities and
return home. Each monitor's previous active workspace and selected window are
restored when still available, including an empty named slot recreated with a new
native ID. An empty dynamic view reuses Niri’s existing spare. Closing a window while disconnected cannot restore that closed window.

Bars show their own set first, then a divider and occupied visitors grouped by
home monitor with a monitor badge. Empty visitors are never appended. A three-second
presentation grace period absorbs disconnect/reconnect blips. Niri's native window
moves happen immediately; the bar never holds up compositor recovery. Bar clicks
use opaque workspace IDs, so a changing display number cannot retarget a click.

**Bring visitors here** is available in the launcher and on **Super+Alt+V** in both
modes. In fixed mode it merges visiting windows into the same-numbered local slot.
In dynamic mode it merges into the current workspace, appending tiled windows as
columns at the right. Selecting a visiting dynamic workspace also adopts that
workspace on its present monitor. Merged/adopted windows stay after reconnect.
The command operates on visitors on the focused monitor and preserves the prior
view/selected window where that window still exists. Floating windows keep their
floating state.

**Send, swap and rotate shortcuts** is an additional opt-in, available only with
fixed sets on all monitors. In the default dynamic-secondary model these shortcuts
are disabled because another monitor has no fixed numbered destination set.

| Shortcut | Action |
| --- | --- |
| Super+Ctrl+1–6 | Send the selected tile to that workspace on the next monitor. |
| Super+Ctrl+Shift+1–6 | Send it to the previous monitor instead. |
| Super+Alt+PageDown / PageUp | Exchange the selected workspace with the next / previous monitor’s selected workspace. |
| Super+Alt+Shift+PageDown / PageUp | Rotate all configured workspace sets forward / backward between monitors. |

Monitor order for these controls follows layout position: left to right, then top
to bottom, with connector name breaking ties and wrapping at either end. They work
in Bird's Eye with the keyboard-selected tile/workspace. Sending keeps the current
view; exchanges retain windows/layout and save the new home assignments. All
connected monitors must have a complete set before an operation starts. A failed
exchange attempts to restore owners, order and views.

The private versioned `desktop.json` store uses `niriWorkspaceMode`
(`primary-dynamic` or `fixed-all`), `workspacePrimaryOutput`,
`monitorWorkspaceActions` and `niriWorkspaceSets` (connector → unique names).
Version 3 migrates the former `niriWorkspaceOutputs` map to `niriWorkspaceSets`
and selects the new default unless a mode was explicitly set. Reading is lazy;
the first successful save, including managed session reconciliation, keeps the
original bytes in `desktop.json.pre-vN-to-v3.bak`. Newer-version stores are refused
without modification. Private output names and assignments stay outside profiles.
Prepared sessions own `workspaces.kdl` and the optional shortcut include; changing
the policy in Settings updates these through the compositor event owner.

**Hyprland differs:** its workspace numbers are global, and its current Nacre
backend has no per-output dynamic workspace sets or reliable original-output
visitor identities. These Niri policy controls, visitor merge action and monitor
send/swap/rotate bindings are therefore unavailable there. The existing global
workspace bar and ordinary move/focus shortcuts remain supported. This change
does not emulate Niri hotplug return behavior on Hyprland.

## Personal trash bindings (opt-in)

Super+Q closes immediately on both compositors. To enable Hyprland's personal
put-away/undo bindings, add `"windowTrashBindings": true` to a copy of your profile
and select that file during setup. The field defaults to false. It never changes
Super+Q. Super+Ctrl+Q puts a window away for 30 seconds; Super+Shift+Q restores it
before the deadline. Restore-all and force-close are also opt-in. These actions
are unavailable on niri. After the deadline, a window that resists closing can be
forcibly terminated, losing unsaved work. See the [generated binding table](usage.md#personal-trash-shortcuts-opt-in-hyprland-only).

## Default apps and shortcuts

Launcher → **Setup → Default applications** edits `~/.config/nacre/apps.json`.
Commands are argument arrays:

```json
{
  "terminal": ["alacritty"],
  "files": ["thunar"],
  "browser": ["firefox"]
}
```

Super+Enter, Super+E and Super+Shift+B use these choices. Missing entries retain
the shipped Kitty, Nacre Dolphin and Chrome defaults; install the chosen apps
separately.

On Hyprland, **Setup → Edit personal shortcuts** opens
`~/.config/nacre/shortcuts.lua`. Unbind an occupied combination before assigning
a different action, then reload with Super+Shift+R. Keyboard overrides live in
`~/.config/nacre/keyboard.lua`. Private monitor, palette, desktop, profile and host
overrides load after the managed base, with host overrides last. Niri uses KDL;
Hyprland Lua overrides are not translated or executed by its preparation path.
The [shortcut tables](usage.md#hyprland-shortcuts) describe generated defaults.
Hyprland's shortcut list reads session bindings; niri cannot inspect private
binding overrides.

On Niri, Super+Tab opens Bird’s Eye. Super+Shift+1–9 (0 for workspace 10)
moves the selected tile to that workspace on the current output, keeping the
overview open and focus on the source workspace. The keyboard-selected tile
is used; pointing at a different preview does not change the keyboard selection.

## Files and XDG paths

At the default locations:

| Location | Contents |
| --- | --- |
| `~/.config/nacre` | Profiles, app choices and private preferences, including `desktop.json` and `launcher.json` |
| `~/.local/state/nacre` | Runtime state and managed release/rollback records |
| `~/.cache/nacre` | Disposable shell caches |
| `~/.local/share/nacre` | Per-user runtime code/references and optional app data |
| `/usr/share/nacre`, `/usr/lib/nacre` | Package-owned shared code and native integrations |

The shell's path service honors absolute `XDG_CONFIG_HOME`, `XDG_STATE_HOME` and
`XDG_CACHE_HOME`, adding `/nacre`. The palette CLI also honors those variables.
Setup and several helpers still use the default home-relative paths, and runtime
code uses `~/.local/share/nacre`; changing XDG variables does not relocate a whole
installation. Keep the defaults for a complete setup today.

Accounts, conversations, wallpaper libraries and the [Brain vault](optional-apps.md)
are separate user data. Do not delete the whole Nacre directory to reset a
preference; use the [safe reset steps](recovery.md#how-do-i-reset-settings-safely).
Run only one full Nacre shell per session so notifications and palette changes
have one owner.

## Saved launcher items and file search

Use **Actions → Saved launcher items** to add actions, snippets, bookmarks,
search engines, workspace recipes and extra file search folders. These forms
write `~/.config/nacre/launcher-workflows.json` (or the corresponding absolute
XDG configuration directory). They do not replace existing launcher menu
extensions or change the project registry. The file stays private.

Each command has an executable and literal arguments, entered one per line;
spaces within a line remain part of that argument. Actions can specify a working
directory. Workspace command lines use `{project}` for the chosen folder.
Named searches use an HTTP/HTTPS URL containing `{query}`; activation encodes
the query before opening the browser. Searching alone does not open a website.

File search includes local registered project paths outside `~/Projects`.
Additional roots must be existing absolute directories; filesystem root is not
a search folder. Previews read at most a small text excerpt or a bounded image,
and resolve symbolic links before checking folder ownership. Saved items can be
removed through their item actions without editing the JSON file.

The versioned `launcher-sources.json` store controls Brain, Files, Calculator,
Actions and AI search sources. All default to enabled. The launcher watches the
file for changes; these owner switches are separate from whether a tool or device
is available. Invalid or newer stores are left untouched and use safe defaults.
`webEngine` selects `duckduckgo` (default), `google` or `brave` for the generic web
search result. Saved named web searches keep their explicit URLs.

### Monospace and terminal fonts

Style → Terminal font and Settings → Appearance → Fonts share the registered
`appearance.fonts.monoFont` preference in `desktop.json`. The curated Phase 1
families can be installed from Style; installed families are selectable without
changing the interface font. Ordinary launcher search finds terminal font choices.
Nacre monospace text and Kitty use this choice. An empty choice restores the
Nacre terminal default, JetBrainsMono Nerd Font. Desktop presets preserve it.

Kitty reads a generated `nacre/kitty-font.conf` include and reloads its configuration
when the choice changes. Private Kitty `custom.conf` remains the final override.
Only this user's terminals with the same HOME are notified; nested fixture homes
cannot reload the owner's terminals. See [Kitty configuration](https://sw.kovidgoyal.net/kitty/conf/).
