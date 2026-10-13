# Configuration and personal profiles

Nacre's default profile has five workspaces shown as pearls, no startup apps and no
application-to-workspace assignments. A profile can supply workspace labels/icons,
startup command argument arrays, window routes and optional app workspace choices.
See `profiles/default.json` for the format. Profiles have `version:1` and are
validated before installation. Supply one with `./install.sh --profile PATH`.

The general profile starts no browser, editor, chat or music apps and assigns no
apps to workspaces. Its empty startup helper leaves your current workspace alone.
AI windows also open on the current workspace unless `aiWorkspace` is explicitly
set. To replace an older customized profile with these defaults, use
`./install.sh --profile default`. An update without `--profile` preserves an
existing profile; private host overrides remain yours to manage.

Pearls run left to right from workspace 1, connected by a fine palette-colored
thread, flowing from primary through secondary to tertiary. Empty workspaces are
hollow rings; occupied ones have filled pearls.
Numbers inside the pearls match the workspace shortcuts, with softer ink on pale
pearls and stronger contrast where needed. Only the active workspace
opens into a capsule with up to three quiet window-category symbols; extra windows
show as a count. Click a symbol to focus its window. The glossy pearl and its
highlight slide to the active group. When space is tight, fewer symbols appear to
keep the other bar controls clear. Hover for window counts and titles, or scroll
over the necklace to switch workspaces.
Attention requests get a tertiary-colored ring; its pulse respects reduced motion.
Extra occupied or active workspaces remain visible, so lowering the configured
count never hides your open windows. Custom workspace icons are still supported.

For a quieter pearls-only bar, turn off **Show window symbols** in
**Settings → Appearance → Workspace bar**. Hover still shows window counts and titles.

Settings stores your desktop preferences under `~/.config/nacre`. Generated palettes,
wallpaper metadata and caches stay local. Private monitor, palette, desktop,
shortcuts, profile and host overrides load after the managed base; host overrides
have final precedence. Existing local config edits are preserved/refused by managed
updates. Edit everyday appearance/device preferences through Settings.

Your favorite apps, wallpaper collection, accounts, conversations and Brain vault
are user data, outside the source repository. Profile selection never distributes
or recreates those data. Use a separate test user/accelerated VM to try general
first-install defaults. Do not run two full Nacre shells simultaneously in one
session: notifications and palette publication have one owner.

The base keyboard layout is US. First installation reads the system X11 layout
with `localectl`, then preserves your `~/.config/nacre/keyboard.lua` override on
updates. Desktop/host overrides load afterwards.

## Personal shortcuts

The [README shortcut table](../README.md#shortcuts) lists the general defaults.
Five pearls are shown initially, but Super+1–9 and Super+0 can reach workspaces
1–10; Shift moves a window to the same target. Super+Q holds a window for 30 seconds
before closing it; Super+Shift+Q restores it during that interval.

In the launcher, **Setup → Default applications** edits `~/.config/nacre/apps.json`.
App choices are argument arrays, for example:

```json
{
  "terminal": ["alacritty"],
  "files": ["thunar"],
  "browser": ["firefox"]
}
```

Super+Enter, Super+E and Super+Shift+B use those choices. Missing entries retain
the shipped Kitty, Nacre Dolphin and Chrome defaults. Only applications you have
installed can be launched.

Use **Setup → Edit personal shortcuts** to edit `~/.config/nacre/shortcuts.lua`.
Named editor, chat and music shortcuts belong here. A profile JSON controls
startup and routing; it does not contain shortcut bindings. For example, to keep
Super+A as your personal launcher, replace the base AI binding explicitly:

```lua
hl.unbind("SUPER + A")
hl.bind("SUPER + A", hl.dsp.exec_cmd("~/.local/bin/nacre-shell launcher"),
    {description="Nacre:launcher"})
hl.bind("SUPER + CTRL + B", hl.dsp.exec_cmd("~/.local/bin/nacre-shell left"),
    {description="Nacre:ai-sidebar"})
hl.bind("SUPER + SHIFT + T", hl.dsp.exec_cmd("uwsm app -- ~/.local/bin/nacre-app terminal"),
    {description="Nacre:terminal"})
```

Unbind an occupied combination before assigning another action. For example,
Super+Shift+S now captures an area; a personal music binding on that key must
explicitly replace it. Reload Hyprland with Super+Shift+R after editing. Managed
updates preserve this private file; the live shortcut list reads session bindings.

Super+S toggles the scratchpad; Super+Ctrl+S moves a window there.
Super+Ctrl+PageUp/PageDown cycles grouped windows. Super+K and Super+F1 open the
shortcut list. Super+. opens emoji, with Super+Ctrl+E as an alternative for layouts
where . requires Shift or AltGr. These defaults avoid layout-dependent brackets,
grave and slash shortcuts.

Hyprland 0.56.x has an [upstream Lua keycode matching bug](https://github.com/hyprwm/Hyprland/discussions/14819),
so the general defaults avoid physical `code:NN` bindings. Letter shortcuts remain
layout symbols; custom keyboards can override any binding.
