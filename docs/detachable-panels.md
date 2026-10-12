# Panels as windows

Hold **Super** and drag anywhere inside an open edge panel to lift it into the
desktop. The full live panel follows your pointer. Move at least 16 logical pixels
to detach; a shorter pull returns smoothly. Super reserves the gesture, so it does
not need the longer distance used by an ordinary pull grip. **Super+Ctrl+D**
lifts the open panel in place, retaining its dimensions and position.

Floating panels keep their original content, with no added title strip. Hyprland
provides the same inactive and focused borders as other windows. Use ordinary
Super-drag to move, Super+right-drag to resize, and the normal tiling/pinning keys.
Bring either a lifted panel or a floating window near its matching edge. The
same pearly film bridges the panel and frame on every pull; release while it is
bright to reattach. The target follows the panel's edge rather than your pointer,
so grabbing near the middle works too. Reattached panels return to hover mode and close when the
pointer leaves; panels deliberately opened by click or shortcut still stay open.
A reattached launcher retains keyboard focus for its search field while using
the same hover dismissal, including when Super is still held at the drop. **Super+Ctrl+D** docks the focused panel when no edge panel is open. **Ctrl+W**
closes its window. Outside clicks do not close floating panels; Escape backs out
of a search or dialog.

Launcher and dashboard windows have independent navigation, searches and scrolling.
Their application catalogue, favorites, media and system readings stay shared.
A launcher stays open after launching by default. Controls use one shared window.

With optional AI enabled, detaching moves the current conversation into its window.
The sidebar gets a fresh conversation. Selecting a conversation already in a window
focuses that window. Docking preserves the sidebar's other conversation in Chats;
closing a view never stops an assistant response or deletes a draft.

Disable panel dragging in Settings to retain ordinary window movement everywhere.
The motion and geometry publisher run only during interaction or panel changes;
there is no permanent pointer polling. Reduced motion disables decorative motion.
Reload recovery retains floating placement, navigation and conversation ownership.
Drafts and window layouts remain private. Hidden/minimized panels do not keep
visualizers or resource sampling running.

Maximized dashboards enlarge their wallpaper area while keeping calendar and
system cards bounded. App tiles retain their spacing as a launcher grows. Detached
wallpaper pickers fit their current window, and grow within the monitor's usable
area when switching to Spotlight or Hexagons. No decorative frame bumps are drawn;
the flat top-center and side-center hover targets remain available.
