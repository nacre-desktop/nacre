# N0156 — Bead shell recovery and QML retirement

Owner authorization: 2026-11-10, “remove all the quickshell stuff and previous
shell, we are using bead now.” No live installation is authorized by this unit.

## Delivery gates

Stage A changes failure recovery and installer migration before removing any
QML source. Stage B removes the QML shell and its exclusive tooling only after
Stage A's disposable-HOME proofs pass. Before pushing any QML deletion, fetch
origin and require this exact check to succeed:

```sh
git cat-file -e origin/main:docs/research/bead-vs-qml-shell-benchmark-20261110.md
```

The check failed at the start of N0156. Stage A may be pushed independently;
Stage B must stay local while the benchmark is absent. Each commit contains
`[no-publish]`; integration uses a normal fast-forward push to main.

## Recovery design

Use the product's versioned copies under `~/.local/share/nacre/bead/` and its
`~/.local/state/nacre/ui-shell/` transaction journal. A copied executable and
matching Luau tree are a candidate, not a known-good release. Only a completed
live health check establishes a recovery candidate. Keep previous healthy copies
through updates and rollback. The lead's private `stable-bead` and
`good-bead-nacre-*` copies are outside the product's recovery contract.

`nacre-ui-shell-fallback.service` must select a previously validated Bead,
validate its executable, restart the shell and check process stability, provider
registration and daemon health. An exhausted recovery attempt stops the shell
and reports a TTY recovery command in the journal; it must not repeatedly
restart the failed candidate or invoke QML. Ordinary scheduled restarts must
not trigger recovery before the service's start limit is exhausted.

## Scope and retention

Keep root `shell/`, `crates/nacre-shell`, `nacre/login` (SDDM QtQuick), and shared
runtime/deployment helpers in `nacre/shell-tools`. `bead-shell/` is owned by N0157
and is outside this deletion. Inventory consumers before removing helpers,
translation IDs, package dependencies or test fixtures.

Temporary QML runtime and switch paths retained for Stage A: delete 0 days after
the Bead migration/recovery replacement is trusted and Stage B passes the
benchmark and full CI gates. Individual `bin/*.rollback` copies use
the rule “delete 7 days after the replacement is trusted”; commit age alone
does not prove a replacement has been trusted.

Stage A temporarily retains the existing QML transaction code alongside the
Bead recovery transaction so the two deliveries can be tested and integrated
independently. Its controller additions are not a second service or a background
job; they reuse the existing fallback unit, release lock, versioned copies and
health checks. Stage B removes the old transaction and QML installer branches.

## Validation and migration evidence

Stage A focused checks ran in the task's Arch container on dev-1 with disposable
HOMEs. The common environment was
`PYTHONPATH=tools:nacre/shell-tools:nacre/shell-cli/src`, with
`NACRE_HELPER_BINARY=/work/target/release/nacre-helper` and
`NACRE_ORIENT_BINARY=/work/deps/orient/target/release/orient` for setup tests.

| Exact command (from `/work`, prefixed `nice -n 10`) | Result |
| --- | --- |
| `python -m unittest discover -s tools/tests -p test_ui_shell_switch.py` | 35 tests, OK |
| `python -m unittest discover -s tools/tests -p test_releases.py` | 21 tests, OK, 1 skipped |
| `python -m unittest discover -s nacre/shell-tools/tests -p test_installer_dropin_guard.py` | 30 tests, OK |
| `python -m unittest discover -s tools/tests -p test_setup.py` | 46 tests, OK |

Pinned Ruff 0.16.10 formatting and lint passed on all eight changed Python files.
The source installer tests exercise fresh/update deployment in temporary HOMEs
with external services mocked; package setup tests exercise real private-file
preparation and rollback with executable fixtures.

Real-systemd command: as UID 1000 `tester` in a task-only Arch container booted
with systemd PID 1, run
`nice -n 10 python /work/tools/tests/rehearse_ui_shell_systemd.py`. Result: exit 0,
`PASS: fresh migration → old fallback update → start-limit recovery → rollback → next update → exhausted-copy stop and journal hint`.
The script uses stub Bead/daemon programs and the actual release controller and
systemd user manager. It proves immutable copy selection, actual
`StartLimitBurst`/`OnFailure` execution, rollback/update convergence and an inactive
shell with unchanged `NRestarts=0` after exhaustion. It does not prove rendered
Bead UI health. The fixture copies its controller into the disposable HOME and
the failure unit executes that persisted copy.

Full desktop CI, installed-package acceptance and actual Bead/daemon recovery
evidence remain pending. No installer runs against the owner's HOME and no laptop
session, units or installed files are changed.

## Owner-run live migration (procedure only; not executed here)

Use a committed, checked checkout and its matching prebuilt Bead executables
(`nacre-shell`, `bead-wallpaper-video`, `nacre-wallpaper-decode`) in
`target/release/`, or the matching installed package bundle under
`/usr/lib/nacre/bead-shell/`. Save work before the shell restart. From that
checkout the lead will run:

```sh
git status --short
systemctl --user show nacre-shell.service -p ExecStart -p DropInPaths -p ActiveState
journalctl --user -u nacre-ui-shell-fallback.service --no-pager -n 50
systemd-run --user --wait --pipe nice -n 10 python3 "$PWD/nacre/shell-tools/install.py" --code-only --dry-run
systemd-run --user --wait --pipe nice -n 10 python3 "$PWD/nacre/shell-tools/install.py" --code-only
nacre ui-shell status
systemctl --user is-active nacre-shell.service nacre-daemon.service
journalctl --user -u nacre-shell.service -u nacre-ui-shell-fallback.service --no-pager -n 100
```

Stage A replaces the managed `nacre-ui-shell-fallback.service` action with
`ui-shell recover-bead --recover`, stages an immutable matched Bead copy, and
health-checks the activated shell. Existing Bead remains recoverable. It retains
QML files under `~/.local/share/nacre/shell/source`, `source.previous`,
`source.good` and `~/.config/quickshell/nacre` while the benchmark gate is open.
The shared `~/.local/share/nacre/shell/tools` and `shell/bin` paths remain product
runtime paths; their name does not make them obsolete QML code.

The way back for a Bead update is `nacre ui-shell rollback`, followed by the
status and journal commands above. A source release rollback uses
`nacre rollback` for the managed file snapshot; a direct install's helper/tree
rollback remains separate. Stage A also retains explicit `ui-shell qml` solely
as compatibility until Stage B. It is never the automatic failure action.
Stage B must record the exact removal/backup policy for old QML files here before
its deletion commit is eligible to push. No command above grants workers live
access.

## Rollback-copy inventory

All 13 `bin/*.rollback` files currently have unverified replacement-trust dates:
`nacre-ai-account.py.rollback`, `nacre-ai-chat.py.rollback`,
`nacre-ai-claude.py.rollback`, `nacre-ai-context.py.rollback`,
`nacre-ai-memory.py.rollback`, `nacre-ai-remote.sh.rollback`,
`nacre-ai-skills.py.rollback`, `nacre-ai-tools.py.rollback`,
`nacre-ai-usage.py.rollback`, `nacre-ai.py.rollback`,
`nacre-brain-maintain.py.rollback`, `nacre-brain-sync.py.rollback`,
`nacre-brain.py.rollback`. Their last edits range from October 21 to November 5;
last-edit dates do not establish trust. Retain pending proof of the trust date;
delete 7 days after each replacement is trusted, with its test/i18n/configure
consumers. The three Brain copies import one another and require a grouped
consumer review.
