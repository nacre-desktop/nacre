# N0018 — Native keystroke launchers

- Status: In progress (Codex worker, lead review)
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
