# N0223 Scroll bars and smooth scrolling adoption

Adopt Bead B0240 (`5c0e3136629ccea18b903c2f4ccd41dd55efb80e`) across Nacre. Scrollbars use `onSurfaceVariant` at rest and `primary` on hover, preserving palette adaptation in both light and dark themes. Overflowing app, launcher results, settings, dashboard, AI, and control-center regions retain the default overlay bars.

The wallpaper carousel and spotlight strips opt out because they scroll horizontally as image rails; their overlay thumb would obscure cards. The control-center outer page scroll opts out because its hit area overlaps edge-aligned tile controls; its notification virtual list retains the default scrollbar. Launcher Up/Down remains result navigation. Page Up/Down and Home/End route to the focused or pointer viewport in settings, dashboard, AI chat, and control center. Bead’s smooth wheel animation is frame-callback paced. Idle goldens did not change because bars fade out; the whole shell pixel suite passed.

Validation on dev-1 (`n0200-build:latest`): Bead lock has one revision; fmt, clippy, and Luau analyzer 0.742 passed. Whole nacre-shell binary: 882 passed, 0 failed, 4 ignored; both control-center interaction retries passed. Whole nacre-helper library: 1,023 passed, 0 failed, 3 ignored. Wallpaper `lab.calls.recv() -> None` failures: 0. Coordinator to verify live 120 Hz feel after deploy.
