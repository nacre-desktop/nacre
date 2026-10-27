# N0088: Install clients, controller modules, and refresh Niri sessions

## Scope

Prevent deploys from leaving stale `~/.local/bin` native client binaries, make
the installed `nacre session` controller self-contained and helpable, and safely
refresh a prepared Niri-on-PC session after generated config changes. This unit
does not execute an installer or change a live desktop; verification uses
temporary HOME fixtures and remote CI only.

## Design

- Install user-facing native command names as symlinks to the installed
  `nacre-helper`. Replace each alias via a same-directory temporary symlink and
  `os.replace`; preserve the first replaced regular file under the shell state
  backup directory.
- Install the controller's transitive Python module set, including
  `package_sessions.py` and its Niri integration. Parse `nacre session` actions
  before importing/starting session integrations. `refresh` prepares through the
  installed helper, validates before replacement, reloads a live Niri session,
  and restores the prior tree in place on failure.
- Build Niri config files in a private staging directory. Run `niri validate`
  against the complete candidate before atomically replacing files in the
  existing config directory, preserving directory inodes for bind-mounted
  sessions. The installer keeps `session.rollback` as the previous copy.

## Acceptance checks

- Fixture verifies regular `nacrectl` replacement, one-time backup, symlink
  targets, and all public helper aliases.
- From a fixture-installed controller, `nacre --help`, `nacre session --help`,
  and each session action's `--help` exit successfully. Assert controller module
  files exist in the installed copy.
- Fixtures verify config validation happens before replacement, a failed
  refresh restores the old tree, custom config remains untouched, and successful
  refresh requests Niri reload when a Niri socket is present.
- Run the full `desktop` CI job in the selected remote Arch CI container.

## Rollback and expiry

The single `session.rollback` copy is retained for recovery; delete it 7 days
after the refreshed session has been trusted in daily use. The one-time backups
of replaced regular client files remain user data and are not temporary copies.

## Evidence

Recorded in `.lead/report.md`; no live HOME, desktop, config, or session changes.
