# N0014 — F4 daemon opt-in switch, lite

Status: implementation for lead review; no live activation or publication.

## Goal and decision

Owner decision, 2026-10-20: split the growing N10 package/setup rollback
controller. Provide a small ordinary-user activation switch on N0013's packaged
vendor-unit baseline, extended by the lead's source-install decision below.
Package downgrade remains an explicit manual owner step
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
`/usr/lib/systemd/user` without HOME shadows, or source units under
`$XDG_CONFIG_HOME/systemd/user`. Source fragments must be regular, user-owned
files without symlink ancestry or group/other write access. Their contents must
not mention `NACRE_DAEMON_CLIENT`, `EnvironmentFile`, `PassEnvironment` or
`UnsetEnvironment`. Mixed vendor/HOME layouts refuse. No source configuration
receipt or SHA-256 match is required: the switch never modifies unit files.
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

## Lead source-install extension and ownership correction

The lead's read-only owner-manager probe found a source deployment with two
harmless shell drop-ins, absent daemon fragments, and a dangling unrelated D-Bus
activation link. The implementer did not probe the live manager. N10's HOME-unit
fence guarded package transactions, outside this switch's scope. Dangling links
are skipped, except when their own name or target mentions `nacre-daemon`.

The lead superseded the initial source receipt requirement after review of
`50309bb7`: configuration's file map must not adopt installer-owned shell units.
The changes to `tools/configure.py` and its test were reverted exactly to their
pre-extension versions. Installer-deployed content may differ from repository
unit bytes; only the path, ownership, regular-file, permissions and forbidden
content rules above apply. Existing effective-environment and foreign-drop-in
checks remain in force. The switch does not read or change configuration.json.

Source socket enablement accepts only the graphical-session.target wants-link
to the checked HOME socket fragment. Source protocol health uses the deployed
regular user-owned executable `~/.local/share/nacre/shell/tools/nacre-helper ctl`;
package health retains its vendor nacrectl check. Missing units/helper refuse
before writes and appear in readiness diagnostics.

Installer verification: `nacre/shell-tools/install.py` deploys the helper at its
`install_native_helper()` step (around lines 250–252). Its full-install loop
(around lines 418–423) stages both daemon unit templates without activating
them. The code-only branch returns earlier and does not reach this loop.
Therefore missing-unit diagnostics point to
`python3 nacre/shell-tools/install.py`, without `--code-only`, not configure.py.
No installer changes were made. The owner procedure distinguishes the managed
source/controller update from the full installer required for missing units.

Correction evidence is recorded after final private-HOME checks. The earlier
source extension's configure receipt regression is withdrawn, along with the
configuration ownership change. Full configure-suite/native-helper limitations
from that extension do not represent changes to configure in this correction.

Final correction evidence, implementation `f45cc8b6`, dedicated SSH test host
in tmux with `umask 077`:

- `PYTHONPATH=tools nice -n 10 python3 -m unittest discover -s tools/tests -p test_f4_switch.py`:
  26 tests passed, including forbidden source-fragment tokens, writable/symlink
  refusals, mixed layouts, missing-unit installer guidance, receipt independence
  and preservation of unit bytes/modes plus unrelated configuration receipt.
- `PYTHONPATH=tools nice -n 10 python3 -m unittest discover -s tools/tests -p test_releases.py`:
  16 tests passed.
- `git diff 68d28945 --exit-code -- tools/configure.py tools/tests/test_configure.py`:
  passed; both files exactly match the version before the source extension.
- Ruff 0.16.10 check/format, py_compile and git diff --check passed. Controller:
  803 lines; no installer changes, live-manager probe, package operation or push.
