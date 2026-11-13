# Switching to Nacre

For people coming from Omarchy, Noctalia, caelestia or a hand-built Hyprland or
niri setup. You already know a tiling compositor; this page is about what is
different and what carries over.

## In one paragraph

Nacre is a desktop, not a dotfiles set. The bar, launcher, panels, settings and
notifications are one native program (the Bead shell, Rust + Luau), fed by a
daemon that owns network, audio, power and settings state. Colors come from
your wallpaper through Orient. The compositor underneath is nacre-niri by
default, with stock niri and Hyprland as supported tiers that hide a few
features.

## Compositor

| You run | Pick at setup | What changes |
| --- | --- | --- |
| Hyprland (Omarchy, caelestia) | **Hyprland** tier, or switch to **nacre-niri** | Hyprland keeps your window model. Workspace collections, edge cues and native panel motion need the fork and are hidden. |
| niri (Noctalia on niri) | **nacre-niri** | Same niri, plus collections, edge triggers, hover overview, scratchpad and pinning. Your niri config keys still work. |
| Stock niri, and you want to keep it | **niri** tier | Core shell. Fork-only features are hidden, not broken. |

The full list is the [compatibility table](compatibility.md). You can prepare
more than one session (`nacre setup --also-compositor hyprland`) and switch at
the login screen.

## Your config files

Nacre generates the compositor config and leaves you one file that always wins:

- niri and nacre-niri: `~/.config/nacre/niri/custom.kdl`, included last. Put
  binds, output settings and window rules there. `niri validate --config
  ~/.config/nacre/niri/config.kdl` checks the whole thing.
- Hyprland: `~/.config/nacre/shortcuts.lua` for binds, `keyboard.lua` for the
  keyboard. Unbind a combination before you rebind it.

The niri tiers keep their generated config under `~/.config/nacre/niri`. The
Hyprland tier is different: its `hypr-config` module manages `~/.config/hypr`
and `~/.config/uwsm` (Nacre's Hyprland config is Lua, `~/.config/hypr/hyprland.lua`).
Back up your `hyprland.conf` before you choose that tier. Themed configs for
Kitty, fish, GTK, rofi and fastfetch are separate modules you can leave off:
`nacre module list` shows them, `nacre module disable theming-fish --plan` shows
what turning one off would do, and `nacre status --files` lists every file Nacre
owns.

## Keybindings

The defaults are close to what you know: Super+Space launcher, Super+Enter
terminal, Super+Q close, Super+1–9 workspaces, Super+Tab overview, Super+F
fullscreen. The differences people notice:

- Super+W is the wallpaper picker, not close.
- Super+V is clipboard history.
- Super+I is Settings, Super+K is the shortcut list.
- Super+Z hides the shell.

Cheat sheet: [keybindings.md](keybindings.md).

## Theming

If you used Matugen (caelestia, Noctalia) or Omarchy themes:

- Orient is the palette engine. `orient palette image.jpg` gives a Material-style
  role palette (`primary`, `onSurface`, `surfaceContainer`, …, `term0–15`) as JSON.
- Orient templates are `.in` files with `#{{role}}` tokens and a subset of the
  Matugen expressions (loops, filters, conditions). Your existing Matugen
  templates are a short edit away; scripts and includes are not executed.
- Nacre applies the palette to its own UI and to GTK, Qt, Kitty, Chromium,
  Discord and Spotify through Settings → Colors → App theming. For anything else,
  `orient export palette.json ./themes` writes the bundled templates (Waybar,
  mako, fuzzel, foot, Ghostty, Helix, Zed, Starship, tmux, fish, …) and you
  include what you want.
- There is no theme pack to choose from. The wallpaper is the theme. Personalities
  (Natural, Pop, Mist, Vivid, …) are the knob.

More in [Theming from a wallpaper](theming.md).

## The bar and widgets

- The shell is not Quickshell, AGS, Waybar or eww. Quickshell QML and Waybar
  modules do not load.
- Widgets are Luau plugins with a `plugin.toml`; Bead's
  [hello plugin](https://github.com/nacre-desktop/bead/tree/main/examples/plugins/hello)
  is the starting point. Add one from the launcher with `>plugin <folder>`.
- Layout of the control center and dashboard is edited in place (Edit button,
  Ctrl+, in the dashboard). No CSS.

## Running Nacre next to your old setup

Setup prepares a new login session; it does not remove your current one. Your
old session stays selectable at the login screen. `nacre uninstall --plan`
shows exactly what would go if you leave.

## What Nacre does not do yet

- No installer ISO; Arch and CachyOS only, through the signed repository.
- Monitor mirroring is unavailable on niri.
- Stock niri and Hyprland: collections, edge cues and native panel motion are
  hidden; see the [compatibility table](compatibility.md).
- The login screen is an SDDM theme; Clasp (a greetd greeter on Bead) is in
  development.
