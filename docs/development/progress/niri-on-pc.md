# Niri on PC progress

Updated: 2026-10-12. Status: phase 1 installed and preflight-tested; real session transition pending.
Branch: `feature/niri-on-pc`. Own worktree: `nacre-niri-on-pc` under the normal
private task-worktree root. One PR for session/tooling/docs work; backend changes
belong to the separate `feature/niri-backend` task.

## Done

- Read repository/workflow instructions and inspected actual SDDM/UWSM startup.
- Wrote TTY recovery first in [niri-testing.md](../niri-testing.md).
- Created private backups and before/after configuration fingerprints. Pointer:
  `~/.local/state/nacre/backups/niri-on-pc-latest`. No backup/account contents in Git.
- Verified the package dry run adds only niri, xwayland-satellite and wlsunset;
  no existing package upgrades are part of this transaction.
- Installed niri 26.04, xwayland-satellite 0.8.3, wlsunset 0.4.0 and read-only
  `wayland-utils` through authenticated pacman. No existing package upgrades.
- Added both named login aliases under `/usr/local/share/wayland-sessions`.
  Package-owned Hyprland entries and SDDM configuration bytes match backups.
- Added separate niri environment/config and service startup gates as new files.
  The shared/Hyprland environment, profile and Hyprland configuration match their
  protected fingerprints; existing Hyprland services remain active.
- Five startup/session fixture tests, Ruff and native niri config validation pass.
- Nested niri IPC preflight passes and leaves activation environment unchanged.
  Sent sanitized schema fields to the backend agent. This does not verify SDDM.
- Confirmed shell supervision/session observation and idle resume contain
  Hyprland assumptions. Plan an isolated niri path, preserving the working source.
- Coordinated target version 26.04 with the niri backend agent. Its branch is
  stacked on compositor PR3; wait for its tested revision before phase 2.

## Next

1. Commit/push the installed/preflight milestone and complete repository CI.
2. Prepare a persistent private acceptance recorder before any real logout.
3. Save all active parallel work, then physically test TTY recovery, niri
   login/logout and return to Hyprland. Logout ends graphical terminal agents;
   resume this same chat/task from this checkpoint after testing.
4. Only after phase-1 acceptance: consume the backend agent's confirmed revision,
   isolated source/personal generated config, manual lock/unlock, idle/night light.
   Integration agent is merging prerequisite PR2/PR3/PR4; do not deploy main early.

## Open questions / limits

- Phase-1 SDDM login/logout and TTY recovery are not yet physically verified.
- Backend revision/generator command are pending the backend agent's milestone.
- Never store authentication values in this note, commands or logs. Use normal
  authenticated system actions; leave account password and private AI policy intact.
- Do not restart SDDM or stop the working Hyprland session during preparation.
- Public results belong in niri-testing.md; private machine/session details stay in
  local backup/checkpoint storage.
