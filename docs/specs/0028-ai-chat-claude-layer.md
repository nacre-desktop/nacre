# N0028 — AI chat, Claude and title layer

- Status: Done
- Date: 2026-10-22
- Dependencies: N0021; N0022 shared AI core

Deliver native standalone `nacre-ai-chat` and `nacre-ai-claude` commands using
N0022 primitives. Preserve Codex local protocol titles/threads, context/knowledge
MCP tools, shared chat records, Claude launch/running sessions/records and exact
session identity. Replace the Claude Agent SDK history dependency with a native
reader, including the existing native sidebar history restore path. The SDK venv
created by `ai/install-claude.py` is unnecessary for the daemon-free native path.

Non-goals: dashboard, provider authentication changes, daemon hosting, installation,
live desktop changes and publication. N0026 desktop extras and N0027 wallpaper are
separate completed units; this spec supersedes the draft's N0026 numbering.
Risk: medium-high because session formats are upstream-owned. Size: L; original
commands total approximately 1,079 lines. Review simplicity above 2.5 times that
product size. Reuse shared core rather than introduce a second controller layer.

## Acceptance

- Replay frozen `nacre-ai-chat` and `nacre-ai-claude` cases with
  `tests/parity/helpers/run.py --command 'nacre-helper NAME'`. Never regenerate
  frozen references. Full CI also replays the complete corpus.
- Preserve relevant historical assertions from `ai/tests/test_chat_titles.py`,
  `test_shared_chats.py` and `test_claude_workflow.py` as Rust tests. Keep the
  Python assertions against retained sources during migration.
- Synthetic fixtures cover current and previous Claude history formats, unknown
  fields, unsupported versions, malformed input and native sidebar text restore.
  Reject unsafe paths and bound input; degrade gracefully for unavailable history.
  Document new security cases as intended differences, not parity waivers.
- Permission behavior remains unchanged: Codex `danger-full-access` with
  `approvalPolicy never`; Claude `--dangerously-skip-permissions` under the same
  existing profile policy. Authentication is never copied or inspected.
- Tests use temporary HOMEs and synthetic providers/history exclusively. Never
  inspect real credentials or session data. No laptop tests/builds. Run the entire
  `.github/workflows/check.yml` desktop job in the Arch container on a permitted
  remote CI host, in tmux, nice 10 and Cargo `-j2`; include all suites, formatting,
  frozen parity and galleries. No push, deployment, units or installed changes.
- Commit author and committer use the approved identity; every commit contains
  `[no-publish]`. Report exact checks, product lines added/removed and limitations.

## Temporary duplicates

Retained Python command originals (`bin/nacre-ai-chat.py.rollback` and
`bin/nacre-ai-claude.py.rollback`), their Python assertion-loader fallback entries, the legacy Python
sidebar SDK path and `ai/install-claude.py`: delete 7 days after the native
replacement is trusted in the owner-approved runtime. Trust requires full CI,
fixture parity and a later owner-approved runtime soak. This unit does not deploy
or remove the owner's existing venv. The hidden `cache_title` module-operation
adapter expires with its frozen fixture on the same schedule. Daemon hosting
remains a separate unit.

## Evidence

The native path dispatches both provider commands without a daemon or Python
history dependency. Native sidebar restore calls the same reader. Existing help
text is retained byte-for-byte for frozen compatibility, including the historical
SDK wording in Claude help; the runtime no longer imports that SDK.

The reader follows the [upstream session implementation](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/sessions.py):
filesystem mtime converted through milliseconds, title precedence, sanitized
project directories, long-path hash/prefix lookup, worktree discovery, parent UUID
chains and text siblings from parallel responses. Compacted context remains
visible. Older string content and current text-block content have separate
synthetic fixtures. Index entries are hints; stale indexes do not hide raw files.
Provider release `version` strings are not schema versions. Explicit unsupported
index/schema markers fail closed; malformed individual lines degrade gracefully.
Ambiguous long-path prefixes and workspace identity mismatches are refused.

Intended security additions (new cases, no frozen-reference waivers): refuse
linked/non-regular or hard-linked session data, linked ancestors/configuration
locks and invalid workspace references; bound history files to 64 MiB, records to
4 MiB, background output/roster and MCP requests to 1 MiB. Existing conflicting or
incomplete workflow configuration is preserved and refused rather than guessed.
Configuration reads are bounded and updates atomic, with private migration
backups. Provider RPC error payloads remain sanitized by the shared N0022 peer.
A failed handshake reaps the exact owned child; no unrelated process is signalled.
Permission flags and profile opt-in behavior remain unchanged.

Lead review added process tests for shared-history insertion order and newest
source selection, inherited account homes, malformed config preservation, linked
lock refusal, Codex resume permissions, Claude full-access profile behavior,
private scratch directories, native MCP context dispatch, absent SDK/history,
missing-command refusal and exact background attach without a roster.

The complete `.github/workflows/check.yml` desktop job passed in a private Arch
container on the permitted CI host, exit 0 (`.lead/desktop5.log`, recipe
`.lead/ci-n0028.sh`). Code head: `9a9164c4`. Cargo formatting, Clippy, examples,
all workspace Rust tests, the minimal protocol consumer and release helper passed.
`tools/check.py` passed Python/QML formatting, all suites and 534 frozen cases:
tools 454 (one existing opt-in skip), AI 120, Brain 52 and shell-tools 479.
The main gallery passed 244 states per language, the AUR pager gallery passed,
and the wide-font gallery passed 185 states per language. The archive replica
uses task-scoped Git trust environment entries so temporary HOMEs retain access
to the checkout and pinned submodule; no host Git configuration was changed.

Focused evidence (`.lead/final-preflight.log`):
`cargo test -j2 -p nacre-helper --lib ai_` passed 44 tests;
`cargo test -j2 -p nacre-helper --test ai_chat_commands` passed 10 tests;
`cargo clippy -j2 --all-targets -- -D warnings` passed; harness integrity passed
28 tests. The final release replay with
`run.py --command '/src/target/release/nacre-helper {name}' --case 'nacre-ai-chat/*' --case 'nacre-ai-claude/*'`
passed 5/5 (`.lead/parity-final.log`). No frozen case or expected JSON changed.
Both helpers are registered for native replays; the historical cache operation
has a small native adapter, and re-record protection remains enforced.

All 23 candidate files matched the tested checkout by SHA-256 before completion
documentation; the two retained Python originals match their base Git blobs.
Claude launch guidance also matches the original byte-for-byte. Permission
flags, selected-account homes, shared-store handoff (including inherited tmux
environment), title fallback and installer preservation assertions passed.

Product physical lines (excluding Rust tests): +3281/-1137, including 1,110 retained Python rollback lines. Native command product: 2,133 lines versus 1,110 Python lines (1.92x).
The simplicity review reused the existing RPC/core and one Claude reader for
both command records and sidebar text; no new framework or dependency was added.

Known inherited behavior: Claude running-session discovery retains the Python
account-prefix match; tightening exact account suffixes is a separate follow-up.
Transcript roots remain account-specific. Upstream format drift remains a risk;
verification used synthetic histories and private HOMEs exclusively. No live
runtime, real authentication/session data, deployment or publication was touched.
