# N0053 — Daemon startup never waits on a backend; atomic code-only rollback

Status: **Implemented; pending lead review** (Claude worker, branch `task/n0053-daemon-startup-20261023`). Blocks deploying
Nacre main to the owner's laptop. Product change: Rust +37/-9, installer +46/-9 (Python), tests +120.

## Incident (2026-10-23 22:19, laptop)

`install.py --code-only` from `f3193ec0` restarted `nacre-daemon` (Type=notify, 15 s start timeout). The new daemon logged nothing
for 15 s, never sent READY and systemd failed it. The installer restored the previous helper (ready in 0.2 s) but had already
replaced `source/` and `tools/`, leaving a mixed install.

## Root cause

The daemon runs on a current-thread runtime with `max_blocking_threads(2)`. READY waits for the compositor and for
`settings::Service::load`, which read files through `spawn_blocking`. Since N0043-N0051 the startup services also run external
probes through the same two-thread pool: `capabilities` (`powerprofilesctl get`, `list`), `controls` (`systemctl`, `rfkill`,
DDC, `pactl`...), `system.status`, the pill controls. Each probe has a 5 s command timeout, but when a backend hangs (a real
`powerprofilesctl` / `systemctl --user` waiting on a busy bus) the two threads sit in those calls, the pool is saturated and the
settings load queues behind them for several 5 s timeouts in a row, past the 15 s unit timeout. CI fakes answer at once, so it
never showed there.

Reproduced on the desktop (Arch, real labwc + nested niri, private HOME/XDG_RUNTIME_DIR/session bus, `NOTIFY_SOCKET` listener,
nothing paired or set). With real services both helpers are ready in ~0.02 s. With `powerprofilesctl` AND `systemctl` replaced
by scripts that sleep forever (any one alone is fine; the pool needs both threads held) the `f3193ec0` helper is READY only
after 5.02 s; with every probed command hung it grows by 5 s per queued probe. The `82d7a142` helper (before the probes) stays
at 0.01 s.

## Fix

* `process::offload(work)` runs blocking work that may wait on an external backend on its own short-lived named thread
  (512 KiB stack, at most 8 at a time) and returns a `Result<T, Offloaded>` that is a drop-in for the `spawn_blocking` join
  result. Every spawn_blocking in the backend-facing daemon modules (`capabilities`, `control_center`, `pill_controls`,
  `system_status`, `bar_services`, `session_policy`, `shell_actions`, `login_theme`, `colors_app`) now uses it. tokio's pool
  stays for settings, file and compositor work, so READY (compositor + settings + `sd_notify`) cannot wait on a backend. The
  services keep the existing pattern: they start `starting`/unavailable and advertise their methods only once healthy.
* Left on the pool on purpose: `widths` (X11 connect, own runtime), notifications/menu/dashboard file work.

## Installer (code-only)

`install.py --code-only` snapshots `tools/` to `tools.previous` before copying (like the existing `source.previous`). If the
restarted daemon is unhealthy, `restart_daemon_checked` restores the previous helper, `source/` and `tools/` together, restarts
the daemon, then restarts `nacre-shell` on the restored tree, and still raises with an explicit message. `--dry-run` states this
plan. Temporary duplicate: `tools.previous` (replaced by each code-only deploy, same lifetime as `source.previous` and
`helper-previous`; delete the three together 7 days after code-only installs are trusted).

## Tests

* `daemon_ready_within_two_seconds_while_every_external_backend_hangs` (`crates/nacre-helper/tests/daemon.rs`, runs in CI):
  thirteen fake commands sleep forever, both D-Bus buses accept and never answer; asserts READY < 2 s, `ctl status` works and
  `capabilities`/`controls` are not `running`.
* `test_code_only_unhealthy_daemon_restores_previous_helper` now also asserts source/ and tools/ roll back, new-only files are
  gone, `*.previous` are consumed and nacre-shell restarts once, after the last daemon restart (fake systemctl and daemon).

## Timings (desktop nested, hung powerprofilesctl + systemctl)

| helper | READY |
| --- | --- |
| `82d7a142` (before) | 0.012 s |
| `f3193ec0` | 5.02 s (real services: 0.026 s) |
| N0053 fix | 0.02 s (real services: 0.03 s) |
