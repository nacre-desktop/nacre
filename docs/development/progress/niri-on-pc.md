# Niri on PC progress

Updated: 2026-10-12. Status: phase 1 installed/preflight-tested; operator requests keeping Hyprland running.
Actual session transition and phase 2 remain pending.
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
- Nine startup/observation tests, Ruff and native niri config validation pass.
- Rebased onto current main `d08e06d` (including prerequisite `fa37a28`); full local gate passes 625 tests and
  GitHub desktop CI passes (two CI skips; none locally).
- Nested niri IPC preflight passes and leaves activation environment unchanged.
  Sent sanitized schema fields to the backend agent. This does not verify SDDM.
- Confirmed shell supervision/session observation and idle resume contain
  Hyprland assumptions. Plan an isolated niri path, preserving the working source.
- Prepared a bounded private acceptance recorder outside the graphical lifecycle,
  then stopped it because real switching was deferred. Restart instructions and
  artifact names are in niri-testing.md. No account linger setting changed.
- Protected drift is limited to generated hyprlock appearance (image/colors);
  auth/geometry and other fingerprints match. Preserve current presentation and
  original backup; palette/wallpaper update is consistent with the timestamps,
  but exact trigger attribution remains unverified.
- Coordinated target version 26.04 with the niri backend agent. Its branch is
  stacked on compositor PR3; wait for its tested revision before phase 2.

## Next

1. Keep the current Hyprland desktop and all parallel graphical agents running.
2. When the operator is ready, restart the private recorder, save all work and
   perform TTY recovery + actual niri SDDM login/logout + return to Hyprland.
3. Resume this same task and inspect recorder evidence. Review generated-lock
   appearance drift rather than silently resetting current preferences.
4. Only after phase-1 acceptance: consume the backend agent's confirmed ready
   revision, isolated source/personal generated config, manual lock/unlock, then
   idle and protocol-based night light. Backend milestone `504d308` is not yet a
   full-ready declaration; ask for the latest tested revision at resume.

## Open questions / limits

- Phase-1 SDDM login/logout and TTY recovery are not yet physically verified;
  the operator explicitly deferred logout to preserve the active desktop.
- Backend revision/generator command are pending the backend agent's milestone.
- Never store authentication values in this note, commands or logs. Use normal
  authenticated system actions; leave account password and private AI policy intact.
- Do not restart SDDM or stop the working Hyprland session during preparation.
- Public results belong in niri-testing.md; private machine/session details stay in
  local backup/checkpoint storage.
