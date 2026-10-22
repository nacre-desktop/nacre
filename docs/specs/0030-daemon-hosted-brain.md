# N0030 — Daemon-hosted Brain

- Status: In progress
- Dependency: N0024 native Brain web worker (landed)
- Risk: medium; size M; parallel: no. Owner-side decision (lead, 2026-10-22):
  residency (a `brain.service` replacement) is dropped in favour of on-demand start.

## Goal

`nacrectl brain open|capture` and a `brain` status topic. The daemon starts the
native launcher worker on demand; that worker starts the N0024 loopback web
server, which idles out after five minutes. The resident `nacre-brain.service`
is no longer installed and an installed one is stopped, disabled and removed.

## Design

- `daemon/brain.rs`: topic `brain` = `{open, capture, error}` (busy flags plus a
  generic error string; no raw errors). Methods `brain.open` and `brain.capture`
  (desktop capability, empty params) start `/proc/self/exe brain-control action
  brain|capture` as a process-group child with `NACRE_BRAIN_DIRECT=1` and return
  `{started, busy}` at once. One run per action; open is limited to 60 s,
  capture to the existing one-hour editor limit. Absent in safe mode.
- Window and workspace handling stays in the existing launcher
  (`brain_launch.rs`), which talks to the compositor itself. Moving it onto the
  daemon's compositor connection is not needed for on-demand start and is not done;
  a separate follow-up if wanted.
- `nacre-brain open|capture`, `nacre-brain-ui` and `brain-control action` call the
  daemon first (`brain_launch::action`). Without a daemon, or one lacking the
  methods, they run the direct launcher as before. SUPER+B / SUPER+N bindings,
  window classes and generated compositor config are unchanged.
- `brain/install.py` no longer writes or enables the unit, no longer waits for a
  resident server, and removes the old unit (backed up first). Arch packaging, release
  tooling and package_runtime still stage/link the unit (tests assert it); see below.

## Temporary duplicates (expiry)

- Direct launcher path in `brain_launch::action` and `watch-brain`
  (`brain_launch::watch_brain`, Python `control.py watch-brain`): delete 7 days
  after daemon-hosted Brain is trusted on the owner machines (with clean-up pass (a)).
- `brain/brain.service`, its staging in `packaging/arch/stage.py`, `tools/prepare.py`,
  `tools/package_runtime.py` and `tools/releases.py` (package/release flows, not yet
  migrated; packaged installs still start the resident watcher), the Rust niri
  session stage/runtime and their frozen references, and the service name in
  maintenance/diagnostics lists: kept only because frozen references must not be
  hand-edited; delete or regenerate with pass (a).

## Acceptance

- Rust tests for the registry, parameter rejection and single-flight worker; the
  protocol/daemon suites and 534 frozen parity cases pass unchanged.
- `brain/tests/test_install.py` covers unit removal and backup restore.
- Full `.github/workflows/check.yml` desktop job in the Arch container on a remote host.
- No live units, keybindings or desktop changes; private/nested sessions only.
