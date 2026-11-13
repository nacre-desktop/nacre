# Widgets and Settings

## Settings

Super+I opens Settings. `nacre-settings` opens it from a terminal;
`nacre-settings panels/dashboard#pages` jumps to one preference.

<!-- media: media/settings.png — Super+I, type "border", Enter highlights Window border width, change it, the window border updates behind -->

- Groups: **Look & feel**, **Desktop**, **Devices**, **System**, **Add-ons**.
  Wallpaper, Colors and Appearance have their own pages.
- Type to search. Results show the current value; Enter opens and highlights the
  row. Escape clears the search, then closes Settings.
- Changes apply as you make them. Display changes ask you to keep or revert.
- The overflow menu has **Reset to defaults**. A small "Set by profile" badge
  marks values that come from the selected desktop preset.
- Pages for hardware you do not have, and compositor options your compositor
  does not support, are hidden.

Pages worth knowing early:

| Page | What is there |
| --- | --- |
| Appearance | fonts, interface gradients |
| Windows & workspaces | window border width, niri monitor workspace policy |
| Panels | every edge; Dashboard pages and Control center layout |
| Launcher | sidebar categories, web search engine, sources |
| Power & battery | automatic AC/battery profiles |
| Displays | scale, mode, position, with a revert timeout |
| Colors | palette mode, Colors studio, App theming targets |
| Software | Apps, Themes, Extensions |

Preferences live in `~/.config/nacre` (`desktop.json`, `launcher.json`,
`launcher-sources.json`, `apps.json`, `widgets.json`). Do not hand-edit them
while the shell runs; see [Configuration](configuration.md) and the
[safe reset steps](recovery.md#how-do-i-reset-settings-safely).

## Control center layout

Open the control center and press **Edit** (top row). You can add, remove and
reorder every tile and slider, including the Light / Dark / Auto selector, and
there is a Reset. Notification history can be added as a tile. The layout is
saved separately from the main settings and keeps its order across sessions.

## Dashboard pages

Default tabs: Dashboard, Media, Weather, System. Press Ctrl+, in the dashboard,
or open Settings → Panels → Dashboard → Pages, to enable Focus, Notifications,
Clipboard, Devices, Network, AI usage, Screenshots or Brain. Number keys 1–9 and
0 select tabs; Ctrl+Tab cycles.

## Widgets

Super+Shift+W opens **Nacre Widgets**. Widgets are small Luau plugins that render
inside the shell. Nacre ships a few; you can add your own.

<!-- media: media/widgets.gif — Super+Shift+W, drag a widget to a new span, open a plugin's settings and change a value -->

- Bundled widgets are on by default. Third-party ones start disabled.
- Add one from the launcher: type `>plugin` followed by a folder path or a git
  URL. The launcher shows the plugin's name and what it asks to read
  ("none beyond drawing" for a widget that only draws).
- Plugins live in `~/.local/share/nacre/plugins/<id>/plugin.toml`. Enable state
  is in `~/.config/nacre/plugins.json`.
- A plugin can only read the stores and send the actions its manifest declares.
  Luau sandboxing is a stability boundary, not a security boundary: enable only
  plugins you trust.
- A plugin that fails to load is reported and isolated; the rest of the shell
  keeps running.

To write one, start from Bead's
[hello plugin](https://github.com/nacre-desktop/bead/tree/main/examples/plugins/hello)
(a `plugin.toml` and one Luau file) and the
[plugin spec](https://github.com/nacre-desktop/bead/blob/main/docs/specs/0203-plugins-v1.md).

## Extensions

Settings → Software → **Extensions** manages launcher packs, palette themes and
independent shell panels. Imported extensions start disabled; updates leave the
new version disabled until you review it. Panels run as separate processes
with your normal user permissions. Format:
[software extensions](development/software-extensions.md).
