# N0041 — `ai.chat.saved` and Bead chat i18n ids

Status: **Implemented; full desktop CI replica green on dev-2 (head before docs); pushed**. Consumer: Bead B0043
(`docs/specs/0043-ai-sidebar.md` on Bead, ask A and D). Additive; QML unchanged.

## 1. `ai.chat.saved` (version 1, class `desktop`, advertised with the other `ai.chat` methods, absent in safe mode)

Params `{channel}` (validated like the other methods; the list is not per channel). Result
`{chats:[{key,id,title,agent,account,state}]}`, newest first, at most 30 (`MAX_SAVED`); `title` is at
most 100 chars, `agent` is `codex|claude`, `account` is `Default` when empty, `state` is the helper's
state (`saved` when absent). Exactly the shape Bead B0043 asked for, so no difference to report.
Types `nacre_protocol::ai_chat::{AiSaved, AiSavedList}`; generated `protocol/schema/ai-chat-saved.json`,
`AiSavedList` in `protocol/luau/ai-chat.d.luau`, and a call/ret pair in `ai-chat.jsonl`.

Storage: the QML "Recent chats" list (`DesktopExtras chats` -> `desktop_extras::chat_list`). The
daemon calls the same function on a blocking thread, so the rows come from the same provider list
helpers (`nacre-ai-chat`/`nacre-ai-claude list`, every local project) and the call refreshes
`state/nacre/shell/extras/chats.json`, the key table `ai.chat.resume` (`load`) resolves. A key is
therefore valid until the next list call replaces the table, as in QML. Timeout 45 s (daemon bound
40 s); one list in flight at a time (`busy`, retryable, `dispatched:false`); helper failure is
`unavailable`.

No paging: the list is capped at 30, as in QML (which shows the first 6). No delete: QML has no
delete for saved chats, so no method was added. Open/resume already exists as `ai.chat.resume {channel,key}`.

## 2. i18n

`tools/i18n-sync.py` (Bead repo, origin/main) `--write` copied Bead's `bead.*` entries into
`nacre/shell/i18n/en.json` and `context.json` (the 29 `bead.panel.chat-*` ids plus the existing
`bead.*` set, re-sorted). `tools/i18n/check.py` accepts `bead.*` ids as external.

## 3. Other B0043 gaps: contract needed?

- **B. Attachments** (picker, drop, screenshot): yes, but not small. Ask: `ai.chat.attach {channel,
  source: "pick"|"screenshot"|{path}}` -> attachment object appended to a per-channel composer list
  published in `ai.chat` (`state.attachments`), or a daemon file-picker route; today `ai.chat.send`
  already accepts up to 8 attachment objects, so Bead can send paths it obtained itself. Needs an owner
  decision on who picks files (Bead surface vs. helper dialog) and the attachment object shape
  (QML `composer.attachments`). Not built.
- **C. Brain tab**: `brain.open` and `brain.capture` already exist (N0031); a notes search/list method
  (`brain.notes {query}`) is the missing ask. Not built.
- **Pin** (keep open on outside click): Bead-local panel behaviour, no Nacre contract.
- **Workspace-chat list** (windows with class `nacre-ai-task`, their thread ids): derivable from the
  `compositor` topic windows; the title/thread-id mapping is QML-local (`ChatWindowTitle`). Ask if
  wanted: publish `threadId` per `nacre-ai-task` window in `ai.chat` (`workspaceChats`). Not built.
- **"Accounts and usage" shortcut**: launches `nacre-ai`; no new contract.

## Temporary duplicates

None added.
