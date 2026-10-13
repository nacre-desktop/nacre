# Side-by-side niri testing

This is an incremental acceptance record, not a claim of completed niri support.
The working Hyprland files and package-owned session entries remain the fallback.
Test results below distinguish measured checks from pending physical-session work.

## Recovery first: return to Hyprland from a TTY

Read this before selecting niri. Save work in the current desktop before logging
out: a real logout stops graphical applications and their terminal-hosted agents.
Keep this document and the private backup pointer accessible from a TTY.

1. Press **Ctrl+Alt+F3** and log into the normal account. Do not disable account
   authentication or the lock screen for this experiment.
2. Check whether the working Hyprland session is still running:

   ```sh
   systemctl --user is-active wayland-wm@hyprland.desktop.service
   ```

   If it says `active`, leave it running and return to its original VT (currently
   **Ctrl+Alt+F1** on the test machine). Do not restart SDDM on a healthy desktop.
3. If the failed session is niri, stop that compositor through its session manager:

   ```sh
   systemctl --user is-active wayland-wm@niri.desktop.service
   uwsm stop
   ```

   `uwsm stop` stops the current managed graphical session and its associated
   services. It is a recovery command for the failed session, not a parallel-session
   launcher. If the greeter does not return, restart it from this TTY:

   ```sh
   sudo systemctl restart sddm.service
   ```

4. At the greeter select the existing **Hyprland (uwsm-managed)** entry or the new
   **Nacre (Hyprland)** alias. They use the same original Hyprland desktop entry and
   configuration. Log in normally. Do not select the broken niri entry again until
   its logs and configuration have been reviewed.
5. If the greeter itself is unavailable, after stopping the failed graphical session
   a temporary TTY fallback is:

   ```sh
   exec uwsm start -e -D Hyprland hyprland.desktop
   ```

   Use `exec` so exiting the compositor does not leave an unlocked shell on that VT.
   The existing Hyprland configuration remains authoritative.

Private backups are referenced by
`~/.local/state/nacre/backups/niri-on-pc-latest`. Their contents and machine/account
inventory must never be committed. Every later changed file needs an original copy
or an explicit record that it was newly created. The original Hyprland directory,
UWSM files, user units and profile bytes are fingerprinted for before/after checks.

## Phase 1: plain niri, separate from the working shell

The package plan is `pacman -S --needed niri xwayland-satellite wlsunset`. Inspect
its transaction before applying: add only the required packages, without an
unrelated distribution/Qt/Hyprland update. Installed versions are niri 26.04, xwayland-satellite 0.8.3 and wlsunset 0.4.0.
`wayland-utils` was added separately for read-only protocol inspection. These
transactions added packages without upgrading existing ones. Existing packages include UWSM
0.27, Hyprland 0.56.2, hyprlock 0.9.6 and hypridle 0.1.8.

SDDM already reads `/usr/local/share/wayland-sessions` and
`/usr/share/wayland-sessions`; additional named aliases require no edit to its
working configuration or the package-owned Hyprland entries. Use UWSM as the
session owner. niri's packaged `niri-session` detects external user-service
management; do not start a second independent niri.service alongside UWSM.

Plain-niri acceptance must pass before deploying the backend agent's Nacre branch.
A Hyprland-only shell must not enter its shared source recovery loop under niri.
Do not enable automatic locking on niri before successful manual lock and unlock.

| Check | Result | Evidence / next check |
|---|---|---|
| Recovery written before changes | Passed | This section precedes package/session changes. |
| Existing Hyprland uses UWSM | Passed | Active `wayland-wm@hyprland.desktop.service`; graphical-session-bound Nacre units. |
| SDDM session directories | Passed | Installed SDDM configuration lists both directories above. |
| Package installation | Passed | Three functional packages and the protocol-inspection utility installed with pacman; private transaction logs and snapshots retained. |
| Private backup | Passed | Private pointer and baseline fingerprints created before changes; original Hyprland/UWSM/profile bytes and original SDDM/Hyprland session entries still match. |
| Named login aliases | Installed, selection pending | Both aliases added under `/usr/local/share/wayland-sessions`; original entries untouched. |
| Plain config | Passed | `niri validate` accepts the phase-1 config. |
| UWSM selection | Dry run passed | `niri.desktop` resolves to `niri-session` with the installed `niri_session` plugin. No live second UWSM session started. |
| Startup/idle isolation | Fixture tests passed | ExecCondition skips shared shell/observer and automatic idle in plain niri, permits unchanged Hyprland path. |
| Nested compositor | Passed, preflight only | Real output/workspace/window IPC and clean exit; display/socket activation environment unchanged. |
| TTY keyboard/login/recovery | Deferred by operator | Do not confuse a documented command with a physically verified fallback. |
| niri login and logout | Deferred by operator | Must be an actual SDDM login and clean logout. |
| Return login to Hyprland | Deferred by operator | Confirm original configuration fingerprints and working shell after return. |
| UWSM environment cleanup | Deferred by operator | Check session targets, display socket and NIRI_SOCKET after logout. |

### Protocol observations

The live Hyprland 0.56.2 registry advertises `ext_session_lock_manager_v1`,
`ext_idle_notifier_v1`, `zwlr_gamma_control_manager_v1`,
`hyprland_ctm_control_manager_v1`, `zwlr_screencopy_manager_v1` and
`ext_image_copy_capture_manager_v1`. Protocol presence makes a shared wlsunset
implementation worth testing; it does not prove correct gamma behavior, lock
coverage or successful password authentication. Preserve external schedules and
current ownership until actual phase-2 tests.

### Local phase-1 gates

New per-user drop-ins use `uwsm/tools/niri-session-gate.py` as ExecCondition for
`nacre-shell.service`, `nacre-session-watch.service` and `hypridle.service`. An
explicit Hyprland desktop context passes through even if a stale niri socket is
present. Plain niri cleanly skips these units. niri's future acceptance record is
separate from existing private profile files; no readiness/unlock success is
recorded yet. The installed package's `Niri` entry also receives the guard because
it applies to service startup rather than only a particular menu alias.

`uwsm/env-niri` selects the task-local `NIRI_CONFIG` and removes Hyprland-only
activation variables and forced GDK backend only in the niri session. The shared
`uwsm/env` and `env-hyprland` bytes remain unchanged. No original unit file was
replaced, and daemon-reload did not request a service or compositor restart.

### Acceptance recorder and deferred session test

The operator requested that the working Hyprland session remain running. Actual
TTY/SDDM switching and phase 2 are therefore deferred; the prepared niri alias
currently opens plain niri, not the new Nacre backend. The task remains incomplete
until the real cycle and phase-2 checklist pass.

A bounded private recorder is provided in `uwsm/tools/niri-acceptance-monitor.py`.
It observes actual UWSM compositor units and activation booleans, rejecting a nested
socket as evidence of a real niri login. It runs as the normal user in a temporary
system service, outside the graphical lifecycle. It does not change linger, claim
password unlock, or write a backend-readiness marker. The preparation instance was
stopped when session testing was deferred.

Before a later test, start it from a normal terminal with administrator authentication:

```sh
sudo systemd-run --unit=nacre-niri-acceptance-monitor --collect \
  --property=User="$(id -u)" --property=Group="$(id -g)" \
  --property=RuntimeMaxSec=7200 \
  --setenv=XDG_RUNTIME_DIR="/run/user/$(id -u)" \
  --setenv=DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$(id -u)/bus" \
  /usr/bin/python3 "$HOME/.local/share/nacre/niri-on-pc/niri-acceptance-monitor.py" \
  --seconds 7200
```

Save all work, log out, select **Nacre (niri)**, log in, open Kitty with **Super+T**
and check the desktop. Use **Super+Shift+E** to exit; select **Nacre (Hyprland)**
and return. Resume this same task and inspect the private recorder's
`acceptance-observed.json` and `acceptance-events.jsonl` in the backup directory.
Separately report actual greeter selection, keyboard/TTY recovery and visual
results. Do not mark a physical test passed just because unit observation succeeds.

The strict fingerprint check detected only a generated lock-appearance change:
background image and color values, with authentication and geometry unchanged.
Scheme, wallpaper-state and lock-config timestamps coincide, and the existing
writer derives lock appearance from those state files. This is consistent with
ambient wallpaper/palette updates; the exact triggering action was not recorded.
The current generated file and original backup were both preserved, without
rewriting current presentation or private preferences to force a byte-equality
pass. Keep this drift visible during acceptance and review its cause separately
from the protected static configuration.

Repository validation on current main `d08e06d` with this task passes: **625 local tests**,
Ruff lint/format, native Qt/QML parsing and regression tests, compositor boundary
and generated-config checks, palette CLI, and browser formatting. GitHub's desktop
job also passes; its shell suite reports two skips while the local gate reports
none. Startup and observation additions have nine focused tests.

## Phase 2: backend branch and personal preferences

The backend agent owns the compositor implementation and generated config. PR12
is merged at `ccc86d3`; the preview consumes public main `3c87a61`, including
the standard binding update. Do not modify its
backend as a local workaround; report reproducible problems on its PR. Keep the
maintainer's personal profile, private preferences and AI access policy byte-for-byte
unless an explicit, scoped migration is required. Use a separate niri shell source
so a failed niri experiment cannot overwrite the working Hyprland source.

### Windowed Nacre preview (2026-10-13)

The operator now requests full Nacre testing while preserving the active Hyprland
desktop and parallel chats. `uwsm/tools/niri-preview.py` stages the merged shell,
backend, generated keyboard/profile/bindings/rules, private appearance and panel
preferences, palette runtime and installed launch controllers in a separate tree.
Original configuration remains untouched. The explicit personal AI access setting
is retained; the preview does not change the real profile or account policy.

From this task worktree, choose a new empty private destination:

```sh
python3 uwsm/tools/niri-preview.py prepare --backend "$PWD" \
  --destination "$HOME/.local/share/nacre/niri-on-pc/preview-test"
python3 uwsm/tools/niri-preview.py run \
  --destination "$HOME/.local/share/nacre/niri-on-pc/preview-test"
```

`bwrap` supplies a read-only host filesystem with writable private configuration,
Nacre state/cache and Kitty cache. A private runtime directory and `dbus-run-session`
separate display, clipboard, notifications and bus ownership. Run plain `niri`,
never `niri --session` or UWSM start, inside the active desktop. No global
activation environment is imported. Resolved installed-runtime symlinks are
mounted into normal directories so hiding real release state does not break the
palette interpreter. Startup and generated shortcut IPC use the same Quickshell
configuration path as installed controllers.

Focus the niri window. Preview-only aliases are **Alt+Space** (launcher),
**Alt+O** (dashboard), **Alt+Enter** (Kitty), **Alt+1…9** (available named
workspaces), and **Alt+Shift+Escape** (exit just the preview). Generated Super
bindings are retained, but Hyprland may consume them first. These aliases do not
change real-login bindings or any Hyprland configuration. The normal Nacre bar,
launcher and panels retain their existing appearance.

Windowed checks passed: native config validation; loaded shell and live workspace
state; launcher open/controller palette route; dashboard IPC open with pinning;
detached launcher represented by a real niri window; Wayland and forced-X11 Kitty windows;
private notification delivery visibly captured; and `grim` output capture.
Unpinned dashboard disappears without hover by existing design; automation pins
it for the state assertion. User-manager environment bytes match before/after,
and the running Hyprland services stay active. Physical key/input acceptance is
still pending. Synthetic wtype events inside nested niri returned success but did
not activate the Alt aliases; do not count this as keyboard acceptance. Test keys
with the actual niri window focused. A five-workspace profile displays a sixth empty workspace because
niri keeps a trailing empty workspace; this is expected dynamic behavior.

This is an integration preview, not a completed login installation. System bus,
PipeWire, power/brightness/session operations, manual lock and idle commands are
unavailable here; UPower/Bluetooth/audio/portal warnings reflect that isolation.
Private AI accounts/history are not copied and real chat writes are read-only.
Custom Hyprland Lua is copied for reference, not executed or automatically
translated. Generated compositor bindings are provided by the merged backend.
Real lock/unlock, idle, night light, mixed-output testing and SDDM service lifetime
remain unverified until a real-session cycle.

### Full Nacre login preparation (2026-10-13)

The operator prefers testing at the login screen; the interactive nested preview
was closed. `uwsm/tools/niri-session-runtime.py prepare --backend "$PWD"` stages
a separate niri shell/tool tree and personal generated configuration under
`~/.local/share/nacre/niri-on-pc/session`. It validates niri config before writing
the `source-revision` marker. The existing private profile remains live and intact;
the prepared profile copy is only source-staging input.

When that marker is present, `uwsm/env-niri` selects the prepared config and adds
the niri-only `session-bin` to PATH. Its niri wrapper intercepts only `--session`;
queries/validation still use the native binary. UWSM retains session ownership.
Both the compositor and shell service see the separately staged code mounted over
the conventional shell path in their own process namespaces. This keeps QML's
helper paths and generated bindings consistent without replacing installed
Hyprland source. The real system/user buses, runtime, device access and private
preferences stay available. This code view is deliberately not the isolated
hardware-disabled preview.

The shell service's new ExecStart wrapper executes the original controller
directly in Hyprland context. In niri it runs the staged supervisor. Nested
supervisor preflight loads the full shell; the namespace can access the render
device. No service or compositor restart is requested when installing the
drop-in. Shell readiness, observer readiness and password-unlock readiness are
separate: allow the prepared niri shell, keep the observer and hypridle gated.
The observer currently attempts unsupported saved-monitor restoration; reported
on [the backend PR](https://github.com/nacre-desktop/nacre/pull/12#issuecomment-6140649789).

The niri-only loginctl wrapper routes the existing explicit Lock action to
hyprlock even while hypridle is gated; other loginctl commands and Hyprland
behavior pass through. Password authentication is unchanged. Test manual unlock
before enabling idle/suspend locking. Night light is not ready under niri yet.

To return this machine to guarded plain niri without changing Hyprland, rename
`~/.local/share/nacre/niri-on-pc/session/source-revision` out of the way and restore
the backed-up acceptance record (or remove the new record if it was absent).
The new runtime drop-in can be removed and the user manager daemon-reloaded;
its Hyprland path already passes through unchanged. Keep backups outside Git.

**Ready for a first login test** means the code/config/startup preparation is in
place, not that SDDM login/logout, hardware, lock/password unlock or night light
have passed. Select **Nacre (niri)** when all current graphical work is saved.
Return through **Nacre (Hyprland)** or the TTY fallback at the start of this guide.

Installation is complete on the test machine with a private rollback receipt at
`~/.local/state/nacre/backups/niri-full-login-latest`. Protected common/Hyprland
environment, profile and Hyprland config hashes match before/after; running shell
and idle service PIDs and shared activation environment are unchanged. Native
niri config and systemd unit verification pass. Local repository validation passes
696 tests plus lint, format, Qt/QML, generated/boundary, palette and JavaScript
checks. Actual login acceptance remains pending.

| Area | Result | Acceptance steps |
|---|---|---|
| Login/logout | Pending | Select both named sessions from SDDM; verify service lifetime. |
| Shortcuts | Pending | Terminal, launcher, settings, overview, close, screenshot and logout. |
| Workspaces | Pending | Focus/move windows, dynamic IDs, saved named-workspace behavior. |
| Launcher | Windowed open/controller route passed; input pending | Search, app launch, Escape, settings routes; no stale Hyprland IPC. |
| Dashboard | Windowed pinned IPC open passed; interaction pending | Open/close, tabs, gauges and focus; preserve appearance. |
| Notifications | Windowed delivery/capture passed; remaining pending | Delivery, timeout, history, focus and placement. |
| Detached panels | Windowed launcher/floating window passed; movement pending | Detach, move, resize, return; floating focus and placement. |
| Manual lock/unlock | Pending | Standard session-lock protocol, password authentication, all outputs. |
| Idle | Pending, automatic locking gated | Test manual unlock first, then idle and suspend hooks. |
| Night light | Pending | Protocol-based tool behind existing owner; on/off, ownership and logout reset. |
| Screenshots | Windowed grim output passed; area/clipboard pending | Area, output and clipboard; confirm available capture protocol. |
| Multi-monitor | Pending | Mixed scale, hotplug, focus, placement and lock coverage. |
| X11 apps | Windowed forced-X11 Kitty launched; input pending | xwayland-satellite app launch, focus, input and exit. |

## Open issues and sources

- The installed backend lacks niri; the prepared niri-only code view and supervisor
  supply the merged implementation without replacing the working Hyprland source.
  Session observation remains gated until saved-monitor restoration respects the
  backend capability. Real startup/teardown acceptance is pending.
- Existing idle resume uses a Hyprland DPMS command. niri needs its own resume hook
  without changing the working Hyprland policy.
- hyprsunset is compositor-specific. Protocol-based night light is a phase-2 change,
  after phase-1 acceptance; external schedules must not be taken over.
- Actual logout terminates terminal-hosted agents. Commit/push a milestone and use
  the progress note to resume this same task after session testing.

Primary references: [niri getting started](https://niri-wm.github.io/niri/Getting-Started.html),
[niri required software](https://niri-wm.github.io/niri/Important-Software.html),
[niri 26.04 session wrapper](https://github.com/niri-wm/niri/blob/v26.04/resources/niri-session),
[UWSM session management](https://github.com/Vladimir-csp/uwsm).
