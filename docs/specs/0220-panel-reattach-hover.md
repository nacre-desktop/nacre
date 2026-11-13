# N0220: Keep a hover-open panel through reattachment

When the floating chat panel is docked under a stationary pointer, the old
floating surface can send leave before the animated docked layer reaches the
pointer and sends enter. Keep the hover-close timer open for 450 ms after
reattachment, covering the panel's 360 ms reveal; cancel that grace on pointer
enter. If no enter arrives, close normally after the grace. Later pointer leaves
retain the existing 130 ms delay. No compositor changes or temporary duplicate
paths are introduced.

Both reattach regression tests pass. Focused fmt, clippy, Ruff, Luau analyzer,
generated SDK and wallpaper renderer checks pass. The whole nacre-shell binary
had five unrelated `extras::tests::wallpaper_*` failures (875 passed); the
wallpaper catalogue failure also reproduces in isolation. A nested
`WAYLAND_DEBUG` capture was not run, so compositor enter behavior remains
unverified by this unit.
