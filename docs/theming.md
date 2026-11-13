# Theming

Nacre's colors come from your wallpaper. Orient reads the image and builds a
palette; the bar, frame, panels, launcher and the apps you theme all use it.

## 1. Pick a wallpaper

Put images and videos in `~/Pictures/Wallpapers`; subfolders are fine. Press
Super+W for the wallpaper picker. Switch between **Static** and **Dynamic**
(animated) scenes, choose the **Carousel**, **Spotlight** or **Hexagons**
layout, or search by name. Choosing a wallpaper applies it together with its
palette.

**Settings → Wallpaper** has **Wallpaper rotation** (a new scene at an interval,
optionally shuffled) and **Wallpaper motion** (Full, Still on battery or Always
still). **Covered scenes** pauses motion while windows cover the wallpaper.

## 2. Shape the palette

| Choice | Where | Options |
| --- | --- | --- |
| Style | launcher **Style → Orient personality**, **Settings → Colors → Palette style** | see below |
| Light or dark | control center, launcher **Style → Light and dark** | Light, Dark, or Auto (follows the wallpaper) |
| Background tint | launcher **Style → Background tint** | Accent tint (a restrained tint of the main accent) or Wallpaper shadows (dark or bright tones from the image) |
| Contrast | **Settings → Colors → Contrast** | stronger separation between surfaces and text |

Styles: **Natural** keeps the wallpaper's original balance. **Pigment** puts more
weight on the subject and small details. **Harmony** uses related hues for a
quieter theme. **Pop** turns a contrasting detail into the main accent. **Mist**
is soft, **Vivid** is stronger, **Pearl** is Nacre's mother-of-pearl look, and
**Tide** shifts with the time of day.

**Settings → Colors → Color source** follows the wallpaper or keeps a fixed
palette. For hands-on work, open **Nacre Colors** (Settings → Colors → Open
Colors studio, or launcher **Style → Nacre Colors**): explore any wallpaper,
compare styles, lock accent colors, check readability, then apply or export.

## 3. Theme your apps

**Settings → Colors → App theming** lists every target with its own switch: GTK 3,
GTK 4, Qt 5 & Qt 6, KDE applications, Dolphin, Thunar, Kitty, Rofi, Lock screen,
Login screen, Chrome/Brave/Chromium, Discord (Vencord) and Spotify (Spicetify).
While a target is on, each palette change rewrites its theme. Some apps need a
one-time setup, such as `adw-gtk-theme` for GTK file choosers, loading the
browser theme once, or enabling the Vencord and Spicetify themes. See
[App theming](app-theming.md) for the exact steps.

## 4. Save and share palettes

Launcher → **Software → Themes** previews and applies saved palettes, imports a
Nacre theme file, and **Create a theme** opens Nacre Colors to fine-tune and save
one. **Settings → Colors → Theme bundles** previews a theme's dark and light
palettes with its original wallpapers before you apply it. Apply another theme
before removing the one in use.

## Related settings

- **Settings → Appearance → Fonts**: interface font, monospace font (Kitty uses
  it too) and text size.
- **Settings → Appearance → Interface gradients**: turn off for solid colors
  instead of blends on controls, pearls and desktop borders.

## Use Orient on its own

Nacre installs the `orient` command, which works without the desktop:

```sh
orient palette ~/Pictures/Wallpapers/forest.jpg --mode dark --personality natural > palette.json
orient export palette.json ./themes      # every bundled template; use a fresh directory
orient render palette.json my-template.css.in > colors.css
```

Templates use role tokens such as `#{{surface}}` and `#{{onSurface}}`, and
Orient also renders Matugen templates. The
[Orient README](https://github.com/nacre-desktop/orient#readme) lists the
bundled targets, and the
[palette JSON format](https://github.com/nacre-desktop/orient/blob/main/docs/palette-json.md)
lists every role.
