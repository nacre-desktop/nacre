# N0213 — Appearance, panels and startup settings

Make the registry's hidden appearance, panel and startup controls functional through
the existing settings writer. Power settings are outside this unit.

- Automatic palette mode uses sunrise/sunset from the configured weather location,
  falling back to 07:00–19:00. The daemon switches the existing light/dark mode;
  a manual change lasts until the next schedule boundary. No separate service.
- Edge handle length was `desktop.lipSize` (24–96 px); N0239 (spec 0237) replaced it with the
  per-edge handle rows, and a saved `lipSize` is ignored.
  Top-bar `desktop.modules` selects individual default widget instances; empty means
  the defaults. Both update live.
- One list editor supports module choices and installed startup apps, including
  removal and startup launch ordering. Module selection preserves saved geometry.
  Startup changes take effect at the next sign-in.
  `menu.query` adds optional `startup: true` for bounded trusted launch records;
  normal launcher responses stay unchanged. Terminal apps use the existing Kitty
  convention; other desktop entries with a working directory use `gio launch`.
- `wallpaper-picker.lockWallpaper` selects a lock and login image; empty follows
  the desktop. Nacre resolves scenes/videos to a static poster before passing the
  image to Hyprlock or the login export. The resolver is reusable by Clasp.
- Pointer enlargement remains planned until Platform NN0033 lands. Current
  nacre-niri main `a8562a11` has no cursor-shake implementation. Its future row is
  gated by `cursorShake`; generated KDL includes `cursor { shake-to-enlarge }` only
  when both setting and capability are on. The capability bridge and generator test land here.

No framework primitive, systemd service or temporary duplicate is introduced. Settings
editors reuse the existing form kit. New daemon state is bounded; memory budget is
at most 1 MiB always-on (not measured in this unit).

Lock stills that Hyprlock reads directly (png, jpg, webp) are passed as is; other formats
are re-encoded once into `~/.cache/nacre/wallpaper-media/lock-<hash>.png`.

Validation: focused dev-1 container checks (fmt, clippy, protocol/menu/helper/shell
tests, Luau analyzer) with goldens reviewed by eye; see the README row. No live session
was changed.
