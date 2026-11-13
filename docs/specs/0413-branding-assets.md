# N0413 — Branding assets (logo "current refined")

Status: done 2026-11-13. The owner picked "current refined" (r2, Platform lead's logo set of 2026-11-13). This unit lands it as repository branding only; nothing is installed or rendered by the product.

| File (`assets/branding/`) | Size | Notes |
| --- | --- | --- |
| `nacre-mark.svg` | 5,850 B | Colour mark, 64×64 viewBox |
| `nacre-mark-mono.svg` | 4,503 B | One colour, `currentColor` |
| `nacre-stacked.svg`, `nacre-horizontal.svg` | 8,562 B, 8,615 B | Mark + wordmark; wordmark ink is `currentColor` (black in an `<img>`) |
| `nacre-horizontal-dark.svg` | 8,610 B | Same, wordmark `#f3eff5` (gallery dark ink); README `<picture>` source for GitHub's dark theme |
| `nacre-avatar-{dark,light,transparent}.png` | 84,797 / 77,930 / 67,600 B | 1024×1024 as before; mark at 736 px centred on gallery `#211d27→#17141b` / `#fff→#f6f3f7` radial backgrounds, or transparent |
| `nacre-social-preview.png` | 56,353 B | 1280×640; dark horizontal logo at 720 px on the dark background |
| `Manrope-OFL.txt` | 4,387 B | The wordmark glyphs are Manrope 700 converted to paths (SIL OFL 1.1) |

The README header shows the horizontal logo in a `<picture>` (dark source + light fallback). PNGs were rendered with `rsvg-convert` and composited with ImageMagick 7 (`-strip`, zlib level 9); no oxipng/pngquant on the host. Orient (`assets/branding/orient-horizontal{,-dark}.svg`, replacing its copy of the old Nacre avatar) and nacre-niri (`docs/nacre/images/nacre-niri-horizontal{,-dark}.svg`, OFL in `docs/nacre/licenses/`) got the family marks the same way.

No packaging paths changed: the Arch package ships no static app icon. The hicolor app icons (`nacre-ai`, `nacre-settings`, …) are generated per palette by `crates/nacre-helper/src/branding.rs` from `nacre/shell-tools/branding/nacre-master.svg`, so they move with N0315 (in-product branding: `branding.rs`, login logo data, shell logo widget, terminal/fastfetch braille). The Pages index (`packaging/arch/repository.py`) is not touched here. The GitHub social preview is a web-UI setting for the owner. Orient's `orient-social-preview.png` still shows the old style. No temporary duplicates.
