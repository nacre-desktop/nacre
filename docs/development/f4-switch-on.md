# F4 daemon owner procedure

Use this only on the exact reviewed main revision containing N0014, after the
owner has chosen the install type and retained the applicable prior package
archive. N0014 changes activation only. It does not update or roll back source
files, package contents, setup state, or settings. Full N10 package/setup rollback
is deferred and is not a prerequisite.

## Common switch and checks

As the ordinary desktop user, update the checkout to the reviewed main revision
and verify `git rev-parse HEAD` matches that revision. Do not run the switch with
sudo. The switch requires the existing
`~/.local/state/nacre/releases/release.lock`; it does not create the lock. The
transaction journal uses `$XDG_STATE_HOME` when set, independently of this lock.

```sh
nacre f4 status
```

Proceed only when `ready_to_enable` is `[]`, `incomplete` is `[]`, and the three
unit fragments and current mode are understood. Status is read-only and does not
socket-activate or health-probe the daemon. On any blocker, stop and resolve the
named path using the relevant install section; do not remove foreign files to
force readiness.

```sh
nacre f4 enable
nacre f4 status
```

Keep the 32-character transaction ID returned by enable. Verify status has
`ready_to_enable: []`, `incomplete: []`, effective client mode `1`, active shell,
and the expected socket/daemon unit states. Then confirm the desktop shell is in
daemon-client mode, daemon protocol health remains ready, and Settings can read
and write a reversible test value; restore the original setting afterward.
Check the normal desktop functions you rely on, including keyboard/display and
notification ownership. These are owner checks, not claims from nested tests.

If the switch reports failure and says prior state was restored, inspect the
transaction and `nacre f4 status`; do not repeat blindly. If recovery is pending,
first run the recovery command in a working terminal or a TTY:

```sh
nacre f4 rollback --transaction TRANSACTION_ID
nacre f4 status
```

If the shell/session is unusable, log in on a TTY as the same user. Inspect
`${XDG_STATE_HOME:-$HOME/.local/state}/nacre/f4-switch/TRANSACTION_ID/journal.json`.
The journal's `prior.file` is either `null` (drop-in was absent) or contains
base64 `data` and decimal `mode`; `prior.units` records `nacre-shell.service`,
`nacre-daemon.service`, and `nacre-daemon.socket` `ActiveState` and
`UnitFileState`. Restore the prior drop-in from that journal, then restore its
recorded socket enablement and active states:

```sh
JOURNAL="${XDG_STATE_HOME:-$HOME/.local/state}/nacre/f4-switch/TRANSACTION_ID/journal.json"
DROPIN="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/nacre-shell.service.d/50-nacre-f4.conf"
python3 - "$JOURNAL" "$DROPIN" <<'PY'
import base64, json, pathlib, sys
journal, dropin = map(pathlib.Path, sys.argv[1:])
r = json.loads(journal.read_text())['prior']
f = r['file']
if f is None:
    dropin.unlink(missing_ok=True)
else:
    dropin.parent.mkdir(parents=True, exist_ok=True)
    dropin.write_bytes(base64.b64decode(f['data']))
    dropin.chmod(f['mode'])
for unit, state in r['units'].items():
    print(unit, state['ActiveState'], state['UnitFileState'])
PY
systemctl --user daemon-reload
systemctl --user disable --now nacre-daemon.socket
systemctl --user stop nacre-daemon.service
```

Use the printed prior rows to run `systemctl --user enable nacre-daemon.socket`
if its prior `UnitFileState` was `enabled`, otherwise
`systemctl --user disable nacre-daemon.socket`. For each of
`nacre-shell.service`, `nacre-daemon.socket`, and `nacre-daemon.service`, run
`systemctl --user start UNIT` when its prior `ActiveState` was `active`, otherwise
`systemctl --user stop UNIT`; for the shell use `restart` instead of `start` when
it was active. Restore socket enablement before starting any unit. Finally run
`nacre f4 status`. If the journal is missing,
malformed, the recorded paths differ, or a manager command fails, leave the
daemon stopped, preserve the journal and ask for recovery help. This manual path
restores only the recorded activation/drop-in state; prefer the controller
rollback whenever a terminal is available.

To intentionally return to legacy mode and turn daemon activation off:

```sh
nacre f4 disable
nacre f4 status
```

Verify mode `0`, inactive daemon and socket, disabled socket, and no incomplete
transaction. Disable refuses independent daemon activation owners. For exact
pre-switch activation restoration instead, use `nacre f4 rollback` (latest
journal) or `nacre f4 rollback --transaction TRANSACTION_ID`.

## Source install

From the owner's source checkout, update to the reviewed main revision first.
This installer targets the default `$HOME/.config/systemd/user`; a custom
`XDG_CONFIG_HOME` needs the unit fragments deployed at the manager's effective
paths before the switch can be ready.

```sh
git pull --ff-only origin main
git rev-parse HEAD
python3 nacre/shell-tools/install.py
nacre f4 status
```

Compare `git rev-parse HEAD` with the exact reviewed revision before installing.
Run the full installer without
`--code-only`: code-only returns before deployment of missing daemon unit
templates. The full installer deploys the helper and stages
`nacre-daemon.service` and `nacre-daemon.socket` under
`~/.config/systemd/user`; it does not enable or start those daemon units. It
reloads systemd and restarts the shell as part of source deployment. Continue
only when status reports an empty `ready_to_enable`; then follow Common switch
and checks.

If any installer step fails, do not enable F4. Use the installer's documented
source restore only if its own deployment transaction is available and reviewed;
otherwise preserve its report/backup and repair the source install before
retrying. `nacre f4 rollback` restores activation state only; it does not undo
the source checkout, helper, or installed tree. There is no package downgrade
for a source install.

## Package install

The package owner installs the exact reviewed archive and completes setup.
Keep a verified previous `nacre` package archive in pacman's cache. N0013 places
the three core user units in `/usr/lib/systemd/user`; setup must leave no
conflicting HOME shadows and creates the release lock. Do not substitute a
whole-system upgrade.

```sh
NACRE_ARCHIVE=/path/to/exact-verified-nacre-core.pkg.tar.zst
test -r "$NACRE_ARCHIVE"
sudo pacman -U -- "$NACRE_ARCHIVE"
nacre setup --plan
# Review the plan, then apply the approved setup:
nacre setup
nacre f4 status
```

Continue only when status reports an empty `ready_to_enable` and no incomplete
transaction; then follow Common switch and checks.

Package downgrade is a separate manual owner action, only after successful
disable and verification of mode `0`, inactive daemon/socket, disabled socket,
and no incomplete transaction:

```sh
nacre f4 disable
nacre f4 status
PREVIOUS_NACRE_ARCHIVE=/var/cache/pacman/pkg/nacre-PREVIOUS_VERSION-x86_64.pkg.tar.zst
test -r "$PREVIOUS_NACRE_ARCHIVE"
sudo pacman -U -- "$PREVIOUS_NACRE_ARCHIVE"
```

Use the exact retained, signature-verified previous package and compatible
dependencies. Do not run an F4 activation rollback after downgrade. This manual
downgrade does not restore setup files or settings; full transactional package
and setup rollback remains deferred to N10.
