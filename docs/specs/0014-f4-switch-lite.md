# N0014 — F4 daemon opt-in switch, lite

Status: implementation for lead review; no live activation or publication.

## Goal and decision

Owner decision, 2026-10-20: split the growing N10 package/setup rollback
controller. Provide a small ordinary-user activation switch on N0013's packaged
vendor-unit baseline, extended by the lead's source-install decision below. Package downgrade remains an explicit manual owner step
following successful disable. No privileged helper, pacman API, setup snapshot
or package rollback machinery is imported from N10.

## Behaviour

`nacre f4 status|enable|disable|rollback [--transaction ID]` routes through the
release controller and holds its existing release.lock throughout observation,
preflight, changes, health and recovery. It requires the release lock created by
setup/source deployment; missing/unsafe locks refuse without creating files. Journals use
XDG_STATE_HOME; the release lock retains the release controller's existing path.
Status does not write or probe a socket-activated daemon. Its `ready_to_enable`
field is a list of blocking reasons (empty means ready), including each missing
daemon unit and the setup/update step that deploys it.

The supported baseline has all three fragments in one layout: vendor units in
`/usr/lib/systemd/user` without HOME shadows, or receipt-verified source units
under `$XDG_CONFIG_HOME/systemd/user`. Source fragments must be regular,
user-owned files without symlink ancestry; their SHA-256 bytes must match the
HOME-relative entries in `~/.local/state/nacre/configuration.json`, using
`configure.digest()`. Mixed layouts and edited/unreceipted fragments refuse.
The managed drop-in directory must be in the manager's UnitPath. Foreign client
environment assignments, unresolved
environment files, endpoint differences, symlink ancestry, unknown activation
links/owners and unsupported unit states refuse with the exact reason/path
before journals or other writes. Inventory is bounded to manager unit roots,
their immediate wants/requires/drop-in directories, and standard autostart/D-Bus
activation directories. It does not claim to discover arbitrary external scripts.

Enable writes `[Service]\nEnvironment=NACRE_DAEMON_CLIENT=1`, reloads, enables
and starts the socket, restarts the shell and checks effective client1 plus F4
protocol health. Disable writes value0, disables/stops socket, stops daemon,
restarts shell and verifies client0 and inactive daemon/socket. Only a completed
owned enable may account for existing activation during disable; an independent
active/enabled baseline refuses. Successful repeats retain the transaction ID.

Before activation, each journal records original drop-in bytes/mode or absence,
unit activity and socket enablement, the single manager opt-in value, effective
client mode, endpoint paths and OS boot ID. Journals are 0600 within 0700
transaction directories. Atomic replacement and directory/file fsync persist
phase intent before action and completion afterwards. Only relevant unit state
and endpoint fields are journaled, not full environments or settings contents.

Failures automatically restore prior state. Explicit rollback supports completed
and interrupted transactions, restores exact prior file/activation state and
restarts (then stops, if originally inactive) the shell. Recovery repeats
idempotent desired-state operations, including when systemd still caches the
previous environment before reload. It refuses foreign file/manager changes;
changed OS boot ID is permitted and reported. Pending journals block another
mutator until recovery. Journals are kept for inspection and repeat recovery.

The implementation reuses N10's vendor-unit checks, bounded startup-inventory
approach, private write-ahead journal pattern and daemon.status/settings.get
health semantics, adapted to activation-only scope. Full transactional package,
setup and activation fencing remains on N10; this switch serializes cooperating
release commands but cannot prevent concurrent direct systemctl or file edits.

## Evidence

Private-HOME/fake-systemctl tests cover enable/disable/rollback and repeats,
manager1 overridden by client0, exact content/mode restoration, health and unit
command failures, pre-write refusals, crash-before-marker-action across forward
and restore phases, crash-after-action, boot changes, lock lifetime, read-only
status, and journal permissions. Tests run on the dedicated SSH test host in tmux with nice priority;
no live manager or package command is issued. Implementation commit `aed657f1` was checked with matching SHA-256 hashes
for the local and remote controller, release router and tests. With `umask 077`:

- `nice -n 10 python3 -m unittest discover -s tools/tests -p test_f4_switch.py`:
  19 tests passed (including phase/crash subtests).
- `nice -n 10 python3 -m unittest discover -s tools/tests -p test_releases.py`:
  16 tests passed.
- Ruff 0.16.10 `check` and `format --check` on the three touched Python files:
  passed; `python3 -m py_compile` and `git diff --check`: passed.

Initial runs with the host's group-writable umask failed private-directory guards;
the final private-umask runs above passed without weakening those guards.
The controller is 708 lines plus nine release-router additions.
 Owner physical checks remain
outside this task. The [owner procedure](../development/f4-switch-on.md) includes
manual cached-package `pacman -U` only after successful disable.

## Lead source-install extension

The lead's read-only owner-manager probe found a source deployment with two
harmless shell drop-ins, absent daemon fragments, and a dangling unrelated D-Bus
activation link. This supersedes the original package-only restriction: N10's
HOME-unit fence guarded package transactions, outside this switch's scope.
Dangling links are skipped, except when their own name or target mentions
`nacre-daemon`, which still refuses.

Source socket enablement accepts only the graphical-session.target wants-link
to the verified HOME socket fragment. Source protocol health uses the deployed
regular user-owned executable `~/.local/share/nacre/shell/tools/nacre-helper ctl`;
package health retains its vendor nacrectl check. Missing units/helper refuse
before writes and appear in readiness diagnostics. Harmless shell drop-ins stay
in place. The source configuration map now stages and receipts the three units;
scoped receipt adoption includes unchanged byte-identical files as well as newly
written files. The owner procedure updates the source shell/helper, reviews and
applies the scoped unit plan, reloads, checks status, enables, verifies and rolls
back activation. No live probe was run by the implementer; the lead reruns it.

Source extension evidence (`50309bb7`, matching local/remote SHA-256 hashes):
private-HOME tests on the dedicated SSH test host, tmux, `umask 077`:

- `PYTHONPATH=tools nice -n 10 python3 -m unittest discover -s tools/tests -p test_f4_switch.py`:
  25 tests passed, including source recovery, helper `ctl` dispatch, harmless
  shell drop-ins, exact HOME socket links, missing deployment diagnostics,
  source receipt/layout refusals and dangling-link regression cases.
- `PYTHONPATH=tools nice -n 10 python3 -m unittest discover -s tools/tests -p test_releases.py`:
  16 tests passed.
- `PYTHONPATH=tools:tools/tests nice -n 10 python3 -m unittest test_configure.ConfigurationTests.test_source_switch_units_are_staged_and_receipted_without_activation`:
  passed; covers receipt adoption of an existing identical shell unit together
  with newly staged daemon units, repeat apply and preservation of later edits.
- Ruff 0.16.10 check/format, py_compile and git diff --check passed.

The optional full configure suite was attempted: existing tests require a native
`nacre-helper` binary absent on that host (11 errors), so no full-suite pass is
claimed. The focused new receipt regression passes without native execution.
The controller is 819 lines; release routing and the bounded configuration
changes keep the implementation below the 1,000-line target.
