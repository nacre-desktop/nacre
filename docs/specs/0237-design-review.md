# 0237 Design review (2026-11-13): one shared spec, one section per D-unit

Source of truth: the owner-approved design review (lead file `design-review-20261113.html`) and
its plan lines. Each D-unit appends its own section here; the "Current status" table in
`README.md` keeps one row per unit.

## N0237: D1 bar status cluster, D2 bar stack, D3 bar title

Scope: the top bar's three regions, in `shell/` Luau on the Bead kit, Rust only where the view
needs data. Sizes and colours come from `shell/bar/tokens.luau` and palette roles.

### D1: the quiet strip (status cluster)

- Geometry: 20 px Material Symbols Rounded glyphs in 24 px boxes, 14 px between boxes, 12 px to
  the pill ends. Each item carries half the gap as padding (`status_padding=7`), so the hit
  targets tile the pill with no seam (`bar_counts_share_icon_hit_areas_and_centered_line_boxes`).
- Colour rule (`context.status_glyph`): `onSurfaceVariant` when idle/off, `onSurface` when
  active/connected (Wi-Fi/Ethernet connected, a connected Bluetooth device, an unmuted sink,
  a battery that is not low). Attention only: battery <= 20 % `peach` (stand-in for the Orient
  `warn` role, kit ask 6), <= 8 % `error`, recording `error`. Off states keep the slashed glyph
  but in the variant tone, so "off" is quiet.
- Badges: updates and notification counts are 15 px `primary` pills (10 px / 600) lifted above
  the glyph's top-right corner; hidden at 0 and while Do Not Disturb is on
  (`do_not_disturb_on` replaces the bell). Stand-in `context.status_glyph(..., badge)` until the
  kit has `kit.Badge` (kit ask 2).
- Clock: a 1 px `outlineVariant` separator (16 px tall), then the time in `onSurface` 15 px 500,
  the only text in the pill. The calendar glyph is gone; the time opens the calendar.
- Power: a 34 px round button after the pill (12 px gap), `onSurfaceVariant` at rest,
  `error` glyph on hover, so the bar has no permanent red. `widgets/default.luau` places it in
  `top_bar/right/1` and migrates a saved layout's stock power child out of the pill (copies only).
- Icon family (Rust `reload/bar_daemon.rs`, `desktop.rs`): battery `battery_android_bolt`
  (charging) / `battery_android_alert` (<= 20 %, percent shown) / `battery_android_0` (<= 8 %,
  percent shown) / `battery_android_full` / `battery_unknown` (unavailable); network
  `wifi` / `lan` (a cable counts even with Wi-Fi off) / `wifi_off`, plus a 10 px `primary` lock on
  the glyph when a `vpn`/`wireguard` connection is active (`bead.desktop.network.vpn`);
  Bluetooth `bluetooth_connected` (+ the device's battery percent when it reports one,
  `bead.desktop.bluetooth.value`) / `bluetooth` / `bluetooth_disabled`; audio `headphones` when the
  default sink names a headset, else `volume_up` / `volume_off` / `volume_mute`; updates
  `deployed_code_update`; recording `radio_button_checked` + elapsed in `error`.
- Battery default style is now "icon": the percent appears only in the two attention states
  (the "icon + value" style still shows it always).
- Known gaps (no data in the bar today, listed for the owner): power-saver leaf (the profile
  lives in the control-centre sources, not a bar store), mic/camera in use (no daemon topic),
  the 1.6 s attention pulse and the badge pop (kit asks 3 and 4).

### D2: the stack folds into the pill

- Default (`bar.stack.style = "pearls"`): a strand of up to three tiny pearls at the pill's
  head: the current stack 11 px bright (the pearl family's cream-tinted primary), the others
  9 px `surfaceContainerHighest`, overlapping 4 px with a 1.5 px `surfaceContainer` ring. Hover
  shows the stack name as a tooltip; click opens the existing switcher popout
  (`collection.switcher.open`). The chip (layers icon, truncated name, chevron, divider) is deleted.
- Setting (`Settings > Panels > Top bar > Stack style: Pearls | Name`, `desktop.stackStyle`,
  `prefs.rs` -> `bar.stack.style`, live): the named 28 px cap inside the pill, `stacks` glyph +
  name (12 px / 500, at most 12 characters, tooltip when ellipsized), no chevron.
- Tier 2 (stock niri, Hyprland): `bar.collections` is empty, so neither appears and the pill
  starts at the current pearl (the existing `workspaceCollections` capability gate).

### D3: the title

- A real themed 16 px app icon from the desktop entry (`bead.title.app_icon`, Rust
  `workspace_rows::desktop_icon_name`) when there is one, else the category glyph in `secondary`;
  then the title in `onSurface` 15 px 500. Accent is no longer spent here.
- The 2 px trace stays, 36 px wide, 5 px above the bar's bottom edge, in the current pearl's
  colour (`context.pearl_accent` by workspace index). It follows the title width retarget
  (collapses while the width is in flight, re-extends as it settles); a true restart-on-focus
  needs kit ask 4 (`-- D16:` marker).
- Titles clamp to one line with an ellipsis in the middle (`middle_ellipsize`, max 550 px);
  hovering a truncated title shows the full title in a tooltip. Click/hover behaviour is unchanged.

### Motion

The bar's transitions are scalar `ui.animate` retargets (title width 180 ms, trace, workspace
capsule 160-180 ms, hover fills), all interruptible by construction (retarget keeps the current
value). No panel slide is added by this unit; the owner's laptop live audit is the gate for
hover/switch smoothness, and `workspace_switch_ticks_every_refresh_at_120_hz_in_both_directions`
keeps the 120 Hz cadence. Reduced motion: Bead snaps every `ui.animate` to its target.

### Evidence

- Goldens (dark + light, real renderer): `bar-parity-{1,1.5}`, `bar-parity-hover-{1,1.5}`,
  `status-pill-hover-{1,1.5}`, `bar-power-hover-1`, `bar-parity-light-1`, `bar-states-1`,
  `bar-states-light-1`, `desk-chip-{long,number,single}-{1,1.5}`, `desk-chip-tooltip-{1,1.5}`,
  `workspaces-pill-*`, `settings-panels-top-bar-1`.
- `settings-panels-top-bar-1` is unchanged: the new row sits below the golden's fold; the row's
  kind, labels and default are asserted in `settings_tests` instead.
- Checks on dev-2 (arch-nacre image, rustfmt 1.10): fmt, clippy `-p nacre-shell --all-targets`,
  the whole `nacre-shell` test binary (913 passed; `layer_motion_wayland_tests::real_prompt_wire_
  sequence_and_stock_compositor_fallback` fails only under host load and passes in isolation, three
  times), Luau analyzer `--check-generated`, i18n coverage.
- Before/after PNGs with notes: the unit worktree's `.lead/renders/README.md` (not committed).

### Strings and settings

- New ids: `bead.bar.open-updates`, `settings.registry.panels.top-bar.panel.stackStyle.*` (label,
  description, option.pearls, option.name) in the shell catalog, the settings registry and
  `nacre/shell-tools/i18n/{en,context}.json`.

### Kit asks (forwarded by the lead)

Palette-tinted monochrome icon, `kit.Badge`, attention pulse, animation restart/`from`, the D16
easing names, Orient `warn`/`good` roles. Stand-ins are marked in the code and above.

## D4 control panel overview (N0238)

Scope: the control panel's home page in `shell/control/` (Luau on Bead kit components, palette roles only).
Rust only where the view needs data (`crates/nacre-shell/src/control_center.rs`).

What changed
- Sliders first: Volume and Brightness are 56 px rows (`slider_row`, radius 16, `surfaceContainer`) with a mute
  icon button at the left, the stepped kit slider, the value 38 px wide and a 28 px end button: the output
  device (opens the sound page) on Volume, the microphone mute on Brightness (`mic_off` in `error` while muted).
  The Mic mute tile is deleted (replaced by that button); Mic volume stays in the registry but out of the default
  layout (the sound page already has the input level).
- Tiles: 2 × 2, 64 px, `kit.ToggleTile`; `primaryContainer` when on. The main press toggles; only Wi-Fi,
  Bluetooth and Power mode (tiles with a detail page) keep the split chevron. DND is a plain toggle; the
  Notifications heading opens the full in-panel list instead of DND's chevron.
- Chips (`chip()`, 36 px pills, wrap by measured width): the appearance segment (Light | Dark | Auto in one
  pill), Night light (hidden on stock niri as before), Screenshot, Pick colour, Clipboard, Updates with a count
  badge from `dashboard.updates` (`cc.updates`), plus Record, Displays, Wallpaper, Next wallpaper and
  Notifications when added. Buttons and `chip=true` toggles are chips; the edit page orders chips in the same
  list as the tiles (one saved order).
- Notifications: heading row (16 px/600, DND and Clear chips at 30 px) then 72 px rows (`notice_row`): app icon
  in a 28 px `surfaceContainerHighest` square, "app · time" caption (`when` from `created_ms`, same
  `time_str` as the toasts), 600-weight title, one body line, × dismiss (`dismiss:notifications:<id>|<rev>`,
  history rows are forgotten). The same row is used by the notifications page and the windowed peek list.
- Section ids `sliders`, `tiles`, `chips`, `notices` under `home` are stable scroll targets for D5.
- Strings: `bead.panel.control-output-device`, `bead.panel.control-mic-unmute` (shell catalog + context).

Known gaps (listed in `.lead/kit-asks.md`, forwarded to the Bead lead)
- Brighter primary container: an Orient tone (`palette.rs` dark container L 0.35 → 0.40), not a shell change;
  lands with an Orient commit + submodule bump + `shell/palette.json` refresh (D17 or its own unit).
- Kit tile keeps radius 24 and 13.3 px/500 labels (ask: tile tokens); the × on notification rows is always drawn
  (ask: hover-only children); plain toggles have no long-press detail path (ask: secondary activation).
- Per-app grouping after 3 notifications is not implemented (D9 shares the row; one later unit).
- Motion: the panel's open/close curves are unchanged (D5/D16); no per-frame allocation was added (the home
  page is a static tree; chips wrap at build time).

Motion evidence (dev-2, `frame_cost_bench` release, CPU renderer, scale 1.5, 120 Hz slide of the real
`control/main.luau` route; candidate vs origin/main base): open total p50/p95/max 0.688/0.752/3.34 ms vs
0.682/0.733/3.36 ms; close 0.314/0.404/0.42 ms vs 0.327/0.416/0.56 ms; 79 prims both. The panel's slide path is
untouched by D4, so the nested desktop PANEL_SLIDE capture is deferred to D5 (the unit that changes the motion);
the laptop live audit by the lead stays the gate.

Evidence: `.lead/renders/` in the worktree (before = previous goldens, after = real-renderer PNGs in dark and
light, hover, pressed and focus states; README there). Goldens updated: control-overview, control-detail,
control-default-order, control-n0046-overview, control-notifications-*, control-unsupported (1× and 1.5×).
