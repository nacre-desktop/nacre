# Updates, recovery and FAQ

Keep desktop packages and Qt/Quickshell compatible. Package updates are followed
by `nacre setup`; source updates use committed, checked code and a reviewed
installation plan. Managed changes keep snapshots, restore failed applies and
refuse to overwrite later edits. `nacre status` shows the current release;
`nacre rollback` restores managed private files. It does not undo system package
upgrades or restore accounts, conversations, notes or personal preferences.

## Package update checks in isolated sessions

The update checker uses `checkupdates` and its separate pacman database, leaving
the system database untouched. In a session with remapped user IDs, Nacre runs
package checks through the systemd user manager. The interactive updater also
uses a user service so pacman's download user and the normal sudo password prompt
work. Update confirmation, Qt compatibility checks and package signatures remain
in effect; normal sessions continue to run the tools directly.

## Recover from a TTY

A TTY is a text console that works independently of the graphical desktop.
Press Ctrl+Alt+F3 and log in. First inspect the session rather than restarting it:

```sh
systemctl --user is-active wayland-wm@hyprland.desktop.service
systemctl --user is-active wayland-wm@niri.desktop.service
journalctl --user -u nacre-shell.service --no-pager -n 50
nacre doctor --report
```

If your working session is still active, return to its original VT; its function
key depends on the login manager. Do not stop a healthy session. If the graphical
session has failed, save any reachable work before running:

```sh
uwsm stop
```

This ends the current UWSM-managed session and its applications. At the greeter,
choose **Nacre (Hyprland)** / the existing UWSM-managed Hyprland entry, or a
previously prepared **Nacre (niri)** session. When niri fails, use the working
Hyprland fallback while investigating. If the greeter does not return, and your
system uses the `display-manager.service` alias:

```sh
sudo systemctl restart display-manager.service
```

Restarting the login manager ends graphical sessions. As a temporary fallback
from the TTY, after the failed session is stopped:

```sh
# Choose one installed, configured session:
exec uwsm start -e -D Hyprland hyprland.desktop
# Or, only if Nacre's niri preparation has completed:
exec uwsm start -e -D niri niri.desktop
```

`exec` replaces the TTY shell so closing the desktop does not leave that shell
open. `nacre rollback` can restore a failed managed update; it refuses later
local edits. For niri-specific preparation and rollback limits, see the
[niri guide](development/niri-testing.md#recovery-first-return-to-hyprland-from-a-tty).

## How do I collect a support report?

Run `nacre doctor --report` for Markdown, or `nacre doctor --json` for redacted
JSON. Launcher → **Health → Copy support report** copies Markdown locally.
Reports include available hardware/software probes and bounded recent errors;
unavailable probes are identified. Nothing is uploaded. Review the report before
sharing, especially free-form log text. A report does not prove cold login,
password unlock, suspend/resume or screen sharing works on your hardware.

## Where are the logs?

Service logs live in the systemd user journal; Quickshell keeps separate runtime
logs, which you can read in the graphical session with its log command:

```sh
journalctl --user -u nacre-shell.service --no-pager -n 50
quickshell log --config nacre --no-color --tail 50
```

Launcher → **Health → Logs** opens recent service logs. Release records are in
`~/.local/state/nacre/releases`; optional AI and Brain have their own state under
`~/.local/state/nacre`. Paths and messages in raw logs can be private.

For Hyprland configuration errors, run `hyprctl configerrors` in the Hyprland
session. For niri, run `niri validate` against its actual config; if using a
prepared/custom path, pass `niri validate --config /path/to/config.kdl`.

## How do I reset settings safely?

Try changing the affected preference in Settings first. For a damaged stored
preference file, save work and log out to the greeter, then enter a TTY. Confirm
`systemctl --user is-active nacre-shell.service` says `inactive` or `failed` so
it cannot write settings while you move them. Back up the settings directory:

```sh
backup="$HOME/nacre-settings-backup-$(date +%Y%m%d-%H%M%S)"
cp -a "$HOME/.config/nacre" "$backup"
# Move only the file you want to reset; omit the other line:
mv "$HOME/.config/nacre/desktop.json" "$backup/desktop.json.original"
mv "$HOME/.config/nacre/launcher.json" "$backup/launcher.json.original"
```

Skip a move if that file does not exist. On next login, missing shell preferences
use defaults: `desktop.json` holds desktop preferences, while `launcher.json`
holds launcher preferences and favorites. Existing compositor options and
private Lua/KDL overrides remain; this is not a full compositor reset. Revisit
Settings to adjust them. To undo, log out again and copy the original file back
from the backup. Keep `profile.json`, keyboard/monitor overrides, accounts,
conversations and the Brain vault. Never remove all of `~/.config/nacre` or
`~/.local/share/nacre` as a reset shortcut. See [path limits](configuration.md#files-and-xdg-paths)
if you have changed XDG variables.
