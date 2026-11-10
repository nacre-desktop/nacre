# N0161: Overview title follows the highlighted window

Niri reports pointer hover through Nacre's optional overview hover stream. In
overview, keyboard selection is exposed through `WorkspaceActiveWindowChanged`
and `WorkspaceActivated`; native window focus may be null. The daemon reducer
publishes the selected workspace and its active window. The title renderer
preferred hover unconditionally, so keyboard navigation could move the
highlight while leaving the title on the pointer's old tile.

While overview is open, the focused workspace's `activeWindowId` is the
highlighted tile and takes title priority over the hover stream. Niri can emit a
new hover hit when keyboard navigation moves tiles under a stationary pointer,
so that hover value may be stale even though the pointer did not move. Pointer
movement updates the selected window as well as hover, so the selected window
still follows the last input. Hover remains a fallback when selected-window
data is unavailable. This uses Niri's existing event stream and requires no
compositor package or relogin.

The title projection contract keeps a stale hover beside a keyboard selection,
then verifies the pointer-selected window takes over. It also retains the
existing hover and focus fallbacks when selection data is absent.

The private desktop proof uses the real Bead bar and daemon with three windows
in nested Niri. Super+Left/Right is forwarded through a private headless Labwc
parent with a standard evdev keymap; dynamically assigned virtual keycodes do
not exercise Niri's Winit backend. Screenshots must show title and highlight
moving together while the pointer is parked, followed by a pointer move.

The concurrent Bead-only cutover removed the QML shell and old hover probe.
This unit preserves that removal. No helper reducer, compositor protocol,
service, or temporary product duplicate is added.

## Evidence

Pending final dev-1 desktop CI and private nested desktop capture.
