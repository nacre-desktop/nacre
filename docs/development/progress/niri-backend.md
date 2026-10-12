# Niri backend progress

## Done

- Task branch `feature/niri-backend`, isolated `nacre-niri-backend` worktree.
- Read the compositor facade and `docs/development/compositors.md` mapping.
- Verified niri 26.04 IPC schema and include behavior in upstream documentation.
  Stable IDs differ from changing per-output workspace indices. Includes are
  supported since 25.11 and merge top-level sections; generate managed fragments,
  never replace the user's `niri/config.kdl`.
- Coordinated ownership with the PC/session, packaging and PR-integration agents.

- Python JSON socket backend and canonical event reducer implemented. Fifteen
  fixture/action/transport tests pass, including large IDs, focus clearing,
  output-relative resolution, stale events and measured floating placement.
- Python facade selects the backend without import-time IPC; exposes `events`
  for QML's persistent consumer. QML event cache, serialized actions, shortcut pulses and reconnect tests pass.
  Focused-output necklace uses display indices while windows keep stable IDs.
  Sixteen targeted Python/native-Qt tests pass; not yet live-ready.

- KDL bindings/rules/unsupported report consume the existing neutral source.
  Native niri 26.04 validates a temporary wrapper with Orient colors and later
  user-owned overrides. Eight config tests and seventeen Python/QML tests pass.
- Six Hyprland-generated files are byte-identical. Boundary checks now include
  niri, and public keyboard/profile/config metadata APIs are exposed.
- Current main baseline is `fa37a28`; QML milestone is pushed as `9cf7807`.
  The scoped pre-push guard is honored; no hook/settings edits by this task.

- Caller audit fixed opaque-ID numeric assumptions in Settings/Colors/Welcome
  activity and focused-output overview grouping. Hyprland branches are retained.
  Native-Qt tests execute the actual app activity predicates for both ID models.
- Full-check pass 1 found stale single-backend generator-test assumptions; updated
  fixture output maps and the unknown-backend assertion, keeping strict validation.
- Full-check pass 2 reached all shell tests; only the facade harness lacked the
  new session script. It now verifies lazy backend creation, niri index/ID split,
  output grouping and invalid selection without warnings. A transient alias-type
  warning was fixed by reading active workspace and capabilities from one backend.
- Current fixture checks: 18 backend/QML harness tests plus 8 config tests pass;
  common generator and facade tests pass. Full source check is being rerun.

- Independent review fixes: preserve wallpaper workspace identity (`var`, not
  integer), clear queued actions and guard execution when disconnected, and honor
  source helper/QS config overrides in generated compositor/shortcut bindings.
  Wallpaper image/provider code is unchanged; the memory agent was notified.
- Real Quickshell + real Python helper integration passes on a private fake Unix
  socket with no compositor/windows: initial state, exact large ID, focus and close
  roundtrip. Native-Qt fixtures also cover actual wallpaper/app predicates.
- First complete source check passed 642 tests and all lint/format/native-source/
  boundary/generator checks. Final review fixes and new integration/event tests
  require a final rerun and final-head CI. PR #12 is draft; previous head CI passed.
- Operator requires preserving the running Hyprland session. The PC agent's real
  login/phase-2 UI acceptance is deferred; no session change is needed for this
  task's fixture/CI completion criteria and no live Wayland claim is made.

- Complete check on `ec7730c` passed 647 tests plus lint/format/native-source,
  boundary, immutable Hyprland registrations and generated outputs. Native KDL
  validator and real Quickshell fake-socket integration passed locally.
- Main advanced to `4ee5346` with the approved Orient dev5 pin and panel
  hide/docking fixes during validation; rebase and final checks/CI are next.
- Source generated-key overrides must be inherited by the nested niri process
  itself. Starting only Quickshell with those exports tests direct IPC, not keys.
  Documentation now distinguishes these paths and keeps app-bind limitations clear.

## Next

4. Push each tested milestone; give the PC agent an exact revision for live checks.
5. Run full tools/check.py, rebase if main moves, open one PR and await CI.

## Open questions and integration limits

- PR #3 is merged at `1c38f0d`; PR #4 merged as `fa37a28`. This branch
  includes both, preserving diagnostics and explicit Hyprland drag release.
- PC agent installed niri 26.04 independently and confirmed the nested IPC
  schema. Full plain-niri login and branch UI/pearl acceptance are pending.
- Niri lacks Hyprland pin/special-workspace semantics and a global cursor-position
  query. Report those capabilities honestly; preserve owned title+PID checks for
  pearls and distinguish native drag hooks from application-owned docking.
- No system packages, session files, installer code or private preferences changed.

## Sources

- [IPC](https://niri-wm.github.io/niri/IPC.html)
- [26.04 IPC schema](https://github.com/niri-wm/niri/blob/v26.04/niri-ipc/src/lib.rs)
- [Include merging](https://niri-wm.github.io/niri/Configuration:-Include.html)
- [Floating windows](https://niri-wm.github.io/niri/Floating-Windows.html)

## Continuation notes

- Python facade/backend and QML singleton are source-selectable; no session or
  global installed files have been changed. `NACRE_COMPOSITOR_TOOLS` and
  `NACRE_PEARL_TOOLS` are source-helper overrides for isolated live validation.
- Target logs: `/tmp/nacre-niri-targeted.log`,
  `/tmp/nacre-niri-config-tests.log`, `/tmp/nacre-niri-config-check.log`.
- Public setup gate is intentionally `setup_ready=false` for niri. Packaging
  agent queries it and retains Hyprland-only setup until its offline deployment
  supports initial includes, keyboard and explicit named profile routes.
- Runtime pin/hide/capture/cursor/fullscreen metadata and desktop/monitor/controller
  settings differ; capability flags and compositor docs state the limits.
- Native Mod+mouse drag-end events are unavailable. Existing application-owned
  lift/release and focused-pearl detach toggle/explicit dock IPC remain available;
  do not fake release based on an idle timer.
