# N0052 — Deterministic timing tests

Status: **Implemented; pending lead review** (Claude worker, branch `task/n0052-deflake-20261023`). Tests only; product
code unchanged. Main CI went red intermittently on load-sensitive tests.

## Causes and fixes

| Test | Cause | Fix |
| --- | --- | --- |
| `daemon::behaviours::tests::rotation_follows_interval_failure_retry_lock_and_library` | The fake clock was moved while the rotation was still finishing the previous change. The rotation reads the clock again after the engine `select` returns (`attempted`, retry time), so under load it recorded the *advanced* time and its next due time moved. The test also wrote `media.json`/`last.txt` itself after the change (the loop could have read the plan first), and used `quiet()` (300 ms real time) to decide that the lock line had been consumed. | The fake engine's `select` now leaves `last.txt` and `media.json` behind like the real one, stamped with the fake clock. The fake `sleep` records its target while it is being polled; the test waits (`parked(at)`) until the rotation idles at the expected due time before every clock move, and (`unarmed()`) until the sleep is dropped after the lock line. No real-time sleep decides an outcome; the `eventually` bound (30 s) only ends a hang. |
| `daemon::osd::tests::capslock_and_night_light_report_only_changes_after_the_baseline` (and the other `session` tests) | `baseline()` slept 400 ms and assumed the silent baseline (wpctl x4, LED, night record) was read by then. Under load the test could change the LED before the baseline read it, so no change was seen. Found by analysis; not reproduced on dev-2 (0/80 before, also with one core and 64 busy workers). | `Fixture::running()` waits for proof that the event loop runs: the fake `wpctl` logs each call, the baseline makes four, and a repeated sink event makes the loop call it again, which happens only after the baseline. |
| `daemon::dashboard_player` | Not reproduced (0 failures in 100 runs under CPU load, see the report). Its waits are already condition polls with a 60 s bound; the remaining fixed sleeps are negative checks that cannot fail under load. Left unchanged. | none |

No temporary duplicates are introduced.
