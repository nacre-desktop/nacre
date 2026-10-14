# Configuration and profiles

Use Settings (Super+I) for everyday appearance, device and desktop preferences.
Profiles choose installation components, workspace labels/icons, startup commands,
window routes and optional app workspaces. They do not contain account data,
wallpapers, conversations or notes.

## Profiles

The [default profile](../profiles/default.json) has `version: 1`, five workspace
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
Niri workspace numbers are dynamic on each output; its preparation path converts
profile routes to named workspaces.

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
