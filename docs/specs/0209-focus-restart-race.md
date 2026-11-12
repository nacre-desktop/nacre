# N0209 — Focus completion across daemon restart

## Symptom

Hosted run 43161851848 failed
`daemon_owned_session_completes_notifies_and_restores_dnd_after_a_restart` with
`WouldBlock` after 28 seconds. Its persisted deadline was only 1.5 seconds after
fixture setup, before daemon startup. Under load, focus can expire during startup
and leave Do Not Disturb on.

## Root cause and evidence

[`Focus::run`](../../crates/nacre-helper/src/daemon/dashboard_extras/focus.rs)
expires an already-past deadline immediately. Previously, `finish` persisted
`done` and cleared `ownsDnd` before reading DND, then treated a read failure as
`false`. That read uses the settings service:
[`desktop_state`](../../crates/nacre-helper/src/daemon/notifications.rs) rejects
settings until its watcher and initial refresh finish and status becomes
`running`. Focus and settings startup run concurrently. The notification server's
readiness is independent of this check.

The original actor passed 43/50 runs beside six CPU workers. All seven failures
reached focus `done`, then timed out at **DND restored** with desktop revision 0
and `dnd: true`. An already-expired original-actor run failed the same wait.
The forced-readiness regression also fails against the original actor: its file
already contains `Done` with ownership cleared while settings is still starting.

## Fix

Restore DND before committing the terminal focus state, propagating failures.
Keep the persisted running session and claim on failure. Disable the expired
timer until a settings/service change, focus-file update, or missed-notice
recovery permits another attempt. The existing transition gate serializes the
restore and completion. No new service, polling job, retry sleep, or schema.

The regression forces expiry with settings unavailable using a paused Tokio
clock, then drives readiness through the real settings writer and private Niri
fixture. The integration test starts with an already-past deadline and requires
an error-free store with DND explicitly false. `until()` bounds reads by its
remaining budget and reports the topic, expectation and last state on failure.
No timeout was increased.

## Checks

Fresh Arch containers, six CPUs/12 GiB, Cargo `-j2`; debug symbols disabled.
Raw logs and harnesses remain in the worktree's `.lead/`, not committed.

- Forced-readiness regression: original fails; corrected actor passes.
- Restart test beside six busy CPU workers: corrected actor **50/50**, repeated
  successfully in the final validation container.
- `cargo fmt --check`: passed.
- `cargo clippy --locked -j2 -p nacre-helper --all-targets -- -D warnings`: passed.
- `timeout 900 nice -n 10 cargo test --locked -j2 -p nacre-helper -- --test-threads=6`:
  passed, including all 12 dashboard-extras tests (final full-run attempt 1).
- An unrelated weather cancellation case flaked once in an earlier parallel run;
  its isolated check and subsequent full runs passed. No other actor was changed.

Focused checks follow the current CI policy; hosted CI remains the deployment
gate for the lead's push. No live changes, publication or temporary product
alternatives. Build-host source export, target and owned container cleaned up.
