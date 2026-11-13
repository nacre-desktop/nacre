# N0319: AI helper API for the Nacre AI app (design review D8, helper half)

**Status:** Implemented; focused dev-2 checks and hosted CI recorded below.

## Goal

The Nacre AI app (D8, Shell's N0248) is a Bead window with a rail: Chats,
Projects, Accounts, Usage and Brain. The owner's vision is an AI application
that can use any model and grow. This unit gives the shell everything that
rail needs over the existing daemon link, provider-neutral, with one provider
abstraction, and keeps the sidebar, launcher and `nacre-ai --tui` paths as
they are. The settled contract the Shell lead builds against is
`protocol/schema/ai-app-*.json` (fixture `protocol/fixtures/ai-app.jsonl`).

## What existed

| Need | Already on the link |
| --- | --- |
| Hosted panel chat | `ai.chat` + `ai.chat.*` |
| General-chat history, open/new/latest in a terminal | `ai.launcher.*` |
| Usage lines per provider | `ai.usage` + `ai.usage.refresh` |
| Brain capture UI, Brain window, recent notes | `brain.capture`, `brain.open`, `brain.recent` |

Projects live in `~/.config/nacre/ai/projects.json` (schema `ai-projects`);
accounts are Nacre profile directories under `~/.local/share/nacre/ai/{accounts,claude-accounts}`
with the active one in `~/.config/nacre/ai/settings.json`; the TUI's setup
page already checks login with `codex login status` / `claude auth status`
by exit code; `nacre-ai-usage read` already knows used % and reset times but
printed them only as text.

## Shipped API (`nacre_protocol::ai_app`)

Topics: `ai.providers` (static registry with capability flags per feature,
including `callerSessionId`), `ai.projects` (`id`, `label`, `path`, optional
`host`), `ai.accounts` (`provider`, `id`, `label`, `active`, `status`
unknown/signed_in/signed_out/missing, `checkedMs`). `ai.usage` rows gain an
optional structured `windows` list (`label`, `usedPercent`, `resetsMs`);
`nacre-ai-usage read` prints the same beside `rows`.

Methods: `ai.projects.refresh`, `ai.accounts.refresh` (runs the login checks
on request only), `ai.chats.list {offset?, limit?}` (the TUI's
`collect_history` across every project and provider, same Running detection,
paged from the last collection; rows carry `session` and `resumable`),
`ai.chats.start {provider, project, account?}` (the "New chat" window; for
Claude a caller-chosen session UUID whose launcher key is returned as `id`),
`ai.chats.resume {id}` (the launcher's open path), `brain.search {query,
limit?}` and `brain.note {text, source?}` (inbox note).

Not provided, on purpose: `ai.chats.stop` for terminal chats (the helper never
signals processes it did not start; hosted chats use `ai.chat.cancel`), a
`models` list (neither CLI exposes one), `lastUsed` on projects (derived from
the chat list), pinned/starred (Shell state).

## Provider abstraction

`crates/nacre-helper/src/ai_providers.rs`: trait `Provider` (`info`,
`profiles`, `active_profile`, `login_status`, `chats`, `start`) and one `Cli`
adapter type instantiated for Codex and Claude Code over the existing helper
functions. `daemon/ai_app.rs` builds every topic from `registry()`, so a local
model later is one more `impl Provider` plus a registry entry.

## Security

- Nothing reads `auth.json`, `.credentials.json`, keyrings or config values;
  status is a directory name plus a CLI exit code with the output discarded.
- Every `ai-app-*` schema is checked by a test that fails on any property named
  like token/secret/key/password/credential/auth/cookie (the walker is
  self-tested with a planted field); `dashboard-extras-state` is covered too.
- Stored history records never open SSH (`resumable` false for remote rows).

## Checks

See the README row and the unit report. Net product delta is in the report.
