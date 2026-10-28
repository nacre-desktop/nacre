# N0090 — Nacre Widgets app entry

Add the `Nacre Widgets` desktop entry and matching Nacre-style SVG app icon for
Bead's existing `nacre-shell --app widgets` app. Install both through the
existing configure mapping. Add Super+Shift+W to the neutral compositor
contract and show it as “Widgets” in the generated shortcut list; Super+W
remains the wallpaper picker. No temporary duplicate paths were added.

Validation: targeted desktop-entry, Niri binding and shortcut-label tests;
full Arch `desktop` CI and `niri validate` in a private temporary HOME.
