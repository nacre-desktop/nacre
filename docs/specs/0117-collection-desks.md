# N0117: Nacre collection desks

The Nacre switcher is hosted in the Bead bar and consumes the optional
`workspaceCollections` compositor projection. On stock niri, the projection is
empty and the switcher is hidden. Collection actions are advertised by Nacre's
daemon only when the running compositor reports the native collection feature.

Wallpaper profiles are stored in
`$XDG_CONFIG_HOME/nacre/collection-wallpapers.json`, keyed by the compositor's
collection name. The setting `desktop.collectionWallpapers` defaults to false.
When enabled, Nacre's wallpaper provider selects a saved profile only for the
output currently showing that named collection. `wallpaper.active.current`
remains the global base wallpaper; `outputs` contains the per-output pair, so
base wallpaper controls and saved desk wallpapers keep separate meanings.

The switcher can save the output's current wallpaper for a named desk, use that
saved wallpaper's primary colour as its optional accent, or clear the desk accent.
The accent is applied only while that named desk is on the focused output. A desk
without an accent uses the current base palette. The wallpaper provider checks
the saved file identities on every refresh; a missing or replaced image falls
back to the base wallpaper until the profile is saved again. The JSON profile's
`accent` may be set to `null` to omit it.

The switcher changes workspace collections only. It never launches applications.
