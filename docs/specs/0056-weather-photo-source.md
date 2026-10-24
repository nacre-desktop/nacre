# N0056 — Leased daemon weather photos

Status: implemented and validated; pending lead review. Additive producer for Bead
B0078; no QML or live installation changes.

## Existing QML behavior

`NacreWeatherPhoto.qml` runs only with a visible consumer, `weatherPhotos=true`
(default), and an awake/unlocked session. The trimmed city is limited to 100
characters. City plus latitude/longitude identify the cache (QML hashes this
identity with MD5). Disabling consent means **no photo network requests at all**;
losing activity or changing location stops the current process and discards its
pending result. QML makes one attempt per city per activation, resetting that
attempt on target changes. There is no periodic refresh timer: seven days is the
cache validity interval, checked when a consumer activates.

First request:
`https://en.wikipedia.org/w/api.php?action=query&format=json&formatversion=2&prop=pageimages%7Ccoordinates&coprimary=primary&colimit=1&redirects=1&pilicense=free&piprop=name&titles=<encoded-city>`.
Select the first page only, namespace 0, with a JPEG/PNG pageimage title shorter
than 240 characters and an Earth coordinate within 50 km of the weather reading.
Distance uses the QML equirectangular approximation, 111.2 km/degree, with longitude
wrapped across the dateline. All coordinates must be finite and within bounds.

Then request:
`https://commons.wikimedia.org/w/api.php?action=query&format=json&formatversion=2&prop=imageinfo&iiprop=url%7Csize%7Cmime%7Cextmetadata&iiurlwidth=960&iiextmetadatafilter=Artist%7CAttribution%7CLicenseShortName&titles=<encoded-File:title>`.
Use the first imageinfo: JPEG/PNG MIME, thumburl (otherwise url), strip its query,
thumbnail dimensions (otherwise original), positive dimensions at most 1400 each
and 1,960,000 pixels. The download must be on
`https://upload.wikimedia.org/wikipedia/commons/`, with no decoded traversal,
backslash or controls. Source must be an HTTPS Commons File page without query,
fragment or whitespace. Strip HTML and entities from Attribution (otherwise
Artist), at most 160 characters, nonempty. LicenseShortName is at most 40
characters and must be CC BY/CC BY-SA 1.0, 2.0, 2.5, 3.0 or 4.0, CC0 (optionally
1.0), or Public domain, case-insensitive.

Every curl uses `-q --proto =https --max-redirs 0 --fail`, connect timeout 3 s,
total timeout 10 s, user agent `Nacre/1.0 (weather photos)`, maximum JSON 262144
bytes and maximum image 2097152 bytes. QML checks JPEG/PNG magic with `od`, then
atomically replaces `NacrePaths.cache/weather-photo/city.image`. Metadata is
`NacrePaths.cache/weather-photo.json`, at most 12000 characters on read. Cached
identity must match; checked may be at most 60 s in the future and less than
604800 s old. QML versions its file URL with checked. Failure drops pending work
and displays “City photo unavailable; using local imagery.”

## Daemon ownership and consumer contract

Subscribe to v1 `dashboard.weather_photo` to lease the source; unsubscribe or
disconnect releases it. Multiple subscribers share one source and one request
sequence. No manual refresh method is needed. Safe mode publishes disabled.
The state is generated from `nacre_protocol::weather_photo::State`:

- `enabled`: effective photo consent; `ready`: an asset is available.
- `status`: `disabled`, `idle`, `paused`, `waiting_location`, `loading`, `ready`,
  or `unavailable`.
- `location`: null or `{city, latitude, longitude}`; coordinates are degrees.
- `asset`: null or `{path, checked, author, license, source}`. `path` is an
  absolute bounded local file path, never a network-supplied filename. `checked`
  is Unix seconds and the version for a consumer's decoded-image cache. Display
  author/license and use source for the Commons credit action.

Missing settings fail closed until the writer publishes its state; missing
`weatherPhotos` then retains the existing default true. An unknown session state
is paused until the session watcher reports awake/unlocked. Disable/pause/release
clears the published asset and drops the async request future, killing its owned
curl child. A changed weather identity cancels the preceding attempt. No failed
request is retried until a fresh activation or location change.

The existing native weather worker owns city resolution and coordinates. N0057
now publishes them on `dashboard.weather`, and this source reads that identity
from the in-process hub, including retained cached readings. It never performs
its own geolocation or reads QML's private photo cache. It waits without network
when the weather reading is absent/invalid. Location topic changes wake the
leased photo source directly; the former once-per-second weather.json reads
have been removed. See [N0057](0057-daemon-weather-producer.md).

The clean cache is `$XDG_CACHE_HOME/nacre/daemon-weather-photo/` (absolute XDG
root, otherwise `~/.cache/nacre/`): one image and one metadata record, plus bounded
temporary work. It preserves seven-day validity, 2 MiB image and 12 kB metadata
limits. The daemon additionally validates the response Content-Type and fully
decodes JPEG/PNG under dimension/allocation limits before atomic installation.
Network is asynchronous, with bounded deadlines and nice priority 10; blocking
file/decode work uses `process::offload`, never the readiness thread pool. Fetches
share a lane and cannot overlap. No fetch is awaited by `serve`'s READY path.

Temporary duplicate: QML's photo implementation remains untouched for the daily
driver. Delete 7 days after the Bead replacement is trusted and the owner retires
the QML shell. The two private photo caches are independent and never imported.

Simplicity review: the native implementation exceeds 2.5 times the QML/JS source
because it also owns typed publication, lease/session lifecycle, full decode and
guarded file I/O. It reuses the existing session watcher and private-file guard,
shares the protocol location type, and adds no weather provider, request pool or
generic service framework. Generated contracts and fake tests are counted
separately from product code.

## Validation

Fake HTTP covers valid photo, HTTP 404, oversize, deadline, non-image and request
cancellation. A private source test covers zero fetch calls with consent off,
lease acquisition, disable/re-enable, disconnect and no overlapping requests;
the READY datagram and hub remain responsive within two seconds during a hung
fetch. The existing actual-daemon N0053 hung-backend startup test remains part
of full CI. The entire desktop job in `.github/workflows/check.yml` passed in an
isolated remote Arch container: formatting, denied-warning Clippy, examples,
workspace tests (including all 717 helper library and 22 daemon integration
tests), minimal protocol consumer, release helper, sandbox integrity, source
checks and galleries. Source suites passed: tools 491 (one conditional skip),
AI 120, Brain 52 and shell tools 487, plus all immutable workflow references.
Translation galleries passed at 244 and 185 states per language. CI tests make
no live photo requests. UI parity is unchanged because this unit adds no UI.
Product Rust delta: +834/-4; tests +386; generated contracts +127.
