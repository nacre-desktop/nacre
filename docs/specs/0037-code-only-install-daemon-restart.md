# N0037 — Code-only install restarts the daemon and refreshes the niri session

Status: **Implemented; pending lead review**. Scope: `nacre/shell-tools/install.py --code-only`.

## Problem

`install.py --code-only` replaced `~/.local/share/nacre/shell/tools/nacre-helper`
and restarted `nacre-shell`, but not `nacre-daemon.service`, so daemon-side changes
stayed inactive until a manual restart. The generated niri session config
(`~/.local/share/nacre/niri-on-pc/session`, including the staged shell copy and
`spawn-at-startup` items) was likewise never refreshed.

## Behaviour (code-only only; applies directly as before)

1. **Daemon restart.** Before replacing the helper, the installer copies the
   installed one to `~/.local/state/nacre/shell/helper-previous/nacre-helper`
   (single slot) when the candidate differs. After the code is deployed and before
   `nacre-shell` is restarted, if a backup exists and `nacre-daemon.service` is
   active, it runs `systemctl --user restart nacre-daemon.service` and polls
   `nacrectl status` (3 s per call, 20 s total). Healthy means: JSON parses, `safe`
   is not true, `services` is non-empty with none `error`/`refused`, and `topics`
   is non-empty. Unchanged helper or inactive daemon: no restart.
2. **Rollback.** If the restart or health check fails, the previous helper is put
   back (atomic replace), the daemon restarted again and checked, and the install
   aborts non-zero with a message saying whether the daemon is healthy again. The
   shell is not restarted in that case. With no backup the failure is reported.
3. **Niri session refresh.** Applies only when
   `session/source-revision` exists (the same marker `uwsm/env-niri` uses to select
   the prepared session) and differs from `git rev-parse HEAD` of the checkout.
   The installer moves `session` to `session.previous` (one slot, replaced each
   time), then runs `nacre-helper niri-session-runtime --backend <repo>
   [--stock-niri] prepare` (`--stock-niri` iff `session/stock-niri` exists; this
   avoids building the hover companion). On failure the backup is moved back and
   the install exits non-zero (after the code deploy and shell restart succeeded).
   `~/.config/nacre/niri/custom.kdl` and everything outside `session/` are never
   touched. Running niri reloads `config.kdl` live; `spawn-at-startup` items (kwallet
   init, startup-apps, welcome, clipboard watcher) only apply at the next login. The
   staged shell copy under `home-view` is refreshed too, so a restarted niri-session
   shell sees the new code. Uncommitted checkout changes do not change the revision.
4. **`--dry-run`** (requires `--code-only`): prints whether the helper changed,
   the daemon-restart plan and the niri refresh plan, and changes nothing.

## Checks

`nacre/shell-tools/tests/test_installer_dropin_guard.py` (temp HOME, fake
`systemctl` and fake helper/`nacrectl`): restart ordering, rollback, no-op cases,
health parser, niri plan, refresh, failure restore, custom.kdl untouched, dry run.

## Temporary duplicates

None.
