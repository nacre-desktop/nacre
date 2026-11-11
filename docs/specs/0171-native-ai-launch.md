# N0171 — native AI launch compatibility

The legacy Load chat action emits `kitty --class nacre-ai-task --title "Load chat"
-- nacre-ai window --worker-command sessions`. The native controller advertised
`window` but rejected it before opening history. Python implemented this command
and paused after startup failure; kitty therefore disappeared only with the
native replacement. The newer Luau recent-chat row instead emits
`nacre-ai resume-record <base64 record>` and needs its own regression coverage.
Source: `crates/nacre-shell/src/launcher_actions.rs:208`,
`crates/nacre-helper/src/ai_dashboard.rs:407`, and the restored boundary at
`ai_dashboard.rs:2143`; Python's original hold is
`bin/nacre-ai.py.rollback:672`. Kitty child stderr stays in its closing terminal,
so an asynchronously spawned terminal does not put that failure in the journal.

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
The regressions are `ai/tests/test_native_launch.py` and
`ai/tests/test_installer_arguments.py`; all histories and accounts are fabricated.

Validated after rebasing onto N0167 (`238f4f76c`), in the disposable Arch
container `n0171` on dev-1 (8 CPUs, 16 GiB): every desktop workflow stage passed,
including `tools/check.py`, AUR gallery and the ignored daemon memory check.
The final complete `cargo test --workspace --locked -j2` passed after retries
for existing reconnect/120 Hz timing flakes; each also passed in isolation.
An earlier LLVM crash required cleaning this task's helper build artifacts.
Release-helper `python -m unittest ai.tests.test_native_launch
ai.tests.test_installer_arguments -v`: **8 passed**, including 16 installer
subcases. Real-tmux probes confirmed both providers hold failures until Enter
and Claude's pane cwd follows its provider. Product delta: **+199/-37**, net +162;
the extra boundary and shared pane hold add no persistent Rust supervisor.
No live runtime, accounts or chats are changed. The lead performs the switch.
Only the launch controllers' covered behavior is cleared by this unit;
account/context/memory/skills/usage and brain maintain/sync receive no new bulk
switch approval from these regressions.
