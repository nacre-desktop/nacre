# N0032 — Daemon-hosted AI chat

Status: **Implemented; pending lead review**. Unpublished, no push. The draft called this N0028; N0028 is the AI chat
Claude layer, so the number is N0032 (N0031 is the daemon-hosted Brain).
Depends on N0023 (native `sidebar-chat server|client`, landed). Owner switch-on
for live use is separate; every test is nested or synthetic.

Goal: host the N0023 session manager as a daemon service. `ai.chat` publishes a
bounded snapshot per hosted channel; methods send, cancel, approve, start a new
chat, resume a saved chat, switch provider and hand off to the workspace.
Providers run as daemon-managed nice-10 workers that stop 30 s after idle.
Non-goals: new chat features, history format changes, other AI surfaces.

## Design

- **Producer first.** `nacre-protocol::ai_chat` (MIT OR Apache-2.0) owns the
  types, bounds, validation, method registry and generated artifacts:
  `protocol/schema/ai-chat*.json`, `protocol/luau/ai-chat.d.luau`,
  `protocol/fixtures/ai-chat.jsonl` (open, send, streaming, approval, cancel,
  refused send, channel removal).
- **One worker process per hosted channel**: `nacre-helper sidebar-chat worker
  --channel C`, started by the daemon from `/proc/self/exe` in its own process
  group. It is the N0023 state machine with a single in-process client on
  stdio (the QML newline-JSON contract), so the socket server and the daemon
  share one implementation; the diff to N0023 is an `admit` function and the
  `worker` loop. A panic or provider fault stays in the worker; the daemon
  (and its crash counter) is unaffected. The worker sets itself to nice 10,
  takes the same per-channel lock as the socket server (one owner per channel),
  and stops provider children and exits when its stdin closes (daemon exit or
  restart) or after 30 s without a command, busy turn or blocking question.
- **Daemon service `ai-chat`** (`daemon/ai_chat.rs`, supervised, absent in safe
  mode): reduces worker events into the topic with the QML `accept` semantics
  (`history`, `state`, `message`, `delta`), coalesced to one publish per 40 ms;
  forwards commands; publishes `sendFailed` as an `ai.chat` event
  `send_failed {channel, text, attachments}` so the composer restores text.
- **QML switch** behind the existing `NacreDaemon.enabled` flag
  (`NACRE_DAEMON_CLIENT=1`). `NacreChatSession` calls `ai.chat.open` on start.
  Only a failed open falls back to the N0023 socket (before anything was
  dispatched). Afterwards every action goes to the chosen backend and is never
  replayed on the other one; a lost reply after dispatch reports uncertainty.
  A 10 s heartbeat (`open`) keeps the worker alive while the drawer is wanted.

## Published interface (version 1, desktop capability, not in safe mode)

Topic `ai.chat`: `{available, channels: {<channel>: {state, messages, trimmed?}}}`.
`""` is the edge channel; other channels match `^[a-z0-9-]{1,48}$`. `state` is the
N0023 `state` event minus `type` (camelCase, includes `question`). `messages`
is keyed by ten-digit creation ordinals (oldest first) with `{id, role, text,
truncated?}`.

| Method | Params | Notes |
| --- | --- | --- |
| `ai.chat.open` | `{channel}` | Starts or refreshes the worker; waits for its first state (10 s). |
| `ai.chat.send` | `{channel, text, attachments?}` | 1..64000 chars, at most 8 attachment objects. |
| `ai.chat.cancel` | `{channel}` | N0023 `stop`. |
| `ai.chat.approve` | `{channel, id, answers}` | Approval and question answers. |
| `ai.chat.new` / `resume` / `provider` / `workspace` | channel plus `key` / `provider` | N0023 `new`, `load`, `provider`, `workspace`. |

Failures before a worker received a command carry `dispatched: false`
(unavailable, busy when four channels are open, validation). Writes that may have
reached a worker report `dispatched: true`.

## Bounds, security and memory

- At most 4 hosted channels, 64 messages and 128 KiB of serialized message
  text per channel, 48 KiB per message (cut once, `truncated`), 32 KiB question,
  all measured after JSON escaping, so the whole topic stays under the 1 MiB
  frame limit. Older messages remain in the provider's transcript; the QML list
  shows the newest window and `trimmed` marks the cut (intended difference from
  N0023, whose history frame is unbounded).
- Worker event lines are limited to 20 MiB (provider frames stay 16 MiB); a larger
  line stops the worker. Commands are validated in the producer types before any
  write; wrong channel names, oversized text and non-object attachments are refused.
- Permission defaults are byte-identical to N0023 (Codex `danger-full-access` /
  `approvalPolicy: never`, Claude `--dangerously-skip-permissions`); nothing
  reads credentials in the daemon, only the worker's provider launch does.
- Memory (F3 whole-group rule): the daemon adds no resident state without an
  open channel (empty topic, no thread, no process). Each open channel adds one
  worker process; judge it as part of the whole group (daemon, workers,
  providers) relative to the control build. No capture was taken in this unit.

## Parity and acceptance

- The reducer is tested against a model of the QML `accept` semantics over the
  same event sequence (history, message, delta, late delta, replaced history).
- `protocol/fixtures/ai-chat.jsonl` replays through `Mirror` with every snapshot
  validated; generated artifacts are checked by the existing freshness test.
- Worker tests (fake Claude, private HOME): handshake equals the socket's
  history+state, nice 10, second owner refused, stdin close stops it, 30 s idle
  stop. Daemon test: real daemon, real worker, fake Claude through `ai.chat`.
- Full desktop CI replica and GitHub CI before push.

## Temporary duplicates and expiry

- `nacre-sidebar-ai.service` and the socket `server`/`client` stay as the
  fallback path. In daemon mode nothing starts them. Delete the unit, the
  socket server/client and the QML fallback 14 days after daemon mode is the
  owner's default and trusted; then the lock file helper is the worker's only user.
- The N0023 Python originals keep their existing expiry.

## Verification

The `desktop` job of `.github/workflows/check.yml` was replicated in
`ghcr.io/archlinux/archlinux:latest` on dev-1 (nice 10, Cargo `-j4`) at the final
code head. The first pass ran `cargo fmt --check`, `cargo clippy --all-targets -D
warnings`, examples, the whole `cargo test` workspace (helper lib 561, daemon
integration 20, sidebar-chat 15, protocol suites), the minimal protocol consumer,
the release helper, the helper sandbox suite and translation coverage; the
`ai.chat` unit, protocol, worker and daemon tests were green there. Its
`tools/check.py` step stopped only because the synced copy had no `.git`; a
temporary snapshot repository of the tracked files fixed that, and the remaining
steps then passed on the same tree: `tools/check.py` (ruff, qmlformat, tools 457,
AI 120, brain 52, shell-tools 479, all OK) and both native galleries with the AUR
pager capture. No frozen reference or parity case changed.

Evidence for the new behavior: `ai_chat` unit tests (reducer against a QML
`accept` model, bounds, fake-worker host: open, every command, event, removal,
refusals before dispatch, channel cap, silent worker release), protocol tests
(transcript replay, request bounds, snapshot bounds, generated artifacts),
`daemon_worker_*` (socket-equivalent handshake, nice >= 10, second owner refused,
pipe close stops it, 30 s idle stop), the real-daemon `ai_chat_is_hosted_*` test
(fake Claude through `ai.chat`), and two QML tests (snapshot to model with
incremental and replace paths, fallback only before dispatch). Not done in this
unit: live provider use, nested-session capture, memory measurement, owner
switch-on, removal of the socket fallback.
