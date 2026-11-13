# N0315: In-product logo swap to the refined mark

## Outcome

The owner's pick (2026-11-13) is the current Nacre silhouette rebuilt as clean
Bézier geometry: 8 chamber paths, a colour ramp from tertiary through secondary
to primary, and a pearl seated in the palette. `nacre/shell-tools/branding/nacre-master.svg`
is now that mark: 10 KB instead of 54 KB. The r2 source is 5.8 KB with baked
colours; the palette version draws six chambers and the cap twice (a base role
plus an overlay role at the ramp's opacity) because SVG has no fill that mixes
two roles.

Every in-product consumer follows from the master with no code change to the
template extraction:

- the helper's `full`/`compact`/`symbolic` templates and their five app
  variants, which give the published `nacre*.svg`/`nacre*.png`, the hicolor
  icons, the lock PNG and the kitty terminal PNG;
- the login `nacre/login/LogoData.js` and `Logo.qml`, and the Brain web symbol
  in `brain/web/index.html` (regenerated with `tools/generate_branding.py --build`);
- the native shell's themed copies: the compact ones in `shell/bar`, `shell/ui`
  (`shell/dashboard/today` links to it), `shell/chat` and
  `shell/notifications/fallback.themed.svg`, and the full ones in
  `shell/settings`, `shell/welcome`, `shell/colors` and `shell/widgets/app`;
- the terminal text logo: the 36x16 braille raster of the mark, with each row
  coloured from the live palette on truecolour terminals.

README, Pages, package icons and `assets/branding/*.png` are untouched; the
Release lead lands those.

## Master and recolouring contract

The master keeps the old coordinate space (crop `2.8 3 58 58`, pearl at
26.19 38.03 r 3.14), so the app labels (`*-label.svg.in`), the chamber/pearl
extraction in `branding.rs::templates` and the shell copies needed no
coordinate changes. The r2 geometry (built in a 0..64 box that is this crop)
was mapped back with `x' = 58/64·x + 2.8`, `y' = 58/64·y + 3`. Painted with
the pearl palette it matches `nacre-current-mark.svg` at 512 px with no
interior pixel more than 12/255 off; only antialiased edge pixels differ.

The palette roles map onto the refined mark's colours like this:

| Refined element | r2 colour (pearl palette) | Master |
|---|---|---|
| chambers c0..c3 (`t` 0, .12, .24, .36) | tertiary → secondary mix | `@TERTIARY@` base + `@SECONDARY@` overlay at opacity 0, .24, .48, .72 |
| chambers c4..c6 (`t` .52, .66, .8) | secondary → primary mix | `@SECONDARY@` base + `@PRIMARY@` overlay at opacity .04, .32, .6 |
| cap | primary → secondary gradient + white sheen | `@PRIMARY@` base + `@SECONDARY@` gradient (opacity 0 → 1) + fixed white sheen |
| pearl | highlight → secondary → deep | radial `@HIGHLIGHT@` → `@SECONDARY@` plus a 40 % black rim shade (secondary at 60 % is the r2 deep `#6b5f8a` within 4/255) |

The roles themselves are unchanged (`branding::role_colors`: contrast-lifted
primary, secondary and tertiary from the wallpaper palette, `primaryFixed` as
the highlight on dark surfaces), so the mark follows the wallpaper. The overlays
live in `<g class="nacre-ramp">`, so the eight direct-child chamber paths and
the `nacre-logo-pearl-body` circle are still what the compact (solid roles plus
a `@HIGHLIGHT@` dot) and symbolic (union plus dot) templates extract. The native
shell copies use the same mapping with Bead's token names (`@tertiary+@`,
`@secondary+@`, `@primary+@`, `@primaryFixed@`).

## Terminal and fastfetch logo

`branding::braille` rasters the full template once with the roles painted as
greys (tertiary 0, secondary ½, primary 1; the pearl counts as secondary) at
the grid's square side and centres it. The overlays then mix to the mark's
in-between ramp steps, so one raster gives both the braille dots and each row's
position on the tertiary → secondary → primary ramp, as `build_r2.py` does.
`text_logo` writes plain lines (`nacre-text.txt`, 36x16) and the same lines with
a truecolour SGR per row mixed from the live roles (`nacre-text.ans`). Both are
written whenever the palette publishes, like every other branding asset, so the
colours always come from the current wallpaper palette and nothing is baked.
`nacre-ai-text.txt` stays a 12x3 grid for the AI dashboard header; its mark is
now square (6 columns of dots centred in the 12) instead of stretched 2:1.

`terminal-branding` keeps its tiers: kitty graphics placement of `nacre.png`
first; otherwise `nacre-text.ans` when `COLORTERM` is `truecolor`/`24bit` and
`NO_COLOR` is empty; otherwise `nacre-text.txt` (16-colour wrap in
`config/fastfetch/logo.sh`); otherwise the word `Nacre`. Theme-bundle
transactions save and restore `nacre-text.ans` with the other branding outputs.

## Fixtures and goldens

The frozen references describe the old artwork, so they are refrozen; inputs
and every non-branding output stay byte-identical.

- `test_nacre_branding.py`: the master's SHA-256 is the new master's.
- `terminal-branding-reference.json`: three render cases added (truecolour,
  `NO_COLOR`, no `COLORTERM`); the harness writes `nacre-text.ans` from `input.ans`.
- `palette-pipeline-reference.json.gz` and `palette-transactions-reference.json.gz`
  were replayed through `palette-publisher-parity` with the real `rsvg-convert`
  recording every request, then rewritten with the same pooled encoding and
  payload digest scheme (`sha256(json.dumps(payload, indent=2) + "\n")`).

The replay changed exactly the branding outputs of the 11 cases that publish
them (`nacre*.svg`/`.png`, `sh*.png`/`sh.svg`, the hicolor app icons,
`nacre-text.txt`, `nacre-ai-text.txt`, the branding PNG cache) and added
`nacre-text.ans`; every other output of all 16 cases is byte-identical. The
pipeline fixture shrank from 3.4 MB to 2.4 MB and the transactions fixture from
1.4 MB to 0.9 MB (smaller logo rasters). `LogoData.js` shrank from 438 KB to 97 KB.

Native shell (`cargo test -p nacre-shell`, rebased on `4080fabba`): seven
tests failed on the new logo alone. Re-rendering with `BEAD_UPDATE_GOLDENS=1`
moved 113 goldens; each one's diff regions were compared with the old golden:

- 103 are re-blessed because their only diff is the logo: bar/desk-chip/
  pill/status goldens (the bar logo), all settings, colors and welcome pages
  (the window logo, the welcome hero), launcher and pearl goldens, the
  notification cards and control-centre notifications (the fallback icon). Two
  of them, `notification-scroll-bottom` and `-reversed`, also pick up the
  overlay scroll bar that main already renders inside tolerance; their test
  fails on the logo without the re-bless.
- 10 are kept as they are: `dashboard-today-*` (4) also shift the wallpaper
  strip's arrow and image glyphs by about 3 px, and `chat-launcher-*` (6) change
  the antialiasing of one list dot. Neither comes from the logo, and both
  still pass within tolerance with the old logo.
- The 16 reveal references under `tests/fixtures/bead-layer-motion/reveal-goldens`
  were re-exported with `NACRE_REVEAL_REFERENCE_DIR` (they are tracked but
  match `.gitignore`'s `*.png`, so add them with `git add -f`); only the
  launcher atlases change visibly (the logo, 26x21 px), the rest re-encode.
- `widget_editor_tests::the_owner_layout_migrates_pixel_identically` pins a hash
  of the whole bar; the geometry assertions are unchanged and the hash moves
  with the bar logo (`11302824993991032218`).

With all of these, every nacre-shell test passes except the two plugin tests
that read `examples/plugins/hello` from a full Bead checkout, which the
prebuilt cargo cache does not carry (hosted CI has it).

## Evidence

- Render check: the master painted with the pearl palette against the owner's
  `nacre-current-mark.svg` at 512 px: no interior pixel differs by more than
  12/255; only antialiased edge pixels differ.
- Published in two palettes (pearl on a dark surface; blue/red/green on a
  light one): `nacre.svg` at 512 and 32 px follows each palette (chamber ramp,
  cap gradient, pearl). The 36x16 braille rows carry per-row colours mixed from
  each palette's roles: pearl from secondary `175;162;223` through
  `209;161;196` (cap rows) to `161;178;216` (lowest chambers); the second
  palette from `93;96;133` to `178;63;56` and `133;90;61`. Without
  `COLORTERM` and with `NO_COLOR` the same rows print uncoloured.
- In a non-kitty terminal fastfetch's kitty `raw` logo source does not exist,
  so it prints its built-in distro logo, as before this unit.
- Checks (fresh Arch container): helper build, `generate_branding.py --build`
  plus `qmlformat` clean, clippy `-p nacre-helper -D warnings`, `cargo test -p
  nacre-helper`, the branding/terminal/login/lock Python suites (30 tests),
  ruff on `tools/check.py`'s file set, `compositor_boundary.py`.

## Not done / follow-ups

- fastfetch inside tmux-in-kitty still reads the stale kitty `.raw` placement
  (pre-existing); writing the braille there instead is a small follow-up if the
  owner wants the braille shown in kitty too.
