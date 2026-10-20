# F4 daemon switch-on and recovery plan

F4 acceptance is complete: commit `9b37c663` passed CI run `39402420054`
([N0002](../specs/0002-f4-whole-group-acceptance.md#final-result--2026-10-20)).
N9 at `ab3bcae4` passed exact-head CI run `39431539570`. N0013 supplies
package-owned core user units. [N0014](../specs/0014-f4-switch-lite.md) provides
the bounded activation switch; its lead review, integration and exact-head CI
must pass before owner execution. Full N10 transactional package/setup rollback
remains separate future hardening and is not required by this lite procedure.
No live command in this document was executed during N0014 development.

## Owner package and setup prerequisites

Package publication, installation and live switching remain owner-controlled.
Require the exact green integrated revision, verified signed package and database
metadata, and a readable, verified previous Core package archive retained in
pacman's cache before installation. Preserve the existing compositor and
notification owner. Build/publication follow the
[package guide](../../packaging/arch/README.md#tested-development-publication);
this switch never publishes, invokes sudo, installs or downgrades packages.

After the owner authorizes installation, install the exact verified archive:

```sh
NACRE_ARCHIVE=/path/to/exact-verified-nacre-core.pkg.tar.zst
test -r "$NACRE_ARCHIVE"
sudo pacman -U -- "$NACRE_ARCHIVE"
nacre setup --plan
# Review the plan, then apply only after owner approval:
nacre setup
```

Do not substitute a whole-system upgrade. Retain the installed profile unless
a change is intended. Setup must complete before switching. Its package layout
must expose all three vendor fragments under `/usr/lib/systemd/user` without
HOME shadows, and create the existing release lock at
`~/.local/state/nacre/releases/release.lock`. Source installs are unsupported.
The CLI's absolute `XDG_CONFIG_HOME` (default `~/.config`) must be in the user
manager's UnitPath. The controller names unsupported files and refuses before
writing; do not remove foreign files merely to bypass that refusal.

## Enable and verify

As the ordinary desktop user, after owner approval:

```sh
nacre f4 status
nacre f4 enable
nacre f4 status
```

Retain the returned transaction ID. Enable journals prior state privately under
`${XDG_STATE_HOME:-$HOME/.local/state}/nacre/f4-switch/ID/journal.json`, writes
`50-nacre-f4.conf` under the shell's user drop-in directory with client value
`1`, reloads systemd, enables/starts the socket and restarts the shell. Success
requires an active shell, effective shell `NACRE_DAEMON_CLIENT=1`, ready daemon
protocol, durable/error-free settings snapshots and a stable daemon identity.
A failure attempts automatic restoration and reports the transaction ID if
manual recovery is still needed. Repeating successful enable is idempotent.

Status is read-only: inspect `effective`, `manager`, the three unit states and
`incomplete`. It does not run a protocol probe that could socket-activate the
daemon. Do the owner-only reversible Settings write/read-back and restore its
original value. Real logout/login, keyboard/display, notification ownership and
compositor recovery remain owner physical checks; nested F4 evidence does not
claim them.

## Disable or restore prior activation

To deliberately return to the legacy client and leave daemon activation off:

```sh
nacre f4 disable
nacre f4 status
```

Disable writes client value `0`, including when the manager contains value `1`,
disables/stops the socket, stops the daemon and restarts/verifies the shell.
It refuses independent baseline daemon owners, extra wants/requires links,
startup inputs or conflicting environments before any write. In particular,
activation that existed before enable is not assumed to belong to the switch.
Repeating successful disable is idempotent.

To restore the exact state saved by a completed or interrupted transaction:

```sh
F4_TRANSACTION=the_32_character_id_returned_by_the_switch
nacre f4 rollback --transaction "$F4_TRANSACTION"
nacre f4 status
```

Without `--transaction`, rollback selects the latest journal. It restores the
prior managed file bytes/mode or absence, socket enablement/activity, daemon
activity and shell activity, then verifies them. It restarts the shell during
restoration. Manager opt-in is recorded and left unchanged; a changed manager
value or foreign file/activation path requires review rather than overwriting
it. A different OS boot ID is allowed and reported. Repeating rollback is safe;
a crash during restoration can be recovered with the same invocation. Preserve
the journal until recovery is verified. If status reports an incomplete
transaction, recover it before another enable/disable.

## Manual package downgrade after disable

Package downgrade is a separate, non-atomic owner operation. First complete
`disable` and inspect status: effective client `0`, daemon and socket inactive,
socket disabled, and no incomplete transaction. If disable refuses, stop and
review the named owner/path; do not downgrade around the refusal.

```sh
nacre f4 disable
nacre f4 status
# Only after the successful checks above, select the retained verified archive:
PREVIOUS_NACRE_ARCHIVE=/var/cache/pacman/pkg/nacre-PREVIOUS_VERSION-x86_64.pkg.tar.zst
test -r "$PREVIOUS_NACRE_ARCHIVE"
sudo pacman -U -- "$PREVIOUS_NACRE_ARCHIVE"
```

Use the exact cached previous `nacre` package, with its verified signature and
compatible dependencies. The switch does not restore setup files, settings or
package hooks, and `nacre rollback` is only a setup-file rollback. Do not run an
F4 activation rollback that re-enables client `1` after downgrading to a package
without F4 support. If older setup files need restoration, review the retained
setup snapshot and that package's setup plan separately. Full crash-safe package
and setup restoration belongs to N10, not this command.
