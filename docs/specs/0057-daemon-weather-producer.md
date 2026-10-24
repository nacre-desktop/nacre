# N0057 — Daemon weather producer

- Status: In progress (implementation and remote desktop CI)
- Date: 2026-10-24
- Scope: produce `dashboard.weather` before the Bead switch; reuse the native worker.

## Existing QML and standalone behavior

`NacreWeather.qml` invokes `nice -n 10 nacre-helper weather` at startup, on city
changes, manually, and every 1,800 seconds. Concurrent requests queue a follow-up;
results for a superseded city are ignored. It loads `weather.json` immediately,
marks readings older than 1,800 seconds stale, and retains conditions on failure.
No QML files change in this unit.

`weather.rs` resolves `desktop.json`'s trimmed `weatherLocation`, reuses cached
coordinates for the same city, or calls Open-Meteo geocoding. Open-Meteo forecast
normalization retains WMO descriptions/icons, current conditions, up to 24 hours
at/after the current timestamp, ten days, timezone/local time and optional US AQI.
Temperatures remain Celsius, wind km/h and pressure hPa. `weatherFahrenheit` is a
view preference; no converted temperatures are written to the cache or topic.
Each request has an 8-second global deadline, a 512,000-byte response cap (the
existing implementation's decimal limit), HTTPS endpoints and HTTPS-only redirects.
AQI failure leaves AQI null; other failures return old conditions and their old
`checked` timestamp with `stale:true` and the existing error text. Without a cache,
the result only carries the stale/error indication. Successful results atomically
replace `$XDG_CACHE_HOME/nacre/weather.json` (fallback `~/.cache/nacre/weather.json`),
with a 0700 directory and 0600 file; linked cache/temporary files are refused.

Consent discrepancy found during implementation: neither the previous native
worker nor its QML caller actually gated the `ipinfo.io` lookup. The N0019 stated
policy is now enforced in the shared worker: IP lookup requires explicit boolean
`weatherIpLookup:true` **and** an empty city. The new desktop preference defaults
to false; missing/invalid consent fails closed. A configured city always bypasses
IP lookup. It can be set through the existing `settings.desktop` set operation;
adding a Settings UI control is outside this unit. This intentional security
correction also applies to the standalone native command. The retained Python
reference is untouched and is not the shipped CLI implementation.

## Producer and contracts

The daemon initially publishes an unavailable state, then reads the existing cache
on an offload thread and displays it while fetching. A 30-minute timer, manual
`dashboard.weather.refresh` call and changes to city/units/consent schedule the same
shared worker. Missing/error settings prevent requests. Changes during a fetch
queue another run and prevent the obsolete result from being published. Refresh
returns immediately (`changed:false`); `refreshing` reports progress. Safe mode
neither starts the producer nor advertises its method. A process-wide lane rejects
overlapping daemon calls with `busy`; a private nonblocking `weather.lock` also
serializes standalone/daemon fetches and atomic cache writes. A busy shared lane
returns the retained stale reading. Calls with neither a city nor IP consent
create no cache directory or coordination file, preserving the frozen standalone
fixture.

The reserved `WeatherState` schema is preserved: `health` (available/stale/error),
`checked`, `provider`, `current`; additive `refreshing` defaults false. `current`
keeps location, timezone, local_time, WMO, description, temperature, feels_like,
high/low, humidity, wind/direction/pressure, AQI, is_day, and ordered entity maps
for hours/days. Optional latitude/longitude are additive and range-validated.
The adapter only renames fields and arrays; normalization remains in weather.rs.
Generated schema/Luau artifacts come from `cargo run -j2 --locked -p nacre-protocol
--example export`. The wire fixture covers coordinates and refresh progress.
Bead's existing Today consumer reads availability, stale status and current
location/temperature/description without requiring a consumer update.

N0056 now takes its location directly from the producer's in-process hub snapshot,
including cached readings, rather than opening `weather.json` every second.
Its existing photo consent, session pause and subscription lease rules remain.
All blocking cache/network work uses `process::offload`; only its short-lived
worker thread is lowered to nice 10. No network work is awaited by READY; each
request is bounded, and no weather work uses Tokio's readiness blocking pool.

Temporary duplicate: QML's scheduling and the standalone native command remain
for the daily driver. Delete 7 days after the daemon replacement is trusted and
the owner retires the QML shell. The retained Python reference follows the same
expiry; no second daemon weather implementation is introduced.

## Verification

Tests use loopback HTTP fakes only: successful JSON, timeout, oversize, insecure
redirect, invalid JSON, consent off, configured city, coordinate reuse, optional
AQI, cache/stale retention, topic conversion, refresh admission and settings changes.
Frozen helper references are unchanged. The existing additive-default adapter
compares missing `weatherIpLookup` as false on both sides, rejects non-boolean
values, and keeps explicit true observable through a negative control.
N0053's full-daemon guard adds a silent local HTTP CONNECT proxy, requires the
weather request actually to start, and still requires READY within two seconds.
Full desktop CI evidence is recorded below after execution; no local builds/tests,
live session changes, deployment or publication are authorized by this unit.
