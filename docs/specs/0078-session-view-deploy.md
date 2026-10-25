# N0078 — preserve the running Niri installation view

## Failure and decision

The prepared Niri runtime binds
`~/.local/share/nacre/niri-on-pc/session/home-view/.local/share/nacre/shell`
read-only over `~/.local/share/nacre/shell` in its Bubblewrap mount namespace.
This exposes separately prepared Niri code without replacing the ordinary
installation or isolating the user's whole home. Workspace binds and other
spawned helpers inherit that namespace and use the ordinary installation path.
The overlay is still needed for the prepared runtime; changing a launcher path
alone would leave the rest of the runtime on the stranded mount.

Previously refresh renamed `session` to `session.previous` and rebuilt `session`.
The mount kept the old directory inode; deleting the previous backup on a later
refresh deleted its contents. Recreating the same pathname cannot repair it.

Keep `session`, `home-view`, and the bound shell directory in place. Prepare a
complete home-view in a temporary directory, validate configuration, then merge
files into the existing view using same-filesystem atomic file replacements.
Retain matching directories and prune obsolete entries. Executable replacement
does not truncate a running binary or create a missing-helper interval. Readers
can see a mixture of old/new files during the merge; this is not a whole-tree
transaction. The installer retains its rollback for preparation failures.

Installer refresh copies the previous session instead of moving it. Failure
restores files atomically into existing directories and removes partial new
entries. Backup deletion never deletes the live mount source. Preparation can
run repeatedly and clears the stock marker when switching away from stock Niri.
User-owned custom configuration remains outside this tree. No runtime memory or
service is added; no temporary parallel implementation is retained. The existing
`session.rollback` snapshot remains bounded to one backup, replaced next refresh.
Legacy `session.previous` is never deleted by the new installer: an older session
may still hold a bind there. Delete it 7 days after the replacement is trusted,
after a re-login and verification that no session namespace still uses it.
Such legacy sessions retain their previous code until re-login (or the approved
remount below); their existing helper paths remain usable across new deploys.

## Recovery of a session already broken by an older deploy

This change prevents future directory replacement; it cannot revive a directory
inode whose contents have already been deleted. Deploy from outside the broken
namespace (the lead's real user-manager namespace), never from a Niri-spawned
terminal with the broken installation view. No live recovery was performed by
this unit.

With separate owner approval for live repair, a privileged lead can repair future
Niri spawns without logging out. Obtain and verify the exact compositor PID
(`pgrep` is insufficient when there are nested sessions); inspect its command
line and `/proc/PID/mountinfo`. In a shell in the real host namespace, set:

```sh
pid=VERIFIED_NIRI_PID
view="$HOME/.local/share/nacre/niri-on-pc/session/home-view/.local/share/nacre/shell"
target="$HOME/.local/share/nacre/shell"
test -x "$view/tools/nacre-helper" || exit 1
sudo nsenter --target "$pid" --mount -- umount --lazy "$target"
sudo nsenter --target "$pid" --mount -- mount --bind "$view" "$target"
sudo nsenter --target "$pid" --mount -- mount -o remount,bind,ro "$target"
sudo nsenter --target "$pid" --mount -- "$target/tools/nacre-helper" niri-session-runtime --help
```

Verify mountinfo identifies `$target` as the broken shell bind before detaching
it; never detach `/`, the home directory, or another mount. Run the three
commands consecutively, check every status, and stop on any error. A deleted
mount root cannot be overmounted with the current mount tool (`move_mount`
returns ENOENT), so detach that exact broken bind first. During the brief gap
the underlying ordinary installation is visible. The replacement is confined to
that compositor's namespace, leaving host mounts unchanged. Verify a workspace
shortcut from Niri afterwards. Existing children with their own mount namespace
do not inherit a parent remount; each would require a separately verified repair
or an approved restart. If capabilities/user namespaces prevent `nsenter`, or the
exact namespace cannot be identified safely, re-login is required. The remount
procedure is checked in the disposable namespace regression; it is not
automatically executed or live-validated.

## Acceptance evidence

`tools/tests/test_niri_session_deploy.py` uses a disposable HOME/backend and a
real `unshare --mount` read-only bind. It runs native prepare twice, runs the
installer refresh function twice (including replacing the prior rollback snapshot), then
forces a failure after another successful native preparation. A process already
inside the namespace must execute the newly staged helper after each success
and the restored helper after rollback; the bound directory inode stays equal.
Set `NACRE_REQUIRE_SESSION_MOUNT_TEST=1` to fail rather than skip on unavailable
mount capabilities. All execution is in the task's disposable remote container.
The regression also holds a legacy `session.previous` bind throughout and checks
the privileged recovery recipe after deliberately deleting a fake mount source.

Security scope: only the private prepared-session tree and its existing backup
are written. Same-directory temporary files retain source permissions; no new
external input, command interface, privilege, dependency, or live-session action
is introduced. CI results are recorded in the unit report.
