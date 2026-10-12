# Configuration and personal profiles

Nacre's default profile has five workspaces shown as pearls, no startup apps and no
application-to-workspace assignments. A profile can supply workspace labels/icons,
startup command argument arrays, window routes and optional app workspace choices.
See `profiles/default.json` for the format. Profiles have `version:1` and are
validated before installation. Supply one with `./install.sh --profile PATH`.

Pearls run left to right from workspace 1, connected by a fine palette-colored
thread. Empty workspaces are hollow rings; occupied ones have filled pearls.
Small window-category symbols stay visible beside every occupied workspace, including those
you are not currently using. The glossy pearl and its highlight slide to the active
group. Click any icon to focus its window. When space is tight, groups show an
overflow count while keeping the other bar controls clear. Hover for workspace numbers,
window counts and titles, or scroll over the necklace to switch workspaces.
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
