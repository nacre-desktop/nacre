# Niri on PC progress

Updated: 2026-10-13. Status: first real Nacre/niri login confirmed. Live scale and
touchpad/two-/three-finger direction fixes applied and confirmed by the operator.
Return-to-Hyprland, lock/idle/night light and full acceptance remain pending.
Branch: `feature/niri-preview` (historical PR18; direct integration into main under
the operator's updated 2026-10-13 policy). Original PR7 merged. Own worktree:
`nacre-niri-on-pc` under the normal private task-worktree root. No new/updated PRs;
backend changes
belong to the separate `feature/niri-backend` task.

## Done

- Operator first requested full-width ordinary apps with half-width Nacre AI, then
  explicitly reverted all apps to standard niri half-width. Saved generator now
  matches the final 0.5 live default. Added scoped rules for both AI classes;
  fresh dashboard/task probes measure 932 logical pixels. Added native Super+R
  width cycling (one-third, half, two-thirds, full); existing fine/mouse resizing
  stays available. AI/profile access policy and launcher scripts are unchanged.
- Published prior live fixes at `6dd866e`; local 726 tests and exact-head desktop
  CI `38369266679` pass. Later AI-width additions need final integrated validation.
  Backend gradient is now on main `220745d` with passing CI. Redock task owns its
  controlled source reload and receipt; this task keeps host config separate.
- Confirmed actual niri UWSM compositor and Nacre shell are active after greeter
  login. Imported saved host scale/natural-scroll and native overview/wheel aliases
  through live config reload. Operator confirms scale and scroll directions.
- Fixed owned control-tools status capability handling; unsupported night-light
  probes no longer break the right controls panel's report. Staged only this tool,
  no shell restart. Detached dashboard and current chats remain running.
- Replaced stale seeded colors include with the existing live palette output;
  consumed backend owner's tested `b2a8527` gradient template into staged source;
  pure palette render/native reload pass without restart. No backend code edits.
- Tested full-width new columns, then reverted the default to 0.5 on the operator's
  newer request. Saved outer gutters/border width remain. Super+M uses
  native maximize-column for tiled windows; prior maximize-to-edges removes gaps.
  Existing columns keep width until resized; floating/explicit-target facade parity
  remains with backend owner.
- First local validation rerun hit an inherited `QT_LOGGING_RULES=*.debug=false`
  suppressing the benchmark's console results. Rerun with that test-process
  logging override removed; do not change the user's runtime logging preference.
- Installed full-login selection, namespace wrapper, explicit-lock shim and shell
  service drop-in with private before/after receipts at
  `~/.local/state/nacre/backups/niri-full-login-latest`. Protected Hyprland/common
  env/profile/config bytes, active shell/idle PIDs and activation environment match.
  Native niri config and systemd unit verification pass; no session switch/restart.
- Full local `tools/check.py` passes 696 tests, lint/format, Qt/QML, boundary/config,
  palette CLI and JavaScript checks. Five new login-runtime tests pass; preview
  tests remain eight. CI must be checked on the final published head.
- Prepared separate full-login code/config and a niri-only executable shim. UWSM
  retains lifecycle ownership; Hyprland shell startup passes through to the same
  original controller. Nested supervisor preflight loads the shell; GPU device
  access in the real code namespace passes. No backend code edited.
- Explicit Lock can invoke hyprlock in niri while automatic idle stays gated;
  unlock remains physically unverified. Separate observer gate prevents unsupported
  monitor restoration; reproducible issue reported on PR12.
- Closed the nested interactive preview at the operator's request. Actual logout
  has not been performed; other graphical chats remain running.
- PR7 preparation and PR12 backend are now merged. New preview work is a follow-up
  because merged PR7 cannot receive new changes. Preview baseline: main `3c87a61`.
- Added an isolated preview runner and regression tests for session isolation,
  profile preservation, resolved release mounts, wallpaper assets and native KDL.
  Preserves personal profile and AI permission; no live source replacement.
- Generated personal niri config validates. Nested shell loads; launcher/controller,
  pinned dashboard, detached launcher/window, notification capture, Wayland/X11 apps and
  grim output probes pass. Global activation environment remains unchanged.
- Synthetic wtype events inside nested niri did not activate the Alt aliases;
  native parsing passes, but real focused-window keyboard testing remains pending.
- Corrected preview-only runtime-symlink and Quickshell config-path mismatches;
  no backend source edits. Added accessible Alt aliases and private Kitty cache.
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
- Backend source/fixture readiness is now verified at PR12 head
  `c37401543cf98043fe4e79fe2e0b1f5c4def2cdf`, based on `9a38ff0`.
  Exact desktop CI run `38257405006` passes: 650 reported tests, three skips.
  This resolves the source-revision dependency; live/setup readiness remains false.

## Next

1. Keep the current niri desktop, detached dashboard and parallel chats running.
2. Commit/rebase on the latest default main, validate and push a normal fast-forward
   main update under the operator's direct-publication policy; monitor CI. No PR
   creation/update or bulk existing-PR merging. Operator renewed GitHub auth.
3. Coordinate
   staged tool/QML updates with the separate redocking task. Do not restart the shell
   or overwrite its staged tools independently.
3. When the operator is ready, restart the private recorder, save all work and
   perform TTY recovery + actual niri SDDM login/logout + return to Hyprland.
4. Resume this same task and inspect recorder evidence. Review generated-lock
   appearance drift rather than silently resetting current preferences.
5. Only after real phase-1 acceptance: integrate the isolated source/personal config
   into actual niri startup, manual lock/unlock, then idle and protocol-based night
   light. Preserve Hyprland policy and private preferences.

## Verified backend handoff (2026-10-12)

Superseded for source selection by merged PR12 `ccc86d3` and preview baseline
`3c87a61` (2026-10-13). Historical CI evidence below remains useful; actual login
and packaged setup readiness remain unverified.

[PR12](https://github.com/nacre-desktop/nacre/pull/12) at
`c37401543cf98043fe4e79fe2e0b1f5c4def2cdf` has
[passing exact-head CI](https://github.com/nacre-desktop/nacre/actions/runs/38257405006).
Read its `docs/development/progress/niri-backend.md` and compositor guide before
consuming it. Full Wayland/UI/pearl acceptance is still deferred by the operator.

For a future isolated probe, export these **before starting nested niri** so spawned
shortcuts inherit them; configuring only Quickshell does not isolate key bindings:

```sh
export NACRE_COMPOSITOR=niri
export NACRE_COMPOSITOR_TOOLS="$PWD/nacre/shell-tools"
export NACRE_PEARL_TOOLS="$NACRE_COMPOSITOR_TOOLS"
export NACRE_QS_CONFIG="$PWD/nacre/shell/shell.qml"
export NACRE_QS_BIN=$(command -v quickshell)
```

These commands describe the backend source worktree and are not a live deployment.
Inherited app-launch scripts still resolve installed paths; matching isolated
runtime belongs to this task/packaging work. Never start the full second shell on
the daily display/DBus session. Generator:
`python3 tools/generate_compositor_config.py --backend niri`; generated fragments
are `niri/bindings.kdl` and `niri/window-rules.kdl`. Personal keyboard/profile and
named workspace routes require the backend's public render APIs and explicit name
mapping, with private source/preferences kept out of Git.

Known unsupported capabilities include global cursor query, window capture,
native drag-end/physical shortcut release, pin, runtime monitor/desktop settings
and backend night-light control. Test existing focused-pearl detach toggle and
`pearls dock` instead of inventing a timer-based drag completion. Lock/idle and a
protocol-based night-light owner remain this task's later phase.

## Open questions / limits

- Phase-1 SDDM login/logout and TTY recovery are not yet physically verified;
  the operator explicitly deferred logout to preserve the active desktop.
- Backend PR12 is source/fixture-ready, not a completed live or packaged session.
  Preserve `setup_ready=false` until packaging/config/runtime integration passes.
- Never store authentication values in this note, commands or logs. Use normal
  authenticated system actions; leave account password and private AI policy intact.
- Do not restart SDDM or stop the working Hyprland session during preparation.
- Public results belong in niri-testing.md; private machine/session details stay in
  local backup/checkpoint storage.
