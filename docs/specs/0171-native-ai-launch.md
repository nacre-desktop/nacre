# N0171 — native AI launch compatibility

The legacy Load chat action emits `kitty --class nacre-ai-task --title "Load chat"
-- nacre-ai window --worker-command sessions`. The native controller advertised
`window` but rejected it before opening history. Python implemented this command
and paused after startup failure; kitty therefore disappeared only with the
native replacement. The newer Luau recent-chat row instead emits
`nacre-ai resume-record <base64 record>` and needs its own regression coverage.

## Change

- Restore `window --worker-command {new,latest,sessions,shell}` using existing
  history/project workers and the existing Niri namespace handoff. Preserve
  project, provider and account selection. Both terminal entry points show
  errors and wait for Enter when attached to a terminal; noninteractive callers
  receive a nonzero status without blocking.
- tmux can return success after its provider fails. Claude and Codex pane
  commands therefore share a small POSIX-shell failure hold. This keeps the
  error visible without retaining the Rust transcript reader during the chat.
- Local Codex resume selects the authentication home independently of history
  ownership, resets inherited CODEX_HOME/history overrides, prepares the selected
  config, then applies only an explicit legacy history override. Existing
  provider permission flags remain unchanged.
- Eight installer/configurator entry points previously ignored argv. Parse it
  before side effects: help exits successfully, unknown arguments exit 2, and
  neither installs, downloads, edits files nor reloads services.

This adds one missing CLI boundary, reusing existing workers and error text;
it adds no daemon, background job, alternative controller or new dependency.
The Python rollback controllers remain the comparison and rollback reference.
Trust requires these regressions, full desktop CI, and the lead's live acceptance;
delete **7 days after the replacement is trusted**, retiring associated parity
fixtures in that owner-controlled cleanup.

## Acceptance and rollout

Private-HOME subprocess/PTY tests execute both terminal launch contracts against
fake providers, compare the supported Python path, and check visible failure,
default/named accounts, explicit history and stale inherited environment.
Installer tests exercise help/unknown arguments with an unchanged empty HOME.
Native `resume-record` has no Python CLI counterpart; its provider result is
compared with Python's history resume for the same synthetic record.

Validation pending: the complete `.github/workflows/check.yml` desktop job in
the task's disposable Arch container on dev-1, including `tools/check.py`.
No live runtime, accounts or chats are changed. The lead performs the switch.
Only the launch controllers' covered behavior is cleared by this unit;
account/context/memory/skills/usage and brain maintain/sync receive no new bulk
switch approval from these regressions.
