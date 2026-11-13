# Troubleshooting

Symptom first, then what to run. The longer mechanics are in [Recovery](recovery.md).

## First: get a support report

```sh
nacre doctor --report      # Markdown
nacre doctor --json        # redacted JSON
```

Nothing is uploaded. Read it before you share it; free-text log lines can
contain paths.

## The bar or panels are gone

- Press Super+Z. It hides and shows the whole shell.
- Check the shell service:

  ```sh
  systemctl --user is-active nacre-shell.service
  journalctl --user -u nacre-shell.service --no-pager -n 50
  ```

- If the shell keeps restarting, the fallback service picks a previous healthy
  shell copy on its own. To do it by hand:

  ```sh
  nacre ui-shell rollback
  ```

## Black screen or stuck at login

1. Press Ctrl+Alt+F3 and log in on the text console.
2. Look before you restart anything:

   ```sh
   journalctl --user -u nacre-shell.service --no-pager -n 50
   nacre doctor --report
   ```

3. If the graphical session is dead, stop it and go back to the greeter:

   ```sh
   uwsm stop
   ```

4. Without a login manager, start a session from the TTY:

   ```sh
   nacre session nacre-niri     # or: nacre session niri / nacre session hyprland
   ```

   `nacre session refresh` regenerates and validates the prepared niri session
   without starting it.

## A setting change broke something

- Change it back in Settings; that is always safe.
- A managed update went wrong: `nacre rollback` restores Nacre's managed private
  files. It does not undo system packages, and it refuses to overwrite files you
  edited since.
- To reset one stored preference file, stop the shell first (`systemctl --user
  is-active nacre-shell.service` must say `inactive` or `failed`), back up
  `~/.config/nacre`, then follow the
  [safe reset steps](recovery.md#how-do-i-reset-settings-safely). Never delete
  the whole directory: profiles and panel layouts live there too.

## Compositor config errors

```sh
niri validate --config ~/.config/nacre/niri/config.kdl    # niri tiers
hyprctl configerrors                                       # Hyprland
```

On niri, your own overrides are in `~/.config/nacre/niri/custom.kdl`. Reload
with Super+Shift+R.

## No Wi-Fi, Bluetooth or sound tiles

They appear only when the service is there: NetworkManager for Wi-Fi, BlueZ for
Bluetooth, PipeWire with WirePlumber for sound. Check `systemctl status
NetworkManager bluetooth` and `systemctl --user status pipewire wireplumber`.
Night light on niri needs `wlsunset`. Screen recording with sound needs `pactl`.

## Updates

```sh
nacre update
```

runs the system update, then shows Nacre's setup plan and asks before applying
it. The bar's Updates item uses the same path. If the compositor or shell
changed, log out and back in.

## Where things are

| What | Where |
| --- | --- |
| Shell and fallback logs | `journalctl --user -u nacre-shell.service`, `-u nacre-ui-shell-fallback.service` |
| Preferences | `~/.config/nacre` |
| Release and rollback records | `~/.local/state/nacre/releases` |
| Caches (safe to delete when the shell is stopped) | `~/.cache/nacre` |
| Files Nacre owns, by module | `nacre status --files` |
| What uninstall would remove | `nacre uninstall --plan` |

## Uninstall

```sh
nacre uninstall --plan
nacre uninstall --purge               # add --purge-data to drop chats and AI/Brain state too
sudo pacman -Rns $(pacman -Qq | grep -E '^(nacre|orient$)')
systemctl --user daemon-reload
```

Brain notes and your own wallpapers are never removed. The pacman repository
entry and key stay until you remove them from `/etc/pacman.conf` and
`/etc/pacman.d/nacre-dev.conf`.
