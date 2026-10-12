# Detachable panel design and research

This feature uses existing, owned Nacre components in normal Quickshell windows.
It does not embed terminal windows or introduce another desktop/settings owner.

Primary references:
- Quickshell FloatingWindow: https://quickshell.org/docs/v0.3.2/types/Quickshell/FloatingWindow/
- Qt native movement and Wayland positioning: https://doc.qt.io/qt-6/qwindow.html#startSystemMove
- Established floating/docking behavior: https://doc.qt.io/qt-6/qdockwidget.html
- Hyprland movement/resize dispatchers: https://wiki.hypr.land/configuring/core/dispatchers/
- Quickshell implementation confirms window-wide app IDs rather than a per-window
  FloatingWindow.appId: https://github.com/outfoxxed/quickshell/blob/master/src/window/floatingwindow.hpp

Nacre targets Hyprland, so initial placement and intentional docking use its real
client geometry. Each window has a unique, human-readable Nacre Pearl title; only
owned titles/processes are manipulated. A title rule floats them by default; tiling
is a user choice. Never take an arbitrary focused window for placement or pinning.

Super+left-drag is routed by an owned Lua binding. It passes the press and release
only inside the shell's published popup rectangles, with a mapped-layer/PID guard.
Elsewhere it dispatches normal Hyprland window movement. Geometry is published on
panel changes; no subprocess or IPC executes inside the pointer callback.

A Qt DragHandler reserves Super and moves the original live item within its existing
layer surface. The sensor is a stationary sibling, not a child of the moving panel;
its press geometry is frozen so translations never feed back through parent movement.
Opening motion finishes before capturing body dimensions. At handoff the old view is
suppressed rather than replaying its closing animation at the edge. Its footprint is excluded from the joined frame while lifted. On
release, an ordinary window is created with the same body dimensions and exact
logical coordinates; its 40-pixel header extends above the body. The original stays
painted until placement completes and the destination has had two frame intervals
to render. There is no screenshot ghost or second miniature panel. This respects
the distinction between Wayland layer-shell and xdg-shell surfaces: their protocol
roles cannot be changed during a mouse grab. A finite fallback releases the lift if
placement fails. A short pull animates back without opening a window.

View state is independent of shared providers. Launcher contexts share catalogues
and system snapshots, with local query results. Provider visibility combines edge
and visible detached views, retaining one Cava/resource/control sampler. AI view
leases share one model per conversation; named session namespaces preserve the
existing default server and allow a fresh sidebar without restarting a busy worker.
Drafts/accounts/auth remain private. Closing a view parks its session; it does not
interrupt an assistant. Same-chat selections focus the existing owner.

Acceptance: real Super-drag/dock and short/canceled pulls on all four edges; two
independent launchers and dashboards; same-chat focus; retained drafts and busy
workers; normal Escape/outside-click behavior; tiling/resizing/pinning; provider
idleness; reduced motion; compositor placement and isolated geometry tests including
negative monitor origins and mixed scales. Physical multi-monitor coverage must be
reported honestly. Publish upstream first, then update the personal consumer pin.
