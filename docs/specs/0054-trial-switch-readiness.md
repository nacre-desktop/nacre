# N0054 — reversible source-build UI trial

Status: done — implementation and isolated validation complete. No live switch authorized.
Scope: extend `tools/ui_shell_switch.py`; source installs, one UI service,
crash recovery and exact restoration of files changed by the switch.

## Audit and decisions

| Path | Finding and disposition |
| --- | --- |
| Source QML install | `nacre/shell-tools/install.py --code-only` refreshes `~/.local/share/nacre/shell`; source units use `~/.local/bin/nacre-shell start` and `shell/bin/nacre-daemon`. Read-only installed-file inspection found these units and `50-nacre-f4.conf` with client mode enabled. This is filesystem evidence, not proof of the live manager's loaded state. No installed files were changed. |
| Source Bead install | Missing previously; `/usr/bin/bead-shell` was the obsolete default. `nacre ui-shell install --binary FILE --shell DIR` now copies a matched source-built `nacre-shell` plus assets to `~/.local/share/nacre/bead/<transaction>/`. The latest **completed** install journal selects its `launch` wrapper. No package, symlink to a checkout, or replacement of the QML launcher. Earlier versions are kept. |
| Assets | Source hardlinks and file symlinks confined to the supplied shell tree become independent regular copies. Escaping links, linked directories, writable-by-others assets and special files are refused. Copies and containing directories are synced before selecting the build. |
| Systemd cutover | Exactly one `nacre-shell.service`; one managed `60-nacre-ui-shell.conf` replaces its ExecStart. The wrapper sets the version's working directory and starts `--bar --core-views --ui-provider` with private-to-session command and daemon socket paths. Restart policy is explicitly on-failure, two seconds, three starts in sixty seconds. Wants/After on `nacre-daemon.socket` ensures the session starts its socket before Bead, even if it was not independently enabled. |
| Settings/Colors/Welcome/chat | Modern Bead uses the same installed binary's `--app settings|colors|welcome|chat`, not separate packages. Nacre's compatibility companion retains the QML windows-only host for direct legacy IPC. It never owns the bar, UI-provider registration or notification bus. Native app routing belongs to Bead's provider; retaining this companion does not turn unsupported provider routes into supported ones. |
| `nacrectl ui` / N0048 | The native helper dispatches `ui.dispatch` to the registered provider. Unknown methods answered by a provider are errors; only an absent daemon/provider may invoke the old QML IPC fallback, and explicit `NACRE_DAEMON_SOCKET` disables that fallback. Existing keybinds can stay on provider-neutral routes. Read-only inspection of the installed runtime found `nacrectl ui` bindings; the direct `nacre-shell detach` wrapper itself delegates to `ui pearls detach`. No direct `qs ipc` spawn was found in those bindings. |
| Niri | `tools/package_niri.py` starts the session target, which wants the same shell service. Startup apps and `nacre-welcome --login` remain. The trial does not rewrite keybindings or run `niri-session-runtime --backend ...`; readiness requires the N0048 helper/routes already installed. Rebuilding session runtime is a separate approved deployment. |
| Autostart / reboot | Existing shell enablement and target dependencies are preserved. The on-disk drop-in selects Bead at the next login/reboot too. The installed build and controller must remain present throughout the trial. A service/socket stop, reload and shell start was rehearsed, verifying that the socket was reactivated; a physical laptop reboot was not. |
| Crash | Existing OnFailure recovery is retained and now exercised with real systemd. Ordinary crashes restart Bead; only `Result=start-limit-hit` triggers QML convergence. Initial stability/provider/daemon-health failure also restores QML. The recovery lock is nonblocking while a switch owns it; that switch handles its own failure. This is crash-loop recovery, not a watchdog for a frozen process. |
| Rollback | Previously the companion files were left behind. Both `qml` and `rollback` now restore all three saved managed files, including original bytes/modes or absence, then reload and restart QML. Every phase is repeatable after interruption. No changes are made to daemon enablement, niri, the greeter or other drop-ins. Installation is additive: retained Bead binaries and audit journals are deliberately not deleted by runtime rollback. |
| SDDM | The greeter remains its separate QML theme. User UI switching does not modify SDDM or root-owned configuration. |
| Notifications | `--notifications none` intentionally has no desktop notification owner during the trial. `daemon` requires the advertised daemon producer and passes `--notices` to the source-built Bead launcher. These are explicit choices; this unit does not enable a new live notification producer. |

### Remaining consumer gaps and limits

Update (Bead B0084, `docs/specs/0084-trial-switch-routes.md` in the Bead repo): the route table statuses below were
re-verified and `shell-routes.json` now states them; every `nacrectl ui` route is answered by Bead except the selector
prompt (fails fast). Lock-screen media buttons (`lock-info previous|next|toggle`) now fall back from a refusing UI
provider to the daemon `player.*` methods, so they work with either shell. The lock screen's prepared content
(`lock-prepare`, fed by QML's presentation) has no Bead feeder yet; see the Bead spec's asks.

`nacre/shell-tools/shell-routes.json` is the inventory of QML entry points. Its
historical Bead statuses are not proof of the selected binary's capabilities.
It lists partial mode/palette/overview, controls sections, drawer names and
pearls mappings; gaps for session, brightness, keyboard-light and selector
prompt open/cancel; and old Colors/Welcome gaps superseded by the modern app
implementation. Welcome refresh, QML lock previews, generated shortcut IPC,
QML supervisor/display-recovery probes and the legacy no-provider fallback
must still be audited when their consumers are retired. Existing app floating rules
match QML app IDs, whereas native Settings/Colors/Welcome use `nacre-settings`,
`nacre-colors` and `nacre-welcome`; native placement/centering remains an explicit
parity check, not proven by merely opening the apps in the nested fixture. No Nacre switch tool
can supply a missing Bead UI implementation. Hardware-key behavior and complete
visual parity remain acceptance gates for the owner/Bead units, not claimed by
this source-install and recovery rehearsal.

The inherited F4 readiness check rejects mixed package/source layouts, foreign
ExecStart overrides, missing source helper, missing daemon socket, non-client
mode, unsupported configuration paths and unmanaged destination files. Do not
silently bypass these failures. `status` reports the selected launcher, loaded
selection, companion state and incomplete transactions. It does not guarantee
that every UI route is implemented or that a running process remains responsive.

## Owner runbook

Run only after the owner approves the trial. Start from the reviewed, durable
Nacre source checkout (not an expiring task worktree). The staged source-built
Bead preview supplies a matching binary and shell tree; it is read, never modified.
The following is a Bash block, so it also works from a Fish terminal:

```sh
bash <<'BASH'
set -euo pipefail
repo="$(git rev-parse --show-toplevel)"
# Refresh only the existing nacre controller from this reviewed checkout.
# This is software preparation; runtime rollback below retains installed software.
REPO="$repo" python3 - <<'PYCONTROLLER'
import fcntl, os, sys
from pathlib import Path
root = Path(os.environ["REPO"])
sys.path.insert(0, str(root / "tools"))
import releases, f4_switch as f4
f4.require_ordinary_user()
f4.mkdir(releases.STATE)
path = f4.checked(releases.STATE / "release.lock")
fd = os.open(path, os.O_RDONLY | os.O_CREAT | os.O_NOFOLLOW, 0o600)
with os.fdopen(fd) as lock:
    fcntl.flock(lock, fcntl.LOCK_EX)
    releases.install_controller(root)
PYCONTROLLER
bead_build="$(realpath "$HOME/.local/share/nacre/ai/lead/bead-preview/current")"
nacre ui-shell install --binary "$bead_build/bin/nacre-shell" --shell "$bead_build/shell"
readiness_file="$(mktemp)"
trap 'rm -f "$readiness_file"' EXIT
nacre ui-shell status | tee "$readiness_file"
READY="$readiness_file" python3 - <<'PYREADY'
import json, os
s = json.load(open(os.environ["READY"]))
assert s["active"] == "qml", s
assert not s["incomplete"] and not s["ready_for_bead"], s
PYREADY
# Explicit choice: no notification server for this first trial.
nacre ui-shell bead --notifications none
nacre ui-shell status
systemctl --user is-active nacre-shell.service nacre-qml-windows.service
nacrectl ui nacre settings sound
BASH
```

Expected: `active` is `bead`, `incomplete` is empty, both services are active,
and native Settings opens. Check window placement/centering, the bar, launcher, Colors, Welcome and hardware
keys before continuing the trial. If a new build is needed, return to QML first,
then repeat `install` with its matching binary/tree; the previous build stays.
Do not run the full shell installer or refresh niri as an incidental part of this
trial. If readiness asks for a prerequisite deployment, stop and use that unit's
reviewed deployment/rollback plan.

One-line runtime rollback, including after an interrupted switch or another boot:

```sh
nacre ui-shell qml
```

Then check `nacre ui-shell status` (`active: qml`, no incomplete transactions) and
`systemctl --user is-active nacre-shell.service`. Repeat the same rollback if
interrupted. Keep the controller, QML install, journals and version directories
in place until the trial is accepted. An unavailable daemon or failed QML install
can prevent readiness verification; the command reports an incomplete recovery
and keeps its journal rather than claiming success.

## Evidence and reproduction

- Focused model tests: `python -m unittest discover -s tools/tests -p test_ui_shell_switch.py`.
  Includes source selection, linked-asset copies, exact companion restoration,
  and interruption before/after all forward and rollback phases.
- Real manager fixture: `python tools/tests/rehearse_ui_shell_systemd.py` as
  disposable container user `tester`, UID 1000, HOME `/home/tester`, with its own
  systemd user manager and bus. The fixture refuses an existing unit directory
  and a non-Docker environment. Source checkout resides at `/work`; set
  `XDG_RUNTIME_DIR=/run/user/1000` and
  `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus`.
  Container packages: systemd, dbus, Python; use the Arch CI image plus the repo
  runtime's normal Python dependencies. Enable linger for this container-only
  user and wait for `/run/user/1000/bus` before invoking the fixture. Never apply
  this fixture to a host account.
- Real Bead: private headless labwc → nested niri, private HOME/XDG dirs and bus,
  under the desktop heavy-job lock. Real helper/QML/Bead processes; the desktop
  supervisor models systemd. Real-systemd semantics are covered separately above.
- Full desktop CI: all executable steps from `.github/workflows/check.yml` in
  `ghcr.io/archlinux/archlinux:latest`, including native checks, full
  `tools/check.py`, translation generation and native/pseudo galleries.
- Focused result: **22/22 passed**. Real-systemd stub cycle: **passed**;
  [command output](../evidence/n0054/systemd.txt) and
  [start-limit/fallback journal](../evidence/n0054/systemd-journal.txt).
- Desktop result: **27/27 passed**, including native Settings/Colors/Welcome/chat
  and provider-neutral routes, rollback, crash-loop fallback and interrupted
  switch recovery; [normalized output](../evidence/n0054/desktop.txt).
  Bead source `a4917a566f04d698cde57fcbc4e6f9d2b82b2906`, staged source-build profile
  `release`; binary SHA-256
  `b0bde190fecae27f57aeb4b33c565b78f64f48137e60c42126498028402d5420`.
  Helper source/base Nacre `2748720158ac8fe361e3f7a0e5663d0e5d10645a`, binary SHA-256
  `ae7c13ede9efe52ec6146812aef1880a3ee16f0ddbe619b7d5160f8a9a92ff2c`.
  Desktop switch-tool SHA-256:
  `33f56a89246a05f830ba64de3acf5c8ca87b1adaa293542f802b3c9b1a5a75cb`.
  This final tool includes socket ordering and retirement of old Bead journals
  after interrupted QML completion; both isolated runtime cycles were repeated.
  Both sessions used private HOME/runtime/bus. The outer headless labwc used
  pixman; Bead used its default renderer selection. No physical display, memory
  or visual-parity claim is made.
- Full desktop CI: **passed, exit 0**; [command/result summary](../evidence/n0054/ci.txt).
  Tools 491 (one existing skip), AI 120, Brain 52, shell-tools 487; helper parity
  534 cases with zero failures. Native gallery 244 states/language, wider-font
  gallery 185 states/language and AUR pager capture all passed. Product/test
  source hashes matched the worktree exactly. The CI host lacked tmux, so the
  job ran with `docker exec -d`; container persistence supplied disconnect safety.
- All task-owned desktop processes/runtime dirs, task containers/images and
  remote build directories were cleaned after preserving the logs. No live
  installed shell, units, configs, keybindings or greeter files were changed.

## Temporary duplicates

QML compatibility companion and legacy UI fallback: delete 7 days after the
replacement is trusted across all registered routes, with owner approval to
retire QML. Superseded/incomplete Bead version directories: delete 7 days after
the replacement is trusted; keep the previous version throughout the trial.
Journals remain rollback/audit evidence. No second switch implementation is added.
