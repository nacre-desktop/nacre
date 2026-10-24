# N0064 — Remove IP-based weather location

- Status: Implemented; pending lead review
- Date: 2026-10-24
- Scope: owner decision 2026-10-24: Nacre never locates the user by IP address.

## Behaviour

Weather uses the configured city (`weatherLocation`) only. N0057's `ipinfo.io`
request and its `weatherIpLookup` consent are deleted from the native worker
(`crates/nacre-helper/src/weather.rs`), the Python fallback
(`nacre/shell-tools/weather.py`), the desktop defaults, the daemon settings
fingerprint, the parity adapter and the Bead settings strings
(`bead.settings.weather-ip-*`). No QML control existed for it.

With no city (empty or blank) the worker returns before taking the cache lock and
makes zero network requests; the topic reports the existing stale/error state
("Weather unavailable. Set a city or try again."). The weather photo (N0056) is
fed only from the produced location, so it makes no requests either.

## Migration

An existing `desktop.json` that still contains `weatherIpLookup` loads unchanged;
the key is simply never read and is not rewritten or validated.

## Tests

- no city (`{}`, blank, legacy key true): fake fetch panics on any call, no lock file;
- city set, with and without the legacy key: normal fetch, no `ipinfo` call;
- daemon integration: store holds `"weatherIpLookup":true`, loopback proxy sees only the configured-city flow;
- parity adapter consent tests removed with the key.

N0057's consent text is superseded by this spec. No temporary duplicates remain.
