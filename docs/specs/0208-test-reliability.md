# N0208 Test reliability

## N0208w — Wallpaper extras fixture reliability

The wallpaper fixture advertised `daemon.status` even though these tests do not
exercise registry refresh. Nacre's client periodically issues that call while
topics or leases are missing. Under parallel wallpaper tests, the fixture's
client could close the Unix socket before the fake daemon wrote the status
reply; its sender was then dropped, making unrelated `calls.recv()` assertions
fail. The fixture now omits `daemon.status`, so these tests cover wallpaper
calls without triggering the unrelated registry probe.

Revision/thread matrix and final verification results are recorded in the unit
matrix below. The first observed failures begin after the Bead pin change and
vary with test-thread count.

| Revision | Threads | Results |
| --- | ---: | --- |
| `a5db20ca1` | 6 | 3 pass, 0 fail |
| `741f78207` | 6 | 1 pass, 2 fail |
| `382f36de6` | 6 | 0 pass, 3 fail |
| `origin/main` | 6 | 1 pass, 2 fail |
| `origin/main` | 1 | 1 pass, 1 fail (two runs) |
| `origin/main` | 2 | 1 pass, 1 fail (two runs) |

The fake daemon logged `Ret write failed for call daemon.status` on failing
runs. Omitting that unused method from the fixture's welcome registry removes
the incidental refresh call. After the fix, the focused wallpaper filter passed
11/11; `cargo fmt --check` and clippy passed; and the complete nacre-shell
binary passed twice (882 passed, 4 ignored each run). One separate flaky
assertion in `wallpaper_hydrates_only_visible_image_ids_and_suppresses_missing_preview_retries`
also appeared during matrix collection at lines 760 and 791; it was outside the
sender-drop failure and passed in final verification.
