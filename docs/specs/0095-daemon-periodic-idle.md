# N0095: Periodic daemon work while idle

Status: Private profile accepted; full desktop CI passed on dev-1 (Arch container); lead live audit pending.
Date: 2026-10-29.

## Findings and changes

The private daemon's always-started `nacre-audio-step` thread used a 100 ms
`recv_timeout`, producing about 10 voluntary context switches each second while
waiting for audio commands. It now blocks on an event channel. A stop command
and atomic stop flag wake it promptly during shutdown, including when the queue
is full.

With the bar's `audio` and `osd` topics subscribed, the main daemon thread also
ran OSD's cap-lock/night-light monitor every 250 ms. It now runs every 500 ms;
the night-light ownership record is still checked every second. The tick is only a
safety net for caps lock and night light (no kernel event exists for the LED).
Volume and brightness OSDs never wait for it: a key step publishes the OSD in the
same call that applies it (`publish_audio_step`), and external volume changes
(another app, hardware keys) arrive through the `pactl subscribe` stream. Tests:
`key_step_publishes_the_osd_in_the_same_call_without_any_poll` and
`external_volume_changes_do_not_wait_for_the_tick` (tick set to one hour). Weather-photo's
1-second tick was only rechecking consent, session state and weather location,
all of which already produce hub or D-Bus events, so the timer was removed.

`dashboard.updates` previously started a package check at startup and every 30
minutes without a consumer. Automatic checks now start when a client subscribes
to that topic, then repeat every 30 minutes while subscribed. Pacman database
changes and updater completion remain event-driven while unsubscribed; explicit
`dashboard.updates.refresh` still works on demand. The 30-minute check is skipped
when the last successful result is still fresh.

## Periodic-job inventory

Intervals below describe daemon-side wake sources. Operation deadlines and
one-shot debounces are listed only when they can recur during a long-lived
service. A leased job sleeps entirely when its topic has no subscribers.

| Job | Before | After | Scope / trigger |
| --- | --- | --- | --- |
| Fast audio action worker | 100 ms receive timeout | Command or stop event | Always-started worker; no timer while idle |
| OSD cap-lock/night-light monitor | 250 ms; night file each 1 s | 500 ms; night file each 1 s | Runs while `osd`, `audio` or `audio.spectrum` is subscribed |
| Weather-photo state check | 1 s | No timer | While leased; settings, weather, session and lease changes wake it |
| Package update count | Startup plus 30 min, regardless of subscribers | First `dashboard.updates` subscription, then 30 min while subscribed | Pacman database watch and updater completion are event-driven |
| Calendar projection | 60 s | 60 s | Also reacts to settings, explicit refresh and calendar-file changes |
| Weather refresh | 30 min | 30 min | Also reacts to weather settings changes |
| Dashboard system metrics | 500 ms warm-up, then 2 s; GPU every 10 s | Same | Only while `dashboard.system` is subscribed |
| Control-center state | 2 s | 2 s | Only during a subscribed control-center session |
| Daemon watchdog | 10 s heartbeat with the 20 s service watchdog | Same | Only when systemd provides `NOTIFY_SOCKET` and watchdog settings |
| System-power reconnect | 5 s after connection failure | Same | Normal updates use D-Bus signals |
| Managed notification reconnect/save retry | 1 s after bus or persistence failure | Same | Failure recovery only |
| Supervisor retry | 250, 500, 1,000, 2,000, 4,000, 8,000, 16,000, then 30,000 ms | Same | Failed services only; disables after the bounded failure budget |
| Dashboard media/player retry | 5 s after MPRIS failure | Same | Subscriber-leased service; normal updates use D-Bus signals |
| Audio peak/spectrum retry | 2 s after capture failure | Same | Subscriber-leased; peak data is read in 66 ms windows |
| Network scan completion | 250 ms, at most 7 s | Same | Only while an explicit scan request is in progress |
| Focus deadline recheck | At most 30 s while a daemon-owned focus session runs | Same | Inactive with no running focus session |
| Wallpaper rotation | User-configured 5–1,440 min; 60 s retry after a failed change | Same | Only when rotation is enabled and not paused/blocked |
| Battery-alert retry | 10 s, at most three failures | Same | Only after a battery event and failed notification delivery |

Tray, compositor, network, session, battery and audio state producers otherwise
wait on their D-Bus, socket, inotify or process streams. Unchanged topic state is
not republished: the hub drops equal snapshots before advancing the topic
revision.

## Private idle measurement

Both runs used a private HOME and private session/system D-Bus buses in the
remote Arch container. The daemon ran with `--notifications=daemon
--behaviours=daemon` at nice 10. A minimal private protocol client held `audio`
and `osd` subscriptions; the client and sampler also ran at nice 10. No live
session, compositor, HOME or user service was contacted.

The sampler read per-thread CPU ticks and voluntary/nonvoluntary context-switch
counters from `/proc` every 10 seconds for 60 samples. Context switches are the
wakeup proxy here; `cpu_per_mille` is thousandths of one core. Child-process CPU
is excluded, matching the reported daemon-only metric.

| Run | Main daemon thread | Audio worker thread | All daemon threads | Average CPU |
| --- | ---: | ---: | ---: | ---: |
| Before, 600 s | 12.433 switches/s | 10.045 switches/s | 22.478 switches/s (22.0–23.7) | 0.633‰ of a core |
| Candidate, 600 s | 6.488 switches/s | 0.053 switches/s | 6.542 switches/s (6.0–7.6) | 0.367‰ of a core |

The candidate is below both private-profile limits: `<10` context switches/s
and `<5‰` of one core. The event-driven audio worker accounts for the largest
drop; the 500 ms OSD tick accounts for the reduction in main-thread wakeups.

## Validation and remaining gate

Passed: `cargo fmt --check` and the symbolized debug helper build in the Arch
container. The private before/after captures each lasted 600 seconds and used
the same HOME, daemon flags and topic subscriptions.

Not run: the required full `.github/workflows/check.yml` `desktop` job. The
task's 90-minute timebox elapsed before that job could start. No suites or CI
result are claimed here. The required live real-shell audit also remains with
the lead; no live daemon was touched and no push or deployment was made.

No new temporary duplicate implementations were added. N0055's existing
standalone QML update worker and expiry remain unchanged.
