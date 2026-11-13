# N0322 — Harden flaky daemon timing tests

Status: implemented. One product line changed (a lost wakeup in the width observer's
shutdown); the rest is test code. Follows the N0173/N0303 rule: default tests check
behaviour, and a latency bound stays only where a regression would always break it.

## Changes

| Test | Old | New | What it still proves |
| --- | --- | --- | --- |
| `daemon::widths::tests::private_x11_hint_restore_and_one_shot` (shutdown) | `Shutdown::finish` used `notify_waiters`; a stop sent while the observer was inside an X11 round trip had no registered waiter, was lost, and the 4 s bound expired ("Width observer shutdown timed out", hosted run 43480933999) | `notify_one`: the permit waits for the observer's next `select!`. The 4 s production bound is unchanged | Shutdown restores every relaxed X11 hint before the daemon exits; the bound now only covers the X11 round trips |
| same test, native-mode rejection | `elapsed() < 100 ms` | `now_or_never()`: the reply must be ready on the first poll | Native mode rejects legacy configuration without waiting on the observer (stronger than the wall-clock bound) |
| same test, four readiness/action waits | 3 s | 15 s | Hang guards only; the 140 ms one-shot lower bound and the 200 ms no-repeat window are unchanged |
| `wallpaper_active_publishes_atomic_pair_and_retains_stale_revision` | one sample `< 450 ms` | best of up to three samples (posters b, a, b) `< 450 ms` | Presentation changes skip the 500 ms legacy debounce: a debounced presentation misses the bound on every sample |
| `wallpaper_watch_shares_inotify_preserves_frozen_lines_and_never_replays_after_disconnect` | `wallpaper-picker.json` created during the run (IN_CREATE + IN_CLOSE_WRITE, both dirty the debounce); the nested burst assumed its three writes finish inside the debounce | the picker file exists before the daemon starts, so the step is one event; a burst whose writes take ≥ 250 ms may yield extra `nested-burst` lines, which are drained before the quiet check; `quiet` reports its caller's line | Every frozen line and every quiet step, and burst coalescing whenever the writes were not stalled by the disk |
| `settings_writer_native_concurrency_hand_edits_and_killed_publication_keep_valid_state` | 5 s call deadline, socket read timeout and four publication/hand-edit waits | 30 s; a failed read names the reply | Same concurrency, stale-revision, hand-edit, repair and kill-during-publication outcomes; latency was never the property. The N0312 failure was a `settings.get` that outlived its 5 s deadline behind a settings flush on a stalled disk |

Inspected and left unchanged: lower bounds (`>= 450 ms` engine debounce, `>= 500 ms`
resume, `>= 300 ms` hotplug, `>= 29 s` idle worker, `>= 2 s` brain lock), the
NACRE_TEST_PERF-gated READY/status gates, and the 10 s `eventually` hang guards in the
Bluetooth and time-settings unit tests.

## Validation

dev-2, `nacre-ci/arch-nacre:current` (rustfmt 1.10.0), read-only cargo cache, one 6-CPU
container. Candidate: `cargo fmt --check` and `cargo clippy -p nacre-helper --all-targets
-- -D warnings` passed; every nacre-helper test target (dev-1, network on, RUST_TEST_THREADS=2)
1319 passed, 1 failed: `desktop_settings` needs the `desktop-settings-test` example, which
CI builds first (unrelated to this change).

Stress, base (origin/main `e1022abd2` plus only the `quiet` caller diagnostic) and candidate
alternating in the same container: 20 runs of the whole `tests/daemon.rs` binary with
`--test-threads=8`, each beside three runs of the widths X11 test, while a release build of
nacre-helper (`-j4`) looped in the same container.

| Tree | daemon.rs binary | widths X11 test |
| --- | --- | --- |
| base | 19/20 (one EINTR in `actual_daemon_ready_native_client_restart_safe_mode_and_cleanup`, see below) | 60/60 |
| candidate | 20/20 (23 tests each, 38.7–40.4 s) | 60/60 |

This load did not reproduce the original failures on the base, which came from hosted CI
and from a disk-stalled dev-1; the causes above are from reading the code. A 20 ms sleep
added to the observer's drain round trip (never committed) did not reproduce the lost
wakeup either (base 10/10). The race also needs the test's 5 ms poll to land inside that
round trip, which a CPU-starved runner causes and the sleep does not.

Open: wait-timeout installs a process-wide SIGCHLD handler (`SA_RESTART`), but
`recv` on sockets with `SO_RCVTIMEO` and `poll` are never restarted, so a child exit in
one test thread can fail another thread's `notify.recv` or `next_line` poll with EINTR.
`poll_until_ready` already retries EINTR. It showed once in 20 base runs at 8 threads (CI
uses 2), so it is left for a separate unit.

Lines: product +3/−1 (widths shutdown), tests +59/−45.
