# First boot

You installed Nacre, logged out and picked the Nacre session. This page is what to
do in the first ten minutes.

<!-- media: media/hero.png — fresh session: bar at the top, workspace pearls, Welcome window open -->

## What is on screen

- **The bar** along the top. Left: workspace pearls, one per workspace. Middle:
  the title of the focused window. Right: status icons, the clock and a power
  button.
- **The frame**: a thin border around the desktop. Hover its left or right edge
  for a moment and a panel slides in. Hover the top middle for the dashboard.
- **Welcome** opens once and walks you through the main controls. Close it when
  you are done; `nacre-welcome` opens it again.

## Five keys

| Key | What it does |
| --- | --- |
| Super+Space | Launcher. Type an app name, a file name, `=2*21`, or `>` for actions. |
| Super+Enter | Terminal (Kitty by default). |
| Super+W | Wallpaper picker. Your colors follow the wallpaper you pick. |
| Super+I | Settings. Type to search a preference. |
| Super+K | The full shortcut list for your compositor. |

Super is usually the Windows key. The complete table is in [Keybindings](keybindings.md).

## Pick a wallpaper

1. Copy a few images into `~/Pictures/Wallpapers`. Nacre watches that folder,
   subfolders included.
2. Press Super+W, choose one, and look at the preview. **Natural** keeps the
   image's balance; **Pop** brings out contrasting details. Dark and light are
   separate choices.
3. Apply. The bar, frame, launcher and terminal recolor at once.

More in [Theming from a wallpaper](theming.md).

## Move around

- Super+1 to Super+9 (and Super+0 for 10) switch workspaces. Super+Shift+number
  moves the focused window there.
- Super+Tab opens the overview (Bird's Eye on niri): every workspace and window
  on one screen.
- Super+Left/Right/Up/Down moves focus; add Shift to move the window instead.
- Super+Q closes a window. Super+F toggles fullscreen. Super+T floats it.

## The panels

- **Control center** (right edge, or click any status icon): Wi-Fi, Bluetooth,
  sound, brightness, power profile, Do Not Disturb, night light. Right-click the
  sound, Wi-Fi or Bluetooth icon for the quick toggle. Scroll on the sound icon
  for volume.
- **Dashboard** (top lip, or Super+O): media, weather, system load. Press Ctrl+,
  inside it to add pages.
- **Notifications**: the bell in the bar opens the history.
- Hold Super and drag inside an open panel to lift it off its edge and use it as
  a window. Drag it back to dock. See [Detachable panels](detachable-panels.md).

## Lock, log out, power

Super+L locks. Super+Escape (or Super+X, or the bar's power button) opens the
power page: lock, log out, restart, power off. The last three ask once.

## If something looks wrong

- Super+Z hides and shows the whole shell. Press it again if the bar vanished.
- `nacre doctor --report` writes a support report. Nothing is uploaded.
- [Troubleshooting](troubleshooting.md) covers a black screen, no bar, no sound
  and how to get back to a TTY.
