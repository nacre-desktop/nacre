# Changelog

## Unreleased

Use the pinned standalone Rust Orient binary for all palette calculations and
template rendering. Provision an immutable native runtime; retain Pillow for
desktop image previews. The gallery and current wallpaper colors are unchanged.

General shortcuts use Super+Space for the launcher, Super+Enter for the selected
terminal, Super+E for Files, Super+I for Settings, Super+Tab for overview and
Super+L to lock. Super+A opens optional AI; the palette moves to Super+Ctrl+Space.
Super+Escape and Super+X open power controls, Super+Ctrl+Left/Right switches
workspaces, and Super+Shift+S also captures an area. Numbered workspace shortcuts
cover 1–10 (0 selects 10). Scratchpad, group cycling and help use accessible
letter/navigation/function keys; emoji also has Super+Ctrl+E for layouts with
shifted punctuation. Browser uses the default-app route; personal editor/chat/music
and startup/rest script bindings are removed from the general base. Local shortcut
overrides remain user-owned. README and the live shortcut guide follow the defaults.

Corner notifications stay visible while the top dashboard is open.

An empty startup profile performs no process launches or workspace changes. AI
launch actions use the current workspace unless a profile explicitly assigns one.

Workspace pearls flow from primary through secondary to tertiary, with softened
number ink. They show their numbers and window occupancy. Only the active pearl
expands with up to three quiet window symbols and an overflow count. The
pearls-only preference keeps numbers, hover details and workspace switching.

Edge panels can detach into ordinary Nacre windows and dock again. Launcher and
dashboard navigation is independent; data and samplers stay shared. Optional AI
conversations move between views with draft and ownership recovery. Pearl grips,
pop-out controls and Super+Ctrl+D support the feature; Settings can disable it.

## 0.1.0-dev.2

Use the system keyboard layout at first install, with US as the fallback. Smaller
4K demo wallpapers, neutral example data, clearer install/contributor guides and
public screenshots improve the development preview. The desktop pins Pillow while
Orient supports Pillow 11–12 for library users.

Nacre system notifications use a palette-aware shell-and-bell mark. Third-party
apps retain their own icons, and the mark is reusable in future panel headers.

Use MIT-licensed Orient 0.1.0-dev.4. Nacre remains GPL-3.0-only; palette and template
behavior are unchanged.

## 0.1.0-dev.1

Initial Nacre development preview: wallpaper-shaped colors, a universal launcher,
Settings, Colors and Welcome, optional AI/Brain, numbered workspaces and configurable
personal profiles. Supports static/animated wallpapers, dark/light palettes and
managed updates with rollback. Uses the independently versioned Orient engine.
