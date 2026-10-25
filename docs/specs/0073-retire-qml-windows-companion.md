# N0073 — Retire the QML windows companion in Bead mode

Status: Implemented; fake-systemd and Arch desktop CI evidence pending.

## Finding and route audit

There is no remaining QML-only gap for Settings, Colors, Welcome, or AI in Bead
mode. The route table defines provider-neutral calls: Settings uses
`nacre.settings` (Bead B0052), Colors uses `colorsApp.open` (B0060), Welcome
uses `welcomeApp.open` (B0059), and the AI sidebar/chat uses `nacre.left` and
the Bead chat app (B0052/B0058/B0059). `nacre-settings`, `nacre-colors`, and
`nacre-welcome` invoke the corresponding `nacrectl ui` routes. Their desktop
entries use those same commands; AI launches its terminal dashboard. The
Settings keybind invokes `nacre-settings`, and notification Settings actions
invoke `nacre-settings notifications`. QML mode keeps the same routes through
its registered provider. Bead window rules cover the three native app IDs.
Evidence: `nacre/shell-tools/shell-routes.json`,
`crates/nacre-helper/src/launchers.rs`, `nacre/desktop/*.desktop`,
`niri/bindings.kdl`, `hypr/conf/keybinding.lua`,
`nacre/shell-tools/desktop-actions.py`, and B0084's route/window-rule table.
The `org.quickshell` StartupWMClass desktop metadata is legacy metadata and is
not used by the Bead app routes or window rules.

## Change

Bead's managed `nacre-shell.service` drop-in now Wants only the daemon socket;
it no longer generates or starts `nacre-qml-windows.service`. A switch stops an
already-loaded companion, removes its managed unit, reloads systemd, then
restarts Bead. Journaled rollback restores prior unit bytes and modes and
converges to QML. The QML-only wallpaper-media session/serve and wallpaper-watch
helpers are spawned by Quickshell, so removing/stopping this companion prevents
them from running in Bead mode; Bead's wallpaper renderer remains owned by
N0069/B0099. QML mode's shell and routes are unchanged.

## Validation

- Fake systemctl tests cover fresh switch/rollback, managed legacy companion and
  helper cleanup, failures, and interruption at each new convergence phase.
- Real-systemd fixture asserts no companion unit or dependency in Bead mode.
- Full `.github/workflows/check.yml` `desktop` job on the designated remote
  Arch container: pending.

## Temporary duplicates

None introduced. Existing companion files are removed during a successful Bead
switch and restored only by the journaled QML rollback.

## Lead application on the laptop

From the reviewed durable Nacre checkout, run the controller-refresh Python
block in N0054's owner runbook (it takes the existing release lock and calls
`releases.install_controller(root)`). Then run this Bash sequence, retaining the
current notification choice:

```sh
set -euo pipefail
notifications="$(nacre ui-shell status | python3 -c 'import json,sys; print(json.load(sys.stdin).get("notifications") or "none")')"
nacre ui-shell qml
nacre ui-shell bead --notifications "$notifications"
nacre ui-shell status
systemctl --user show --property=LoadState nacre-qml-windows.service
```

Confirm status is Bead and `LoadState=not-found`. Undo or recover with
`nacre ui-shell qml`.
