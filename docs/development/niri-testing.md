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
unrelated distribution/Qt/Hyprland update. Current repository candidates are niri
26.04, xwayland-satellite 0.8.3 and wlsunset 0.4.0. Existing packages include UWSM
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
| Package plan | Passed, not installed yet | Dry-run transaction contains only the three requested additions. |
| Private backup | Passed | Private pointer and baseline fingerprints created before changes. |
| TTY keyboard/login/recovery | Pending operator test | Do not confuse a documented command with a physically verified fallback. |
| niri login and logout | Pending | Must be an actual SDDM login and clean logout. |
| Return login to Hyprland | Pending | Confirm original configuration fingerprints and working shell after return. |
| UWSM environment cleanup | Pending | Check session targets, display socket and NIRI_SOCKET after logout. |

## Phase 2: backend branch and personal preferences

The backend agent owns the compositor implementation and generated config. Wait
for its exact tested revision and command before consuming it. Do not modify its
backend as a local workaround; report reproducible problems on its PR. Keep the
maintainer's personal profile, private preferences and AI access policy byte-for-byte
unless an explicit, scoped migration is required. Use a separate niri shell source
so a failed niri experiment cannot overwrite the working Hyprland source.

| Area | Result | Acceptance steps |
|---|---|---|
| Login/logout | Pending | Select both named sessions from SDDM; verify service lifetime. |
| Shortcuts | Pending | Terminal, launcher, settings, overview, close, screenshot and logout. |
| Workspaces | Pending | Focus/move windows, dynamic IDs, saved named-workspace behavior. |
| Launcher | Pending | Search, app launch, Escape, settings routes; no stale Hyprland IPC. |
| Dashboard | Pending | Open/close, tabs, gauges and focus; preserve appearance. |
| Notifications | Pending | Delivery, timeout, history, focus and placement. |
| Detached panels | Pending | Detach, move, resize, return; floating focus and placement. |
| Manual lock/unlock | Pending | Standard session-lock protocol, password authentication, all outputs. |
| Idle | Pending, automatic locking gated | Test manual unlock first, then idle and suspend hooks. |
| Night light | Pending | Protocol-based tool behind existing owner; on/off, ownership and logout reset. |
| Screenshots | Pending | Area, output and clipboard; confirm available capture protocol. |
| Multi-monitor | Pending | Mixed scale, hotplug, focus, placement and lock coverage. |
| X11 apps | Pending | xwayland-satellite app launch, focus, input and exit. |

## Open issues and sources

- Installed shell supervision and session observation still assume Hyprland; keep
  phase-1 niri isolated until those startup paths are adapted.
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
