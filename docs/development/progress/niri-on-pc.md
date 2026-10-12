# Niri on PC progress

Updated: 2026-10-12. Status: phase 1 preparation; no live session transition yet.
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
- Confirmed shell supervision/session observation and idle resume contain
  Hyprland assumptions. Plan an isolated niri path, preserving the working source.
- Coordinated target version 26.04 with the niri backend agent. Its branch is
  stacked on compositor PR3; wait for its tested revision before phase 2.

## Next

1. Commit/push this fallback milestone and open the task's draft PR.
2. Back up root session metadata, install the reviewed pacman additions and add
   the two named SDDM aliases without changing existing Hyprland entries/config.
3. Prepare plain niri with UWSM lifecycle and automatic-lock gate. Verify generated
   units/config before a real logout.
4. Save all work, then physically test TTY recovery, niri login/logout and return
   to Hyprland. Logout ends graphical terminal agents; resume this chat/task from
   this checkpoint rather than pretending that nested niri proves SDDM acceptance.
5. Only after phase-1 acceptance: consume the ready backend revision, isolated
   niri source/personal generated config, manual lock/unlock, idle and night light.

## Open questions / limits

- Phase-1 SDDM login/logout and TTY recovery are not yet physically verified.
- Backend revision/generator command are pending the backend agent's milestone.
- Never store authentication values in this note, commands or logs. Use normal
  authenticated system actions; leave account password and private AI policy intact.
- Do not restart SDDM or stop the working Hyprland session during preparation.
- Public results belong in niri-testing.md; private machine/session details stay in
  local backup/checkpoint storage.
