# App theming

Open **Settings → Colors → App theming** to choose which apps follow your Nacre
palette. Enable a target, then apply a palette to create its theme files. These
steps work with Nacre installations and dotfiles setups that use its palette publisher.
The app-theming feature does not install or patch app clients; you perform the setup below.

| Target | What changes | When it updates |
| --- | --- | --- |
| GTK 3 / GTK 4 | GTK colors, dark/light preference, and supported file choosers | Live; an app that caches its theme may need reopening |
| Kitty | Terminal colors | Live |
| Qt / KDE | Supported Qt/KDE widgets and Dolphin colors | Live where supported; some apps need reopening |
| Discord (Vencord) | Surfaces, text, accents and controls | Live after enabling the theme |
| Spotify (Spicetify) | Encore surfaces, text, accents and controls | Live after refresh completes; the theme checks for CSS changes every 400 ms |
| Chrome, Brave, Chromium | Browser frame, toolbar, tabs, address bar and new-tab colors | On browser restart; manual extension reload also works |

## GTK and file choosers

Install the [adw-gtk-theme package](https://archlinux.org/packages/extra/any/adw-gtk-theme/)
on Arch/CachyOS. It provides `adw-gtk3` and `adw-gtk3-dark`, which Nacre selects
automatically for light and dark palettes. Without it, Nacre falls back to Adwaita;
GTK 3 file choosers may not use the generated palette colors.

## Discord

Install Vencord on the official Discord client using the
[Vencord installer](https://vencord.dev/download/). Enable Discord theming in
Nacre and apply a palette, then enable **Nacre** in **Vencord → Themes**.
Nacre writes `~/.config/Vencord/themes/nacre.theme.css`.

Discord client modifications are against [Discord's Terms of Service](https://discord.com/terms).

## Spotify

Install `spicetify-cli` and open Spotify once to create its configuration. For a
native Spotify installation at `/opt/spotify`, run these one-time setup commands:

```sh
sudo chmod a+wr /opt/spotify && sudo chmod -R a+wr /opt/spotify/Apps
spicetify backup apply
```

Enable Spotify theming in Nacre and apply a palette so the **Nacre** theme exists,
then run:

```sh
spicetify config current_theme Nacre color_scheme Nacre inject_theme_js 1 && spicetify apply
```

Nacre writes `color.ini`, `user.css` and `theme.js` under
`~/.config/spicetify/Themes/Nacre/`. Later palette changes trigger a detached
`spicetify refresh`; the running client reloads the CSS without restarting.
Nacre does not run the setup, permission, backup or apply commands for you.
For other Spotify installation layouts, consult the
[Spicetify setup documentation](https://spicetify.app/docs/getting-started/).

## Chrome, Brave and Chromium

Enable Chromium theming in Nacre and apply a palette. Print the generated folder's
absolute path (`~` means your Home directory):

```sh
printf '%s\n' "$HOME/.local/share/nacre/themes/chromium"
```

Choose either setup method:

1. Open `brave://extensions` or `chrome://extensions`, enable **Developer mode**,
   choose **Load unpacked**, and select the folder printed above.
2. If your browser launcher supports a flags file, print the flag with an absolute path:

   ```sh
   printf -- '--load-extension=%s\n' "$HOME/.local/share/nacre/themes/chromium"
   ```

   Add the printed line to the flags file used by your browser, such as
   `~/.config/brave-origin-flags.conf`, `~/.config/chrome-flags.conf` or
   `~/.config/chromium-flags.conf`. If you already use `--load-extension`, append
   the theme path to its comma-separated list. Use an absolute path, without `~`.

Restart the browser to pick up later palette changes, or reload the unpacked
theme manually on the extensions page. Website content is not themed.
Nacre never edits browser flags files or profiles.

## Turn a target off

Turn off its switch in **Settings → Colors → App theming**. Future palette changes
leave that target's generated files and current colors alone. This does not uninstall
a theme or restore the app's defaults: disable **Nacre** in Vencord, select another
Spicetify theme, or remove the browser theme/flag yourself if you want to stop using it.
