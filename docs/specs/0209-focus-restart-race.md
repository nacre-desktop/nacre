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

## Follow-up: notification subscription EOF

Main at `325ee2f5` intermittently failed `managed_notifications_private_bus`:
its watcher reached EOF before a snapshot. This is a **separate ordering bug**
from the focus/settings race above and is fixed in a separate follow-up commit.
After restart the fixture waits for the Freedesktop bus name, but
`notifications::run` previously acquired that name before publishing its first
Hub topic. A subscription in that interval receives nonretryable `UnknownTopic`;
`ctl watch` exits, and the fixture's reader reports `Disconnected`. Unrelated
system-bus activation warnings do not explain this path.

Publish the existing unavailable baseline, including restored history, before
D-Bus connection/name acquisition. The existing ready publication then updates
that same subscription. The observer consumes snapshots and patches with
`Mirror` until health is available before using the notification API again.
Capture observer stderr, join both reader threads, and include observer and
private-bus child exit status in failures. No timeouts were increased, sleeps
added, or new product background work introduced.

Follow-up evidence (fresh Arch containers, same limits and focused policy):

- Unmodified producer: notification test **50/50** beside six CPU workers; the
  narrow race did not occur naturally in this run.
- Temporary scheduler yields widened only the post-acquisition/pre-publication
  interval, beside the same six workers. Old producer failed with observer
  **exit status 1, `Unknown topic`**, then channel EOF; corrected producer passed
  the identical experiment. Instrumentation exists only in `.lead/` evidence.
- Uninstrumented candidate: notification restart **50/50** and focus restart
  **50/50**, alternating beside six CPU workers.
- Forced focus/settings readiness test, `cargo fmt --check`, and
  `cargo clippy --locked -j2 -p nacre-helper --all-targets -- -D warnings`: passed.
- `timeout 900 nice -n 10 cargo test --locked -j2 -p nacre-helper -- --test-threads=6`:
  passed on the first attempt (library: 1005 passed, 3 ignored; all integration
  binaries passed). Per-unit target peaked below 7 GB and was deleted with the
  source export inside the validation container.
