# F4 owner switch-on and rollback plan

**Status:** owner execution plan; do not run until the F4 release gate below is
green and this plan has been reviewed. This worker did not publish, install,
enable, start, or change anything.

The source review used the published repository at this worktree's starting
revision and read-only F4 candidate
`1382a28ab544743e32e906f7542bcecdfdfca083` (tracked in
[`N0001`](../specs/0001-f4-settings-completion-audit.md)).
That candidate is unpublished. It is not an installed binary or proof that the
release gate passed. Replace `F4_GREEN_SHA` below with the exact N0002-accepted
main commit and its successful package-run receipt; do not substitute a moving
branch or a historical helper receipt.

## Gate before touching the daily-driver session

Proceed only after all of these are recorded for the same exact source:

1. N0001 and N0004 remain reviewed Done; the final F4 source passed the fresh
   N0002 correctness, whole-group, and performance gates and is integrated on
   `main`.
2. Source CI is green for `F4_GREEN_SHA`. The pinned Core package run passed
   against that source, and its install proofs and source/package receipt name
   the same revision. Record its run ID as `PACKAGE_RUN_ID`.
3. The signed Arch repository publication completed for that successful run.
   Check the package index and package/database signatures before installing.
4. The owner has saved the exact previously installed `nacre` archive as
   `PREVIOUS_NACRE_ARCHIVE`, verified the existing `nacre` release snapshot is
   rollback-ready, saved work, and accepted the shell restart/session recovery
   window. Keep the current compositor session and other notification owner
   unchanged; this plan does not enable a second notification service.

If any gate is missing, stop before package installation. N0002 explicitly
requires fresh evidence and does not accept the older v25/v9 receipts for the
current candidate. The remaining physical login/display/input acceptance is an
owner check after the nested gates; a successful package install is not that
acceptance.

## 1. Build and publish the reviewed package

Run these owner-controlled GitHub Actions commands only after the gate above.
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
`NACRE_DAEMON_CLIENT` value. If the actual state differs, stop and expand the
rollback record before proceeding; the rollback block below assumes those
preconditions. `nacre status` reports the current managed release state. Do not
overwrite the previous package archive or its user release snapshot. It is a
release-record check, not the daemon health check used below
([`tools/releases.py`](../../tools/releases.py#L717)).

## 3. Install and prepare while both writers are offline

The package operation updates system-owned files. The setup operation writes
private profile/configuration files, including the whole profile; F4 supports
that replacement only while the daemon for this destination is stopped. First
prevent new socket activation, then stop its service and the shell process that
currently uses the legacy file path:

```sh
systemctl --user stop nacre-daemon.socket
systemctl --user stop nacre-daemon.service
systemctl --user stop nacre-shell.service
sudo pacman -Syu nacre
nacre setup --plan
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

The stop order is part of the ownership boundary: stopping the socket first
blocks new connections from socket-activating the daemon; stopping the service
then terminates an already activated process; stopping the shell ends the legacy
file writer before profile replacement. Existing inotify/fingerprint checks
detect or preserve some conflicting edits, but they do not prove exclusive
ownership and are not an active-daemon install guard. The owner must keep these
offline preconditions until setup finishes; a future managed installer guard
remains a bounded product follow-up, not a capability claimed here
([`N0001`](../specs/0001-f4-settings-completion-audit.md)).

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

## 4. Enable daemon ownership and opt QML in

The candidate QML reads `NACRE_DAEMON_CLIENT=1` when Quickshell starts. The
packaged `nacre-shell.service` has no `Environment=` setting, and package/setup
only stages the daemon templates. The reviewed tree has no managed command that
persists this QML opt-in. The following owner-approved session switch uses the
user systemd manager environment, then starts the previously stopped shell so
the new Quickshell process reads it:

```sh
systemctl --user set-environment NACRE_DAEMON_CLIENT=1
systemctl --user enable --now nacre-daemon.socket
systemctl --user start nacre-shell.service
```

This manager setting is volatile: it lasts only while the user manager retains
it, and must be set again if that manager is recreated. The daemon socket's
enablement link is persistent. This switch restarts the shell and briefly
interrupts the current UI; it does not reboot or log out the compositor. If the
owner requires a boot-persistent QML setting, stop here: add and review a bounded
managed opt-in mechanism and its rollback before executing this section. Do not
invent an environment file or edit the installed service by hand. The exact QML
gate is in candidate
[`NacreDaemon.qml`](../../nacre/shell/services/NacreDaemon.qml#L10), and the
service template is
[`nacre-shell.service`](../../nacre/shell-tools/nacre-shell.service#L5).
systemd documents that `set-environment` changes the manager environment inherited
by user services and that `unset-environment` removes that override
([`systemctl`](https://www.freedesktop.org/software/systemd/man/258/systemctl.html)).

Do not enable a notification unit or change notification D-Bus ownership as part
of this switch. A second notification owner is outside F4.

## 5. Verify settings, service health, and recovery

Run these read-only commands after the shell reconnects:

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

Stopping the socket removes its old listening socket and prevents new activation;
stopping the service separately ensures the daemon process exits. Starting only
the socket lets the next client request activate a fresh daemon. Confirm CLI and
QML reconnect, the stored value remains present, and there is still one settings
writer. A real logout/login, keyboard/display, notification ownership, and
compositor recovery check remains physical owner validation; this plan does not
claim to have run it.

The CLI grammar is from candidate
[`client.rs`](../../crates/nacre-helper/src/daemon/client.rs#L293). `settings.get`
requires a store name; `settings.set` requires a key, value, and current revision
(`protocol/schema/settings-get.json` and `settings-set.json` at candidate
`1382a28ab544743e32e906f7542bcecdfdfca083`). The checks above
avoid a hand-built mutation because they do not retrieve/validate the current
revision and owner intent on the command line.

## One-block rollback

Keep `PREVIOUS_NACRE_ARCHIVE` pointing to the exact signed, previously installed
Core package archive; keep the successful `nacre setup` release snapshot. The
block assumes the initial state was inactive/disabled with the QML variable
unset, as checked in step 2. It stops socket activation first, then the daemon
and shell, removes the temporary opt-in and socket enablement, restores the
setup-owned private files, downgrades the Core package, reloads units, and starts
the prior shell:

```sh
set -eu
PREVIOUS_NACRE_ARCHIVE=/path/to/saved/previous-nacre.pkg.tar.zst
test -r "$PREVIOUS_NACRE_ARCHIVE"
systemctl --user stop nacre-daemon.socket
systemctl --user stop nacre-daemon.service
systemctl --user stop nacre-shell.service
systemctl --user disable nacre-daemon.socket
systemctl --user unset-environment NACRE_DAEMON_CLIENT
nacre rollback
sudo pacman -U -- "$PREVIOUS_NACRE_ARCHIVE"
systemctl --user daemon-reload
systemctl --user start nacre-shell.service
nacre status
```

This is one operator invocation, not an atomic transaction. Run it as the normal
user; only pacman requests elevation. If any command fails, stop the block, keep
the shell/daemon stopped, preserve the rollback snapshot and report the failing
step before retrying or repairing. `nacre rollback` refuses to overwrite
user-edited paths whose fingerprints changed after setup; preserve those edits
and resolve them explicitly. Pacman downgrade requires the exact previous
archive; setup rollback alone does not downgrade a system package. The F4
rollback also leaves application settings data intact: it restores only files
captured by setup and never deletes user state. Verify the restored package with
`pacman -Q nacre`, the prior shell with `systemctl --user is-active
nacre-shell.service`, and daemon inactivity with
`systemctl --user is-enabled nacre-daemon.socket` plus
`systemctl --user is-active nacre-daemon.socket`.

Use this full block only after a successful `nacre setup` created the release
snapshot. For a plan-only stop or an apply failure that already restored itself,
do not call `nacre rollback` against the prior release: stop socket/service/shell,
unset the temporary environment, install the saved package archive, reload units,
and start the shell, checking each result. This narrower recovery omits private
file restoration because no successful setup transaction remains to undo.

Source basis: package rollback action and compare-before-restore behavior are in
[`tools/releases.py`](../../tools/releases.py#L214) and its `rollback` CLI at
[`tools/releases.py`](../../tools/releases.py#L658); the package-mode restore
does not stop/restart live units. Therefore this block performs service/environment
rollback explicitly. It has no cross-pacman/user-file atomicity: interruption
requires inspecting package, unit, manager-environment, and release-snapshot
state before resuming.

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
