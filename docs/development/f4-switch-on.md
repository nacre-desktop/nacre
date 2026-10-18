# F4 owner switch-on and rollback plan

**Status:** reviewed-plan draft with execution blockers. The source/N2 gate below
can authorize a package run. Installation and switch-on remain blocked until a
successful package/publication gate is recorded, an exclusive daemon ownership
guard is implemented and source-reviewed, and a bounded managed persistent QML
opt-in plus rollback is implemented and reviewed. This worker did not publish,
install, enable, start, or change anything.

The source review used the published repository at this worktree's starting
revision and read-only F4 candidate
`1382a28ab544743e32e906f7542bcecdfdfca083` (tracked in
[`N0001`](../specs/0001-f4-settings-completion-audit.md)).
That candidate is unpublished. It is not an installed binary or proof that the
release gate passed. Replace `F4_GREEN_SHA` below with the exact N0002-accepted
main commit; do not substitute a moving branch or a historical helper receipt.

## Gate before starting the package run

This gate authorizes only the package workflow. All of these must be recorded
for the same exact source:

1. N0001 and N0004 remain reviewed Done; the final F4 source passed the fresh
   N0002 correctness, whole-group, and performance gates and is integrated on
   `main`.
2. Source CI is green for `F4_GREEN_SHA`.

If any source/N2 gate is missing, do not start the package workflow. N0002
requires fresh evidence and does not accept older v25/v9 receipts for the
current candidate.

## 1. Build and publish the reviewed package

Run these owner-controlled GitHub Actions commands only after the source/N2
gate above.
The package workflow accepts an exact main ancestor, and repository publication
admits only a successful package run with passing source CI. The publishing
workflow normally follows a successful package run automatically; dispatch it
manually only for recovery, using that same successful run ID.

```sh
F4_GREEN_SHA=put_the_full_N0002_accepted_main_commit_here
gh workflow run arch-packages.yml --repo nacre-desktop/nacre --ref main \
  -f source_revision="$F4_GREEN_SHA" -f component=core -f use_released_orient=true
PACKAGE_RUN_ID=put_the_successful_package_workflow_run_id_here
# After that exact run succeeds, only if automatic publication did not run:
gh workflow run publish-arch.yml --repo nacre-desktop/nacre --ref main \
  -f run_id="$PACKAGE_RUN_ID"
```

## Gate before installation

Do not install until the pinned Core package run for `F4_GREEN_SHA` succeeds,
its install proofs and source/package receipt name that exact revision, and the
signed Arch repository publication completes for that run. Record its
`PACKAGE_RUN_ID`; check the package index and package/database signatures.
Also save the exact previously installed `nacre` archive as
`PREVIOUS_NACRE_ARCHIVE`, verify the existing `nacre` release snapshot is
rollback-ready, save work, and accept the shell restart/session recovery window.
Keep the current compositor session and other notification owner unchanged.
A successful package install is not physical login/display/input acceptance;
that remains an owner check after the nested gates.

Review the completed run and signed `release.json` for exact source revision,
package version, recipe digest, successful runtime proofs, and archive/database
signatures. Do not publish an AUR package, stable release, or unrelated package.
The repository's `[no-publish]` convention means a documentation commit does not
itself trigger binary publication. See
[`packaging/arch/README.md`](../../packaging/arch/README.md#tested-development-publication),
[`arch-packages.yml`](../../.github/workflows/arch-packages.yml), and
[`publish-arch.yml`](../../.github/workflows/publish-arch.yml).

## 2. Record the starting session state

Run as the ordinary desktop user before changing package or user files. Save the
output with the owner’s private release record; it contains only unit state and
the value of the one opt-in variable.

```sh
pacman -Q nacre
systemctl --user is-enabled nacre-daemon.socket || true
systemctl --user is-active nacre-daemon.socket || true
systemctl --user is-active nacre-daemon.service || true
systemctl --user show-environment | grep '^NACRE_DAEMON_CLIENT=' || true
nacre status
```

The expected pre-F4 state is an unenabled/inactive daemon socket and no
`NACRE_DAEMON_CLIENT` value. If the actual state differs, record it for the
follow-up rollback design; this plan has no executable rollback yet.
`nacre status` reports the current managed release state. Do not
overwrite the previous package archive or its user release snapshot. It is a
release-record check, not the daemon health check used below
([`tools/releases.py`](../../tools/releases.py#L717)).

## 3. Install and prepare (blocked pending ownership guard)

The package operation updates system-owned files. The setup operation writes
private profile/configuration files, including the whole profile; F4 supports
that replacement only while the daemon for this destination is stopped. First
prevent new socket activation, then stop its service and the shell process that
currently uses the legacy file path:

```sh
systemctl --user stop nacre-daemon.socket
systemctl --user stop nacre-daemon.service
systemctl --user stop nacre-shell.service
nacre setup --plan
```

These stop commands are best-effort quiescence only. Socket/service stop does
not exclude a racing start or another activation path, and stopping the shell
does not prove there is no other writer. Nacre stages these user units in
`~/.config/systemd/user` ([`stage.py`](../../packaging/arch/stage.py#L145),
[`install.py`](../../nacre/shell-tools/install.py#L418)). systemd documents that
runtime masks can conflict with local unit files at that path, including in
user mode; therefore this plan has no compatible verified temporary mask to
recommend ([systemctl mask documentation](https://www.freedesktop.org/software/systemd/man/258/systemctl.html)).
The missing exclusive-activation guard is a blocker: do not replace the profile
until a source-reviewed guard/fence covers racing and alternate activation and
has a defined restoration path. The installer’s active-service checks and
inotify/fingerprint checks do not establish that guarantee
([`tools/releases.py`](../../tools/releases.py#L378),
[`N0001`](../specs/0001-f4-settings-completion-audit.md)).

After the package gate and ownership-guard follow-up, install the exact verified
Core archive from that successful run using the source-supported local-archive
path documented in [`docs/installation.md`](../installation.md#arch-packages-and-setup).
`pacman -U` is
the documented archive install operation; do not use `pacman -Syu nacre`, which
can upgrade unrelated system packages. If the exact archive cannot be obtained
and verified, or setup requires dependency changes, stop and review the exact
transaction and its broader system effects separately.

```sh
NACRE_ARCHIVE=/path/to/exact-verified-nacre-core.pkg.tar.zst
test -r "$NACRE_ARCHIVE"
sudo pacman -U -- "$NACRE_ARCHIVE"
```

Review the setup plan and confirm it preserves the installed profile and local
files. Then apply the ordinary interactive setup without `sudo`:

```sh
nacre setup
```

Do not use `--profile default` here: setup without a profile retains the current
installed profile. If the plan reports a foreign file, a collision, or an
unreviewed profile change, stop before applying and do not force through it. No
user-file rollback is needed for a plan-only stop; restore the previous package
and restart the shell using the recorded state. If `nacre setup` itself fails,
its apply path restores its own snapshot; do not run `nacre rollback` a second
time against an older successful snapshot. That automatic restore does not undo
a successful pacman upgrade. References:
[`docs/installation.md`](../installation.md#updates) and
[`tools/releases.py`](../../tools/releases.py#L378) show the package/setup split;
the latter's source deployment path runs `profile-policy.py install` before
cutover and checks pre-existing active services, so it is not the supported
active-daemon installer path.

The staged unit source has `nacre-daemon.socket` listening on the private
`%t/nacre/daemon.sock` path, with `RemoveOnStop=yes`; the service requires that
socket and runs the installed `nacre-daemon` alias. Package staging copies these
templates but creates no enablement link and starts no daemon. After successful
setup, reload the user manager's unit files:

```sh
systemctl --user daemon-reload
```

Source: candidate [`nacre-daemon.socket`](../../nacre/shell-tools/nacre-daemon.socket)
and [`nacre-daemon.service`](../../nacre/shell-tools/nacre-daemon.service),
[`packaging/arch/stage.py`](../../packaging/arch/stage.py#L145), and
[`nacre/shell-tools/install.py`](../../nacre/shell-tools/install.py#L418).

## 4. Enable daemon ownership and opt QML in (blocked pending persistent opt-in)

The candidate QML reads `NACRE_DAEMON_CLIENT=1` when Quickshell starts. The
packaged `nacre-shell.service` has no `Environment=` setting, and package/setup
only stages the daemon templates. The reviewed tree has no managed mechanism
that persists this QML opt-in. `systemctl --user set-environment` is volatile,
while enabling the daemon socket is persistent. That creates a mixed reboot
state: the socket remains enabled while the QML client opt-in can disappear.
This is a top-level execution blocker. Do not switch on until a bounded,
Nacre-managed persistent opt-in and its rollback are implemented and reviewed.
No product command or configuration path is specified here because none is
currently supported by source. Do not edit the installed service by hand. The
exact QML gate is in candidate
[`NacreDaemon.qml`](../../nacre/shell/services/NacreDaemon.qml#L10), and the
service template is
[`nacre-shell.service`](../../nacre/shell-tools/nacre-shell.service#L5).
systemd documents that `set-environment` changes the manager environment inherited
by user services and that `unset-environment` removes that override
([`systemctl`](https://www.freedesktop.org/software/systemd/man/258/systemctl.html)).

Do not enable a notification unit or change notification D-Bus ownership as part
of this switch. A second notification owner is outside F4.

## 5. Verify settings, service health, and recovery

After the ownership guard and persistent opt-in blockers are resolved, run
these read-only commands after the shell reconnects:

```sh
systemctl --user is-enabled nacre-daemon.socket
systemctl --user is-active nacre-daemon.socket
nacrectl status
nacrectl call daemon.status
nacrectl call --list
nacrectl call settings.get '{"store":"desktop"}'
nacrectl watch settings.store.desktop
```

The last command remains attached until stopped with Ctrl-C. Confirm the daemon
is reachable and safe/healthy, `settings.get` returns the current desktop store,
and the watched snapshot has the expected revision/data with `dirty:false` and no
`error`. A CLI connection alone does not prove QML opted in or that a write was
saved.

Then, as an owner-only interactive check, open Settings in the current session,
change one reversible preference, and confirm the UI shows the new value with no
visible save error. Read the same store again with `nacrectl call settings.get
'{"store":"desktop"}'` and verify the intended value is present at a clean
revision; restore the preference through Settings and verify again. The QML
storage exposes per-store errors but has no separate durable-success callback;
the CLI snapshot is the durable-state check. Do not manufacture a write with a
guessed revision or replay a timed-out mutation. A failed save must retain its
existing visible error and must not be called successful merely because a later
unrelated store revision is clean. N0001 defines the matching-value durable-
success check; see [`protocol.md`](../architecture/protocol.md#settings-stores),
the N0001 spec, and `protocol/schema/settings-set.json` at candidate
`1382a28ab544743e32e906f7542bcecdfdfca083`.

Check restart and session recovery without killing processes:

```sh
systemctl --user stop nacre-daemon.socket
systemctl --user stop nacre-daemon.service
systemctl --user start nacre-daemon.socket
nacrectl status
nacrectl call settings.get '{"store":"desktop"}'
```

After the compatible guard is applied, stopping the socket removes its current
listening socket and stopping the service terminates the current daemon process;
the guard must also exclude racing and alternate activation during that interval.
Starting only the socket lets the next client request activate a fresh daemon.
Confirm CLI and QML reconnect, the stored value remains present, and there is
still one settings writer. A real logout/login, keyboard/display, notification
ownership, and compositor recovery check remains physical owner validation;
this plan does not claim to have run it.

The CLI grammar is from candidate
[`client.rs`](../../crates/nacre-helper/src/daemon/client.rs#L293). `settings.get`
requires a store name; `settings.set` requires a key, value, and current revision
(`protocol/schema/settings-get.json` and `settings-set.json` at candidate
`1382a28ab544743e32e906f7542bcecdfdfca083`). The checks above
avoid a hand-built mutation because they do not retrieve/validate the current
revision and owner intent on the command line.

## Rollback contract (blocked pending persistent opt-in)

Keep `PREVIOUS_NACRE_ARCHIVE` pointing to the exact signed, previously installed
Core package archive and keep the successful `nacre setup` release snapshot.
There is no executable one-block rollback yet: the managed persistent opt-in
does not exist, its cleanup path is undefined, and the exclusive activation
guard has no restoration procedure. Those are implementation/review blockers,
not operator steps to guess. The completed follow-up must define one reviewed
rollback that restores prior opt-in and unit activation state, setup-owned
private files, and the exact previous Core package archive. It must preserve
application settings data.

The source-supported pieces are `nacre rollback` (fingerprint-checked restore
of setup-owned private files) and `sudo pacman -U -- "$PREVIOUS_NACRE_ARCHIVE"`
(exact-archive downgrade); they are not by themselves a complete switch rollback.
If setup fails after its own restore, do not call `nacre rollback` against an
older successful snapshot.

Source basis: package rollback action and compare-before-restore behavior are in
[`tools/releases.py`](../../tools/releases.py#L214) and its `rollback` CLI at
[`tools/releases.py`](../../tools/releases.py#L658); the package-mode restore
does not stop/restart live units. A future switch rollback must therefore restore
service, opt-in, package, and setup state explicitly. It will not be a cross-
pacman/user-file atomic transaction; interruption will require inspecting
package, unit, manager-environment, and release-snapshot state before resuming.

## Source and command evidence

This is a source-reviewed plan, not an execution record. No package workflow,
systemd, pacman, Settings write, physical session, or rollback command was run
for this document. The release commands and package gates are documented in
[`packaging/arch/README.md`](../../packaging/arch/README.md#tested-development-publication)
and the two linked Actions workflows. The package install/setup commands are
documented in [`docs/installation.md`](../installation.md#arch-packages-and-setup).
Unit, QML, CLI, and rollback command names/flags above are traced to the cited
source. The cited candidate unit/QML details must be rechecked against the exact
N0002-green source before the owner executes the plan.
