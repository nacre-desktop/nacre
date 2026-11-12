# Compositor compatibility

Nacre has three compositor tiers. Tier 1, **nacre-niri**, has everything. Tier
2, **stock niri** and **Hyprland**, run the core shell and hide fork-only
features. Some shared features use a fallback or have reduced behavior.

| Feature | nacre-niri | Stock niri | Hyprland |
| --- | --- | --- | --- |
| **Desktop basics** | | | |
| Session, lock and idle | Yes | Yes | Yes |
| Wallpaper and backdrop | Yes | Yes | Yes |
| Notifications and on-screen feedback | Yes | Yes | Yes |
| Screenshots | Yes | Yes | Yes |
| Shortcut hints | Yes | Partly: hints are generated locally | Yes |
| **Workspaces and windows** | | | |
| Workspace switching | Yes | Yes | Yes |
| Window hints and taskbar glyphs | Yes | Partly: native hints are missing | Partly: some glyph hints missing |
| Collections and stacks | Yes | No (hidden) | No (hidden) |
| Launcher workspace placement | Yes | Partly: native placement is missing | Partly: native placement is missing |
| **Panels** | | | |
| Open and close motion | Yes | Partly: uses client-owned motion fallback | Partly: uses client-owned motion fallback |
| Detach panels | Yes | Partly: uses prepared-window fallback path | Partly: native detach needs confirmation |
| Panel drag setting | Partly: setting hidden; cue still | No (hidden) | Yes |
| Edge cues | Yes | No (hidden) | No (hidden) |
| **Overview** | | | |
| Window and workspace focus | Yes | Partly: focus action runtime unconfirmed | Partly: focus action runtime unconfirmed |
| Live previews | Partly: capture needs backend support | Partly: icons replace live previews | Yes |
| Hover title and highlight | Yes | Partly: native hover stream unavailable | Partly: hover behavior unconfirmed here |
| **Settings** | | | |
| Compositor-specific options | Yes | Partly: unsupported choices are hidden | Partly: controls lack capability gates |

The fresh-Arch VM gate checks install, session startup, rendered output and
purge for each tier. It last ran on 2026-11-12. The recorded results confirm
install, session and render for all three; the all-tier purge result needs a
rerun after the state fix.
