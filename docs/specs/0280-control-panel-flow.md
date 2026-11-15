# 0280 Control panel: size, flow and hover-close

Owner asks (2026-11-14): the panel sometimes comes up as a thin strip with only a back arrow; opening
Bluetooth shrinks the panel and it then closes by itself because the pointer is no longer on it; the lip
must always open the overview; a status icon click opens its own section. Everything else (status
colours, title trace, AI panel, Media page, edge swell, slide direction) stays.

## Size and placement: one fixed, full-height dock

The control panel is a docked side panel on the right, 424 px wide (`tiles.luau` layout width, the
shared scale), the full height of the frame (`cc.max_height`, what the live frame leaves for it), on
every page. Content starts at the top; what does not fit scrolls inside. The panel never resizes when
moving between the overview and a detail page.

Why this and not "size to content" or "the whole screen":

- Its hit region is its body. Bead takes the panel's `body` node rectangle as the surface input rect
  and covers everything else with the click-outside catcher. A body that shrinks under the pointer hands
  the pointer to the catcher, which is a pointer leave for the panel, and the hover-close timer fires.
  That is the Bluetooth bug. A constant body makes it impossible, with no extra guards.
- Detail pages are lists (networks, devices, notifications, streams): they want height, and the
  overview already uses most of the frame. One size for all pages means no jump, no re-layout of the
  pointer target under the hand, and a stable place for the back arrow.
- It matches the QML `NacreControlSurface` sidebar (parity rule) and the owner's instinct ("might as well
  be the whole screen"). Full screen width was rejected: the panel is a glance-and-tap surface used
  while the window underneath stays visible.

The transparent full-height surface and the `body="control-center"` indirection from the content-sized
round (265e4a9ad) go away: the panel node is the body.

## The flow

- Right lip hover: opens the panel on the overview, always. A closed panel forgets its page (reset on
  `PanelVisibility(false)`), so there is one rule, not a per-opener special case.
- Status icon click (Wi-Fi, Bluetooth, sound, battery/power, updates, notifications): opens the panel
  and selects that section (`CompanionSelect` then `CompanionModel` with `cc.page=detail`,
  `cc.detail=<section>`). Hover on an icon stays a peek. A second click on the icon of the section
  that is already on screen closes the panel; a click on another icon switches to that section. The
  bar cannot see the dock's page, so the dock judges the repeat (`reload.rs`, the dock's model arm).
  The same rule toggles the `nacre controls <page>` IPC route. An unknown detail id shows the overview
  but keeps `cc.page=detail`, so Escape there goes "back" to the overview first and closes on the second press.
- Overview tile chevron, the volume end button, the battery header, the Notifications heading: open
  the detail page in place (`detail` action). The panel does not move or resize.
- Back arrow or Escape on a detail page: back to the overview. Escape on the overview: close.
- Close: Escape, click outside (catcher), lip click (toggle), or pointer leave after the grace
  period (hover-opened only, below). Closing resets to the overview.

## Hover-close policy

A panel opened by lip hover closes only when the pointer has left both the panel body and the lip
for 450 ms. Any click or key press inside the panel pins it: it then closes only by Escape, lip click
or click outside. Opening a detail page is a click, so it pins; it never closes or shrinks the panel.

450 ms: Bead's current `EXIT_MS` is 130 ms, tuned for a small bar popup. A 424 x frame-high dock has a
long inner edge; reaching for a tile near it overshoots by a few px for 100-300 ms routinely. 450 ms
absorbs that and stays under the ~500 ms where a panel starts to feel stuck open.

Where it lives: the timer and the lip/body hover state are Bead's edge controller
(`bead-wayland/src/edges.rs`, `settle_edge_exit`), shared by every hover-opened panel. Nacre cannot
reach `edge.hover_opened` from the view. Bead ask (sent to the lead): make `EXIT_MS` 450 ms for
edge-docked panels (or a `PanelSpec` field), and clear `hover_opened` on any pointer press or key event
on a companion surface (the pin). Until it lands, the fixed-height body alone removes the only
observed cause of the self-close (the shrink); the remaining 130 ms leave-close only triggers when the
pointer really leaves the panel.

## Root cause of the back-arrow strip

`main.luau`'s `detail()` has a fallback for a `cc.detail` id that is neither a docked page, nor
notifications, nor updates: it builds a header (back arrow, empty title) and then adds rows only if a
tile with that id has a toggle or choices. For an empty or unknown id nothing follows the header, and
the content-sized panel became exactly one header tall: the strip. Three paths produced that state:
(a) `pearls.rs` saves `cc.page` but not `cc.detail`, so a remembered session restores `page=detail,
detail=""`; (b) the `control` IPC route forwards any page name as a detail id; (c) any model event that
sets `cc.page=detail` without `cc.detail`. 8126115db reset the stores on close, which covers (c) only
after a close. The new design removes the state at its source: `detail()` renders an unknown or empty
id as the overview (the fallback branch is deleted), and the panel is frame-high on every page, so no
page can render as a strip. (a) `pearls.rs` no longer remembers `cc.page`: a restored dock always opens on the overview
(remembering `cc.detail` instead had an edge: restored while open, the first same-icon click closed it).

## Renders

The SVG mocks were replaced by real-renderer goldens (`control_center_pages_match_goldens`, 488 x 704):
`shell/tests/goldens/control-overview-1.png`, `control-docked-bluetooth-1.png` and the `-1.5` variants.

## Tests (phase 2)

(a) lip hover (a plain `PanelVisibility(true)`) always opens the overview; (b) a status-icon click
model opens its section; (c) overview and detail pages have the same panel height; (d) the view
declares no close-on-leave of its own and a pinned panel stays open across a pointer leave; (e) the
back-arrow-only state cannot occur: `cc.page=detail` with an empty or unknown `cc.detail` renders the
overview.

## Status (2026-11-15, evidence pass for N0205)

Implemented; waits for the owner's live check. Last full dev-1 run on main: whole nacre-shell binary 939
passed, 0 failed (recorded in the v0.1 checklist, N0203 row). Not re-run for this docs commit.

Evidence per item:

- Fixed 424 px, frame-high dock on every page: 265e4a9ad, then ac5f1ef40 and 94d8be535 (frame height on
  every page, popup keeps content sizing). Test `the_dock_keeps_one_height_and_never_renders_a_bare_back_row`
  (tests c and e: same height for home, dnd, updates, empty and unknown detail; popup stays smaller).
  Goldens `control-overview-1.png` (full panel, 640 px, overview with tiles, chips and notifications) and
  `control-docked-bluetooth-1.png` (the Bluetooth page in the same dock) were read and show this.
- Lip hover always opens the overview: 8126115db (reset on close), c8e970b4f. Tests `hover_opened_dock_shows_the_full_overview`
  and `a_closed_dock_reopens_on_the_overview_after_a_status_click` (test a).
- Status icon click opens its own section, a second click closes, another icon switches: 7f2ebe3a9 (the dock
  judges the repeat in `reload.rs`), 16c413295 (Updates section). Tests `status_pill_click_routes_switch_escape_and_reopen`
  (service_views_tests, the bar emits `CompanionSelect` then `CompanionModel` per icon; test b) and
  `a_pointer_leave_never_closes_the_dock_by_itself` (switch keeps it open, repeat closes it).
- Overview chevrons and back arrow open and leave a page in place: `docked_pages_keep_the_surface_route_actions_and_return_home`.
- Hover-close 450 ms and the press pin: Bead 56a46431 (`DOCK_EXIT_MS` 450, `edge_panel_pin`), on main via the pin
  dc014a640 (now 285aede6). Guarded in Bead by `docked_panels_get_a_longer_exit_grace_and_reentry_cancels_the_close`
  and `pinning_clears_hover_ownership_and_its_pending_exit`; Nacre's side (test d) is
  `a_pointer_leave_never_closes_the_dock_by_itself` (no view-side close-on-leave, no pending hover deadline).
- Back-arrow strip root cause removed: `detail()` fallback deleted, `pearls.rs` no longer saves `cc.page` (7f2ebe3a9).
- Updates page lists pending packages: 62864b71b, 76b605a3a; test `the_updates_page_lists_packages_and_matches_goldens`;
  golden `control-updates-packages-1.png` (read: 8 packages old to new, Update now, Refresh, System page link).
- Docked tab rides the panel: Nacre b5d9ccf7b (`frame-lips-docked.png`), Bead B0279, B0287, B0289, B0292 (pins 2a1ebd2ce to 1a4d17df6).

Not done or not automated: no test drives the real 450 ms timer against a live compositor (Bead unit tests only);
the real hover-and-click flow, the tab look and 120 Hz smoothness (N0207) are live-only. The "Bead ask" above
is satisfied (Bead 56a46431), so the "until it lands" sentence is history. Parity of the dock against the old
QML sidebar is not re-captured here.

### Owner live check

1. Move the pointer to the right edge: the overview opens in full (424 px wide, full height, Wi-Fi/Bluetooth tiles, notifications).
2. Click each top-right status icon (Wi-Fi, Bluetooth, sound, battery, updates, notifications): each opens its own section, with the panel size unchanged.
3. With a section open, click a different icon: it switches. Click the icon of the open section: the panel closes.
4. Open the panel by hover only and move the pointer off: it closes about 0.5 s after leaving both dock and lip, and not sooner. Move back within that time: it stays.
5. Hover-open, then click inside (a tile chevron): the panel pins and stays open after the pointer leaves; Escape goes back to the overview, Escape again closes.
6. Open Bluetooth and move the pointer around the list: the panel never shrinks or closes by itself.
7. Updates icon: the page lists the pending packages with old and new versions; Refresh and Update now respond.
8. The lip tab sits on the panel's inner edge while it is open and lights on hover; after close, the next lip hover opens the overview again, not the last section.
