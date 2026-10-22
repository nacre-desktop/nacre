# N0017 — Daemon notifications producer contract

Status: **Accepted — owner decisions recorded 2026-10-22** (see "Owner decisions" below). Implementation phases A–C may proceed; the live ownership switch on the desktop remains a separate owner gate.
Inputs: lead drafts `nacre-protocol-request-notifications.md` and `bead-B0016-draft.md` (2026-10-21). Evidence is Nacre base `961de0a3`; `Bead:` paths refer to
Bead `origin/main` at `b5bdbc9d`, not a dependency pin or permission to edit Bead.

## Owner decisions — 2026-10-22

The owner accepted the lead's recommendations:

1. **Ownership:** a separate, explicit managed-ownership opt-in, not tied to
   `NACRE_DAEMON_CLIENT=1`. The short no-owner gap during handoff is accepted.
2. **Bounds:** the proposed admission and window bounds are accepted, and v1
   has no full-history paging.
3. **Popup timing:** the frozen QML behaviour (the current configured
   default) is the policy, not Bead's 5 s default, so nothing visibly changes.
4. **Dismiss vs. forget:** dismiss retains the notice in history for every
   consumer; only an explicit clear/`forget:true` deletes it.
5. **Durability:** Notify replies immediately and history is flushed shortly
   after. The small crash-loss window is accepted for low latency.

## 1. Ownership and cutover decision

**Propose nacre-daemon as sole session-bus notification server and history writer**, hosting headless bead-services on its existing runtime.
This follows `docs/architecture/foundation.md:21-37,80-96`. Today QML creates
`NotificationServer` (`nacre/shell/services/NacreNotifs.qml:231-237`), and installed
D-Bus activation starts the shell (`nacre/shell-tools/install.py:555-559`). Bead's
shell registers its own notification owner/actions (`Bead:crates/nacre-shell/src/reload.rs:399-424,1006-1015`);
the host library accepts a connection/history path and owns no runtime (`Bead:crates/bead-services/README.md:7-30`). Bead becomes a socket consumer only.

**Do not attach ownership to `NACRE_DAEMON_CLIENT=1`.** N0014 currently switches
shell routing and daemon/socket activation, with exact restoration on failure
(`docs/specs/0014-f4-switch-lite.md:40-60`); it does not hand off the notification
bus. Keep that behavior. Propose a separate explicit managed ownership mode,
`legacy|daemon`, default `legacy`; the controller supplies daemon startup authority
and shell consumer-only mode together. Exact configuration spelling is owner-held.
Daemon mode requires published producer support and consumer readiness, including daily-driver QML. Arbitrary client connections cannot enable it.

Future owner-approved handoff, first proved on a private bus:

1. Under the release lock, preflight binaries, consumers, history permissions and
   bus owner identity; refuse an unrelated owner. Journal original activation,
   shell mode and ownership authority privately, using N0014's recovery pattern.
2. Fence competing D-Bus/autostart activation, stop admission at the cooperating
   old server, flush its history, stop that server and confirm its name release.
   Old/new writers never overlap. On flush failure preserve data/journal, abort
   and report incomplete recovery; never overwrite with stale history.
3. Start the daemon producer; request `org.freedesktop.Notifications` with
   `DoNotQueue`, without `ReplaceExisting` or `AllowReplacement`. A held name is
   refusal, never takeover, waiting in a queue or killing its owner. Clean up only
   this attempt's interfaces. This matches `Bead:crates/bead-services/README.md:27-30`.
4. Verify the unique owner, producer health, fresh snapshot and consumer-only UI;
   only then complete the transaction. Fence legacy activation for its lifetime.
5. Failure/disable: revoke daemon acquisition authority, stop admission, flush,
   release/confirm name loss, then restore legacy activation/mode and its owner.
   If another owner acquired the name, leave both managed servers off and report
   refusal. Interrupted recovery repeats these steps; never restart both owners.

A short no-owner interval is unavoidable; v1 has no atomic active-notice transfer. Retained text survives a successful flush; transient/live action handles do not. Rollback restores ownership/configuration, not application notification IDs.
Live execution needs a new owner decision (`docs/architecture/foundation.md:160-162`).
On daemon crashes retain daemon authority and restart only the daemon; UIs never
claim the name as reconnect fallback. Name contention disables this service until
explicit recovery, leaving other daemon services running. Bus disconnect/restart
retires its source generation and live handles before fresh acquisition. In-boot service
restart keeps sequence counters, remaps retired IDs and restores retainable notices only as inert history.

## 2. Notifications v1 state and limits

Use existing `CoreSnapshot::Notifications`, same-UID transport and opaque entity
maps (`crates/nacre-protocol/src/core_services.rs:8-60,204-263`; `docs/architecture/protocol.md:9-34,57-84`). Add only optional structural fields.
This producer guarantees fields below when healthy. Older reserved snapshots remain valid and imply no producer support (`docs/architecture/core-services.md:3-8`).

| Snapshot member | Producer guarantee |
| --- | --- |
| `health {available,stale,error?}` | Available only while this instance owns the bus and can process calls; no fabricated empty/healthy state on failure. |
| `do_not_disturb` | Boolean authoritative desktop `dnd` setting; daemon settings writer is the sole persistence path. |
| `active`, `history` | `{items:{id:Notification},order:[id]}`; disjoint maps, active oldest arrival first, history newest first. History contains closed records only. |
| `popup_order` | Required from this producer: ordered eligible active IDs; no client reconstruction or replay when suppression ends. |
| `inhibited` | Effective suppression (DND OR active inhibitors); no inhibitor reasons/identities exposed. |
| `history_count` | Total stored closed records plus nontransient active records (including feedback-filtered active notices), preserving the reserved meaning; not window length. Omit if unknown before storage initialization. |
| `history_storage {ready,writable,error?}` | Independent of bus health; unreadable storage is protected, failed saves retain memory. |
| `history_more` (new) | Boolean: additional closed records exist beyond the published window; omit if unknown. An empty window with storage not ready is unknown history. |

Every Notification keeps required `app_name,summary,body,urgency,resident,transient,
created_ms,actions` and existing optional `icon,image_path,timeout_ms,remaining_ms`.
`urgency` is `low|normal|critical` (native 0/1/2); `created_ms` is the latest
Notify/replacement Unix-ms time. `actions` is an ordered ID map of `{label}`.
New optional members: `revision` (safe integer, starts 1, increments on replacement),
`desktop_entry` (untrusted app identity hint), `retained` (effective history policy),
`expires_ms` (Unix-ms projected deadline), `paused` (boolean). Producer always supplies `revision,retained,paused`; all timestamps/revisions use safe integers.

Daemon IDs include boot and source-generation namespaces; remain stable across
replacement, close-to-history and reconnect until source retirement; never recycle them.
Native u32 IDs, raw action keys, bus senders/paths and persistent history keys stay private.
Each revision gets fresh opaque action IDs; validate parent/revision and action. App identity is display metadata, never authority.
Restored history gets fresh boot-scoped IDs with empty `actions`, `paused:false`, no `expires_ms/remaining_ms`
or live images. This respects boot scoping (`docs/architecture/core-services.md:26-33`).

Proposed new producer admission/window bounds (owner review required):

- Native strings: `app_name,icon,summary,body` each at most 8,192 UTF-8 bytes,
  `desktop_entry` 1,024, `image_path` 4,096, action label 1,024, private action key 256; no NUL.
  At most 16 actions/notice and 128 active notices; encoded notice at most 128 KiB.
  Preserve accepted content; reject oversized new/replacement Notify atomically,
  never silently truncate or evict an existing active notice.
- Active map including ordering at most 512 KiB encoded JSON; history window at
  most 64 rows AND 256 KiB including ordering. Select a contiguous newest prefix;
  never skip a newer row to fit an older one. `history_more` describes truncation.
- Whole encoded snap/patch including envelope must remain below 1,048,576 bytes;
  budget worst-case JSON escaping and IDs before accepting Notify, not character
  counts. Count/byte exhaustion rejects admission with sanitized native errors.
  The smaller wire budgets are additional to Bead's existing per-notice limits
  (`Bead:crates/bead-services/src/notifications.rs:43-107`), not claimed frozen parity.
- User text occurs in `app_name,summary,body,actions.*.label,desktop_entry` and
  untrusted icon names/path metadata; any can be sensitive. No raw hints, secrets
  API, argv, sender identities or text in events/logs/errors/crash reports.
  Notification content may itself contain secrets: user-only storage is not redaction.
  Keep plain-text rendering (`nacre/shell/modules/notifications/NacreNotice.qml:163-184`).
  Resolve supported local image sources into private files before publication (4 MiB each/16 MiB cache; refuse excess safely);
  `image://`/`qrc:/` renderer URLs cannot cross as file paths. Preserve adapters or block
  cutover, never silently drop them (`nacre/shell/utils/scripts/notification-images.js:1-9`).
  No remote URLs/base64/inline pixmaps (`docs/architecture/core-services.md:140-148`).

Expiry belongs to the producer: monotonic timers, no per-frame updates; zero means
no auto-close. For -1, propose preserving current host policy: Bead normal 5 s,
critical persistent; QML configured default (owner must resolve before implementation).
Hover pauses lifetime: publish `paused:true`, remove `expires_ms`, keep transition `remaining_ms`; resume computes a new deadline.
Running deadlines are projected Unix-ms hints, never client expiry commands; clock jumps cannot change monotonic lifetime. Clear removed fields with RFC 7396
nulls. DND/inhibition suppress popups without suspending native expiry; leaving
suppression does not replay. Preserve separate QML popup-hide/native-close policy
where applicable (`nacre/shell/services/NacreNotificationEntry.qml:39-49`), rather
than conflating popup disappearance with close. UI animation/expansion stays local.
Existing transition timing semantics remain (`docs/architecture/core-services.md:107-120`).

## 3. Methods, acknowledgement and failures

All methods v1, class `desktop`, default/max service deadline 3,000 ms; service queue at most 64 including in-flight. Shorter caller deadlines apply.
Every success is existing `CoreResult {changed:boolean}`.
`id`/`action` below are EntityIds; `revision` is the published notification revision.

| Method | Exact parameters | Processing acknowledgement |
| --- | --- | --- |
| `notifications.set_dnd` | `{enabled:bool}` | Settings writer acknowledged DND change; equal value gives false. Follow settings changes from all clients; no second DND file. |
| `notifications.dismiss` | `{id,revision,forget?:bool}` (default false) | Close active notice, send native close reason 2, retain per policy; `forget:true` also removes retained copy, and may target history alone. |
| `notifications.clear` | `{scope:"history"\|"retained"\|"popups"}` | history removes closed records; retained also dismisses retainable active notices without rearchiving; popups hides eligible popups without closing notices. Capture targets at execution; later arrivals survive. |
| `notifications.invoke_action` | `{id,revision,action}` | Validate live action, emit native ActionInvoked; close nonresident notice with reason 2, retain resident. True means signal submitted, never app work completed. |
| `notifications.set_hover` | `{id,revision,hovered:bool}` | Only registered UI provider; pause/resume producer timer. Hover state belongs to that connection and clears on disconnect. |
| `notifications.set_inhibited` | `{inhibited:bool}` | Only registered UI provider; connection-scoped suppression contribution, cleared on disconnect. Native inhibitors remain independently owned and bounded. |

`changed:false` means acknowledged no-op (equal setter/empty clear), not missing target. History IDs cannot invoke actions/hover. No daemon `details` method:
Bead's existing Details toggles expansion (`Bead:crates/bead-services/src/notification_bus.rs:824-831`);
the full bounded text is already in state, so move that toggle to consumer view state.
No pagination/details retrieval in v1; a larger history browser is a later contract.
`notifications.feedback` is existing OSD behavior, outside this producer; preserve
its existing route until a separate OSD contract (`Bead:crates/nacre-shell/src/notifications.rs:184-207`).

Reply after service processing, not queue insertion. Dismiss/action includes local
state transition and bounded bus signal submission; disk flush and application
effects may finish later. State remains authoritative; mark later save failures in
`history_storage`, never retroactively fail an acknowledged call. This explicitly
qualifies success, as N0016 does for network admission (`docs/architecture/core-services.md:39-41,61-69`).
`{changed}` does not guarantee durability; a crash can lose unflushed updates (owner decision).

Errors use `{code,message,retry}`: malformed params=`validation`/false;
oversized params=`too_large`/false; vanished ID/revision/action=`stale_revision`/true;
forbidden caller/policy=`permission_denied`/false; no bus owner or stale
producer=`unavailable`/true; full queue=`busy`/true; deadline=`timeout`/true;
bus submission I/O=`io`/true; sanitized unexpected failure=`internal`/false.
Partial multi-target effects or lost replies are ambiguous, never rolled back or
replayed. Native CloseNotification uses reason 3, expiry reason 1, other closure
reason 4; user dismissal/action reason 2. Re-read fresh state before explicit retry.
No arbitrary destinations, commands or paths in calls (`docs/architecture/core-services.md:150-163`).

Advertise topic only after ownership and initial settings DND readback; methods require usable handlers. Per-method
policy/storage readiness gates apply (DND needs settings; destructive history
operations need initialized writable storage). Recheck at execution for existing
connections; stale clients fail, not succeed optimistically. Existing subscribers
receive unavailable/stale health on source loss; future welcomes omit unavailable
capabilities. A UI retains stale display, disables controls/popups and resubscribes
after reconnect; no automatic local owner fallback or mutation replay.

## 4. Events, restart and persistence

Events use topic `notifications` and short names (not dotted names): `shown {id,
revision,state_seq}`, `updated {id,revision,state_seq}`, `closed {id,reason,state_seq}`
(reason `expired|dismissed|application|other`), `action {id,revision,action,state_seq}`,
`dnd_changed {enabled,state_seq}`, `history_changed {state_seq}` (forget/clear/prune),
`presentation_changed {state_seq}` (hover/suppression/popup selection). `shown` means
first successful Notify admission even if suppressed; `updated` means replacement. All seq fields are safe integers.
Allocate state seq before events; event seq is a separate increasing per-topic counter. Delivery may coalesce state or interleave
replies; `state_seq` ties an event to a state revision, never applies a mutation.
No-op setters emit no event; events contain no notification text.

Use existing 256-event per-client ring and `lost {topic,from,to}` inclusive ranges;
on loss, broken state base or unexplained event gap, retain last-good state, disable
actions/popups and `resync {topics:["notifications"]}`. Resume only on a fresh snap.
Never replay missed popups/actions or infer history from events. Reconnect resubscribes;
a new boot resets both chains and invalidates all old IDs/pending calls
(`docs/architecture/protocol.md:78-84,165-179`). A snap recovers state, not missed events. Never persist the event ring.

Persist through the existing store at `$XDG_STATE_HOME/nacre/shell/notifications/history.json`, defaulting under
`~/.local/state`; preserve version-1 array/envelope compatibility, whitelist fields,
30-day retention, transient/feedback exclusions and backup rules
(`crates/nacre-helper/src/notification_history.rs:7-19,20-70,87-149`;
`nacre/shell-tools/nacre-state-files.json:318-326`). Daemon is sole writer; legacy
helper save must refuse while its writer lease is held, including helper.execute.
Keep at most 10,000 stored rows/16 MiB encoded file (existing reader/file ceilings:
`nacre/shell/services/NacreNotifs.qml:65`; `crates/nacre-helper/src/state.rs:14,778-790`).
Evict oldest closed rows first; if active retained content cannot fit, report storage failure, never discard live state. Retainable active
text is saved too, and restored only as inert history; no popup/action/deadline revival.
Use atomic fsync writes, existing private backups, 0700 directories, 0600 files/
locks/backups/cache. Reject symlink/foreign/unsafe destinations; unreadable/future
files stay untouched. Clear/dismiss before asynchronous load uses tombstones to
prevent resurrection (`nacre/shell/services/NacreNotifs.qml:62-91,99-160`).
Persist only approved display fields, not raw hints, credentials or action routing; text cannot be guaranteed secret-free. No content in diagnostics or
telemetry. Cache cleanup follows retained references and bounds; expired native
image URLs never persist (`nacre/shell/services/NacreNotificationEntry.qml:90-96`).

## 5. Implementation inventory, phases and decisions

Exact proposed contract/artifact change set (none generated in this design unit):

- `crates/nacre-protocol/src/core_services.rs`: optional state/notice fields above,
  six CoreCall variants/params; keep legacy validators compatible, enforce new producer
  bounds separately. Unchanged CoreResult shape and existing required members.
- `crates/nacre-protocol/src/{notifications,lib}.rs`: typed event payloads
  and notification method descriptor builder; no unconditional core_methods entry.
- `crates/nacre-protocol/src/core_artifacts.rs`: extend generator with optional fields/calls/event schemas;
  regenerate `protocol/schema/core-service-{call,state}.json`, add
  `protocol/schema/notification-event.json`, regenerate `protocol/luau/core-services.d.luau`.
  `core-service-result.json`, `message.json`, `nacre.d.luau` stay unchanged.
- Add generated `protocol/fixtures/notifications-{lifecycle,controls,errors,loss-restart,ownership,storage}.jsonl`;
  cover replacement/action/hover/expiry, every method/error, resync/coalescing,
  capability refusal, restart and persistence failures. Keep original `events`,
  `core-notifications` and `core-compat-notifications-*` transcripts byte-identical.
- Extend `crates/nacre-protocol/tests/core_services.rs`, add `tests/notifications.rs`;
  update `protocol/fixtures/README.md`, `docs/architecture/{protocol,core-services}.md`.
  Use existing export (`crates/nacre-protocol/examples/export.rs:1-14`) and generator (`crates/nacre-protocol/src/core_artifacts.rs:6-45`).

| Phase | Deliverable/gate | Estimate (engineering days; not measured) |
| --- | --- | --- |
| A | Owner resolves decisions below; producer Rust types, schemas, goldens and contract CI; still reserved. | 1–2, medium |
| B | Daemon-hosted library adapter, private bus ownership/refusal fixtures, writer fencing, settings DND, limits, persistence, errors/restart; usable capability advertisement. Full desktop CI; no live enablement. | 4–7, large |
| C | Private-HOME/QML consumer-only path and journaled opt-in/rollback fixtures, preserving frozen behavior; full desktop CI in isolated container. Owner separately approves live switch plan. | 2–4, medium/large |
| D | Lead publishes producer source + exact-head green CI; then Bead pins/adopts, tests absent capability/legacy fallback, lifecycle/loss/readback parity and exact-head Bead CI. | 2–4, Bead-owned |

Publication means reviewed source/protocol integration, not tags/packages. Bead must wait for B's usable producer publication + CI; A alone cannot unblock adoption
(`docs/architecture/standing-rules.md:27-29`). Private fixtures must cover duplicate
owners, acquisition races, rollback crash points, writable/read-only/corrupt history,
byte escaping, replacement-stale actions, signal failure after mutation, lost ranges,
slow consumers, timer disconnect cleanup and no automatic takeover. Preserve frozen
backends/references (`docs/architecture/standing-rules.md:56-61`), especially
`nacre/shell-tools/tests/notification-qml/tst_notifications.qml:63-108` and
`Bead:crates/bead-services/tests/notification_api.rs:32-104` (private-bus lifecycle).

Owner questions/risks: approve separate ownership opt-in and no-owner handoff gap;
approve smaller aggregate admission/window bounds and lack of full-history paging;
choose frozen QML vs Bead default/popup timing policy before B (no silent behavior
change); approve dismiss-retain vs forget mapping for each consumer; decide whether
Notify must wait for durable history before replying (latency vs crash-loss window).
Hosting requires a reviewed Bead pin/license boundary; integrate settings/history writer instead of Bead's independent persistence.

Design acceptance here: reviewable proposal only; run `nice -n 10 git diff --check`.
No builds/tests/CI, desktop access, live probes, installation, publication or push.
