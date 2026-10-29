# N0099 Nacre Widgets app icon

Status: implemented (branch task/claude-n0099-widgets-icon-20261029), lead gate pending.
Pairs with Bead B0198 (widgets drag redesign), which uses the same mark in the app header.

## Goal
The owner asked for Nacre Widgets to have its own logo "like the other apps": the widgets
glyph inside the Nacre shell, in the style of Settings, Colors, Notifications and Brain.

## Design
- `nacre/shell/branding/nacre-widgets.svg` is the Nacre shell (`nacre.svg`, unchanged geometry)
  plus an app glyph at the shared app-glyph centre (49, 44.5) beneath the shell lip: three
  rounded tiles and one rotated tile (the Material "widgets" mark), primary tiles with a
  secondary diamond, the same roles and stroke weights as the Settings glyph.
- The file is installed as-is to `~/.local/share/icons/hicolor/scalable/apps/nacre-widgets.svg`
  by `tools/configure.py` (unchanged mapping), so it is pre-painted with the default pearl
  roles (primary #9AA9FF, secondary #D8BFFF, tertiary #73D5E7, highlight #F6F1FF) and has no
  `@ROLE@` placeholders. The desktop entry keeps `Icon=nacre-widgets`.
- Not done on purpose: adding `widgets` to `branding::APPLICATIONS` (palette-painted per theme)
  would also regenerate LogoData.js, the login copy and the gzipped palette-pipeline
  reference fixture. Follow-up if the owner wants the icon to follow the palette.

## Checks
- `tools/tests/test_configure.py`: the installed icon is byte-identical to the source, carries
  `data-logo="nacre-logo"` and `aria-label="Widgets"`, and has no placeholders.
- Visual: rendered next to the painted Settings icon (B0198 evidence `icon-widgets.png`,
  `icon-settings.png`).
