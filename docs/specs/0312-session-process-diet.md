# N0312 Session process diet

Review E (2026-11-13) mapped every process a Nacre niri session starts and
ranked the ones with no job (recommendations 2, 6, 8, 9). This unit removes
them, toward the owner's goal of matching Noctalia's resource use. Every claim
was verified before the change; `niri.widths` stays untouched pending an owner
decision.

## Verified findings

| Item | Finding | Action |
| --- | --- | --- |
| Rec 9, notification activation | `busctl --user status org.freedesktop.Notifications` on the live laptop: `Comm=nacre-daemon`. The daemon runs with `--notifications=daemon` (ui_shell_switch drop-in) and owns the name; the activation file named `nacre-shell.service`, which only worked because the shell `Wants=` the daemon. | `install.py` and `packaging/arch/stage.py` write `SystemdService=nacre-daemon.service`. |
| Rec 8a, `nacre-sidebar-ai.service` | Runs `sidebar-chat server` (Unix socket `nacre-sidebar-chat.sock`). No consumer in `shell/`, `crates/nacre-shell`, the Bead repo, `config/` or the bindings; the Bead chat view uses the daemon `ai.chat` topic, whose workers are `sidebar-chat worker` children of the daemon. The unit has no `[Install]` section, the installer never enabled it, and it is inactive live; the only starter is `sidebar-chat client` (`start_server` in sidebar_chat.rs), which nothing invokes. | Unit and all install/stage/package/release wiring removed. |
| Rec 8b / rec 2, `nacre-session-watch.service` | `session-watch watch` asks the daemon for `session.watch`, which answers immediately, so the process exits at login (inactive live). The daemon's `session_policy` applies the same effects (check, restore monitors, wait), writes the same `state/session/latest.json` and `events.jsonl`, and listens to logind `PrepareForSleep` and compositor output events. The standalone observer only ran without a daemon, which the Bead shell requires anyway. | Unit and wiring removed; the `session-watch check/ready` commands stay. |
| Rec 6, key binding hops | `config/niri/bindings.kdl` (generated from `compositor-contract.json`), `config/hypr/conf/keybinding.lua`, the contract itself and the live generated niri config contain no `nacre-shell <verb>` spawn; every binding already runs `nacrectl ui …` or `nacrectl key-step …`. | Already done on main; no change, goldens untouched. |
| Rec 8c, `nacre-power-key.service` | `systemd-inhibit --what=handle-power-key … sleep infinity`: two live processes whose only job is holding a logind inhibitor fd. | `daemon/power_key.rs` takes the same `handle-power-key` block inhibitor over zbus at daemon start (3 s timeout, warning only on failure, held in safe mode too) and drops it at exit. Unit and wiring removed. |

## Upgrade path

`install.py` `retire_units()` runs on every deploy before `daemon-reload`: for
each retired unit it runs `systemctl --user stop` and then `disable` (separate
calls: `disable --now` skips the stop once a package upgrade has removed the
unit file), then removes the HOME unit copy and the
`graphical-session.target.wants/` link, so a link left behind by a package
upgrade that already removed the vendor unit cannot dangle. The Hyprland and
niri target `Wants=` lines, the vendor unit list, the configure manifest
(`files()`, `LEGACY_SYNC`), release snapshots and rollback stop/start lists no
longer name the units. Package installs keep the existing `package_runtime`
migration, which removes the recorded `graphical-session.target.wants/` links.
`nacre-niri-widths.service` is unchanged.

Known gap, Hyprland only: on niri, `nacre-niri` holds its own
`handle-power-key` block inhibitor (`systemd-inhibit --list` on the laptop:
`niri … handle-power-key Power key handling block`), so nothing changes there.
On Hyprland no inhibitor is held between the stop of `nacre-power-key.service`
during an upgrade and the new daemon's start, during a daemon restart, and
after a rollback to a pre-N0312 helper (installer restore after an unhealthy
restart, or a release restore); a power press then reaches logind's default
`HandlePowerKey=poweroff`.

## Process count (niri session, laptop)

Before (read-only `ps` and `/proc/<pid>/cgroup` on the live laptop niri session,
2026-11-13 19:40): nacre-niri, wl-paste, nacre-daemon, systemd-inhibit, sleep,
hypridle, nacre-shell, nacre-helper wallpaper-media watcher,
bead-wallpaper-video = 9 Nacre-owned processes. `systemd-inhibit` and its
`sleep infinity` child are the whole `app.slice/nacre-power-key.service`
cgroup; `nacre-session-watch.service` is inactive after its login-time run and
`nacre-sidebar-ai.service` never started.

After: 7. The inhibitor pair is gone and the daemon holds the inhibitor as a
file descriptor, not a process; the login-time `session-watch` process and its
daemon connection are gone too. This is derived from the cgroup membership
above; the live count after deployment is the deploy coordinator's check
(`systemd-inhibit --list` then shows `Nacre … nacre-daemon handle-power-key`).

## Left for later

`nacre-helper sidebar-chat server|client` (its client now fails to auto-start the removed unit), the standalone `session-watch watch`
observer, `niri_acceptance_monitor.rs`, `maintenance.py` and the diagnostics
unit list (frozen N0156 reference outputs) still name the retired units; they
are reporting/legacy code, not install wiring, and belong to N0310/N0313.

## Checks

Focused checks on dev-1 in `nacre-ci/arch-nacre:current` with the read-only
cargo cache, on origin/main 67166efa4 plus this change (runs d3, d4):

- `cargo fmt --check`; `cargo clippy -p nacre-helper --all-targets --locked
  -- -D warnings`: passed.
- `cargo test -p nacre-helper --locked --no-fail-fast`: lib 1074 passed and
  every integration binary passed, except three timing-bound `tests/daemon.rs`
  cases under host load (450 ms presentation bound, inotify quiet window,
  settings-writer concurrency); 3/3 isolated reruns passed. The two
  `lock_dashboard` frozen-font tests download a font and fail only under
  `--network none` (DNS error, run d1); with the network they pass.
- ruff 0.16.10 `format --check` and `check` on the `tools/check.py` file set
  (258 files), `compositor_boundary.py`, `generate_compositor_config.py
  --check`, `generate_usage_keybindings.py --check`: passed.
- Python suites: shell-tools `test_installer_dropin_guard` (17, including the
  upgrade test below), `test_native_helper_install`,
  `test_compositor_helper_migration`, `test_session_workflows`,
  `test_compositor_backend`, `test_desktop_settings`, `test_niri_daily_driver`,
  `test_startup_apps`; tools `test_setup` (48), `test_releases` (26),
  `test_profiles`, `test_configure`, `test_install_modules`,
  `test_niri_session_deploy`, `test_nacre_niri_session`, `test_f4_switch`,
  `test_ui_shell_switch`, `test_compositor_*`, `test_bindings`,
  `test_packaging_bundle`, `test_release_config_map`,
  `test_notifications_contract`; ai `test_installer_arguments`: passed.

Installer proof (`test_upgrade_retires_session_units_and_activates_notifications_via_daemon`,
disposable HOME in the container, recording `systemctl`): the HOME starts with
all three old units as HOME copies, each enabled through
`graphical-session.target.wants/`, and the power-key link dangling into a
removed vendor path. After `deploy()`: `systemctl --user stop` and `disable`
ran for each unit before `daemon-reload`, none was enabled again, the unit
files and wants links are gone, no symlink under `~/.config/systemd/user`
dangles, `nacre-niri-widths.service` stays, and the Notifications activation
file names `nacre-daemon.service`. `test_setup` covers the niri/Hyprland target
`Wants=` lines, the configure manifest and the Arch package layout without the
units.
