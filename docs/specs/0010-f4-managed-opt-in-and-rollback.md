# N0010 — Managed F4 QML opt-in and complete switch rollback

- ID: N0010
- Repo: nacre-desktop/nacre
- Implementer: one fresh Luna worker; coordinator final review
- Status: **Ready: parallel local implementation authorized**; integration after F4/N0009
- Depends on: exact published-green F4 source and accepted/published N0009
- Date: 2026-10-19

## Goal

Provide an explicit persistent QML opt-in with guarded enable/disable commands.
Give the owner one rollback invocation that restores package, setup-owned files
and recorded activation without overwriting later user changes or assuming defaults.

## Context and scope

Starting source: prospective F4 integration b003aa8fb0ce095a851bba95ceabf202fd3bf9e2.
Implement in a separate worktree; coordinate N0009 lease API without editing its
files. Local commits only, integration requires accepted N0009 and green F4.
Read N0001/N0004/N0006/N0009, standing-rules and docs/development/f4-switch-on.md.
Relevant files: tools/releases.py and bounded switch module; package/controller
staging; shell-tools/nacre-shell.service and nacre-daemon service/socket templates;
NacreDaemon.qml opt-in gate; release snapshot/restore paths and focused tests.

QML reads NACRE_DAEMON_CLIENT=1 only at startup; shell service has no persistent
setting, while manager set-environment is volatile. User-config units make runtime
masking unreliable as an installer fence. Snapshots omit daemon units and package
restore's live=False skips activation. Recheck on starting commit; no complete
existing switch rollback is claimed. N0009 provides destination writer exclusivity.

Allowed: managed drop-in, controller commands/journal, exact rollback coverage,
focused tests and source-backed N0006 command updates. No default enablement,
protocol/schema/capability/dependency, generic transaction framework, notification
ownership, AI-policy/paru or unrelated installer redesign.

## Steps

1. Add proposed controller API (NOT commands available today):
   `nacre f4 status`, `nacre f4 enable --previous-package ARCHIVE`,
   `nacre f4 disable`, `nacre f4 rollback --transaction ID`.
   Route through tools/releases.py and package/controller staging; keep a small
   switch module if needed. No separate framework. Status is read-only; mutators
   are ordinary-user commands, requesting sudo only for exact package downgrade.
   Every mutator takes release.lock BEFORE preflight, journal creation or unit
   stops and retains it through health/restoration and final journal publication.
   Enable requires a successful setup snapshot and verified prior package archive.
   Validate package identity, signed provenance and hash; pin an immutable verified
   transaction copy. Privileged handoff securely stages a root-owned package copy
   and revalidates identity/signature/hash there before installation; root must
   install those same pinned bytes. Never trust a mutable caller-selected path.
2. Persistent opt-in is an owned shell-service drop-in at
   `$HOME/.config/systemd/user/nacre-shell.service.d/50-nacre-f4.conf`, containing
   `[Service]` and `Environment=NACRE_DAEMON_CLIENT=1`. No manager-wide setting.
   Disable writes the same managed override with value `0` so a pre-existing
   manager value `1` cannot reactivate legacy mode. Rollback restores original
   absence/content instead. Never replace a unit or foreign drop-in; reject an
   occupied unmanaged path or conflicting effective override. Verify effective
   shell environment, not merely the file. No changes to other env variables.
   This HOME-relative drop-in is supported only if confirmed to be in the actual
   manager's effective user-unit load path; otherwise refuse before mutation.
   Before ANY disable mutation (including stops/journal creation), validate its
   desired state against the saved baseline and current effective environment.
   This bounded controller refuses disable if the baseline has an independent
   daemon owner: active/enabled daemon, active/enabled socket, or another recorded
   startup path able to activate it. Report which baseline owner/startup conflicts
   with client0; do not stop that owner or rewrite the baseline. In particular,
   manager client1 + active shell + active daemon/socket cannot become legacy
   client0 alongside that owner. Even an initially inactive enabled socket counts.
   Such baselines may enable/rollback if otherwise supported; disable must refuse.
   Supported disable stops only transaction-owned activation, leaves daemon/socket
   inactive with no future activation path, then verifies shell client0. If that
   condition cannot be proved from bounded unit/startup inspection, refuse with
   the exact unsupported path before stops/writes; no broad startup redesign.
3. Before mutation record a private 0600 journal/owned-files manifest in
   `$HOME/.local/state/nacre/f4-switch/ID/`: original drop-in/absence/mode/hash,
   touched unit files/enablement links and enabled/disabled/static/masked state,
   active state of shell/socket/daemon, presence/value of manager opt-in variable,
   exact package archive/hash and setup release ID. No secrets/full environment.
   Preflight records absolute runtime destinations from the actual daemon and
   shell startup environments (including unit Environment/EnvironmentFile inputs).
   Record config = absolute XDG_CONFIG_HOME/nacre or HOME/.config/nacre,
   state = absolute XDG_STATE_HOME/nacre or HOME/.local/state/nacre,
   cache = absolute XDG_CACHE_HOME/nacre or HOME/.cache/nacre, and fixed
   HOME/.local/share/nacre, using each consumer's actual HOME. Also record its
   socket/runtime endpoint. Record each install/restore destination derived from
   its explicit HOME/.config, HOME/.local/state and HOME/.local/share separately;
   never substitute the CLI's ambient XDG_CONFIG_HOME for a running unit's value.
   Inventory every written config root and unit/drop-in/state destination with
   canonical no-follow identity, expected owner and role. Reject relative XDG,
   unresolved/conflicting environments or unsupported destinations before any
   stop/write; do not infer HOME/.config when runtime XDG differs. Acquire N0009
   leases for EVERY written config root and every runtime config root whose
   writers must stay quiescent during restoration; deduplicate roots by identity,
   in bytewise absolute-path order under release.lock, before snapshots/writes;
   acquire all or release all without mutation. State/unit destinations use the
   same pinned ancestry/ownership checks. Record both CLI and runtime resolutions
   so status/recovery use the saved map, with identity revalidation.
   Before each phase, fsync a write-ahead intent with exact expected pre-state and
   allowed post-state (absence/content/mode/link target, package identity, or unit
   activation); fsync its parent after atomic journal replacement. After mutation
   and verification, durably mark completion before advancing. For a pending
   intent, resume accepts exact post-state as already applied and records completion,
   executes only from exact pre-state, and refuses every other state as a conflict.
   Apply this rule to restore as well as forward phases, including package change;
   after-fingerprints alone cannot distinguish a completed unrecorded mutation.
   Extend release snapshot coverage for daemon service/socket and switch-owned
   drop-in/manifest; exclude lease and transaction backups from recursive capture.
4. Enable stops shell, socket then daemon; acquires all destination leases,
   verifies owned files and prepares drop-in, then releases all leases BEFORE
   starting daemon/socket and waiting for health. Enable socket and restart shell
   only after preparation succeeds. Lock order is release.lock then ordered leases;
   failed competing daemon/start is an error. Never start daemon while holding
   writer lease or drop release.lock between journal/preflight/stops and completion.
   After the desired-state preflight in step2, disable stops transaction-owned
   shell/socket/daemon, takes the full destination lease set, writes managed value0
   and restores the supported baseline's inactive daemon/socket links/masks.
   Release leases before restoring shell active state and verifying legacy health.
   Preserve a pre-existing daemon owner by refusing incompatible disable, rather
   than stopping it or silently changing recorded enablement.
   Leave manager environment untouched. Repeated enable/disable is idempotent;
   preserve the original baseline, reject unrelated/incomplete transactions.
5. Rollback's ONE invocation stops consumers/socket/daemon, takes the destination
   leases, reconciles phase intents and guarded fingerprints, restores setup-owned files,
   original drop-in/unit links and exact previous package, reloads manager, then
   releases leases before restoring recorded activation and checking health.
   Before first mutation, stage a self-contained ordinary-user Python runner at
   `$HOME/.local/state/nacre/f4-switch/ID/recovery/rollback.py` plus ALL private
   imported modules/resources (0700 directories, 0600 files, pinned hashes); use
   only the recorded supported system Python/stdlib outside that bundle. Refuse
   if the downgrade can remove that interpreter/runtime or required tools. The
   concrete recovery entrypoint is
   `/usr/bin/python3 -I "$HOME/.local/state/nacre/f4-switch/ID/recovery/rollback.py" --transaction ID`;
   require that interpreter and record the fully expanded invocation in status/journal.
   Isolated mode excludes user site/PYTHONPATH/current-directory imports; load any
   bundled modules only by verified explicit paths inside the pinned recovery tree.
   Both the new CLI rollback and a fresh process after downgrade invoke this same
   bundle; never import modules from the replaced package or current working dir.
   Verify bundle hashes/ownership and saved paths before every resume. It and the
   journal are outside package/setup-owned trees and survive package downgrade.
   Preflight old package
   scriptlets/default setup; stop if they could violate the lease or restoration
   order, start consumers, or overwrite guarded state. Do not rewind application
   settings changed since setup; refuse conflicting profile restoration rather
   than overwrite it. Package/user-state restoration is not atomic: use step3
   intents for every phase, including activation. On interruption/conflict during
   file/package restoration, keep consumers stopped and allow the same documented
   invocation from a fresh process to reconcile/resume; already-restored exact
   state is not a conflict. No force/auto retry.
   Before file/package mutation install and verify transaction-owned temporary
   runtime masks for shell/socket/daemon, retaining them until restored state is
   consistent. Refuse BEFORE mutation if these masks cannot fence these units
   (including user-config unit precedence); preserve pre-existing masks. The only
   supported installation for this bounded fence is package vendor units in
   `/usr/lib/systemd/user` with no higher-precedence unmaskable user unit/alias;
   verify the actual effective mask, never assume it from file presence.
   Source-staged units in HOME/.config/systemd/user are explicitly unsupported:
   refuse enable/rollback preflight with that exact reason. This is not yet a
   runnable source-install switch plan; supporting it needs a separately reviewed
   bounded fence design before N0006 can claim coverage. Journal fence changes
   too. This guarantees fail-stopped file/package recovery only
   within the same boot and user-manager lifetime, before activation begins.
   Record boot ID and manager instance identity. On reboot/manager restart,
   status/recovery diagnoses lost fencing and refuses further mutation/activation;
   it cannot promise consumers stayed stopped. The owner must quiesce the session
   for separately reviewed recovery; no persistent recovery service is in scope.
   Removing the fence/restoring activation is a distinct journaled final phase;
   interruption there reports partial activation and resumes by state reconciliation,
   without claiming consumers remained stopped after activation had begun.
   Snapshot restore must not unconditionally start defaults or run older rollback
   after failed setup already restored itself. Preserve original masks; unsupported
   or contradictory state is a preflight refusal, before mutation.

AC/evidence: private HOME/fake systemctl+pacman tests for clean/previously-active,
masked/foreign/conflicting units, manager unset/0/1, reboot-effective drop-in,
disable refusal before all stops/writes for manager1 + active shell + baseline
active daemon/socket, and inactive-but-enabled socket/future-startup conflicts;
supported disable leaves no competing writer now or at later socket/startup.
Cover CLI XDG differing from runtime XDG under private HOME: record both actual
roots, lease each written root in stable order, preserve untargeted roots and
refuse unresolved environment inputs before mutation. Cover idempotence and exact
enable/disable/rollback commands; phase failure/interruption,
archive mismatch/replacement/signature failure, scriptlet refusal, later-edit guards
and package-mode restore including daemon units. Verify the old package lacks
`nacre f4`: kill the runner after file restoration
and separately after package downgrade, before each completion marker, then run
the documented bundle invocation from a fresh process; reconcile exact restored
state and complete, while unrelated edits refuse. Verify bundle imports survive
downgrade and crash leaves the runtime fence effective. Simulate changed boot/
manager identity: explicit refusal/known loss of fail-stopped guarantee, no claim
of unattended recovery; interrupt final activation and report/reconcile its actual
state. Preserve post-setup user settings and XDG isolation. One bounded private user-manager
integration check only if separately granted; no owner bus/session probe.

## Stop, review and owner boundary

Stop for unsupported/contradictory prior state, unsafe archive/privileged handoff,
scriptlet interference, missing rollback coverage, widened privilege/dependency,
required settings migration or broader scope. Report exact gap without workaround.
Return exact commit/files, named AC outcomes/limits and cleanup; Sol review and
independent rollback/security review precede integration. Coordinator marks Ready
only after dependency verification and worker/correctness-slot assignment.
After acceptance update N0006 with implemented command syntax, saved-state
requirements and failure recovery. Owner approval remains the final step for
package publication/install, live units, QML opt-in and rollback execution.
This draft grants no implementation or measurement; no build/test/CI, live probe,
publication, installation or unit change was run. Approved docs-only commit identity,
docs(spec):, [no-publish], [skip ci]; no queue edit or push for this revision.

## Owner implementation grant — 2026-10-19 afternoon

This supersedes draft-only dependency holds: Luna implements now in its own
worktree, local commits only, no live/package/unit action or push. Coordinator
reviews security/rollback evidence. Serialize builds/tests before tonight’s
N0002 capture; request heavy slot before execution. Agree N0009 lease API with
that worker and keep ownership disjoint. Never weaken refusal/rollback criteria
to meet the deadline. Integration remains after green F4 and accepted N0009.
