---
name: nacre-project-work
description: Start or resume an independent local or SSH project task in the user's terminal workflow, preserving parallel sessions and project-specific instructions.
---

# Nacre Project Work

The desktop launcher is conversation-first: **New chat**, **Resume latest chat**,
**Load chat**, **Open brain**, **Settings**, **Close**. New chat uses the default
assistant (Codex initially) and selected account, with no project/provider prompt.
Settings changes the default assistant, accounts, usage and project registry.
Existing chats retain their assistant and workspace. Saved Codex chats resume
with the selected Codex login; all Codex accounts share the chat list. Running
remote workers retain their original login. Switching the default never converts
chats or interrupts workers.

Start in a neutral private chat folder. Answer questions normally; a mention of a
project does not authorize editing. When asked to work, resolve relevant project
candidates with `nacre-ai-context QUERY` or `nacre_workflow.resolve_context`.
Read `$HOME/.config/nacre/ai/projects.json`, project AGENTS.md / CLAUDE.md and
specifications. Keep this conversation when switching subjects. Use explicit
working directories and one isolated worktree per independent writing task.
Ask only when the target is actually ambiguous; one chat can span several projects.

Manual project terminals and creating projects are no longer main-menu steps.
Registry management remains in Settings, and explicit project CLI commands remain
compatible. Escape goes back; Close exits only the controller.

Projects may declare build_host and build_path. These designate a remote build
machine while the project path remains the local editing checkout. For heavy
builds/tests, use SSH and an isolated remote worktree containing the exact task
revision and changes; use tmux for long runs and bring artifacts/results back.
Never build an unrelated stale remote checkout or overwrite another task.
Task sources preserve access to chats started on another host; resuming those
chats stays on their original host, while new tasks use the primary project path.

Each new writing task starts in its own worktree. Do not move another task's
branch or reset shared changes. Read the project's AGENTS.md and relevant SPEC
before editing, then run the smallest proof named there. Save a concise handoff
to the private brain when it contributes reusable knowledge.

Remote tasks use the host's existing Codex executable and credentials. Do not
upgrade, migrate, or log out another running Codex client as part of launching
a task. tmux keeps workers alive when SSH disconnects. New remote worktrees are
under `$HOME/.local/share/nacre/ai/worktrees`; remove only clean, idle ones after
the project has finished using them.

`nacre-ai --account NAME login` creates a separate local Codex home. Login
still requires the user's own authentication. Codex chat state is shared through
the supported `sqlite_home` setting; credentials remain separate. Older private
histories stay discoverable in place and resume through an explicit storage
override without restarting peers. Never copy or merge live databases, interpret a Codex config
profile as an account or copy auth.json to imitate switching accounts.

Name new chats automatically once the request is understood:
the `nacre_workflow.set_chat_title` MCP tool, with `CODEX_THREAD_ID` from
your shell. This changes the native Codex name shown in Resume. Save durable findings
to the brain at milestones and before final handoff without asking the user to
remind you. Obsidian need not be open.

Claude Code shares the project registry and brain guidance but has its own login.
Personal sidebar and worker sessions explicitly use --dangerously-skip-permissions,
matching the Codex full-access/never-approval policy.
Use nacre-ai login --agent claude to authenticate.
Named Claude profiles use separate CLAUDE_CONFIG_DIR homes, never Codex auth.
The launcher appends guidance to read repository AGENTS.md alongside CLAUDE.md.
For Claude titles, use native session names or /rename; do not call the Codex
set_chat_title tool for a Claude session.

Settings provides independent Codex/Claude account selectors, named sign-ins,
a default assistant choice, and a setup status check. Load chat combines all
Codex histories and the selected Claude account's history. Switching accounts
does not alter running sessions. Both assistants use the same private brain.

## Durable task checkpoints

Use [the handoff guide](references/handoff.md) for significant multi-step work,
at milestones and before ending unfinished work. On resume, retrieve the latest
relevant checkpoint and verify its state before continuing. Do not generate
checkpoints for trivial questions or duplicate an unchanged one.
