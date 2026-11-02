# B0217 — shared Bead scroll speed

Bead retains one shared 100 logical px wheel notch across launcher, Widgets, Settings, chat and notifications.
Touchpad finger events remain immediate and pixel-for-pixel, including the first event.
The per-SceneSource wheel override and product-store multiplier are removed; surfaces cannot tune wheel speed independently.
Nacre keeps separate compositor settings: touchpad scroll defaults to 0.35 (range 0.25–2.0), and mouse-wheel scroll defaults to 1.5 (range 0.5–3.0). Both apply to the active generated niri session configuration. Bead's shared wheel distance and 1:1 touchpad input remain independent.

| Gesture | Before | After |
| --- | ---: | ---: |
| Touchpad event of N logical px in Bead | N px | N px |
| Nacre touchpad base event of 100 px, factor 0.25 → 0.35 | 25 px | 35 px |
| Wheel `value120=120` / discrete notch | 100 px | 100 px |
| High-resolution wheel `value120=30` | 25 px | 25 px |

High-resolution wheel values retain fractional detents; the legacy 15-unit wheel fallback maps to 100 px per notch.
Wheel glide remains over 125 ms. Touchpad inertia follows immediate finger movement.
Nacre Settings exposes separate touchpad and mouse-wheel rows with independent ranges and labels. Bead keeps its fixed gesture mapping.

Product scene tests send 23.5 px then 100 px and verify 123.5 px in launcher, Widgets, Settings, chat and notification history.
Wayland conversion tests cover finger precedence, discrete notches, full and fractional `value120`, and legacy wheel input.
Full Bead CI passed with exit 0 in the capped Ubuntu 24.04 container on implementation commit `45b547f`;
the following documentation-only clarification is `66e82f0`. No local builds or tests were run.
Live/nested comparison remains unavailable because the desktop test host timed out.
