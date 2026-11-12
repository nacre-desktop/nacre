# N0216: Apps survive service restarts

## Decision

Applications launched from daemon or shell services must leave the service cgroup. `process::scope_command` is the sole scope builder; it starts `systemd-run --user --scope --collect`, preserves compositor launch activation, removes service-only environment, and starts the requested argv without a shell. Shell-owned app windows call the same helper implementation. The AI dashboard keeps its private append-only launcher log.

## Spawn audit

| Path | Before | After / disposition |
| --- | --- | --- |
| `brain_launch.rs`, `software_tui.rs`, `software_actions.rs`, `brain_web.rs` | Brain-specific `launch_app` used its own transient scope | Use `process::launch_app`; Brain module no longer owns the generic launcher |
| `ai_dashboard.rs` | Kitty and Brain/settings windows directly spawned in a process group; output logged | Shared scope command; `launcher.log` append redirection and mode 0600 retained |
| `daemon/dashboard_extras/screenshots.rs`, `desktop_extras.rs` | `xdg-open` and `nacre-shell brain` spawned under caller service | Shared detached scope launcher |
| `daemon/tools_launch.rs`, `desktop_actions.rs` | Tool apps and `uwsm app` spawned under daemon | Shared detached scope launcher |
| `launchers.rs` startup apps | Duplicated direct `systemd-run --user --scope` argv builder | Shared detached scope launcher; ordinary terminal service launch stays specialized |
| `software_library.rs` extension action | Duplicated scope argv/environment cleanup | Shared detached scope launcher |
| `nacre-shell/src/launcher_actions.rs`, `welcome.rs`, `app.rs` | User-selected apps and shell app windows were direct children | Hand off to helper `launch-app`, which uses shared scope launcher; shell app log stdout is inherited |
| `daemon/menu_worker.rs` | Catalog app already starts as transient systemd user service in `app.slice`, with sanitized environment | Keep existing user-manager service implementation |
| `classic_state.rs` | `timeout -k 1s 60s spicetify refresh` detached | Keep direct: the utility has a strict 60-second bound and does not own a user window |
| Wallpaper renderer/video preview, screenshot/capture helpers, queries, clipboard/notification helpers | Owned child worker or bounded request process | Keep attached to the service/request lifetime; these are not launched user applications |
| `ai_dashboard.rs` Niri CLI worker and software root-action bridge | Already launched through transient user services with `--wait` | Keep specialized service/interactive semantics |

## Validation — 2026-11-12

Fake-runner tests record scope argv for the shared launcher, AI launcher log, desktop note opener, screenshot viewer and daemon tool launches. On the rebased source in dev-1: helper lib 1,016 passed, 3 ignored; dashboard extras 12 passed; shell suite 875 passed, 5 ignored; all 16 Python software tests passed. `cargo fmt --check`, Ruff format/check, compositor boundary, focused AI test, and touched-crate Clippy passed. Earlier parallel runs surfaced timing-sensitive existing tests; the affected cases passed in isolation and the final serialized run passed. The dev-1 container has no `systemd --user`; coordinator performs the parent-unit survival check after deploy.
