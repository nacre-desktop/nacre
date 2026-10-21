# N0018 — Native keystroke launchers

- Status: Implementation and evidence complete (Codex worker; lead review/integration pending)
- Date: 2026-10-21
- Plan: AI/Brain/Wave-4 migration plan 2026-10-21

Goal: `nacre-app`, `nacre-settings`, `nacre-welcome`, `startup-apps` (plus `hypr/scripts/startup-apps.sh`) run as
native multi-call commands, removing a Python start from every launcher hotkey (SUPER+RETURN/E/SHIFT+B/I,
login autostart). Foundation: keybindings use `nacrectl`, no Python.
Scope: ports of bin/nacre-app (112), bin/nacre-settings (100), bin/nacre-welcome (92), shell-tools/startup-apps.py
(106); keep `~/.local/bin/<name>` paths as thin installed links to the helper; keep restricted-niri-session and
host-terminal logic, valid-route checks and literal argv (no shell strings). Non-goals: welcome.py, profile-policy,
generated keybinding files, compositor config, daemon methods.
Dependencies: none (reuse `menu_worker`/`menu.rs` launch code and compositor crate). Parity: cases `nacre-app`,
`nacre-settings`, `nacre-welcome`, `startup-apps`, `window-hide` style fake-binary harness; plus a cold-start
timing note (ms and Python launches 0).
Risk: low-medium (keybinds are the daily path; failure is visible at once). Size: S-M. Parallel: yes.
Stop: any behavior that needs a compositor-contract change.

## Common rules

Common to every unit (not repeated): base is latest main; `[no-publish]` on every commit; private/nested
sessions only; nice 10, cargo -j2; frozen parity references under `tests/parity/helpers/<name>` are replayed
with `run.py --command 'nacre-helper {name}'` and never regenerated; historical Python test assertions are
kept as Rust tests; security additions are explicit new cases with intended-difference notes; no live
desktop, installed Nacre, units, keybindings or publication; existing AI permissions and the paru command
unchanged; standalone native command ships first, daemon hosting is a separate unit. Status for all: Proposed.
Numbers are proposals; N0017 is the last used.

## Evidence

- Replayed the frozen N0018 fixtures with `tests/parity/helpers/run.py --case 'nacre-app/*' --case 'nacre-settings/*' --case 'nacre-welcome/*' --case 'startup-apps/*' --command 'nacre-helper {name}'`: 9 cases, 0 failures. Frozen expected files were unchanged.
- The full frozen parity run passed: 534 cases, 0 failures. Rust launcher tests retain app argv and the Niri terminal bridge assertions, route validation, Welcome claim/ack/retry behavior, and startup readiness/deduplication/workspace assertions.
- Replayed the complete `desktop` job in `ghcr.io/archlinux/archlinux:latest` on dev-1, in the task's named container and tmux session. `tools/check.py` passed: tools 441 tests (1 existing skip), AI 116, Brain 45, and shell-tools 478; Niri/Hyprland legacy assertions, native translation audits (244 and 185 states), gallery checks, QML formatting and all build checks passed. Overall CI command exited 0.
- In a private HOME with a fake browser and a `python3` PATH trap, the first `~/.local/bin/nacre-app browser` process launch took 2.848 ms and recorded 0 Python child launches. The filesystem cache was warm; this measures first process startup, not cold-disk I/O.
