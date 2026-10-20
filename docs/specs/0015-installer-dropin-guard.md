# N0015 — installer drop-in retirement guard

The shell installer must not delete code still referenced by managed user-unit
drop-ins. Both full and `--code-only` installs inspect these drop-ins before any
build, backup, deployment or systemctl operation. Inspection includes the shell,
sidebar, session watcher, Niri widths, power key, idle, daemon and masked/disabled
units, plus systemd type and dash-prefix drop-ins. User config/data/runtime search
locations, XDG search directories and `SYSTEMD_UNIT_PATH` are inspected directly;
no manager query is needed. Unreadable drop-ins or directories abort inspection.

The guard checks `ExecStart*`, `ExecCondition*`, `ExecStop*`, `Environment*` and
`EnvironmentFile` values for retired helper/script paths and conditionally retired shell config/DBus
files, including `%h`, HOME
spellings, continuations and systemd C escapes. It reports every conflicting
file/path pair. It conservatively inspects shadowed assignments and drop-ins too.

For the three already-retired `niri-on-pc` scripts (`niri-session-runtime.py`,
`niri-acceptance-monitor.py`, `niri-preview.py`), the supported opt-in is:

```sh
python3 nacre/shell-tools/install.py --retire-niri-experiment
# Add --code-only when performing a code-only install.
```

Only drop-ins referencing those script paths are removed, after their original
bytes or symlink identity and SHA-256 hashes are recorded in the installer's
backup/manifest. A conflict with another retired helper still refuses even with
this flag: edit or remove the named drop-in to use a supported native command,
then rerun. Unrelated drop-ins remain untouched. In particular, an acceptance
drop-in referencing only `niri-session-gate.py` remains: that gate is not retired
by this installer. The flag does not retire the entire experiment directory.

The three scripts are also backed up before removal, even without drop-ins.
`python3 nacre/shell-tools/install.py --restore` restores the tracked scripts and
drop-ins, retaining file bytes/modes and symlink identity. Restore refuses later
edits at retired targets or corrupted retirement backups before stopping services.
This extends the existing installer rollback; it does not make the entire deployed
source/runtime tree part of that rollback.

Regression tests use private HOMEs, fake systemctl and fixture deployment for
both install modes. They cover refusal without writes, managed-unit/search-path
coverage, opt-in selectivity, hash/mode/byte restoration, symlinks, masks, parser
escapes, unreadable directories, no-drop-in retirement and an absent experiment.

The CLI help change is tracked by the ordinary `install/help` Python reference:
the case invokes `nacre/shell-tools/install.py --help`, and `install` is not a
native helper in the parity registry. The documented single-case `--record`
workflow updates only `expected-help.json` for the intentional
`--retire-niri-experiment` option. This case is not a frozen F4 or Wave 3 source
reference.
