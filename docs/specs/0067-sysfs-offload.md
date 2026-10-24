# N0067 - ACPI-backed sysfs reads never run on the daemon's async thread

Status: **Implemented; pending lead review** (Claude worker, branch `task/n0067-sysfs-offload-20261024`). Urgent fix for the
owner's laptop. Product change: Rust about +120/-35, tests +45.

## Incident (2026-10-24 17:20-17:30, laptop)

On the ASUS laptop `/sys/class/leds/asus::kbd_backlight/max_brightness` took about 160 ms per read (the asus-wmi ACPI firmware
became slow); other LEDs and `intel_backlight` took about 5 ms. A sample of the daemon showed its main thread in
`max_brightness_show` -> `acpi_os_wait_semaphore` for 15+ s. `sd_notify` READY missed the 15 s `TimeoutStartSec`, followed by a
crash loop and safe mode. With a longer timeout it reached READY, but `nacrectl status` timed out continuously.

## Root cause

N0053 moved external *commands* off the main thread but sysfs attributes were treated as cheap file reads. Three kinds of caller
still read them synchronously on the single-threaded runtime:

1. **Method tables.** `Hub::methods()` (hello, status, every capability change) calls `bar_services::available_methods` and
   `control_center::available_methods`; both ran `light_devices::discover`, i.e. `max_brightness` + `brightness` of the first
   backlight and of every `*kbd_backlight` LED. `control_center::dispatch` did the same through `backend_present` for each
   request. That is the periodic poll behind the 5 s `status` timeouts, and during startup the sum of many such calls.
2. **OSD caps-lock poll.** `osd::session` read `/sys/class/leds/*::capslock/brightness` (plus the `read_dir` discovery) inline,
   at start and every 250 ms.
3. **GPU sensors.** `dashboard_metrics` sampled DRM/hwmon files with `spawn_blocking` on tokio's two-thread pool (the pool
   settings and compositor reads depend on).

## Fix

* `light_devices`: `max_brightness` is read once per device and cached (it never changes); `write()` still re-reads it
  uncached for its identity check, off-thread. `keyboard_cached(root)` returns the last scan's answer (`false` until one has
  finished) without touching sysfs; `note_keyboard`/`refresh_keyboard` fill it. `sys_root()` returns `/sys`, or
  `NACRE_SYS_ROOT` (tests point the daemon at a fake tree; the variable is read only by the daemon's own env constructors).
* `control_center`: `scan_lights` does the backlight/keyboard/DDC discovery on its own offload thread, **one scan in flight**
  (an atomic flag cleared by the thread itself when it finishes), bounded by 2 s (8 s when DDC outputs are included). While a
  scan is stuck the next ones return `None` immediately and `collect` keeps the prior values: that is the back-off. A finished
  scan records keyboard presence. `initial` and the `session` loop call `scan_lights` first, then `collect` (already offloaded)
  with its result. `keyboard_present` (method table and `dispatch`) reads the cache.
* `bar_services::keyboard_light_present` reads the same cache. Production path: the `controls` producer's `initial` runs at
  startup, so the methods appear as soon as the first scan succeeds (not before).
* `osd`: caps-lock discovery + read run through `process::offload` as a single in-flight job per session (`start_caps`,
  `caps_done` select arm); a tick while a read is outstanding is skipped.
* `dashboard_metrics`: GPU sensor sampling uses `process::offload` instead of `spawn_blocking`.
* Already off-thread and unchanged: the brightness / keyboard-light **writes** (`bar_services`, `control_center` dispatch) hold
  the shared brightness lane inside an offload thread, so a hung write only makes later writes answer `busy`; `capabilities`
  (`laptop_battery`) runs inside `probe` via `offload`.

Not changed (listed, not hidden): `control_center::rfkill_present` still lists `/sys/class/rfkill` on the async thread when
building method tables (driver-memory attributes, not ACPI); `diagnostics`, `lock_info` and `software_*` read sysfs only in
helper subprocesses or CLI commands. Event-driven change detection (`brightness_hw_changed` poll/inotify) was not added: the only
remaining poll is the 250 ms caps-lock read, now off-thread and single-flight. Follow-up if wanted: replace it by inotify.

## Tests

* `daemon_ready_within_two_seconds_while_every_external_backend_hangs` (`crates/nacre-helper/tests/daemon.rs`) now also points
  `NACRE_SYS_ROOT` at a tree where `asus::kbd_backlight/{max_brightness,brightness}`, `input3::capslock/brightness` and
  `intel_backlight/{max_brightness,brightness}` are named pipes nobody writes to (every read blocks forever), holds a `controls`
  subscription, and asserts READY < 2 s and five `ctl status` calls each < 200 ms.
* `control_center` tests prime the keyboard cache in `Fixture::install` and use `collect_now` (synchronous light scan).

## Timings (dev-2 Arch container, debug build, all LED/backlight reads blocked forever)

| build | READY | `ctl status` |
| --- | --- | --- |
| before (same test, old code plus only the `NACRE_SYS_ROOT` hook) | < 2 s (assertion passed) | fails: native client timed out after 5.1 s (main thread blocked in the FIFO read) |
| after | 107 ms | 27-32 ms (five calls) |

For the incident's slow-but-finite case (160 ms per read) the old code cost about 160 ms x 2 per `Hub::methods()` call on the main
thread; the new code costs 0 on the main thread (the scan runs once per `controls` tick, off-thread).

## Temporary duplicates

None added.
