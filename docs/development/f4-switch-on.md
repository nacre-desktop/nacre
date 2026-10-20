# F4 daemon switch-on and recovery plan

**Current state: F4 acceptance is complete; switching the installed daemon is
still blocked by source work.** N0002 accepted the owner-approved memory-debt
result, and exact F4 commit `9b37c663` passed CI run `39402420054`
([N0002](../specs/0002-f4-whole-group-acceptance.md#final-result--2026-10-20),
[N0012](../specs/0012-f4-qml-memory-debt.md)). The current N9 integration head
is `ab3bcae4`; its exact-head CI run `39431539570` passed desktop and fresh-install
checks, and N9 is accepted. N10 has reviewed local private work at `cfa86087` with
handoff `d651ff4b`; its focused 156-test result is local evidence only. N10 is
not integrated or CI-validated, its root helper remains preparation-only, and
all public switch mutators remain disabled.

This document separates source gates from later owner-controlled package and
live-session actions. **Every command below is an example for a future run and
must not be run while any source gate remains open.** No command in this plan
has been executed.

## Source gates before package work

1. **Completed:** N9 passed exact-head CI on `ab3bcae4` and was accepted.
   N9 provides a cooperative lease for daemon and installer writers;
   it does not itself switch an active daemon or replace the separate N10
   rollback work ([N0009](../specs/0009-f4-switch-prerequisites.md)).
2. N10 must finish and pass source review, integrate with accepted N9, and pass
   CI on the exact integrated main commit. N10's current local result records
   156 focused tests passing, but package execution is still private and the
   public `enable`, `disable`, and `rollback` routes fail closed. The source
   handoff at local commit `d651ff4b` identifies the remaining gates: prove the
   helper-to-pacman lock-descriptor lifetime, complete package/setup/unit
   restoration and fresh-process recovery, and keep the exact package state
   journaled. The private package API at `cfa86087` is not evidence that the
   production helper calls pacman safely or that switching is runnable.
3. Resolve the actual unit origin and package-hook policy before choosing a
   supported path. N10's current fence accepts vendor units under
   `/usr/lib/systemd/user` and refuses source-staged units under
   `~/.config/systemd/user`. Do not recommend a runtime mask for those
   source-staged units: user unit precedence makes that fence unsupported.
   N10 also blanket-refuses preparation when ALPM hooks are present; their
   trigger and effect need review before such a host is supported. These are
   source limitations, not instructions to edit installed units by hand.

F4 itself is no longer the blocker. The previously described absent ownership
guard and wholly absent persistent opt-in are outdated: N9 implements the
destination lease, and N10 has a managed shell-service drop-in design and
partial private transaction code. Neither makes the switch available today.
The proposed `nacre f4 enable ...` and rollback forms must not be presented as
working commands until N10 integrates and its public routes are deliberately
enabled after all acceptance criteria pass.

## Package build and signed publication

Only after all three source gates above pass may the owner choose to build the
Core package from the exact green integrated commit. The package workflow and
signed repository publication are separate gates. Review the successful
package run's source receipt, install proofs, recipe digest, archive signatures,
and signed `release.json`; publication normally follows the successful package
run automatically. Dispatch publication manually only to recover that same
successful run ([Arch package workflow](../../.github/workflows/arch-packages.yml),
[publication workflow](../../.github/workflows/publish-arch.yml),
[package guide](../../packaging/arch/README.md#tested-development-publication)).

Future command template — **DO NOT RUN until N9/N10 are integrated, exact-head
CI is green, and the owner has chosen to start the package run**:

```sh
# DO NOT RUN while any source gate is pending.
F4_GREEN_SHA=exact_full_green_integrated_main_commit
gh workflow run arch-packages.yml --repo nacre-desktop/nacre --ref main \
  -f source_revision="$F4_GREEN_SHA" -f component=core -f use_released_orient=true
```

Manual publication recovery template — **DO NOT RUN unless that exact package
run succeeded and automatic publication did not start**:

```sh
# DO NOT RUN except to recover publication for the same successful package run.
PACKAGE_RUN_ID=exact_successful_package_run_id
gh workflow run publish-arch.yml --repo nacre-desktop/nacre --ref main \
  -f run_id="$PACKAGE_RUN_ID"
```

## Install and prepare

Installation and setup change system and private user files. They remain
owner-controlled and are not authorized by F4 acceptance or by this document.
Before any future install, require the exact Core archive from the successful
package run, completed signed repository publication, verified package and
database signatures, and the prior signed Core archive saved at an exact path.
Record the installed version, unit enablement/active state, manager opt-in value,
and existing release snapshot for the private recovery record. Preserve the
current compositor and notification owner.

Install only that verified archive with pacman's local-archive operation; do not
run a whole-system upgrade to install Nacre. The existing setup flow is plan,
review, then ordinary interactive apply. A source setup that stages daemon units
in the user's unit directory is currently unsupported by N10's activation fence;
stop at that preflight refusal until reviewed source-staged-unit support exists.

Future command template — **DO NOT RUN until N9/N10 source gates pass, the exact
package is signed and published, and the owner authorizes installation**:

```sh
# DO NOT RUN before the package and installation gates above pass.
NACRE_ARCHIVE=/path/to/exact-verified-nacre-core.pkg.tar.zst
test -r "$NACRE_ARCHIVE"
sudo pacman -U -- "$NACRE_ARCHIVE"

# DO NOT RUN until the owner has reviewed the plan and approved its file changes.
nacre setup --plan
nacre setup
```

Do not substitute `pacman -Syu nacre`; that can upgrade unrelated system
packages. Do not use `--profile default` when the intended action is to retain
the installed profile. If plan review finds a foreign file, collision, or
unreviewed profile change, stop before apply. If setup fails after performing
its own snapshot restore, do not run an older `nacre rollback` snapshot over it.
The documented package/setup split is in [installation](../installation.md#arch-packages-and-setup)
and [release handling](../../tools/releases.py).

## Switch, verify, and recover

There is no runnable switch command yet. The existing `nacre rollback` restores
setup-owned private files; it does not by itself restore the prior package,
daemon/socket activation, or QML opt-in. N10's proposed
`nacre f4 rollback --transaction ID` is still fail-closed and is not an
executable rollback contract until the full transaction and recovery path are
accepted. Do not improvise a one-command shell rollback from separate commands:
package and user-file restoration is not atomic, and order/fingerprints matter.
Keep the exact prior package archive and setup snapshot untouched for the later
reviewed recovery path.

After N10 is integrated and exposes an accepted command, the switch must preserve
the recorded unit state and one-writer guarantee, install a managed persistent
QML opt-in, and fail before mutation on unsupported unit origins, hook effects,
changed state, or incomplete recovery evidence. It must not create a second
notification owner. Until then, do not enable the daemon socket, set the manager
environment manually, edit the installed shell service, or run the proposed
`nacre f4` mutators.

Once a future accepted switch has completed, verify daemon reachability and
health with the documented `nacrectl` status and settings-read operations; check
that the desktop settings snapshot is clean, has no error, and contains the
expected durable value. A CLI connection alone does not prove QML opted in or
saved a change. Then perform the owner-only reversible Settings write/read-back
and restore it through Settings. A real logout/login, keyboard/display,
notification ownership, and compositor recovery remain physical owner checks.
The F4/N2 nested results do not claim those checks were performed.

Source basis: [N0001](../specs/0001-f4-settings-completion-audit.md),
[N0002](../specs/0002-f4-whole-group-acceptance.md),
[N0009](../specs/0009-f4-switch-prerequisites.md),
[N0010](../specs/0010-f4-managed-opt-in-and-rollback.md), the staged daemon
units, and the QML opt-in at [`NacreDaemon.qml`](../../nacre/shell/services/NacreDaemon.qml#L12).
