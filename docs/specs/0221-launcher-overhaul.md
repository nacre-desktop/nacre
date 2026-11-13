# N0221 Launcher overhaul

This spec is extended by later launcher phases.

## N0221a owner fixes

- Add the missing English labels for Codex and Claude Code and render every Settings navigation page in the existing harness, failing on visible untranslated-key-shaped text.
- Read active Niri bindings through the generated `nacre-niri.kdl` overlay, its included `config.kdl`, generated binding includes and final user `custom.kdl`; verify that complete layout in a temporary HOME. Hyprland retains its existing provider, which reads custom and Nacre Lua shortcut sources.
- Launcher hand-offs use the shared scoped app launcher, which passes the compositor activation token when available. When focus cannot be confirmed, display an “Opened <label>” notification through the desktop notification service.
- Validation on dev-1: active overlay/include fixture, token-argument test, Settings all-pages render test, hand-off argv/toast test, and custom page goldens passed. Full shell binary: 879 passed, 4 ignored. Helper library: 1,017 passed, 3 ignored; one colors-preview timing assertion failed in the full run and passed in isolation. Helper and shell clippy, fmt, Ruff 0.16.10, compositor boundary, translation coverage and Python software suite (16 passed) passed.
- Known out-of-scope test issue: `wallpaper_latest_activation_survives_view_retirement` failed once in isolation, while the final full shell binary passed. No wallpaper code changed.

## N0221b keyboard shortcuts

- Learn > Keyboard shortcuts opens the existing `keybindings` Extras view, which consumes the daemon `shortcuts` topic's ordered `{items: {items, order}}` data. It no longer opens the empty launcher menu route.
- Shortcut descriptions prefer Niri's `hotkey-overlay-title`; when absent, the helper derives a readable label from the action and arguments. The active `nacre-niri` process argv is authoritative for config selection, ahead of environment fallback.
- Verification fixture: the live topic shape with 91 ordered synthetic bindings renders its first key and description in the opened keybindings view. No temporary duplicate implementation remains.
- Validation on dev-1: `cargo fmt --check` and touched-crate clippy passed; helper library passed (1,024 passed, 3 ignored). Shell binary: 881 passed, 1 Wayland motion wire test hit its 8-second child timeout under load, and 4 were ignored; the failing test passed alone in 10.81 seconds. Both wallpaper activation tests also passed alone after timing-related failures in an earlier loaded run.

## N0221d design options

Design pass only (owner 2026-11-12: "everything should work and be user friendly and look good"). Renders in the unit worktree's `.lead/renders/` (README there), made with the real launcher harness (`bead_test::Harness`, shell tree plus uncommitted variant patches, 1920×1200 at 1.5×, Orient *natural* palette of the demo wallpaper "Northern Lights", dark and light; glyphs from the harness's Material Symbols test subset). Nothing here is implemented yet; the owner picks one option per item.

### (a) Style › Wallpapers

What exists: `style.wallpapers` is a provider group (`crates/nacre-menu/src/providers.rs`, provider `wallpapers`): one plain result row per wallpaper (name, "Wallpaper"/"Animated wallpaper", `checked`, `preview`) plus the `next-wallpaper` action, all drawn as text rows because `shell/ui/components/results.luau` ignores `preview`, `checked` and `status`. The full picker already exists as the Extras view `shell/extras/wallpaper/main.luau` (kind `wallpaper` in `crates/nacre-shell/src/extras.rs`: carousel/spotlight/hexagon layouts, Static/Dynamic filter, search, previous/next, thumbnail hydration, revision-fenced activate), and the launcher already routes `route wallpaper` to it (`launcher_actions.rs`: `nacre mode wallpaper`). The dashboard hero has previous/pause/next on the daemon routes `wallpaper.previous|pause|next`; there is no shuffle route.

- A1 `a1-{dark,light}.png`: Style › Wallpapers opens the existing picker (add a `route wallpaper` row at the top of the group, or make the group itself open it), with shuffle and play/pause added to its navigation row. Cost: ~10 lines Luau in `extras/wallpaper/main.luau` + two `extras.do` requests in `extras.rs` (~20 lines), one catalogue row; shuffle needs a daemon route `wallpaper.shuffle` (or `wallpaper.next` with `{"random":true}`), ~15 lines in the Python daemon. No new Bead primitive.
- A2 `a2-{dark,light}.png`: a thumbnail grid inside the launcher content area, rail and search visible, header row previous / play-pause / next / shuffle + Static|Dynamic. Cost: new `shell/ui/components/wallpapers.luau` (~60 lines, uses `kit.VirtualGrid`), `results` routing in `ui/main.luau` (~5 lines), the row mapper must pass `preview`/`checked` through (`launcher.rs`, ~5 lines), thumbnail hydration for launcher rows (today only the Extras view hydrates `wallpaper:thumbnail:*` ids: ~80 lines of `extras.rs` logic to share or duplicate), same daemon shuffle route. No new Bead primitive (image `icon=` + `clip_polygon` already exist).

Recommendation: A1. It reuses the finished picker (three layouts, search, hydration, keyboard model, warm scene) and only adds the two missing transport buttons; A2 duplicates the hydration path and gives a smaller, plainer grid for ~4× the code. If the owner wants wallpapers "inside" the launcher later, A2's component can host the same rows.

### (b) Toggle and value feedback

What exists: the daemon decorates every row with `checked` (`context.rs::checked`, state kinds `setting`, `palette-mode`, `background`, `personality`, `dnd`, ...) and `status`; `launcher.rs` copies both onto the launcher row, but `results.luau` draws nothing for them, so Animations / Background blur / Window shadows and the value groups (Light and dark, Background tint, Orient personality, Accent choice) look identical before and after a click. The group rows carry no "current value" (the daemon only marks the child rows). Quick controls (`quick.luau`) already show status text + a switch and are the model for B1. The kit already has `kit.Switch`, `kit.ToggleTile` (primaryContainer fill when on) and `kit.LinkTile` (detail line + chevron).

- B1 `b1-{dark,light}.png`, `b1-*-changed.png` (Window shadows just switched on), `b1-*-chooser.png` (Light and dark chooser open): toggle rows get "On/Off" + `kit.Switch` at the right edge; value groups show the current value as the subtitle and a chevron; clicking a value group opens a small chooser card under the row (current option checked) instead of navigating into the group. Cost: `results.luau` ~35 lines, chooser ~40 lines in `ui/main.luau` (or a new `components/chooser.luau`), behavior store for the open chooser ~10 lines; daemon: a `status` for value groups = label of the checked child (`context.rs::decorate`, ~10 lines); the row mapper already passes `checked`/`status`. No new Bead primitive; focus ring = the existing `kit.Button` ring, chooser options are buttons in the content zone.
- B2 `b2-*.png` (same three states): "on" toggle rows filled with `primaryContainer`, ink `onPrimaryContainer`, check at the right; "off" rows outlined; value groups show the value as a chip + chevron; same chooser, current option tonal. Cost: same files, ~15 lines more than B1 (chip, tonal colours).

Recommendation: B1. It matches the quick controls beside it (same switch, same On/Off caption), the switch is the control the owner expects to click, and the subtitle value answers "what is it now" without a second colour system. B2 is louder and, with several toggles on, fills most of the list with accent colour. In both, the clicked state is immediate: the launcher flips `checked` locally (`Act::Flip` already stays open) and the daemon's next `checked` confirms it.

### (d) Icon tint

What exists: `shell/ui/components/item-icon.luau` draws action/group glyphs (Material Symbols Rounded) in `onSurface` (near white in dark mode, 15.8:1); app icons are theme images and stay untinted. The rail (`sidebar.luau`) uses the same `onSurface`.

- D1 `d1-*.png`: glyphs in `onSurfaceVariant` (muted, monochrome). Contrast on `surface`: dark 8.8:1, light 8.6:1.
- D2 `d2-*.png`: glyphs in `primary` (the wallpaper accent). Contrast: dark 5.7:1, light 6.3:1.
- D3 `d3-*.png`: glyph in `onPrimaryContainer` on a `primaryContainer` circle (tonal). Glyph vs circle: dark 9.2:1, light 12.1:1; the circle itself vs `surface` is decorative (dark 1.7:1, light 1.3:1) and not required to meet 3:1.

All three pass WCAG 3:1 for icons and AA for text with every demo palette; the ratios above are the lowest of the two themes for the chosen wallpaper. Cost: D1/D2 one token in `item-icon.luau` (1 line); D3 ~8 lines (circle container). Rail glyphs can follow the same token (1 line in `sidebar.luau`).

Recommendation: D1 for the grid and results, D2 for the rail's selected entry only. D1 removes the "strong white" the owner dislikes while every glyph stays readable and the app images keep their own colour; D2 is pleasant with this green palette but with a saturated wallpaper (Pop personality) twenty primary-coloured glyphs compete with the selection marker; D3 is the busiest and makes action tiles look like app tiles. Accessibility: the focus ring and selected marker are unchanged in all options.

### Next unit

After the owner picks: implement (a)+(b)+(d) in one unit with the catalogue `status` change in the daemon, goldens updated after a visual check, parity screenshots against the QML launcher, no temporary render code (this unit's render harness stays in `.lead/render-src/` and is not committed).
