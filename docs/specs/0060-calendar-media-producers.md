# N0060 — calendar and media daemon producers

Status: implementing; no live changes or publishing. Scope: existing reserved
`dashboard.calendar`, `dashboard.media`, and `dashboard.calendar.refresh` contracts.
No schema additions are needed. The protocol exporter adds separate producer
lifecycle transcripts, preserving the frozen scenarios.

## QML and shared calendar reader

`NacreDashboardState.qml` reads the `dashboard-data read` helper's `calendar`
result, refreshes when opened, and refreshes every 60 seconds while shown.
Preferences come from the registered `dashboard` store (XDG config
`nacre/dashboard.json`). `calendarFile` defaults to empty and accepts an absolute
local path or `~/…`; the latter expands against HOME. There is one local ICS
source, no URL fetching, remote consent or calendar network connection.

The existing Rust `dashboard_data::calendar` reader is reused, with an explicit
Paths entry point for fixtures. It unfolds continuation lines, inspects at most
500 VEVENT blocks, ignores cancelled events and malformed dates/unknown TZIDs,
sorts by start and returns at most eight entries from today through now + 31 days.
RRULE is **not expanded**: the base event is shown if in range, accompanied by
“Recurring events require an expanded calendar export.” Exports must pre-expand
recurrences, including exceptions. All-day dates use midnight and “All day”. UTC
`Z` and named IANA TZIDs are supported; ambiguous named-zone times use the earlier
instant and nonexistent times are skipped. Floating times retain the helper's
current system-local fixed offset; displayed date/time labels use system Local.
This preserves the existing limitation for floating events across a DST change.
Malformed blocks are skipped, so a wholly malformed export can be an empty list.

The daemon reads only the settings owner's published snapshot. It publishes an
unavailable empty state synchronously, then offloads file reads and inotify setup
onto N0053's bounded short-lived worker lane. Settings changes, a coalesced refresh
method, parent-directory inotify (including atomic replacement and symlink target
parent), and the same 60-second fallback cause refreshes. Failed watcher setup
falls back to the timer. Calendar work never gates READY. Reads require regular
files, use nonblocking open to reject FIFOs, and enforce 2,000,000 bytes both by
metadata and a capped read to handle growth. No previous source result publishes
after the preference changes. Stable event IDs hash the existing event payload;
identical projected duplicates retain distinct occurrence suffixes. Recurrence warnings use health.error
while retaining available events; file errors publish unavailable, empty events.

## Media decision and consent

Media is **not redundant** with player. `DashboardMediaState` and
`protocol/fixtures/core-dashboard-media.jsonl` carry lyric lines/plain text,
palette, lyrics_message and cache_seconds. `dashboard.player` carries MPRIS
metadata and playback controls. Bead origin/main `crates/nacre-shell/src/dashboard.rs`
leases media for its Media page (85–99) and separately player (256–267).
A player alias would violate the media schema and lose lyrics.

Reuse `dashboard_media::track_data`; no new network/provider implementation.
As in `NacreMediaDetails.qml`, work runs only while the Media page has a topic
subscriber, and `dashboard`'s lyrics preference (default true) controls lyrics
queries. Missing/errored settings prevent requests. An internal bounded client
leases the existing player service; closing media releases it. Track and lyrics
setting changes clear old details. Results are published only if the same metadata
and subscription are still current. There is one in-flight request; already
started work can finish after closing/consent changes, but cannot publish then.
The helper keeps its HTTPS-only LRCLIB fetch, six-second global timeout, 300 KB
response bound, private track-keyed cache and existing cache expiry. Shared cache
reads now require regular files and cap both metadata and actual reads at 8 MB.
The topic also caps total serialized lyric lines at 500 KB, leaving room for plain
lyrics and entity IDs within the protocol's 1 MiB frame limit. No idle retry
or second MPRIS backend. Browser identity presentation cleanup follows media.js.
As current QML does, artwork is empty and palette false, so no new artwork fetch
or album-palette work is enabled; the reserved palette field remains empty.
The existing QML spectrum process is outside this contract and remains unchanged.

## Acceptance and cleanup

Fixtures cover all-day, named-zone, UTC, recurrence warning, cancelled/outside
window and malformed records. A sparse huge file tests the capped read. Tests
cover stable entity collections, atomic replacement, refresh validation/safe mode,
media interest, immediate player lease release and setting changes during delayed
fake work, cache bounds and large projected lyric payloads, and an actual daemon
READY < 2 seconds with a calendar FIFO that would block an ordinary reader.
Full desktop CI runs remotely in the disposable Arch CI container; no local tests.

The QML and daemon adapters share the same Rust readers. The legacy QML adapter
is a temporary dual path: delete 7 days after the Bead replacement is trusted
(owner core switch and stable calendar/lyrics use); do not duplicate its parser.
No user-visible UI/layout changes or private desktop captures are part of this unit.
