# Configuration and personal profiles

Nacre's default profile has seven numbered workspaces, no startup apps and no
application-to-workspace assignments. A profile can supply workspace labels/icons,
startup command argument arrays, window routes and optional app workspace choices.
See `profiles/default.json` for the format. Profiles have `version:1` and are
validated before installation. Supply one with `./install.sh --profile PATH`.

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
