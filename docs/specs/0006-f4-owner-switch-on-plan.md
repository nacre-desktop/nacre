# N0006 — F4 owner switch-on and rollback plan

- ID: N0006
- Repo: nacre-desktop/nacre
- Implementer: fresh Luna docs worker
- Status: Review — activation-only owner procedure updated for N0014
- Depends on: F4 green publication before executing plan; drafting eligible now
- Date:2026-10-18
- Size: one focused session, maximum2hours

## Goal

Give the owner an executable, source-verified plan to publish/install the F4 package,
enable daemon ownership and opt QML in. Make rollback one documented command with
all prerequisites and effects explicit, preserving the current daily-driver UI.

## Context

Read N0001/N0002/N0004, foundation/protocol/standing-rules, CONTRIBUTING,
security-review, rust-migration, packaging/release scripts and staged daemon/socket
units. F4 candidate1382a28ab544743e32e906f7542bcecdfdfca083 is not yet published.
Inspect that commit read-only where relevant; reference exact final green head
as a placeholder until N0002 completes, never claim unpublished bits installed.

## Scope

Docs only: docs/development/f4-switch-on.md plus spec/queue report. Read product
code/config/unit scripts only. No shell script implementation, builds/tests,
lab/network-systembus probes, package publishing, installation, unit enablement,
QML opt-in or live configuration changes. No unrelated deployment or Bead switch.

## Steps

1. Inventory actual CLI commands/flags, staged units and QML opt-in setting from
source. Cite paths/lines. Distinguish package build, publication, installation,
unit activation and UI restart. Never invent command names or flags.
2. Write numbered exact command blocks for owner execution after green F4,
using existing managed release/deploy mechanism. Each block states precondition,
files/settings/services changed, verification, failure response. Identify the
owner-only publication boundary and required package channel/tool. If existing
mechanism lacks a safe capability, state exact gap and propose bounded follow-up;
do not pretend a new command exists.
3. Profile whole-document installation is an explicit offline deployment writer:
destination daemon must be stopped, including socket activation/startup races.
Document exact stop order and future guard limitation; inotify is not exclusivity.
Protect one settings writer during handoff. Never enable two notification owners.
4. Specify one rollback invocation using EXISTING managed rollback command if
available; list required saved release/config state, restoring previous unit/socket
state and QML mode. If no existing one-command rollback covers all effects, give
one owner-run shell command block composed of verified existing commands, explain
atomicity limits and require review; do not add product code. Preserve user data.
5. Include nested correctness/performance gates as prerequisites and distinguish
remaining owner physical validation. Include settings read/write/saved error,
restart/session recovery and daemon/CLI health checks from existing commands.
No claim these checks were executed by this worker.
6. Validate Markdown/diff and source-check every command. Commit docs with
approved identity/docs(spec):/[no-publish]; normal main integration allowed for
this docs-only plan, inspect concurrent changes and exact-head CI if any.

## Measurements

None. No heavy job grant, host access or live execution. Coordinate with N0002
capture and avoid starting CI-heavy code changes. Docs publication is not package
publication.

## Acceptance criteria

- [ ] Exact source-cited package/unit/QML commands, preconditions/effects/checks.
- [ ] Single-writer offline deployment boundary and socket races addressed.
- [ ] One rollback invocation with saved-state prerequisites and honest limits.
- [ ] Owner-only operations clear; no command executed or product change made.
- [ ] Concise owner-ready plan and exact docs commit/report to coordinator.

## Stop conditions

Need new installer/guard/product API, ambiguous package target, destructive state
migration or missing rollback capability: report exact source gap/options/cost;
complete independent plan portions rather than inventing behavior.

## General rules

Standing rules/approved siverteh noreply identity; isolated worktree; docs-only
normal push/no forcepush/own rebase/checkCI. nice10, no builds. No personal paths,
secret values or machine inventory in public docs. No live session/config/units/
installation/publication/AI-policy/paru changes; stop only owned PIDs. Extras
report-only. Coordinator thread supplied privately; one spec per worker.

## Report format

N0006 Review: start/final commits, plan path, source citations for commands,
rollback coverage/limits, unresolved owner decisions and validation. Send to
coordinator and exit task work; leave visible chat readable.

## N0014 split decision — 2026-10-20

The owner approved a small activation-only switch instead of making the full
N10 package/setup transaction a prerequisite. N0014 implements ordinary-user
`nacre f4 status|enable|disable|rollback [--transaction ID]`. After N0014 lead
review, integration and exact-head CI, use the updated
[owner procedure](../development/f4-switch-on.md): enable, inspect status and
perform physical verification; disable to client0 with daemon/socket off, or
rollback the recorded transaction to its exact prior activation state.

Retain the exact verified previous Core archive in `/var/cache/pacman/pkg`.
Package downgrade is manual and follows successful `nacre f4 disable` and status
verification, then `sudo pacman -U -- "$PREVIOUS_NACRE_ARCHIVE"`. It is not part
of activation rollback and is not atomic with setup-file restoration. Full N10
transactional package rollback stays deferred on its existing branch. This
section supersedes this spec's older N10-only execution hold, without granting
package publication, installation or live switch execution to implementers.
