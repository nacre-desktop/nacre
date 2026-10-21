# N0021 — App theming targets (GTK3 file chooser, Chromium browsers, Discord, Spotify)

- Status: In progress (Codex worker, lead review)
- Date: 2026-10-21
- Owner request 2026-10-21: make as many apps as possible follow the Nacre palette, with Settings toggles.
  Owner choices: Discord via Vencord on the official client, Spotify via Spicetify, GTK3 via adw-gtk3.

Goal: extend the installed palette publisher (`nacre/shell-tools/theme-targets.json`, dispatch in
`crates/nacre-helper/src/classic_state.rs`) so palette changes reach more apps, each with its own
Settings → Colors → App theming toggle.

## Targets

1. **GTK3 theme name (existing `gtk3`/`gtk4` targets).** When `adw-gtk3` / `adw-gtk3-dark` is installed
   (`/usr/share/themes/adw-gtk3*` or `~/.local/share/themes/adw-gtk3*`), the dark/light switch sets
   `gtk-theme-name` to `adw-gtk3-dark` / `adw-gtk3`, otherwise `Adwaita-dark` / `Adwaita` as today. Also set
   `org.gnome.desktop.interface gtk-theme` via `gsettings` (when present, bounded timeout), but only when the
   current value is one of `Adwaita`, `Adwaita-dark`, `adw-gtk3`, `adw-gtk3-dark` or empty. A custom theme the
   user picked is never replaced. The existing `@define-color` output is unchanged. adw-gtk3 honours these
   names, so the GTK3 file chooser (and browser file dialogs that use it) follows the palette.
2. **`chromium`: Chrome, Brave, Chromium.** Render an unpacked theme extension into
   `~/.local/share/nacre/themes/chromium/manifest.json` (manifest_version 3, `"theme": {"colors": …}`: frame,
   frame_inactive, toolbar, tab_text, tab_background_text, bookmark_text, ntp_background, ntp_text,
   ntp_link, omnibox_background, omnibox_text, button_background, toolbar_button_icon; `theme.properties`
   ntp_logo_alternate as appropriate). `version` is derived from the palette (for example `1.<hash16>.<hash16>`)
   so browsers reload a changed theme. Delete a stale `Cached Theme.pak` in that directory on each write.
   Presence: any of `google-chrome-stable`, `google-chrome`, `brave`, `brave-browser`, `brave-origin`,
   `chromium` on PATH. The browser loads it once through Load unpacked; Settings description says so.
3. **`discord`: Vencord.** Write `~/.config/Vencord/themes/nacre.theme.css` (BetterDiscord-style meta
   header, `@name Nacre`) that maps the palette onto Discord's current CSS variables (backgrounds, text,
   interactive, brand/accent, scrollbars, input, modifier states; cover both the legacy `--background-primary`
   family and the newer `--background-base-*` / `--background-surface-*` / `--bg-*` family). Only write when
   `~/.config/Vencord` exists (status otherwise `tool-missing`). Enabling the theme in Vencord is a one-time
   step the user does.
4. **`spotify`: Spicetify.** Write `~/.config/spicetify/Themes/Nacre/color.ini` (section `[Nacre]`, standard
   Spicetify keys: text, subtext, main, sidebar, player, card, shadow, selected-row, button, button-active,
   button-disabled, tab-active, notification, notification-error, misc) and a minimal `user.css`. Presence:
   `spicetify` on PATH. When the Spicetify config already selects `current_theme = Nacre`, run
   `spicetify refresh` detached with a bounded timeout after writing. Never run `spicetify apply`,
   `backup` or anything that patches Spotify; never chmod `/opt/spotify`.

Each new target: row in `theme-targets.json` with `tool`, a template `.in` file using the existing `{{role}}`
syntax, dispatch in `set_theme_target` and the palette-apply path inside the existing transaction and
rollback snapshot, settings descriptor `colors/app-theming.targets.<id>` (default `true`, adapter
`app-theme`), `appThemes` keys in `settings_validation.rs` and `owned_settings.rs`, `en.json` and
`context.json` strings, and the QML publisher fixture. Disabled targets leave existing files alone (as tested
today). All writes go through `atomic_write`, files 0600 and dirs 0700 unless the app needs otherwise.

## Non-goals

Installing or patching anything (Vencord installer, `spicetify apply`, packages, browser policies,
`/etc` or `/opt`), editing browser `Preferences`, `theme-targets-original-v1.json` (a frozen historical
fixture: new rows go into the live registry and new tests, not the frozen reference), Firefox, and any live
action.

## Acceptance

- Focused Rust tests for each target: rendering, presence gating, disabled leaves files alone, rollback on
  failure, gsettings guard (fake `gsettings` on PATH that records calls), and no `spicetify` apply/backup
  calls (fake binary).
- `cargo test -p nacre-helper --test palette_pipeline` and the settings validation/frozen-values tests pass;
  frozen parity references unchanged.
- Settings prototype QML/Python tests pass with the new rows.
- The repo CI `check.yml` job replicated in the Arch CI container on dev-1 passes.
