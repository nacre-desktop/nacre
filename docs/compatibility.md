# Compositor compatibility

Nacre has three compositor tiers. Tier 1, **nacre-niri**, has everything. Tier
2, **stock niri** and **Hyprland**, run the core shell and hide fork-only
features. Some shared features use a fallback or have reduced behavior.

| Feature | nacre-niri | Stock niri | Hyprland |
| --- | --- | --- | --- |
| **Desktop basics** | | | |
| Session, lock and idle | Partly: idle lock is opt-in (see below) | Partly: idle lock is opt-in (see below) | Yes |
| Wallpaper and backdrop | Yes | Yes | Yes |
| Wallpaper picker (Strand) | Yes: windows fade out while you browse | Partly: switches the focused screen to an empty workspace instead (not yet checked in a session) | Partly: switches to an empty workspace if one exists, else the windows stay (not yet checked in a session) |
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
| Panel drag setting | Yes | Yes | Yes |
| Edge cues | Yes | No (hidden) | No (hidden) |
| **Overview** | | | |
| Window and workspace focus | Yes | Yes (not yet checked in a session) | Yes (not yet checked in a session) |
| Live previews | Partly: capture needs backend support | Partly: icons replace live previews | Partly: icons replace live previews |
| Hover title and highlight | Yes | Partly: native hover stream unavailable | Partly: hover behavior unconfirmed here |
| **Settings** | | | |
| Compositor-specific options | Yes | Partly: unsupported choices are hidden | Partly: some options may not apply |

On nacre-niri and stock niri, Nacre always locks before suspend. Idle lock stays
off until you confirm that password unlock works: lock with Super+L, unlock with
your password, then create `~/.config/nacre/niri-on-pc/acceptance.json`
containing `{"phase":"nacre","backendReady":true,"unlockVerified":true}`. It
takes effect at your next login, or at once after
`systemctl --user restart nacre-niri-lock.service`.

The fresh-Arch VM gate checks install, session startup, rendered output,
`nacre update` and purge for each tier, plus an upgrade from an earlier package
set. It last passed every stage on all three tiers on 2026-11-24 (main
49601c1a1, nacre-niri r3130); the upgrade stage last ran on 2026-11-16 (main
7e12fa0e9). On CachyOS (generic, x86-64-v3 and x86-64-v4 repositories), a
container check of install, setup and purge passed on all three tiers on
2026-11-16; there, most dependencies, including niri and Hyprland, come from CachyOS's
rebuilt packages.
