# N0022 — AI core library and small wrappers

- Status: In progress
- Date: 2026-10-22
- Dependency: N0020 shared vault module

Goal: one Rust AI core for accounts and homes, environment building, umask 077,
libsecret with a 0600 credential fallback, shared history, `agent_full_access`,
context resolution and usage cache. Small commands use this core:
`nacre-ai-context`, `nacre-ai-memory`, `nacre-ai-usage`, `nacre-ai-skills`,
`nacre-ai-tools`, `nacre-ai-account`, and Codex/local and remote wrappers.

Scope includes shared pieces repeated across AI commands, sidebar-chat and
desktop-extras. Retire `nacre_ai_controller.py` only when its last caller moves.
Non-goals: chat peers, dashboard, installers, live installation, changes to
permission defaults, daemon hosting or publication. Risk: medium; credentials
and shared agent-session behavior. Size: M.

## Delivery phases

1. Shared core and context, memory and usage commands; memory writes use N0020's
   vault module. Review, run the complete desktop CI job and commit.
2. Skills, tools, account and Codex/remote wrappers; integrate shared callers
   where applicable. Review, run the complete desktop CI job and commit.

Both phases follow N0018–N0020: `crates/nacre-helper/src/main.rs` argv0 dispatch
and shell shims in `bin/`. Every commit contains `[no-publish]`; no push.
The draft N0021 number was superseded by N0021 app theming on main.

## Acceptance and security

- Replay immutable frozen cases under `tests/parity/helpers/nacre-ai-{context,memory,usage,skills,tools,account}`
  and remote wrapper cases using `run.py --command 'nacre-helper NAME'`.
  Never regenerate references. Preserve historical assertions from
  `ai/tests/test_shared_skills.py`, `test_conversation.py`, `test_workflow.py`
  as Rust tests for migrated behavior.
- Set umask 077 before every provider launch. Repair existing account-home
  directory modes without copying authentication. Credentials use libsecret
  when available, otherwise private 0600 files; doctor reports the backend.
- No secrets in argv or logs. Skill archives require digest verification and
  bounded download/extraction sizes. New security differences get explicit tests
  and an intended-difference explanation.
- Never read, copy, print or move real credentials, provider authentication or
  keyring contents during development. Tests use fixtures and temporary HOMEs.
- Preserve deliberate permission behavior: Codex `danger-full-access` and
  `approvalPolicy never`; Claude `--dangerously-skip-permissions`, including
  existing explicit opt-in/profile behavior. Preserve `paru -Syu --skipreview`.
- Run each phase's entire `.github/workflows/check.yml` desktop job in
  `ghcr.io/archlinux/archlinux:latest` on the CI build host using SSH, Docker and
  tmux, nice 10 and Cargo `-j2`. All checks must pass, including `tools/check.py`,
  formatting, all suites and galleries. No laptop builds/tests or live sessions.

## Evidence

Phase-one intended security differences: reject linked account/cache paths and
non-regular or hard-linked capture records/locks; bound usage capture input to
1 MiB and Codex replies to 4 MiB. Provider and delegate deadlines cover blocked
input writes. Provider error payloads are replaced with a generic usage failure.
Raw note metadata is validated before normalization or deduplication. Tests use
temporary HOMEs and synthetic providers only. Historical Python implementations
remain as `.py.rollback` sources for comparison during the migration soak.

Phase one: the full desktop job passed in the Arch CI container, exit 0
(`.lead/desktop3.log`). Formatting, Clippy, examples, all Rust tests, the protocol
consumer, release helper, sandbox integrity, translations, `tools/check.py` and
both gallery audits passed. Suites: tools 452 (one existing opt-in soak skipped),
AI 117, Brain 45, shell-tools 479. All 534 frozen cases passed, including the
nine explicit context/memory/usage replays. Galleries covered 244 states per
language and 185 states per language for the wide-font selection. All 20 changed
source files matched the CI candidate by SHA-256; rollback sources match the
original Git blobs. No frozen reference changed. Phase two evidence pending.
