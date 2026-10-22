# N0029 — nacre-ai dashboard

Status: Done (Luna medium; branch `task/n0029-ai-dashboard-20261022`).

## Goal

Port the standalone `nacre-ai` terminal dashboard and its local/SSH task, history,
account, project, settings, usage and brain-launch workflows to the native helper.
Keep the existing Kitty UI: the owner uses it daily and requires its layout, keys,
palette colours, account switching, chat list and resume behavior to remain
identical. Keep the `nacre-ai-dashboard` / `nacre-ai-task` window classes, desktop
entry and Niri rules. This is a pure Rust port, not a QML window.

## Dependencies and boundary

Depends on N0021, N0026 and N0028. N0028 is in progress in another worktree; do
not edit its files (`crates/nacre-helper/src/ai_chat*`, `ai_claude*`,
`bin/nacre-ai-chat`, `bin/nacre-ai-claude`). Interact through the existing
`nacre-ai-chat` provider command contract and document any API follow-up needed.
N0021/N0026 command helpers are consumed as existing CLI boundaries.

## Scope

- Native `nacre-ai` command: same default dashboard, settings, account selection,
  project wizard, local/SSH launch, worker windows, history listing/resume, brain
  open, and startup/login command behavior.
- Reuse existing helper state, process, branding text logo and terminal palette
  facilities where they fit; retain curses-like alternate-screen behavior, search,
  navigation, cancel/back behavior and Kitty image output semantics.
- Preserve account/project/settings schemas and byte-identical permission
  defaults. All tests use private temporary HOME trees and never inspect real
  credentials.
- Keep `nacre/desktop/nacre-ai.desktop`, Kitty classes and any Niri rules intact.

## Acceptance

- Replay frozen `nacre-ai` cases with `run.py --command 'nacre-helper nacre-ai'`;
  never regenerate frozen references. Deterministic terminal output is byte
  compared. SSH behavior uses a fake `ssh` executable.
- Retain the historical assertions from `ai/tests/test_workflow.py` and
  `ai/tests/test_codex_startup.py` as Rust tests, including cancellation,
  account forwarding, GUI log isolation, alternate screen, root menu, exact
  history selection/resume, local startup recovery, project creation and SSH.
- Add explicit security cases with intended-difference notes. Permission defaults
  remain byte-identical. Use temporary HOME only.
- Run the full `desktop` job in the required Arch CI container on prod and report
  exact commands/results. Confirm GitHub CI green before safe-push.
- Report product line additions/removals. If native product exceeds 2.5x the
  1,592-line Python original, do a simplicity pass before Done.

## Temporary duplicates and expiry

The Python original `bin/nacre-ai` and any new `*.py.rollback` duplicate remain
until pass (a); delete them 7 days after the native replacement is trusted. Do
not create additional rollback copies. Frozen parity references remain immutable
and are removed only with their original in the planned clean-up pass.

## Non-goals and constraints

No daemon hosting, installer/deployment or live desktop action; standalone native
command ships first. No changes to N0028-owned files, AI permission defaults,
`paru -Syu --skipreview`, keybindings or units. No tags, releases or packages.
Commit as `siverteh <92440237+Siverteh@users.noreply.github.com>` with
`[no-publish]`; push only after full CI and confirmed GitHub green using the
specified safe-push flow.

### N0028 provider-command follow-up

The dashboard uses the command boundary for `nacre-ai-chat list/resume` and
`nacre-ai-claude list/new/resume/login`; local Codex general-chat creation stays
in the dashboard so it can create and trust the private scratch directory with
the same selected profile and permission flags. N0028 should confirm the chat
CLI options and preserve the `--all-accounts` compatibility fallback. N0029
does not import or reach into N0028 implementation files.
