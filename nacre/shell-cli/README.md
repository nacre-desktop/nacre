# Nacre desktop palette adapter

This package retains the `nacre_shell` command/library contract for desktop
publication. It depends on the separately versioned `orient-colors` package.
The `src/orient` link points to the pinned engine dependency.

The standalone `orient` command is provided by that dependency and is read-only
with respect to the desktop. Nacre's adapter/bridge/publisher remain the single
owner of live wallpaper/palette publication. See the upstream README,
[Orient](https://github.com/nacre-desktop/orient).
