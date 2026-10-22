# N0023 — Sidebar chat server and composer

- Status: Implemented; pending lead review
- Date: 2026-10-22
- Dependencies: N0021 theming; N0022 AI core (`9331b3d1`)

Native `sidebar-chat server|client` and `composer` preserve the existing Unix
socket protocol and QML contract, retiring the resident Python chat server.
The draft's N0022 number is superseded by N0022 AI core. Daemon hosting belongs
to N0028; Claude history listing belongs to N0026.

Scope: Codex `app-server --stdio` JSON-RPC, Claude `--output-format stream-json`,
allow-once/decline approvals, per-channel state, streaming, cancellation,
history, titles, friendly errors and usage-limit detection; drafts, local
attachments, clipboard and area screenshot. No protocol topics, visual changes,
history format changes or desktop-adapter migration (N0025).

## Phases

1. Native composer with private drafts, attachment validation, screenshot expiry,
   dispatch and shim. Review, full desktop CI replica, commit.
2. Native resident chat server/client and provider peers; preserve QML contracts,
   switch source callers and service template, review, full desktop CI, commit.

Each phase runs the complete desktop job from `.github/workflows/check.yml` in
`ghcr.io/archlinux/archlinux:latest` on the CI build host using SSH, Docker and tmux, nice 10,
Cargo `-j2`. No laptop builds/tests, live providers, credentials, live session,
installed files, configuration, units, push or publication. Every commit carries
`[no-publish]` with the prescribed author and committer identity.

## Acceptance and security

- Frozen `sidebar-chat` and `composer` references are replayed using
  `run.py --command 'nacre-helper {name}'`, never regenerated.
- Historical assertions in `ai/tests/test_chat_titles.py`,
  `test_claude_workflow.py`, `nacre/shell-tools/tests/test_sidebar_chat.py`,
  `test_sidebar_service.py` and `test_composer.py` remain covered, with migrated
  behavior asserted in Rust. Providers are synthetic executables in fixture HOMEs.
- Preserve all message, state, delta, question, answer, cancellation, reconnect,
  history and handoff contracts consumed by `NacreChatSession.qml`.
- Bound line lengths, client counts and idle reads; validate object shapes.
  State directories are 0700, socket/files 0600 before writing. Expire draft
  screenshots after 30 days; chats remain until owner deletion. New security
  cases document intended differences separately from frozen references.
- Preserve Codex `danger-full-access` / `approvalPolicy: never`, Claude
  `--dangerously-skip-permissions`, and existing explicit opt-in behavior.
  Any required permission-policy change stops the task. Preserve the paru command.

Risk: high (resident process and tool approvals). Size: L. Standalone native
commands ship before daemon hosting. Original Python remains rollback evidence
until the migration soak is complete; remaining Python boundaries are reported.

## Intended security differences

Composer refuses linked directory ancestors, linked/hard-linked draft files and
draft reads over 1 MiB. Selected files retain their existing canonical-path,
eight-file and 30 MiB limits. Missing selected files do not prevent saving draft
text. Newer state versions are reported and preserved through the shared state
layer, and existing translated attachment errors are retained. Area capture
writes through an already-open 0600 file descriptor; the
draft and screenshot directories are 0700. Screenshots older than 30 days are
removed on composer activity, without following links; chat records are never
aged out. Synthetic capture tools verify permissions before their first write.
These new security cases are separate from the immutable frozen references.

The resident transport bounds client frames to 1 MiB and provider frames to
16 MiB (including resumed transcripts), connected clients and
queued submissions to 32 each, client idle reads to 30 seconds, provider writes
to 5 seconds and RPC waits to 45 seconds. A provider that produces no frames for
300 seconds is disconnected with the conversation retained. Invalid object
shapes and linked/non-regular private state paths are refused. A private lock
prevents competing servers from replacing an active channel socket. Reconnects
retain the existing history/state handshake; outgoing history is not silently
discarded by the inbound frame limit.
Runtime directories must already be owned by the session user and 0700; shared
or linked runtime directories are refused without changing their permissions.
Generation fences prevent a late resume handshake from replacing a new chat;
workspace handoff waits for any unpublished peer to exit before terminal resume.

## Phase one verification

The complete Arch desktop job replica passed (`.lead/desktop4.log`, exit 0):
`cargo fmt --check`, `cargo clippy -j2 --all-targets -- -D warnings`, examples,
`cargo test -j2 --quiet`, minimal protocol consumer, release helper,
`tools/check.py`, translations, sandbox integrity and both native galleries.
Composer passed 4/4 explicit frozen replays; the complete sweep passed 534/534.
The helper suite passed 366 tests. Application suites passed tools 452 (one
existing opt-in skip), AI 117, brain 45 and shell-tools 479. Galleries passed
244 states per language and 185 with the wide font; the AUR pager was captured.
All seven shipping source files matched the CI snapshot by SHA-256.

## Protocol evidence

`crates/nacre-helper/tests/sidebar_chat.rs` runs the compiled helper against fake
Codex/Claude executables with cleared environments and private fixture HOMEs.
The socket assertions cover the QML `history`, `state`, `message`, `delta`,
`sendFailed`, question and answer contracts rather than only internal functions.

| Boundary | Synthetic coverage |
| --- | --- |
| Codex | Streaming, user echo suppression, busy/idle transitions, steering and its completed-turn race, allow once/decline, stale answers, interruption, restart/resume, channel isolation and exact permission parameters. |
| Claude | Multiple streamed messages, session allocation/resume, exact permission flag, usage-limit errors and cancellation with prompt retention. |
| Questions | Restored/async cards, reset, answer suppression and blocking questions; a reconnect reusing an RPC ID cannot lose the new question to an old waiter. |
| Lifecycle | Workspace ownership/peer exit, busy handoff refusal, late-resume fencing, provider default changes and named-account setup before launch. |
| Transport/composer | Partial client-frame reconnect, client admission/reclamation, bounded provider frames, private captures and newline-terminated QML requests whose stdin remains open. |

## Phase two verification and remaining boundaries

The complete Arch desktop job replica passed (`.lead/desktop-phase2-final4.log`,
exit 0), including all commands listed for phase one. The helper suite passed
373 tests; seven server unit tests and 13 socket/provider integration tests
passed. Explicit frozen replays passed composer 4/4 and sidebar-chat 3/3;
the complete sweep passed 534/534 with no reference changes or exceptions.
Application suites passed tools 452 (one existing opt-in skip), AI 117, brain 45
and shell-tools 479. Translation coverage passed 3354 messages; galleries passed
244 states per language and 185 with the wide font, plus the AUR pager capture.
All 12 shipping source files matched the final CI snapshot by SHA-256.

The source QML client and service template now invoke native `nacre-helper`.
Named-account setup still delegates to `nacre-ai-chat configure`; selected
Claude history restoration uses a one-shot Python SDK boundary; workspace
handoff delegates to `desktop-extras.py resume`. Other AI controllers, views and
workflows remain Python. The original composer/sidebar Python sources remain
for historical tests and rollback evidence. Claude history listing (N0026) and
daemon hosting (N0028) are separate units. No installed files, live units,
desktop session, provider credentials, permissions policy or publication were
changed. The two phases are local unpublished commits; no push occurred.
