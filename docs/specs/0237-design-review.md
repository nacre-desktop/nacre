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

## D7 Nacre AI sidebar (N0240)

Scope: the AI sidebar's launcher and conversation in `shell/chat/main.luau` (Bead kit components, palette
roles and size-scale tokens); Rust only where the view needs data (`crates/nacre-shell/src/chat.rs`). The views
stay plain functions of the `chat.*` / `launcher.*` stores so the D8 app can host them. The N0218 running/idle
states and the N0234 not-installed state behave as before.

What changed
- Composer first: a composer card ("Ask Codex…", attach, send) under the header. Enter or send starts the quick
  chat with the header's provider (`ai.chat.provider` when it differs) and sends the text (`ai.chat.send`); an
  empty Enter just opens the quick chat. The New chat / Quick chat pair is gone; "New chat in project" is a chip
  at the bottom left, Brain and Settings are bottom-right icon buttons with tooltips.
- Header: logo, "Nacre AI" 600, the provider as one segmented pill (chosen one `primary`), pin as a round icon
  button (`primary` while pinned).
- Rows: 8 px dot (`success` + 25 % halo + the N0218 pulse while running, `outlineVariant` idle), title 500, a
  time caption, mono outlined provider chip, "Open" (`primary`) on running rows. Running / Today (< 24 h) /
  Earlier groups come from the adapter (`group` on each group's first row; running rows sort first, stable
  otherwise, keyboard selection uses the same order). Headings are sentence case (the mockup uppercases them;
  the north-star "no uppercase in the shell" wins). The newest row's Continue chip (`ai.launcher.latest`) is
  removed: the composer and Open replace it.
- Messages: yours in `primaryContainer`, no caption, one-line ones right-aligned; the assistant's in
  `surfaceContainerHigh` captioned with the provider; code blocks on `surface`.
- Composer: one `surfaceContainerHigh` card (radius 16): field on top, attach and phone left, stop (`error`
  glyph, only while working) beside send. Scroll-to-latest is removed (following resumes at the bottom as
  before). Attach and phone stay disabled: `ai.chat` has no such actions yet.
- Conversation header: back, title 500, a `success` working dot before the subtitle while busy, overflow.
- Keyboard unchanged: focus opens on the search field (Enter opens the selected row, Up/Down move it); the
  composer is one Shift+Tab away. Icon buttons and send draw the focus ring; the composer shows its caret only
  (`focus_ring=false`, as before).

Motion
- Panel open 240 ms `ease` (cubic out), close 160 ms (Bead's accelerate curve via `return_duration`); was
  400 ms ease-in-out / 180 ms. `-- D16:` marks the swap to out-quint. Edge growth, `fade=false`, `reveal` and
  the detach/dock path are unchanged. Reduced motion: Bead already snaps panel and `ui.animate` motion.
- Frame cost (dev-2, `frame_cost_bench` release, CPU tree renderer, scale 1.5, 120 Hz slide of the real
  `chat/main.luau` launcher via a temporary bench route; base = origin/main's view on the same binary): open
  29 frames, total p50/p95/max 0.165/0.281/2.888 ms (base 48 frames, 0.168/0.920/2.201); close 19 frames
  0.294/0.383/0.436 ms (base 22 frames, 0.284/0.317/0.410); 55 vs 44 primitives. No per-frame allocation added
  (the transcript stand-in measures only visible user messages). The nested-desktop PANEL_SLIDE max_gap
  capture was not run; the lead's laptop live audit is the gate.

Accessibility: text >= 4.5:1 and icons >= 3:1 in both fixture palettes (stop `error` on
`surfaceContainerHighest` 4.7 / 4.7, captions on `surfaceContainerHigh` 7.7 / 7.1, Open 12.7 / 5.2). The idle
dot and chip outline (`outlineVariant`, 1.8:1) are decorative; state is also in the heading and Open text.

Asks (forwarded by the lead; stand-ins marked in the code)
- Bead: transcript entries with per-entry alignment, max width and no label band (stand-in `CompactTranscript`;
  multi-line user messages stay full width until then); attention pulse (header dot is static; shared with
  N0237); D16 `out-quint`; `kit.PillButton{ink=...}` and a default focus ring on icon-only pill buttons (stop is
  a hand-built `kit.Button`); `kit.Tag{mono=true}` (stand-in: outlined Box + mono Text).
- Platform (D8): per-message time (assistant caption), real projects (the helper sends the legacy constant
  `project: "general-chat"`, so the adapter keeps stripping it; the view prefixes `project ·` once present),
  the quick chat's working directory (the mockup's folder chip), attachment and remote-device actions.

Known gaps: the search field stays (the mockup has none; it is the filter and keyboard path). When the chat
worker has not published its state yet (first open after login), Enter in the launcher opens the quick chat
with the text left in its composer instead of sending it.

Evidence
- Goldens (looked at each): `chat-{launcher,launcher-detached,launcher-warning-tooltip,conversation,code,
  approval,empty,missing}-{1,1.5}`, new `chat-{launcher,conversation}-light-1`, `pearl-*-{1,1.5}` (they render
  the chat launcher). Other goldens unchanged.
- dev-2 (arch-nacre, rustfmt 1.10), rebased tree: fmt --check, clippy `-p nacre-shell --all-targets -D
  warnings`, whole `nacre-shell` binary (928 passed, 4 ignored), Luau analyzer `--check-generated`, i18n,
  compositor boundary. `real_prompt_wire_sequence_and_stock_compositor_fallback` failed once under host load in
  an earlier run and passed three times isolated (as in N0237).
- New test: Enter in the launcher composer sends `ai.chat.provider` and `ai.chat.send` with the text.
- Before/after PNGs with notes (hover, pressed, focus, pinned, busy, idle, groups, not-installed, dark and
  light): the unit worktree's `.lead/renders/README.md` (not committed).

Strings: new `bead.panel.{chat-ask,chat-phone,launcher-today,launcher-earlier,launcher-open,
launcher-new-in-project}`; removed with their controls `bead.panel.{chat-you,chat-latest,chat-screenshot,
launcher-resume,launcher-quick-chat}` (shell catalog + context; no registry ids, `nacre/shell-tools/i18n`
untouched).

## N0239 — D12 Settings controls

Segment control, link rows with titles, the Edge handles rows and the Top bar status rows.

### What changed

- **Pill segment control** (`shell/widgets/segment.luau`, a stand-in for a kit `Segment`; kit ask
  recorded in the unit's `.lead/kit-asks.md`): a `surfaceContainerHigh` pill (34 px, radius full,
  padding xs, gap hairline) with one full-radius option each (caption text at weight 500, padding
  md); the selected option is `primary` / `onPrimary`, the others `onSurfaceVariant` with the kit
  hover tint (`onSurface` .08, .12 while pressed) and a focus ring per option. `segment.Segment`
  is the bare control (D13 Orient tabs, AI panel), `segment.Row` the `kit.FormRow` shape every
  Settings enum row (`kind == "choice"`, up to four options) now uses instead of `kit.ChoiceRow`.
- **Link rows with titles**: every row that opens a page or a tool (`kind == "link"`) shows a
  title, a description and a `chevron_right` glyph; inside a section card the row is flat with the
  row hover tint, at the top of a page it keeps its own card. Tool rows (`launch_row`: open sidebar,
  accounts and usage, power service, AUR policy, Welcome, `nacre setup`, audio manager, connection
  editor) are links with an `action` (`do:open|<key>`) instead of a bare button beside an empty
  label; a missing daemon method dims the row. The AI page rows gained descriptions
  (`bead.settings.ai-open-detail`, `bead.settings.ai-accounts-detail`) and the sidebar link reuses
  its page description.
- **Edge handles** (Panels page, section `panels.lips`, relabelled "Edge handles"): `Handle style`
  (segment: Pearl grabber, the default, or Arrows → `desktop.edgeHandleStyle`), `Left edge`,
  `Right edge`, `Top edge` toggles (`desktop.edgeHandleLeft/Right/Top`, default on; an edge whose
  handle is off has no lip, the bar button fallback is N0247). The old `panels.lips.lipSize` row,
  its strings, `Prefs::lip_size` and the lip-length override are gone (a saved `lipSize` is ignored).
  Readers: `Prefs::edge_arrows` / `Prefs::edge_handles`; `ShellGate::apply` filters the frame's lips
  per edge and switches the lip art live (`frame_art::ARROWS`, a process-wide flag the one gate
  sets; a decoration variant bump repaints open panels' art too). The pearl grabber art (`frame_art::grabber`) is the minimal D6 handle: a
  6 × 56 rounded bar in `primary` at .9 centred on the lip, 8 × 72 while the dwell charges with the
  charge filling it top-down in `secondary`; N0247 adds the 16 px hit target and the hover growth
  (the framework's `LipState` has no hover flag yet, kit ask 3).
- **Inner edge push** (nacre-niri only, rows gated by the new `innerEdgePush` capability): `Push
  through shared edges` (`desktop.innerEdgePush`, default on), `Hold time` (`innerEdgeHoldTime`,
  60–600 ms, default 180) and `Push distance` (`innerEdgePushDistance`, 16–240 px, default 64).
  The helper writes nacre-niri's `edge-triggers { inner-edge-push { … } }` block into
  `nacre-niri.kdl` with only the keys the user changed (`hold-time-ms`, `cross-distance`, `<edge>
  false` for an edge whose handle is off; nothing at the defaults, NN0041 interface). Turning push
  off writes all four edges `false`, not `off`: `off` makes nacre-niri report the capability as
  false, which would hide the row that turns it back on. The overlay is written when the session is
  prepared, so these three rows apply at the next sign-in (the toggle's description says so).
  Capability chain: `NacreCapabilities.inner_edge_push` → compositor snapshot `innerEdgePush` →
  daemon flag `innerEdgePush` → registry `visibleWhen`.
- **Top bar status** (Top bar page, new section `panels/top-bar.status`): `Badges`
  (`desktop.statusBadges`, default on → `bar.status.badges`; N0237's `status_glyph` in
  `shell/widgets/context.luau` hides every count pill when it is off).
  The status order stays the existing Modules list; the section description points there, so there
  is one ordering control.

### Motion

No new transitions: the segment and link rows use the kit hover/pressed fills (the framework's
state motion, `kit.motion.state` 120 ms) and the existing focus ring. The handle style change
repaints the lip art through the decoration variant (no animation; the lip is 10 px wide). The
grabber's charge is the existing dwell fill. Nothing allocates per frame. No panel motion changed,
so there is no nested PANEL_SLIDE capture for this unit; reduced motion is unaffected.

### Evidence

- Goldens updated (looked at each): every Settings page with an enum row now draws the segment and
  every link row its chevron: `settings-{addons,brain,date-time-language,launcher,nacre-ai,panels,
  power,search,windows-workspaces}-1`, `settings-kit-1` (the kit gallery), and
  `frame/frame-lips-{idle,dwell,open}` (grabber instead of chevron). `settings-panels-top-bar-1`
  is unchanged (the Status section is below its fold; asserted in `settings_tests`).
- Before/after real-renderer PNGs (bead_test CPU harness, 960 px, dark and an Orient light scheme):
  Panels (edge handles, arrows + top off, push rows, segment hover / pressed / focus), AI page
  (segment, link hover / pressed / focus), Top bar (Status). Kept with a README in the unit
  worktree's `.lead/renders/` (not committed).
- Checks on dev-2 (arch-nacre image, rustfmt 1.10): `cargo fmt --all --check`, clippy `-p nacre-shell` and
  `-p nacre-helper --all-targets -D warnings`, the whole `nacre-shell` test binary (929 passed, 4
  ignored), all `nacre-helper` test targets (1320 passed), Luau analyzer `--check-generated` (186
  modules), i18n coverage (1705 messages), compositor boundary; run on origin/main 2770bb45b, then
  fast-forwarded over two commits that touch none of these files.

### Kit asks (forwarded by the lead)

1. `kit.Segment` (Luau component; stand-in `shell/widgets/segment.luau`, `ChoiceRow` should adopt it).
2. `kit.LinkRow` (title + description + chevron, row-wide hover; stand-in: the `link` branch in
   `shell/settings/main.luau`).
3. `LipState.hovered` (bead-wayland edges): the grabber's 6 → 8 px hover growth before the dwell.
4. Edge art repaint on demand: switching grabber ↔ arrows bumps the decoration variant (full chrome
   rebuild) today.

### Known gaps

- Grabber hover growth before the dwell, the 16 px hit target and the inner-edge hint style: N0247
  (kit ask 3). N0247 also owns the bar buttons shown when a handle is off.
- The inner-edge push rows apply at the next sign-in: `nacre-niri.kdl` is written when the session
  is prepared, and a live rewrite would be a new daemon path (not built).
- A vertically stacked monitor's bottom inner edge stays sticky (default `bottom true`) though
  Nacre's default frame has no bottom lip; left to N0247 with the rest of the inner-edge behaviour.
- The stepper value column stays the kit's 64 px (the review's 48 px tabular column is a kit
  `NumberRow` change). N0237's Stack style description is clipped beside the wider segment at
  960 px; shorten it with the next string pass.
- Light goldens for Settings: D17 (N0249); this unit's light evidence is the renders above.
- Delete `shell/widgets/segment.luau` and the `link` branch's chevron when `kit.Segment` /
  `kit.LinkRow` land (asks 1 and 2).

## D11 launcher polish (N0241)

Scope: the launcher's content column, rail, tiles, result rows and Quick controls card in `shell/ui/` (Luau on
Bead kit, palette roles and size tokens). Rust only where the row model needs it (`launcher.rs` icon default).
The owner's N0221e picks (On/Off switches and choosers, wallpaper picker, tinted glyphs) and the N0219 keyboard
zones are unchanged.

What changed
- One heading: the section title is dropped when it repeats the page title (`main.luau`; "Applications" twice
  on All apps). Home ("Home" / "Favorites") and results ("Results" or the file hint) keep it, so the N0219
  stationary first result row still holds across results states; the app grid starts 28 px higher.
- Initial-circle fallback (`components/item-icon.luau`): a row without an icon is a circle of `surfaceContainer`
  tinted 12 % toward `primary` with the name's first character in `primary`, 600 weight, 40 % of the icon size
  (grid 36 px, results 30 px). Contrast 10.0:1 dark, 4.76:1 light. `launcher.rs` no longer substitutes the
  `application-x-executable` theme icon for an empty icon (the generic grey tile the review flagged).
- One selection cue: the tile, the result row and the rail entry keep the 3 px `primary` accent bar and their
  existing subtle fill; the framework's selected border is off (`border_color="transparent"`). Bead's keyboard
  focus ring (1 px `primary`) is unchanged, so focus stays visible.
- Rail (D1 tint): glyphs 18 px (`T.icons.large`, was 20), muted `onSurfaceVariant` tinted by the N0221e
  wallpaper tint (`icon-tone`), the selected glyph in `primary` and its label at 500.
- Quick controls: the card's top is the page title's top (16 px, the panel's top padding; was 84 px); its bottom
  edge is unchanged (`metrics.luau`).

Known gaps
- An icon name the theme cannot resolve still shows the generic fallback asset: resolving every row on the
  source thread would cost theme scans (B0187 measured ~12 ms per miss); only empty icons get the circle.
- The mockup's tile glyph is a 48 px rounded square; the review text asks for a circle like the pearls, and the
  grid keeps its 36 px icon size.

Bug fixes in this unit (separate commits)
- B4 (Platform N0316 tier-2 smoke, "launcher jumps sideways on Left/Right"): not reproduced. The smoke's own
  frames (`hypr-smoke2-02..04`, `niri-extra-30..35`, taken 300 ms after each key) show the panel body at the
  same pixels (Hyprland: x 411-1508 in all three), and the positioner input does not depend on focus: the
  committed scene's body stays 1100 x 590 and the bottom-attached origin is identical across
  right/right/left/left/right (rail -> content -> rail -> content), now pinned by
  `launcher_panel_origin_stays_put_while_keyboard_focus_crosses_zones`. The keys in that run landed inside a
  1.2-1.3 s open slide (nested `PANEL_SLIDE ... max_gap_ms=1013.6`), the likeliest source of what was seen;
  confirm on a live session.
- N0309 (extras command palette): a `stale_revision` reply to `menu.activate` no longer shows "Action
  unavailable; refresh and try again". The palette re-queries silently and retries the same row once with its
  fresh activation ticket, like the launcher's one-shot retry; a second stale reply, a vanished row or a failed
  query still shows the message. The retry is dropped if the query, route or daemon boot changes first (the
  existing "no obsolete activation survives a new query/boot" rule). Test:
  `palette_requeries_and_retries_a_stale_activation_once_without_a_message` (one and two stale replies).

Motion
- No transition changed: the launcher's bottom-edge reveal (0.3 s open, 0.18 s return, interruptible retarget)
  and its frame contour are untouched; D11 only changes the static layout inside the panel, with no new
  animation, per-frame allocation or relayout. Reduced motion behaves as before.
- `frame_cost_bench` (dev-2, release, CPU renderer, scale 1.5, 120 Hz slide of the real `ui/main.luau` route;
  candidate vs origin/main `shell/`): open total p50/p95/max 0.734/0.863/3.07 ms vs 0.707/0.808/3.13 ms, close
  0.582/0.710/0.748 ms vs 0.592/0.653/0.739 ms, 30 vs 31 primitives. The slide path is untouched, so the nested
  desktop PANEL_SLIDE capture is left to D5/D16 (the units that change panel motion); the lead's laptop live
  audit stays the gate.

Evidence
- Before/after real-renderer PNGs (dark and light; grid and results; selected, hover, tile focus, rail focus)
  with notes: the unit worktree's `.lead/renders/` (not committed).
- Goldens updated (looked at each): `launcher-rows-{grid,results,scrolled}`, `launcher-two-line-{1,1.5}`,
  `launcher-conversion`, `launcher-conversion-error`, `launcher-guidance`, and the launcher reveal fixtures
  under `crates/nacre-shell/tests/fixtures/bead-layer-motion/reveal-goldens/`.
- `files_hint_keeps_first_result_at_the_same_pixel_row` now starts from a results query: the grid no longer
  has the reserved section line, results states still keep the first row on the same pixel row.

Strings and settings: none added or removed. Kit asks: none required (optional `kit.Monogram` noted in the unit
worktree's `.lead/kit-asks.md`).

## D10 toasts and the pill OSD (N0243)

Scope: the notification toasts (`shell/notifications/main.luau`) and the OSD (`shell/osd/`), Luau on Bead kit
components with palette roles; Rust only for the surfaces' geometry and the OSD hold
(`notice_views.rs`, `notice_service.rs`). Keyboard behaviour (toasts take pointer input, no keyboard focus; the
OSD is click-through for the compatibility owner) and reduced motion (the kit's) are unchanged.

What changed
- Toast header: the title (16 px/600) leads, "app · time" (13.3 px, `onSurfaceVariant`, at most 40 % of the
  header) sits on the right, then the expand and dismiss buttons (28 px, transparent, `surfaceContainerHighest`
  on hover, the N0238 row's dismiss style). The app glyph sits in the N0238 rows' 28 px `surfaceContainerHighest`
  tile and the caption uses the same `time_str`, so a toast and its control-panel row read the same. A collapsed
  title that needs more than two lines ends in an ellipsis and earns the expand button; the accessible name keeps
  the full text.
- Toast body 13.3 px in `onSurfaceVariant`; actions are 30 px chips (500 weight, 14 px side padding, measured
  with `ui.text_width`), the first filled `primaryContainer`/`onPrimaryContainer`, the rest
  `surfaceContainerHigh` (`surfaceContainerHighest` on hover). Actions stay behind expand as before (QML
  behaviour). Card: 380 wide (was 400), radius 18, padding 12/12/12/14, gap 8, `surfaceContainer`.
- OSD: every kind (volume, output, microphone, brightness, keyboard backlight, night light, power profile, Caps
  Lock) is one 336 x 60 pill (was 60, 68 or 76 tall): the glyph in a 36 px `surfaceContainerHigh` circle
  (`primary`; `onSurfaceVariant` when muted or off; `success` for the power-saver leaf via the new `tint` field in
  `kinds.luau`), the title at 14.7 px/600, the 8 px gradient bar (`surfaceContainerHighest` track) under it on
  level kinds with the detail inline ("Volume · Headphones", ellipsized, dropped when only a stub would fit), the
  detail on its own line for the others, and the value at 16 px/600 on the right without "%" (IBM Plex Sans
  digits are tabular: 600 units each). The value column always fits "100", so the bar never changes length while
  the level moves. Fill `surfaceContainer` (was `surface`) with the existing 1 px outline at 28 %.
- OSD placement: bottom-centre with its lower edge 96 px above the output edge (a positioner in
  `notice_views::osd`; before, only its 32 px shadow margin sat below it). Hold 1.2 s (was QML's 2 s).

Known gaps
- Size: 336 x 60 instead of the review's 360 x 64. Bead prewarms and permanently caches the OSD shadow only for
  396 x 120 (336 x 60, radius 30) and 396 x 136 at scale 1.5 (B0156: a cold shadow raster at 1.5x cost a
  dropped first frame); any other pill re-rasterises its shadow through the evictable template cache. The
  review's size waits on kit ask 4.
- No toast shadow: Bead draws shadows for `Panel` nodes only, so cards keep their 1 px `outlineVariant` border
  (kit ask 2). The expand and close icons are always drawn, not on hover only (kit ask 3, same as N0238).
- The review's "stack shifts 180 ms" is not animated: rows jump to their new offsets (kit ask 5).
- No press scale (.97) on chips: the kit has no press feedback for buttons (kit ask 6, D16/D5).
- No QML side-by-side: the QML OSD and toasts are retired; the review's mockup is the reference.

Motion (design review "Timings for 120 Hz")
- OSD: open 160 ms with a 12 px rise and the fade (was 150 ms, 4 px); close `return_duration=.2`, which the kit
  caps at .75 x open = 120 ms with its accelerate curve (was 105 ms); the level tweens 120 ms out-cubic (was
  130), still snapping on the first visible frame (B0207).
- Toasts: open 220 ms sliding 24 px from the right with the fade (was a 160 ms fade in place); close
  `return_duration=.2`, capped at 165 ms (was 112 ms). Open curve: the kit's EaseOut (out-cubic) for the review's
  out-quint; `-- D16:` markers in both views for the B0263 easing tokens; the close cap is kit ask 1.
- Both are interruptible as before (the kit retargets a reversed slide); no per-frame allocation or relayout was
  added (the views build once per model change; the slide is the presenter's pose).
- `frame_cost_bench` with the OSD and toast routes added for the run (dev-2, release, CPU renderer, scale 1.5,
  120 Hz poses; candidate vs origin/main `shell/`; total per frame p50/p95/max): OSD open 19 frames
  0.149/0.520/0.565 ms vs 18 frames 0.168/0.524/0.559 ms, close 14 frames 0.150/0.415/0.592 vs 13 frames
  0.168/0.450/0.541; toast open 26 frames 0.141/0.638/0.935 vs 19 frames 0.200/1.092/1.216, close 20 frames
  0.140/0.147/0.653 vs 13 frames 0.200/0.831/0.832. Every frame stays far inside the 8.33 ms budget; the surfaces
  shrink (OSD 600 x 186 px, toast 666 x 272 px at 1.5).
- Nested 120 Hz capture (`PANEL_SLIDE ... max_gap_ms`): not run. The desktop TEST host held 2.5 GiB of swap
  (gate: under 64 MiB, the same gate N0308's desktop arms wait on); a swap reset is the lead's call. The lead's
  laptop live audit stays the gate.

Evidence
- Before/after real-renderer PNGs (dark and light; every OSD kind; the review's toast and a stack with a long
  title, a critical and a title-only toast; hover on each chip, expand and dismiss; Tab focus; open-motion frames)
  with notes: the unit worktree's `.lead/renders/` (not committed).
- Goldens updated (looked at each): `notification`, `notification-scroll-{top,bottom,reversed}`, `osd`,
  `osd-{volume,volume-low,volume-muted,volume-clamped,output,microphone,brightness,keyboard,night-light,power-profile,capslock}-1`.
- Tests follow the view: OSD values without "%", the inline detail texts ("· Speakers"), the toast canvas 444
  wide, and the B0207 first-frame test compares against the settled fill instead of a hard-coded track width.

Strings and settings: none added or removed. Kit asks (forwarded by the lead; unit worktree `.lead/kit-asks.md`):
1 a close longer than 0.75 x open (`Motion::transition` cap), 2 `shadow=true` on any surface node, 3 hover-only
children, 4 caller-supplied shadow prewarm (replaces the hard-coded OSD shapes in bead-render), 5 animated list
reflow for the toast stack, 6 press feedback on kit buttons.
