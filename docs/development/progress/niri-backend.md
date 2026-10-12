# Niri backend progress

## Done

- Branch `feature/niri-backend`, isolated `nacre-niri-backend` worktree, one
  PR: https://github.com/nacre-desktop/nacre/pull/12. Every milestone is committed
  and pushed. Current baseline: main `4ee5346`, including compositor PR #3,
  diagnostics/performance integration, approved Orient dev5 pin, and panel fixes.
- Python JSON socket backend and event reducer: initial state, outputs refreshed
  on workspace/config changes, stable opaque u64 IDs, canonical events, focus,
  urgency, dynamic per-output indices, stale IDs and unknown future events.
- Actions: focus/close, workspace/window movement, floating/resize/placement,
  directional scrolling-layout operations, fullscreen/maximize, reload and logout.
  Owned pearl title+PID checks retained; placement uses measured relative deltas
  in logical pixels and requeries after cross-output moves. No guessed struts.
- QML facade selection, persistent stream, stable window objects, serialized
  actions, bounded reconnects and shortcut pulses. Disconnect clears pending work
  and blocks new/queued actions. Invalid selection fails; unused Hyprland singleton
  stays uninstantiated. Necklace/overview use focused-output display indices.
- Minimal caller fixes: app activity recognizes opaque IDs, unsupported minimize
  avoids empty processes, capture timers honor capabilities, wallpaper workspace
  identity is `var`. No image/provider changes; memory agent notified.
- Shared-source KDL bindings/rules/unsupported report; Orient role palette template.
  Public config/keyboard/profile APIs; explicit name mapping required for initial
  numeric profile routes. Generator never writes user-owned `niri/config.kdl`.
  All six Hyprland-generated files and immutable registrations remain unchanged.
- Targeted checks pass: 22 backend/QML/real-Quickshell harness tests and 9 config
  tests. Real Quickshell + source Python helpers exercise state and focus/close
  on a private fake socket with no compositor/windows. Native Qt tests execute
  actual app/wallpaper predicates and facade backend-selection behavior.
- Complete rebased source check passed 648 tests on `b1267f8` plus lint, format,
  native source parsing, boundary and generator checks; later progress edits are
  documentation only. Niri 26.04 validates a temporary
  include wrapper and palette; test proves parser acceptance and unchanged wrapper,
  not inspection of niri's effective resolved configuration. Required `desktop` CI status for the exact branch head is recorded on PR #12.
- Independent documentation/code reader review completed; all findings resolved,
  including action queue on disconnect and source-binding environment inheritance.

## Next

- Review/merge PR #12 through the enforced final-head CI gate. The backend's
  implementation/fixture criteria are complete; current CI result remains linked
  in the PR instead of freezing a possibly stale check status in this note.
- PC/session agent can perform live Wayland/UI/pearl acceptance after the operator
  resumes its session phase. Keep Hyprland running until then; no deployment here.
- Preserve this clean worktree for branch acceptance/review. Remove it only after
  no further edits are needed; exact tested source and logs are recorded below.

## Open questions / known differences

- Operator requires keeping Hyprland running. PC agent verified nested niri 26.04
  IPC schema independently; real login and branch Wayland/UI/pearl acceptance are
  deferred. No live session acceptance claim is made by this task.
- Unsupported capabilities: pin, special-workspace hide/force-kill, global cursor
  query, window capture, physical shortcut release, fullscreen metadata, runtime
  desktop/monitor configuration and night-light controller. Fail explicitly.
- Niri-native Mod+mouse movement lacks a drag-end event. Existing application-owned
  lift/release and focused-pearl detach toggle / `pearls dock` IPC remain; never
  infer release from an idle timer.
- `setup_ready=false` for niri until the packaging agent completes offline initial
  config/includes/keyboard/profile deployment. Backend support alone is not
  installer/session readiness. No installer, system package, session, Orient-repo,
  private profile, account or AI-permission edits by this task.
- Generated compositor actions honor `NACRE_COMPOSITOR_TOOLS`; shortcut IPC honors
  `NACRE_QS_CONFIG` and `NACRE_QS_BIN`. Export them before starting isolated nested
  niri for key testing: `spawn-sh` inherits niri's environment. Setting exports only
  for Quickshell tests direct IPC. Inherited app-launch scripts still use installed
  paths; matching isolated runtime is PC/packaging scope.

## Checks / continuation

- `/tmp/nacre-niri-full-check.log` and `.exit`: full local gate.
- `/tmp/nacre-niri-targeted.log`, `/tmp/nacre-niri-config-tests.log`: focused gates.
- `/tmp/nacre-niri-native-runtime-test.log`: real Quickshell fake-socket integration.
- Run `git fetch origin refs/heads/main:refs/remotes/origin/main`, verify clean
  state, then rebase when needed. Remote fetch refspec tracks main only; use an
  explicitly known old task-head lease for rebased pushes. Honor scoped pre-push
  privacy guard; no hook/settings modifications by this task.
- Use `NACRE_COMPOSITOR=niri`, source `NACRE_COMPOSITOR_TOOLS` and
  `NACRE_PEARL_TOOLS` for helpers. Docs distinguish offline generator/validator
  commands from queries requiring the intended `NIRI_SOCKET`.

## Sources

- [IPC](https://niri-wm.github.io/niri/IPC.html)
- [26.04 schema](https://github.com/niri-wm/niri/blob/v26.04/niri-ipc/src/lib.rs)
- [Include merging](https://niri-wm.github.io/niri/Configuration:-Include.html)
- [Floating windows](https://niri-wm.github.io/niri/Floating-Windows.html)
