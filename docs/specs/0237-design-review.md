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
- Titles clamp to one line with an ellipsis in the middle (`middle_ellipsize`, max 550 px).
  Hover changes nothing about the title: the full-title hover tooltip was removed after the owner's
  2026-11-24 report (it covered the title in grey, ran wide and spilled under the bar). The middle
  ellipsis keeps both ends. Click/hover behaviour is otherwise unchanged.

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

## D9 dashboard pages (N0242)

Scope: the top panel's Media, AI usage, Brain, Notifications, System and Weather pages in `shell/dashboard/`
(Luau on Bead kit, palette roles and size tokens), two shared components in `shell/ui/components/`, and the
projections that feed them (`dashboard.rs`, `dashboard_controls.rs`). The panel's slide/peek mechanics, the tab
strip and Today are untouched (N0246 owns the motion; the review keeps Today).

What changed
- Shared empty state `ui/components/empty-state.luau`: a hero-size outline glyph in `outline`, one line in
  onSurface (500), one line in onSurfaceVariant, at most one compact pill chip, centred in the given box. Media,
  AI usage, Brain (both cards) and Notifications use it; any surface can require it.
- Shared notification row `ui/components/notice-row.luau`: the D4 row (72 px, 28 px icon tile, "app · time",
  600 title, one body line, × dismiss) moved out of `control/main.luau`, which now requires it; the dashboard's
  Notifications page renders the same row on its card (`surfaceContainerHigh`).
- Media: an art-led card. The backdrop is `kit.Fill` from a 35 % secondary tint of the container into the
  container; with album art the blurred art (`kit.BlurredArtwork`, clipped to the card radius) sits under a 72 %
  container scrim. 120 px art, player caption, 20/600 title, "artist · album", the transport row (±10 s,
  previous, 44 px primary pause, next; gated by the topic's `can_*` flags as before) and an 8 px meter with the
  position left and the duration right. The lyrics card renders only while lyrics exist (7 + 5 columns, 14 px
  gutter); otherwise the player spans the page. Without a player the shared empty state says "Nothing playing".
  The identity pill, the "Track details" column and the 84 px note placeholder are gone.
- AI usage: one ring per quota window the provider reports (`ai.usage` windows, Platform N0319): a 96 px arc,
  primary for the first provider and secondary for the rest, the percent inside, "provider · window" under it
  and "Resets in 3h" from `resetsMs` against the shell clock (absent when the provider reports no reset). A
  provider without windows keeps its text lines; nothing is estimated. Refresh is the card's icon action and,
  before the first read, the empty state's chip. The preview fixture gives Claude two windows.
- Brain: a "Brain" card (7 columns) with an "Open Brain" chip (`ui.dispatch` → `nacre brain`, the route
  `nacrectl ui nacre brain` takes, N0217), "Recent notes" rows that open the note by id (ids are checked against
  the topic before anything is sent), and an "Open loops" card (5 columns) with checkbox rows. World dots and
  dates are not drawn: `brain.recent` carries neither (platform ask below; nothing is invented).
- Notifications: one card with the title, an All | DND segment (the D12 `widgets/segment` pill; the selected
  option is a no-op, the other toggles `notifications.dnd`), the shared rows in the existing virtual list (same
  scroll store and `notif.*` actions), a "Clear history" chip bottom-right and the stored-count note beside it.
  The Status column and the "DND on/off", "Dismiss" and "Forget" text buttons are gone.
- System: an unavailable gauge keeps its track and says "Not available on this device" once (no "—" readings,
  no second "Unavailable"); the Network card is drawn only with samples or a rate, otherwise Top processes
  spans the row.
- Weather: the 24-hour line and its area follow a monotone cubic (Fritsch–Carlson, 8 samples per hourly step),
  so the chart curves without overshooting; labels keep the reading positions.
- AI usage reads the typed `AiProvider::windows` (the shell uses the workspace nacre-protocol by path since N0252),
  so the raw-topic read and its `shortcut:` note are gone.
- Test clock: the dashboard goldens' `view()` helper now uses a fixed clock (`fixed_now`, as the extras tests
  already did), so "Mail · now" does not drift into "988d" as the calendar moves; the Notifications goldens
  were re-recorded with it.

Motion
D9 changes no transition: static page layouts only; kit hover/press uses `kit.motion.state`; the panel slide
and tab strip belong to N0246. No `-- D16:` literal was added. `frame_cost_bench` dashboard route (release,
dev-2, Today page, 1572x1071; the bench opens the panel on Today, which this unit does not change), total_p50 /
total_p95 / total_max in ms, base then candidate: open 0.176 / 0.822 / 5.579 vs 0.171 / 0.795 / 5.543; close
0.599 / 0.687 / 0.793 vs 0.585 / 0.631 / 0.754. No change beyond noise.

Evidence (dev-2, `nacre-ci/arch-nacre:current`, rustfmt 1.10, whole shell binary built against the vendored
cache; Bead pin unchanged)
- `cargo fmt --all --check` clean; `cargo clippy -p nacre-shell --all-targets -D warnings` clean.
- `cargo test -p nacre-shell --bin nacre-shell`: 931 passed, 0 failed, 4 ignored.
- Luau analyzer (`tools/check_luau.py --check-generated`), `tools/i18n/check.py` (1705 owned messages) and
  `tools/compositor_boundary.py` pass.
- Renders of every changed state (dark and light; hover, pressed, focus; with and without data) were looked at
  next to the review's mockups: Media (art, player, empty), AI usage (rings, empty, hover/pressed refresh),
  Brain (cards, empty, hover note), Notifications (rows, empty, focus, hover dismiss), System (gauges with and
  without readings, Network card present/absent), Weather. Today's goldens are untouched (main's are kept
  and pass; an update run re-records them with a small shift of the wallpaper-strip icon only, not from this unit).
- New goldens: `dashboard-{ai-usage,brain,media,media-art-seek,media-player,notifications,system,system-gauges,
  weather}-{1,1.5}.png`.

Known gaps
- Brain notes carry no world dot or relative date, and the empty state does not name the vault path
  (`brain.recent` has no such fields; kit asks 4).
- Media empty state has no "Open Spotify" chip (no launchable player identity; kit asks 5).
- Chip labels ("Open Brain", "Clear history") sit about 2 px above their icon's centre: the kit button's label
  baseline, shared with every compact chip; left to the kit.
- The Weather fixture's three readings are collinear, so the golden line is straight; the curve is covered by
  the 24-sample unit test.
- Stand-ins awaiting kit homes (EmptyState, NoticeRow, rounded image clip): `shell/ui/components/empty-state.luau`,
  `notice-row.luau`, `rounded_clip()` in `dashboard/media/page.luau`. Delete each when the kit component lands
  (asks 1 to 3 below).

Kit and platform asks (forwarded by the lead)
1. `kit.EmptyState{glyph,title,detail?,chip?,on_chip?,width,height}`: the shared empty card (stand-in above).
2. `kit.NoticeRow{id,icon,caption,title,body,on_dismiss,width,fill}`: the D4 row, shared by control panel and
   dashboard (stand-in above); a kit home lets D10 toasts reuse it.
3. Framework: `clip_children="radius"` (clip to the box's rounded rect); the stand-in is a 32-point
   `clip_polygon` on the blurred backdrop and the 120 px art.
4. Platform: optional `world` and `modified_ms` on `BrainNote`, vault path on `BrainRecentState`.
5. Platform: a launchable player identity (`desktop_entry` or a `dashboard.media.player` setting) for an
   "Open Spotify" chip.

## D14 Welcome (N0244)

Scope (design review section 13, owner-approved): tinted hero, a named step rail instead of the five dots,
primary Next with an arrow and a quiet Skip/Back, and a key-cap grid of the "first ten minutes" shortcuts plus
the panel keys, all read from the live `shortcuts` topic. First-run behaviour, routes, deep links, Enter on Next
and Escape are unchanged. Files: `shell/welcome/{main,catalog,behavior}.luau`, `shell/welcome/assets/hero-tint.themed.svg`,
`crates/nacre-shell/src/welcome{,_tests}.rs`, `shell/i18n/{en,context}.json`, goldens `shell/tests/goldens/welcome-*.png`.

What changed:
- Tint: one themed SVG (`hero-tint.themed.svg`, a `@primaryContainer@` radial glow at 20 % / 0 % of the window,
  75 % -> 19 % at 55 % -> 0, `image_fit="fill"`) under the whole window, matching the mockup's `--wall` stops in both
  schemes (dark 0 % ~ #3d3a2a, light ~ #f1e9c4). It sits behind every page, so leaving the hero shows no pop.
- Hero: wordmark IBM Plex Sans 600 (Manrope is not a shell font: owner default, kept), tagline `onSurfaceVariant`;
  logo and sizes still follow the window height.
- Footer: Skip tour / Back is quiet text (transparent, `onSurfaceVariant` 13/500, 8 % `onSurface` hover); Next is the
  34 px `primary` pill, 13/600 label plus `arrow_forward` 16 px; the rail is `kit.Chip` per step (36 px, radius 18,
  short names Welcome / Colors / Shortcuts / Apps / Ready, `primaryContainer` when current) centred between them,
  node ids `step-<name>` kept. In a window too narrow for every name the other chips show their step number.
- Shortcuts: 4 x 2 grid (2 columns under 760 px content) of 96 px `surfaceContainer` tiles (radius `metrics.radius.tile`)
  with the topic's own title and key caps (`surfaceContainerHighest`, radius 6, 1 px `outline` 60 %, mono 13 px;
  `SUPER+SHIFT+S` -> Super Shift S, all-caps names title-cased, RETURN -> Enter, ESCAPE -> Esc). Rows are picked by
  the bind's `Nacre:<id>` action (`launcher, terminal, files, browser, close, overview, screenshot, settings`; then
  panels `ai-sidebar, dashboard, control-panel`), first bind per action; binds without Nacre titles fill the grid in
  topic order (custom setups still get keys). An "All shortcuts" chip opens Settings > Keyboard & mouse.
  `overview` (Super+Tab) replaced `workspace` from the draft: workspace has ten binds and no single cap.
- Data: `nacre-protocol` `Shortcut.action: Option<String>` (serde default, skipped when none, 64 chars), set by
  `nacre-helper desktop_extras.rs` from the `Nacre:<id>` title before translation (niri and Hyprland), passed
  through `daemon/shortcuts.rs` and `welcome.rs::rows()`. Additive; Platform should know.
- Strings: added `bead.welcome.chip-{colors,shortcuts,apps,ready}`, `shortcuts-all`, `shortcuts-panels`;
  removed `shortcuts-apps`, `shortcuts-other` (the grouped list is gone); `shortcuts-intro` reworded.

Motion (design review 15; `ui.motion` tokens, sampled markers, no Luau or relayout per frame):
- Step chip: fill mixes `surfaceContainerHigh` -> `primaryContainer` over `switch` (180 ms) out-cubic, both the old
  and the new chip, retargeting from the current value when reversed.
- Page: enters 24 px from the side it came from (`welcome.from`, set by `go`; help counts as after Ready) over
  `peek` (200 ms) out-cubic via `translate_x`; the first page of a window and deep links do not move.
- Reduced motion: both snap (`ui.animated` honours the policy).

Owner defaults used (open questions): the dashboard cap needs Super+O titled `Nacre:dashboard` in the neutral
compositor contract; that is a separate, droppable commit (contract, regenerated niri/Hyprland configs, "Open the
dashboard" in the helper's niri titles, `test_bindings` migrating Super+O like the hardware keys). Without it the
Panels row shows only the AI sidebar. No control-panel bind was added. The wordmark stays IBM Plex Sans 600.

Evidence: goldens for every page (10 updated, light hero and shortcuts added) and a temporary render test of hover,
focus, the first slide frame and 720 px states, looked at against the mockup (unit worktree `.lead/renders/`).
Fixes from that review: Next hover uses `onPrimary` 10 % (the `onSurface` tint was invisible on cream), and the
footer's child order makes Tab go Skip, steps, Next.

Known gaps: no 120 Hz nested measurement of the page slide and chip fade yet (both are sampled markers: no Luau
or relayout per frame); the mockup's page-content fade is not done (no container opacity in Bead; the slide alone
carries the change); the large soft tint shows faint 8-bit banding arcs; chip focus rings use the framework's
rounded rect, not the pill radius; press scale .97 waits for Bead.
Kit asks: `kit.KeyCap{keys}` (the local `keycap` stand-in), gradient fill on `ui.Box` (the SVG stand-in), container
opacity for page crossfades.

Status: implemented; evidence and checks in the unit report.

## D8 Nacre AI app (N0248)

**Status:** first version (Shell half of D8, option A). Platform half: N0319
(`docs/specs/0319-ai-helper-api.md`).

- **Window.** `nacre-shell --app ai` is a Bead window app packaged exactly like the Brain app:
  `shell/ai/{main,behavior}.luau` + `bundle.json`, `crates/nacre-shell/src/ai_app.rs` (adapter and
  `window_spec`, size restored from `ai-window.json`), one `app::APPS` entry and the window branch in
  `app::run`, the five Brain-style `Runtime` hooks (`with_ai_app`, subscribe, viewport, `ai.do`,
  relayout). `nacre ai [chats|projects|accounts|usage]` opens or deep-links it (command catalog
  route). `nacre/desktop/nacre-ai.desktop` runs `nacrectl ui nacre ai`, so the launcher's app row and
  the desktop menu land in the window. The terminal UI is unchanged as `nacre-ai` (the launcher's
  `tasks` action still opens it in kitty).
- **Rail.** 200 px `surfaceContainer` rail: the Nacre AI mark and name, Chats, Projects, Accounts,
  Usage, Brain, and Settings at the foot (36 px rows, 10 px radius, 18 px icons; the current page
  `surfaceContainerHigh` with a `primary` icon). Brain opens the Brain app; Settings is the sidebar's
  `ai.launcher.action settings`.
- **Provider abstraction.** No provider is named in the view or adapter: the Chats header is a
  `kit.Segment` over `ai.providers` (chat-capable entries), the new chat card says
  "New {provider} chat", and chat chips, account groups and usage ring tones all come from the
  registry. A local model later is one more N0319 `Provider` with no Shell change.
- **Pages.** Chats: the new chat card (`ai.chats.start` with the chosen provider and project) and the
  `ai.chats.list` rows in the D7 row style, grouped Running / Today / Earlier; a row resumes through
  `ai.chats.resume` when the helper marks it resumable. Projects: settings rows from `ai.projects`
  (remote ones show `host:path`), the chosen one ticked; it is the new chat's project. Accounts: rows
  from `ai.accounts` grouped by provider with the sign-in status and an "active" chip; "Check sign-in"
  calls `ai.accounts.refresh` (checks run only on request). Usage: one ring per reported quota window
  (`primary` for the chosen provider, `secondary` for others), text lines otherwise. The row model is
  `ai_app::usage_rows`, now shared with the dashboard's AI usage page (the dashboard loop was moved,
  not copied).
- **Tests.** `ai_app_tests.rs`: each page's model over `protocol/fixtures/ai-app.jsonl` and
  `dashboard-ai-usage.jsonl`, the projection with stored choices and without a daemon, the guarded
  actions; goldens `ai-app-chats-1` and `ai-app-usage-1` (1120x760, 1x, dark).

- **Checks (dev-2, nacre-ci/arch-nacre:current).** rustfmt 1.10 applied; clippy `-p nacre-shell
  --all-targets -D warnings` clean; whole `nacre-shell` binary 944 passed, 1 failed (the route-count
  constant, fixed to 125; the focused re-run passed clippy, `ai_app` + `commands::` 19/19 and
  `cargo fmt --check`); Luau analyzer 185 modules OK;
  `nacre-shell --check shell/ai/main.luau` OK; i18n coverage passed; ruff clean on the touched test.
- **Renders.** The two goldens are the evidence (looked at next to the mockup): rail, chosen page,
  provider segment, new chat card, Running/Today groups with the running halo and "Open", usage ring
  with the reset caption and text lines for a provider without windows.

Known gaps (next units):

1. Chats hosts D7's row look, not the sidebar view itself: shell/chat/main.luau is one panel file.
   Hook for the chat owner: expose the launcher list and composer as a function module
   (`require("../chat/launcher")(ui, kit, geometry)`), then the Chats page calls it and the local row
   code here is deleted (expiry: the unit after that hook lands).
2. The composer of the mockup needs a prompt on `ai.chats.start` (Platform ask) or the hosted
   `ai.chat` adapter in the window; tonight the card starts a terminal chat with no prompt.
3. The usage ring markup is still drawn twice (here and `shell/dashboard/ai-usage/page.luau`):
   extract one `shell/ui/components/usage-ring.luau` when the dashboard is not in flight
   (expiry 2026-11-28).
4. Wordmark in Manrope (the shell bundles only IBM Plex here); the running dot does not pulse yet
   (kit attention pulse, N0237 kit ask 3); no light golden (D17, N0249).
5. Motion: the window opens as a compositor window (no panel growth applies); page switches are
   instant. `nacre-ai --tui` is not a helper flag yet (Platform ask: alias it to `dashboard`).

## D13 Orient studio (N0245)

Scope: the Nacre Colors window (`shell/colors/`), Luau on Bead kit components with palette roles and tokens;
Rust (`crates/nacre-shell/src/colors.rs`) only for the data the view needs.

What changed
- Header: the wordmark (24 px mark, "Nacre Colors" 17 px/600), the pages as one `kit.Segment` (Studio |
  Compare | Accessibility, the N0239 segment that has landed in the kit), and the dark/light preview switch as
  an icon-only segment in the same track (stand-in `modes()`, kit ask 1). The rule under the header is gone.
- Studio is two columns (`1fr 300px`, 14 px apart; stacked below a 720 px canvas). Left: the wallpaper
  (`kit.ImagePick`, half as tall as it is wide), a name row (16 px/500 name, the hint caption, a
  `Wallpapers` chip), the pigments as a strand of pearls sized by image area (18 px at none to 28 px for the
  largest family, overlapping by the bar's `strand_overlap`, 2 px `surface` border; each pearl is the sample
  button) with "42 · 21 · 14 · 6 · 5 % of the image, 12 % other shades" beside it, the style chips
  (`kit.Chip`, wrapped by measured width) plus the Fine-tune chip, and the style's one-line description.
  Fine-tune opens under the chips in the left column (tint chips, the two sliders, the time-of-day and
  workspace-colour switches as `kit.ToggleRow`s, a Reset chip).
- Right column: a miniature of the bar drawn in the previewed palette from `shell/bar/tokens.luau` at 70 %
  (the workspace pill with the current pearl in primary, an occupied pearl in secondary and an empty
  pearl ringed in tertiary, joined by secondary traces; the status pill with secondary icons and the
  tertiary clock), then three role rows named by their job: Accent ("Pearls, active tiles"), Shell
  ("Status icons, traces"), Text ("Clock · readable 7.12:1", the audit's verdict that used to be a separate
  line). Row = 56 px, radius 12, `surfaceContainer`, 14 px/500 name over a 12 px caption, a 26 px swatch and
  the pin button; clicking the row arms it (2 px `primary` border, caption "Now click a color in the image").
  The hex values left the rows (they stay in the accessible name). Export and Apply (`kit.PillButton`,
  34 px) sit right-aligned under the rows with the status line above them: where the eye ends.
- The permanent wallpaper rail is gone: the `Wallpapers` chip swaps the image for a picker (search field
  over a `kit.VirtualGrid` of thumbnails, `surfaceContainer`, radius 18); choosing a wallpaper closes it
  (`colors.picker` view state, `picker` action).
- Compare keeps its layout; Accessibility keeps its layout with the vision choice as a `kit.Segment`. Both end
  with the same status + Export/Apply block, so Apply is reachable from every page.
- The old miniature desktop card no longer appears in Studio (Compare and Accessibility still use it).
- Strings: role names re-texted in place (`target-main/second/third` = Accent/Shell/Text, pin labels), new
  `role-*-detail`, `role-text-readable`, `role-text-low`, `role-armed`, `strand-coverage`, `wallpapers`,
  `sample-time`, `bar-preview`; removed `families`, `grouped-note`, `color-style`, `desktop-preview`,
  `readable`, `check-readability`, `other-shades` (shell catalog + context; the Colors strings live only there).

Primary container (the D4 follow-up, an Orient tone, not a Nacre change)
- Proposed Orient commit: `crates/orient-core/src/palette.rs`, in the accent-role loop,
  `let container = color(if self.dark { 0.35 } else { 0.88 }, container_chroma, h);` becomes
  `color(if self.dark { 0.40 } else { 0.88 }, container_chroma, h)`. It lifts every dark accent container
  (primary, secondary, tertiary) by one step; the on-container ink is derived from it, so cream text stays
  (the review measured 8.7:1 for #f6f0d2 on #4c4523). Then a `deps/orient` bump, a `shell/palette.json`
  refresh and the goldens that draw containers. Nacre stays token-based: tiles and chips already fill
  `primaryContainer`, so nothing in the shell changes.

Motion
- The Colors window is a toplevel, not a panel: no edge growth, detach or dock applies. Every state change
  uses the kit's state motion (hover/pressed fills, 120 ms `kit.motion.state`) and the existing focus ring;
  the picker swap and page changes are instant rebuilds (no transition was there before; none was added).
  Nothing animates per frame, so there is no PANEL_SLIDE capture for this unit; reduced motion is unaffected.

Evidence
- dev-2, `nacre-ci/arch-nacre:current`, `--network none` with the read-only vendor cache: `cargo fmt --all` and
  `--check` (rustfmt 1.10), `cargo clippy -p nacre-shell --all-targets -D warnings`, the whole `nacre-shell` test
  binary (937 passed, 0 failed, 4 ignored), `tools/check_luau.py --check-generated`, `tools/i18n/check.py`
  (1710 messages). Goldens re-recorded and looked at: `colors-{studio-1,studio-1.5,studio-tall-1,fine-1,compare-1,
  access-1,narrow-1,unavailable-1}`.
- Real-renderer PNGs, dark and an Orient light scheme (studio, fine-tune, compare, accessibility, narrow, picker
  open; hover and pressed on Apply, a role row, a segment option, a pearl, a style chip; keyboard focus on a role
  row, Apply and the preview switch) next to the mockup: kept with a README in the unit worktree's
  `.lead/renders/` (not committed).

Kit asks (forwarded by the lead)
1. `kit.Segment` option `glyph` (icon-only option, `label` stays the accessible name); stand-in `modes()` in
   `shell/colors/main.luau`, delete when it lands.
2. `kit.ImagePick` forwards `radius` to its image (the photo keeps square corners until then).
3. A hover ring for buttons filled with a data colour (`hover_border_color`): the strand's pearls have no hover
   change beyond the focus ring.
4. `kit.Chip` measures its label at weight 400 and draws 500, so long labels wrap.

Known gaps
- Photo corners (ask 2) and pearl hover (ask 3) as above.
- `shell/widgets/segment.luau` (N0239's stand-in) is now duplicated by `kit.Segment` at the pinned Bead; Colors
  uses the kit one. Settings and the dashboard still import the stand-in (other units' files): delete it there.
- Without a daemon the strand shows the active palette's five roles at one size; the `surface` pearl is drawn on
  the surface and reads as a gap.
- The bar miniature is a schematic of the real bar (pearls, traces, status icons, clock) from the bar's tokens, not
  the live bar widgets: the window's theme is the applied palette, the miniature draws the previewed one.
- Light goldens: D17 (N0249); this unit's light evidence is the renders above.
