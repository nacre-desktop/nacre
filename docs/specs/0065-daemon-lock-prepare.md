# N0065 — Daemon lock-prepare behaviour

Status: **Implemented; pending lead review and CI; unpublished**. Origin: Bead B0084
(`0084-trial-switch-routes.md`, Asks 2). Part of the N0039 `behaviours` bundle.

## Problem

hyprlock (external) shows content prepared by `nacre-helper lock-prepare`. Only QML's
`LockWidgets.qml` fed it. Under Bead the lock stays secure but its colours, wallpaper, media, weather
and notices stay as last prepared.

## What QML did (exact)

- `LockWidgets.snapshot()` builds one JSON object: `colors` (active palette), `wallpaper` (poster),
  `system {cpu,memory,storage,temperature}` (fractions 0-1), `greetingHour`, `preferences`
  (`lockMedia|lockWeather|lockNotifications|lockNotificationContents`), `weather`
  (location, icon, detail, range, description, temperature, stale, error; all already localized and
  unit-formatted), `media` (title, artist, album, art URL, playing, canToggle) or null,
  `notifications` (newest first, last 64 retained; `summary`/`body` are `""` unless
  `lockNotificationContents`), `count`, `battery`.
- Trigger: any change of that JSON string. 100 ms settle timer, one writer process at a time,
  re-run when dirty on exit. Also once at startup. No periodic timer and no "on lock" trigger.
- It runs `nacre-helper lock-prepare` and writes the JSON to stdin. That calls `lock_prepare::prepare`
  which writes, under `$XDG_CACHE_HOME/nacre/lock-ready/` (dir 0700, files 0600, atomic temp+rename):
  `weather|media|play-icon|status.txt`, `notifications-safe.txt` (contents forced off),
  `notifications-private.txt` (contents on), `art-path.txt`, `initial-art.png`, `snapshot.json`, plus
  the rendered lock panels (`lock_dashboard::publish_with_context`) and pruning to 12 `lock-art-*.png`.
- Privacy: with `lockNotificationContents` false the presentation holds empty summary/body, so no
  notice text reaches any file. (`notifications-private.txt` is rendered from the same empty text.)

## Daemon behaviour (`daemon/lock_content.rs`, service `lock-content`)

Supervised with the other three behaviours, only under `--behaviours=daemon`, never in safe mode.
It calls `lock_prepare::prepare` in-process (on a blocking thread) with the same inputs:

| QML field | Daemon source |
| --- | --- |
| colors / wallpaper | `wallpaper.active` `current.palette.colours` / `current.path` (cached still poster) |
| media | `dashboard.player` active entity; `art` is `file://` + `art_path` |
| weather | `dashboard.weather`; text via the same i18n ids (`ui.weather.detail.feels-like-value0`, `weather.high-low`, `weather.cached-temperature`, `weather.condition.*`); `weatherFahrenheit` honoured; WMO code -> the QML icon names |
| notifications / count | `notifications` `history` plus non-transient `active`, newest first, 64 max; `history_count` |
| preferences | `settings.store.desktop`; a missing key uses the registry default (contents default **off**) |
| system gauges | `dashboard.system` snapshot if populated (cpu, memory, disk `/`), else null; temperature null |

- Triggers: a change of the five topics above (`wallpaper.active`, `dashboard.player`,
  `dashboard.weather`, `notifications`, `settings.store.desktop`). **750 ms** quiet period (restarts on
  each change; QML used 100 ms but its input also moved every second with CPU gauges). One run at a time;
  changes during a run schedule one more. A 30-minute tick refreshes the clock-derived greeting and the
  gauges. A run whose presentation and hour are identical to the last written one writes nothing.
- A failed run keeps the old files, logs a warning, and retries on the next change or tick.
- N0053: nothing runs on the readiness path; the task starts after the other behaviours, waits the
  quiet period, and works on a blocking thread. `tests/daemon.rs` proves READY < 2 s with the behaviour
  enabled and the buses absent.
- Stand-down: `LockWidgets.qml` skips its writer when `NacreDaemon.behavioursHeld` (same signal as
  rotation, power profile and battery alerts: the `behaviours` entry of the `daemon` topic). The QML
  `lockWidgets state` IPC is unchanged. No double writes.

## Known differences / limits

- Gauges: `dashboard.system` only has data while a UI subscribes to it, otherwise the lock card shows
  "—" for them; temperature is always null (QML used the same for no sensor). A lock screen with a
  fresh CPU reading is not a goal; gauges refresh at most with the 30-minute tick.
- Battery text comes from sysfs inside the helper, as before.
- The daemon palette is the active wallpaper pair, which QML's `NacrePresentation.active` also follows.
- QML's `snowing` icon quirk is kept (`weather_snowy` renders as the default cloud).

## Temporary duplicates

The QML writer in `LockWidgets.qml` (guarded) and the `lock-prepare` CLI stay as the rollback path:
delete the QML writer 7 days after QML is retired (Group F QML deletion pass).

## Tests

`daemon/lock_content/tests.rs` (fixture fonts, temp HOME, fake topics): presentation shape from topics;
stale/Fahrenheit/empty inputs; privacy off writes no bodies anywhere under the cache (on does, as control);
files and 0600 mode; identical inputs skip; debounce (a burst renders only the final state); and
`tests/daemon.rs::lock_content_behaviour_keeps_daemon_ready_within_two_seconds`.
