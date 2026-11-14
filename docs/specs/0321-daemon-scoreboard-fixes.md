# N0321 — daemon fixes from the N0308 scoreboard

**Status:** Implemented (sections 1 and 3); section 2 is a finding handed to Shell/Bead

Daemon side of the [N0308 scoreboard](../research/noctalia-scoreboard.md)
losses, measured at 26fd8a62e: lock-dashboard re-renders during interaction,
CPU kept after first use, and idle wakeups.

## 1. Lock dashboard renders only when it is seen

**Cause.** `lock_content` prepared the Hyprlock files (text labels, snapshot
and the per-output dashboard PNG) 750 ms after every change of
`notifications`, `dashboard.player`, `dashboard.weather`, `wallpaper.active`
or `settings.store.desktop`. Each notice burst and each expiry re-rendered the
dashboard (`image_paint::composite` 15.9 %, Orient resample 8.8 % of shell +
daemon interaction samples).

**Fix.** Wallpaper and settings changes still prepare after the quiet period.
Notices, media and weather only mark the content stale, and it is prepared at
once when it is about to be seen:

- Hyprlock opens `dashboard-<key>-initial.png` at start and `cat`s the
  `dashboard-<key>.txt` pointer every 2 s (`reload_cmd`); an inotify `IN_OPEN`
  watch on `lock-ready/` sees both. The daemon never opens those names itself.
  While a read is under 5 s old (Hyprlock is running), changes prepare after
  the quiet period as before.
- `session.state` (N0317) `locked` or `preparing_sleep` prepares stale content
  at once (suspend, or a future locker that sets `LockedHint`; hypridle and
  Hyprlock 0.9 do not set it, and the niri `loginctl` wrapper starts Hyprlock
  directly, so the read is the signal that covers every lock path).
- Without the watch (inotify unavailable, or `lock-ready/` missing) every
  change prepares at once, as before.

No fallback timer: the first Hyprlock read already bounds staleness. The
30-minute tick is unchanged. The path goes away with the Clasp switch.

**Freshness.** Hyprlock shows the last prepared image on its first frame and
switches to the fresh one at its next 2 s reload. Before this change, content
that changed in the last ~1 s before locking had the same window.

## 2. CPU after first use (finding, not changed here)

Rest CPU is unchanged by this unit (daemon 0.32 % base, 0.33 % fix). A 20 s
strace of the rest window shows the dashboard's subscriptions still live after
it closed: `/proc` sampling every 2 s (`dashboard.system`) and
`/sys/class/{rfkill,leds,backlight}` (controls). Bead keeps a closed panel
built for `PANEL_KEEP_ALIVE` = 120 s with its daemon sources, so the fix belongs
in the shell/Bead (drop dashboard leases when the panel hides), not the daemon.

## 3. Idle wakeups

**Cause.** No keepalive in the N0311 offload pool: an idle pool thread blocks
in `recv_timeout(30 s)` and exits. Both idle wakers were the stock-niri Caps
Lock LED readback (`osd`, 500 ms). Each tick woke the main thread for the
timer, handed the sysfs read to an offload thread (`nacre-offload` 2/s) and
woke the main thread again for the result (main thread 4.1/s).

**Fix.** With a known LED the read job itself polls on its offload thread
(`watch_caps`: sleep one tick, read, return only on a change, a vanished LED
or when the OSD session drops its guard). The main-thread tick remains only to
look for a missing LED (2 s) and for unwatchable night-light records. The read
stays off the async thread (N0067), single-flight; a failed job falls back to
the rediscovery tick. Caps Lock latency is unchanged (≤ 500 ms).

## Lock dashboard and the memory test

Wallpaper and settings changes are prompt topics, so the startup pass and
wallpaper changes still write `lock-ready/dashboard.json` after the quiet
period without any reader; only notices, media and weather are lazy. The CI
`memory::` test (`wait_lock` waits for the wallpaper in `dashboard.json`) passes
with the fix (23 s, `NACRE_MEMORY_BINARY` = the fix binary).

## Measurements

dev-1, `tools/bench/noctalia` short profile, software rendering, clean arms
(no perf/strace), image `arch-nacre-session:current`. Base e1022abd2 (2 runs;
a second arm of the first batch failed on a flaky notification call and was
rerun), fix (2 runs). Daemon CPU is percent of one core.

| Metric | Base | Fix |
| --- | ---: | ---: |
| Daemon CPU during interaction | 3.23, 4.18 % | 0.90, 0.89 % |
| Daemon CPU after interaction (rest) | 0.315, 0.332 % | 0.329, 0.336 % |
| Daemon idle CPU | 0.065, 0.071 % | 0.059, 0.059 % |
| Daemon idle context switches | 12.1, 11.8 /s | 6.4, 6.4 /s |
| Idle main thread wakeups | 8.5, 8.1 /s | 2.7, 2.7 /s |
| Idle `nacre-offload` wakeups | 3.6, 3.6 /s | 3.6, 3.7 /s |
| Lock probe: dashboard re-rendered with no reader | yes | no (fresh 0.1-3 s after Hyprlock reads) |

The idle change removes about 5.5 main-thread wakeups per second; the offload
pool is unchanged (its poll now lives there). Rest CPU is not improved (section 2).

## Checks

dev-1 (arch-nacre image): `cargo fmt --check`, `clippy -p nacre-helper
--all-targets -D warnings`, `lock_content` and `osd` tests (21), and the full
`cargo test -p nacre-helper --no-fail-fast`: all pass except
`desktop_settings::direct_user_manager_call...` before pointing
`NACRE_DESKTOP_TEST` at the built example (then passes) and two frozen-font
tests that need network (container ran `--network none`). Raster memory test
passes (above).
