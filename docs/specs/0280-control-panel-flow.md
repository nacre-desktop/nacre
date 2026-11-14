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
  `cc.detail=<section>`). Hover on an icon stays a peek.
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
page can render as a strip. (a) is one line in `pearls.rs` (also remember `cc.detail`, or stop
remembering `cc.page`); it is outside this unit's files and is asked of the lead.

## Mock renders

Rendered through the shell's own test harness (`control_center_pages_match_goldens`, 488 x 704 at
scale 1) after the change:

- Overview: `assets/0280/overview.png`
- Bluetooth detail page: `assets/0280/bluetooth.png`

## Tests (phase 2)

(a) lip hover (a plain `PanelVisibility(true)`) always opens the overview; (b) a status-icon click
model opens its section; (c) overview and detail pages have the same panel height; (d) the view
declares no close-on-leave of its own and a pinned panel stays open across a pointer leave; (e) the
back-arrow-only state cannot occur: `cc.page=detail` with an empty or unknown `cc.detail` renders the
overview.
