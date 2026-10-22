# N0022 — AI core library and small wrappers

- Status: Implemented (pending lead review)
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
9/9 explicit context/memory/usage replays. Galleries covered 244 states per
language and 185 states per language for the wide-font selection. All 20 changed
source files matched the CI candidate by SHA-256; rollback sources match the
original Git blobs. No frozen reference changed.

Phase-two implementation and intended security differences:

- The remaining small commands and local Codex wrapper dispatch to Rust. Python
  views/controllers use a thin native client for permissions, selected accounts,
  environment deltas and shared history. Inherited environment values are never
  serialized. The client performs the parent's umask operation before launches.
- New named Codex homes receive the provider's supported `auto` credential policy
  before first sign-in, with a 0600 config and serialized initialization. Existing
  homes retain their storage policy; no authentication migration occurs. Existing
  provider directories are repaired to 0700. Missing Claude profiles remain
  provider-created at launch, under umask 077.
- Doctor's live report adds configured credential policy and a bounded D-Bus
  service-name ownership probe. It does not assert an active credential store or
  retrieve secret payloads. The frozen diagnostics collector is unchanged.
- Skill archives require the checked-in SHA-256, validated source metadata and a
  60-second transfer deadline. Limits are 16 MiB compressed, 4 MiB per extracted
  entry, 32 MiB total extracted and 4096 archive entries. Unsafe paths, archive
  symlinks and linked library/destination directories are refused; managed skill
  aliases still follow shared-library relocation. ZIP 9.0.5 is used, above the
  [patched extraction boundary](https://rustsec.org/advisories/RUSTSEC-2026-0367).
- Provider config reads reject links/non-regular files and exceedances of 1 MiB;
  malformed TOML never appears in errors. Capture records use the shared versioned
  state writer and refuse newer versions, while retaining Python's fingerprint.
- Chat peers, dashboards, MCP/history configuration, installers and their remaining
  controller-loader callers stay Python. Installer changes only ship the new
  declared client dependency with private controller copies. Original small-command
  sources remain executable rollback files. The independently deployed
  `ai/remote-codex-wrapper.sh` stays standalone Bash with its legacy-home guard.

Phase two: the entire desktop job passed in the same Arch CI environment, exit 0
(`.lead/desktop6.log`), including all Rust checks, the protocol consumer,
`tools/check.py`, all suites and both galleries. Suite counts remain tools 452
(one existing opt-in soak skipped), AI 117, Brain 45 and shell-tools 479. All 534
frozen cases and all 17 explicit AI replays passed; no reference was changed.
The sidebar's optional-component availability check retains its frozen missing
launcher error. Quota RPC retains the per-session history-home override, verified
by a fake provider's exact arguments. All 44 changed source files matched the CI
candidate by SHA-256. The public pinned-archive fixture installed both archives,
exposed all ten skills to both providers, repeated from cache and preserved a
synthetic 0600 auth file (`.lead/skills-verify.log`). All real authentication files
and keyring values were outside the development and test scope. No live install,
push, tag or release occurred.

## Lead replay review

The initial report's “explicit AI replays 9/17 passed” was a reporting error:
phase one passed **9/9** and phase two passed **17/17**. It did not mean that
eight cases failed. The additional phase-two cases below each matched their
immutable frozen reference (`.lead/desktop6.log`, helper groups at lines 313–320;
also independently replayed in `.lead/phase2-focused4.log`, lines 94–100).

| Frozen case | Classification | Reason |
| --- | --- | --- |
| `nacre-ai-skills/help` | Passed; no difference | Frozen help output matched. |
| `nacre-ai-skills/empty` | Passed; no difference | Empty-library output matched. |
| `nacre-ai-tools/usage` | Passed; no difference | Frozen usage output matched. |
| `nacre-ai-tools/delegate` | Passed; no difference | Fixture delegation behavior matched. |
| `nacre-ai-account/missing` | Passed; no difference | Missing-action behavior matched. |
| `nacre-ai-account/invalid-account` | Passed; no difference | Invalid-account rejection matched. |
| `nacre-ai-remote/invalid-project` | Passed; no difference | Invalid-project rejection matched. |
| `nacre-ai-remote/relative-path` | Passed; no difference | Relative-path rejection matched. |

None of these eight requires an intended-difference exception, an environment or
harness waiver, or a behavior fix. The security changes listed above are separate
new cases, not explanations for failed frozen replays. Frozen references remain
unchanged.
