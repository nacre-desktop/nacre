# Theme bundles

Nacre ships two reviewed, offline pilots: Catppuccin Mocha/Latte and Rosé Pine
Main/Dawn. Software → Themes and Settings → Colors show both complete palette
previews and both original Orbits wallpapers before Apply. Choose the dark or
light mode and **palette only**, **wallpaper only**, or **both**. Closing the
preview changes nothing. Ordinary launcher search finds either bundle.

These are credited Orient adaptations, not pixel-identical upstream ports.
Upstream surfaces and semantic colors seed a complete Orient role map. Orient
repairs foregrounds against every associated background, first repairing accents,
then text on those accents. Every normal text and selected-action pair passes
Orient's 4.5:1 contrast policy. Disabled controls retain their normal disabled
opacity and are distinguishable by state; disabled text is not claimed to meet
normal-text contrast at that opacity. Qt's active, inactive and disabled exports,
GTK, KDE, all sixteen terminal colors and selected-action text are covered in
native fixtures. These checks do not claim identical results in every toolkit.

## Format and provenance

`nacre/shell-tools/theme-bundles/schema.json` describes catalog v1. Each bundle has
an immutable upstream revision, source colors, complete dark/light role maps,
mode names, upstream MIT credit and license text. Each wallpaper records its
license, original SVG source, dimensions, and separate SHA-256 hashes for the
source and rendered PNG. Runtime browsing and Apply verify hashes and complete
Orient roles; the catalog does not fetch or execute anything.

Palette sources:

- [Catppuccin palette](https://github.com/catppuccin/palette/tree/07d02aa110ef9eb7e7427afca5c73ba9cf7f8ebd), copyright Catppuccin Org, MIT.
- [Rosé Pine palette](https://github.com/rose-pine/rose-pine-palette/tree/92af52b465ab6e47437aca223c9b8d3009a2023b), copyright Rosé Pine, MIT.

The Orbits wallpapers are original geometric SVG compositions generated from
those palette colors, dedicated to CC0. No community gallery images were used.
Their sources and per-asset hashes ship beside the PNG files. Render an SVG with
`rsvg-convert INPUT.svg -o OUTPUT.png`; update its hash if the rendered bytes change.
Use `cargo run -j2 -p nacre-helper --example generate-theme-bundles` to regenerate
complete role maps from the recorded source colors. The native test verifies that
these mappings are reproducible with the pinned Orient library.

## Apply transaction

The native publisher serializes Apply with its existing palette publication lock.
It validates the bundle and artwork before writing. It retains exact previous
preference, scheme, presentation, wallpaper links and declared application-export
bytes for rollback. A failed publisher restores them, attempts to restore live
consumers, then restores the exact stored bytes again. Failure reports any live
recovery error; disconnected consumers cannot be guaranteed to redraw immediately.
Palette-only preserves the wallpaper. Wallpaper-only preserves palette preferences
and the existing palette. Collections and saved Colors favorites are retained.
The registered `wallpaper-picker.palettePreset`, `paletteMode` and
`paletteAutoMode` remain the selection authority; there is no parallel settings
file. Explicit palette application disables automatic mode selection, as other
explicit Colors choices do.

Tests and galleries run with private HOME/XDG state and synthetic devices. Actual
hardware theme application requires a separately approved live check.
