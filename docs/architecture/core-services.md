# Core service contracts v1

These are reserved schemas and example transcripts, approved on 2026-10-17.
They are **not implemented capabilities**. This change does not register methods,
claim a bus name, load bead-services, or change Quickshell's service ownership.
Clients enable a control only when the daemon's actual `welcome.methods` and
`welcome.topics` advertise its supported version. Missing support stays disabled
or uses an explicitly selected existing fallback; never simulate success.

Rust payloads live in `nacre_protocol::core_services`. Generated artifacts are
`protocol/schema/core-service-{call,result,state}.json` and
`protocol/luau/core-services.d.luau`. Supplemental `core-*.jsonl` transcripts
exercise successful payloads, unavailable sources and policy failures. Original
wire and helper reference outputs are unchanged. These examples describe future
backend behavior, not successful hardware tests.

## Framing and identity

The existing [private link](protocol.md) carries every message. `CoreCall` models
the `m` and `p` fields of `call`; `CoreSnapshot` models `topic` and `data` in
`snap`. Keep the ordinary envelope's `id`, `timeout`, `seq`, `base` and boot ID.
There is no additional transport or nested JSON string. RFC 7396 patches apply
to `data`; validate the resulting complete snapshot, not an incomplete patch.
Within version 1 clients ignore unknown optional fields.

Entities are `{items: {id: entity}, order: [id, ...]}`. Each ID appears exactly
once in `order`, all references must resolve, and collections contain at most
2,048 entities. Entities have opaque daemon-issued IDs, 1–128 ASCII letters,
digits, `_`, `.`, `:` or `-`. IDs are not shell commands, MAC addresses, device
paths or D-Bus destinations. The backend keeps that mapping. Do not reuse an ID
after backend replacement; a vanished/stale target returns `stale_revision`.
IDs and pending operations are scoped to the daemon boot; never replay a mutation
after connection loss. Reconcile with a fresh snapshot before another user action.

Every state has `health: {available, stale, error?}`. An unavailable service may
retain last-good data, visibly marked stale. Absence of data means unknown, never
zero. Error uses the existing `{code,message,retry}` shape, with sanitized text.
`retry` indicates that a new explicit attempt may work; it does not authorize
automatic replay. A successful `CoreResult` is `{changed: boolean}`: the backend
acknowledged the operation, and subsequent topic state remains authoritative.
A failed or ambiguous operation must not update the UI optimistically.

## Topics and calls

| Topic v1 | Data | Reserved methods v1 and parameters |
| --- | --- | --- |
| `network` | Radio state, devices, access points, saved connections; no secrets | `network.set_wifi {enabled}`; `network.connect {device,target:{kind:"access_point"\|"connection",id}}`; `network.disconnect {device}` |
| `bluetooth` | Adapters and their power/discovery state; devices with connected/paired/trusted/pairing/battery | `bluetooth.set_power {adapter,powered}`; `bluetooth.connect`, `bluetooth.disconnect`, `bluetooth.pair`, each `{device}` |
| `power.profiles` | Available profiles, active ID, optional degraded/unavailable reason | `power.set_profile {profile}` |
| `battery` | Independent optional battery percentage and charging readback | Topic contract only; no profile selection |
| `audio` | Input/output/playback/capture nodes, default input/output, stream target IDs | `audio.set_default {node}`; `audio.set_volume {node,volume}`; `audio.set_mute {node,muted}`; `audio.route {stream,target}` |
| `notifications` | Do-not-disturb state, active notifications, bounded history window | N0017 A/B producer; advertised only by the ready opt-in daemon owner |
| `osd` | Service-selected transient volume, microphone, brightness or Caps Lock display | Topic contract only; no control or permission grant |
| `tray` | Items, local icons, flattened menu maps and child ID lists | Topic contract only in this step |
| `dashboard.weather` | Provider, checked time, optional current reading with hourly/daily entities | `dashboard.weather.refresh {}` |
| `dashboard.updates` | Checked time, optional count, existing status text | `dashboard.updates.refresh {}` |
| `dashboard.calendar` | Configured flag and existing local ICS event summary | `dashboard.calendar.refresh {}` |
| `dashboard.media` | Existing enrichment key, lyrics/line times, palette, cache lifetime | Topic adapter only; not playback controls |
| `dashboard.system` | Existing process identity/CPU/memory and filesystem capacity snapshot, plus optional `cpu_percent`, `memory`, `gpu`, a bounded `network` rate series (N0033) and a static `host` block (user, os, kernel, uptime; N0038) | Read-only; no process termination method |
| `dashboard.player` | MPRIS players with status, track text, cached local `art_path`, position/length and ability flags (N0033) | `player.play_pause`, `player.next`, `player.previous` `{player}`; `player.seek {player,position_ms}` |

Network controls return `{changed}` at admission: NetworkManager accepted the
property write or activation/deactivation request. They do not wait for activation
completion or publish optimistic state. Method errors describe admission failures
only: policy/authorization is `permission_denied`, vanished targets are
`stale_revision`, and unavailable/timed-out backends are `unavailable`. Subsequent
native-agent failures, including missing agents and cancelled prompts, appear in
authoritative network topic health as one sanitized, retryable authentication
failure; they are not errors on an already admitted call. This network-specific
rule qualifies the general agent-error mapping below without changing v1 schemas.

Audio setters accept the application's linear value in **[0,1]**, preserving
Nacre's current 100% cap; it is not decibels or hardware gain. Readback keeps the
actual finite backend value, including amplification set by another application.
That does not authorize a Nacre setter to exceed 100%. Inputs and outputs are
distinct node kinds;
default selection accepts only a ready input/output node. Routing connects a
playback stream to an output, or capture stream to an input, subject to the
existing PipeWire/WirePlumber policy. It grants no new microphone permission.
Devices disappearing during an operation are errors, not successful no-ops.

### Additive compatibility readback

`AudioNode.readback`, when present, is authoritative and has independently optional
`linear_volume`, `perceptual_volume` and `muted`. Missing means unknown, including
an empty `{}`; never substitute zero volume or an unmuted state. The original
`volume` and `muted` fields stay required for v1 compatibility, but are readings
only when `ready` is true. When `ready` is false, those legacy fields may contain
placeholders or retained values and must not be displayed as live readback.
If `readback` is absent, an older producer's ready legacy values remain usable.
Individually known `readback` members remain displayable even when `ready` is
false; that flag gates the complete legacy tuple, not independently known values.
Never fall back to legacy fields for a member missing from a present `readback`.
The perceptual slider value is the cube root of linear volume, matching Bead's
existing backend; for example, linear `0.125` is perceptual `0.5`. Convert a slider
value to its cube before calling the unchanged linear `audio.set_volume` setter.
Both values describe the same max-channel sample, not independent authorities.
External amplification can make either readback exceed one; the setter's existing
linear [0,1] limit remains. Removing readings uses RFC 7396 null members, not zeros.

`network.primary_connection`, when present, references an active entry in
`connections.items`. It is the backend's primary connection, not the first active
map entry, strongest access point or first device. Omission means no known primary;
a disconnect patch removes the reference with null. `battery` is independent of
`power.profiles`, with optional percentage in [0,100] and optional `charging`.
A battery-less or unavailable backend supplies no fabricated zero-percent reading.

`notifications.popup_order`, when supplied, is the service's ordered selection of
eligible active IDs; missing means unknown and an empty list means no popups.
Clients never reconstruct it from `active`, DND or `inhibited`. Clearing inhibition
does not replay previously suppressed notifications. Optional `timeout_ms` and
`remaining_ms` describe policy/lifetime at service transitions, including hover
pauses; the service owns expiry, without per-frame topic updates.
`remaining_ms` is a value at a transition, not a running client countdown: never
decrement it locally or infer that a paused hover is running. Its absence means
unknown/no countdown; zero means no lifetime remains. A resolved `timeout_ms` of
zero means no automatic expiry, independently of an omitted `remaining_ms`.
Optional `history_count` is the total stored history plus nontransient active notices,
not the bounded `history.order` length. `history_storage: {ready,writable,error?}`
reports storage initialization and persistence independently of notification-bus
`health`; a read-only history error does not imply the notification backend died.

`osd.current`, when present, has a boot-scoped increasing `serial`, explicit
`channel`, explicit `unit`, optional `value`/`muted`, and absolute Unix-millisecond
`expires_ms`. Volume/microphone values use `linear` or `perceptual`; brightness
uses `fraction` in [0,1]; Caps Lock uses `boolean` (0 or 1). Unknown readback omits
the value. Repeated equal-valued requests change the serial so clients can restart
the visual pulse. The service removes `current` on expiry; an expired snapshot
never replays an OSD. Receiving the same serial again never restarts the pulse or
expiry. On a direct channel change, patches must explicitly delete inapplicable
members: changing an object with RFC 7396 merges it rather than replacing it.
This reserves no arbitrary client-driven OSD method. The schema examples test
expiry removal and identical serials; they do not claim an implemented UI has
already passed expired-snapshot or pulse tests.

Seven new `core-compat-*.jsonl` transcripts exercise these optional additions,
including custom-theme tray `icon_path` materialized as a local cache file.
The original core transcripts stay byte-for-byte unchanged. All examples are
schema scenarios, not recordings of implemented services or hardware measurements.

Notifications keep the existing history format, backup and privacy rules. Text
is display content, never executable markup. Preserve the current safe markup
and local-image URL policy. Tray/notification image bytes remain on disk in a
private cache; `image_path`/`icon_path` are absolute local paths, never HTTP URLs,
inline pixmap arrays or base64. Menus use stable IDs and acyclic trees, at most
16 levels, with one parent per child. Neither topic exposes executable commands
or arbitrary bus routing.
Notification/tray action contracts may be added separately before their cutover;
these topic schemas do not authorize raw-row execution.

## Permission boundaries

The private socket checks the same UID; it does not grant system authority.
NetworkManager, BlueZ, power-profiles-daemon and PipeWire keep their existing
authorization and session policy. Network connect uses the configured native
secret/authentication agent. Bluetooth pairing uses the existing interactive
agent and can be cancelled. Do not auto-accept pairing, auto-trust devices,
persist passwords/PINs, pass credentials over this link, or add passwordless
polkit rules. Policy/agent denial or cancellation returns `permission_denied`;
missing backend/agent returns `unavailable`. Only IDs published by that service
may be acted upon; reject cross-device, wrong-kind and stale references.
No client-supplied bus path, process argv, shell text, URL or filesystem target
is accepted as an action destination. Implementations must bound backend calls
and honor the existing request deadline without logging private payloads.

## Dashboard compatibility and later workers

Current `dashboard-data` and `dashboard-media` are `helper.execute` v1 lanes with
the original stdout JSON string in `HelperReply`; they are **not** dedicated
`dashboard.media` or `dashboard.system` topics. `dashboard-system` is currently a
standalone native helper. `network.watch` is availability/change generation only,
not the new `network` device topic. Existing clients retain those contracts.
Calendar already exists in `dashboard-data`'s ICS snapshot: the future typed
topic adapts it rather than creating a second parser or changing recurrence rules.
The reserved `dashboard.media` adapter maps `lines` to entities and renames
`lyricsMessage`/`cacheSeconds` to `lyrics_message`/`cache_seconds`. Line times are
seconds. It describes the existing enrichment helper, not MPRIS playback state
or actions. The reserved `dashboard.system` adapter maps process/disk arrays to
entities and `cpuSeconds` to `cpu_seconds`; memory/disk counts are bytes, CPU is
cumulative seconds, and `start` retains the existing process-start identity.
It does not expose a new termination operation. Visible-only sampling and existing
read permissions remain; do not introduce an idle process-scanning loop.

Weather maps the existing helper's fields to snake_case (`feelsLike`→`feels_like`,
`isDay`→`is_day`, `localTime`→`local_time`). Temperatures are Celsius, wind km/h,
pressure hPa, humidity percent. Hour/day lists become ID maps with an `order`
list; their weather `code` becomes `wmo`, while `rain` stays precipitation
probability percent. Sunrise, sunset, UV and optional AQI remain available.
Readings may be null/absent when unknown; do not display those as zero.
Limits remain 24 hours/10 days. Optional `checked` retains fractional Unix
seconds; calendar `start` uses Unix seconds and notification `created_ms` uses
Unix milliseconds. Time strings keep the
provider's existing local timezone interpretation. Existing on-disk caches and
preferences remain readable without migration.

The later worker port preserves current weather triggers and accepted IP lookup
when no location is set; it introduces no new background requests. Refresh uses
stored settings, never a caller-supplied endpoint. Updates refresh checks status
only: it does not install or upgrade packages, change package-manager authority,
or alter the deliberate `paru -Syu --skipreview` update flow. Coalesce concurrent
refreshes; fetch/check work runs in bounded daemon-managed workers and follows
the charter's timeout/idle-exit rules. Freeze existing helper outputs before that
port. N0017 now pins permissively licensed `bead-services` for its headless
notification state engine. Nacre owns notification bus admission, policy and
its existing fenced history store; the standalone Bead host is not started.
Tray/audio hosting remains separate work.
