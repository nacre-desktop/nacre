# Nacre private protocol v1

Contract for nacre-protocol and all Nacre clients. See
[foundation.md](foundation.md). Committed schemas, Luau definitions and transcripts
live under `protocol/`; the daemon and native client share the Rust wire types.
Reserved, schema-only device and hosted-service contracts are documented in
[core-services.md](core-services.md). They are not runtime capability claims.

## Transport and bounds

Connect to `$XDG_RUNTIME_DIR/nacre/daemon.sock`. The directory is 0700, the socket
0600, and both peers verify the same UID with `SO_PEERCRED`. There are no TCP
listeners. `NACRE_DAEMON_SOCKET` selects an absolute private test socket; it does
not disable ownership or peer checks. Socket activation adopts systemd's listener
only after validating LISTEN_PID/LISTEN_FDS and its Unix listening socket type.

UTF-8 JSON lines: exactly one object followed by LF, maximum 1,048,576 bytes before
LF. CRLF is accepted. Empty lines, invalid UTF-8/JSON, non-object roots, duplicate
required fields, excessive nesting and oversized frames are protocol errors.
Reject an oversized frame before allocating its unbounded remainder. Closing
mid-line discards that incomplete frame. Blobs are local cache file paths, never
base64; receivers validate their owning operation's path/size policy.

The first message is hello, due within five seconds. Limit 64 clients, 128
subscriptions and 64 outstanding calls per client; requests beyond these bounds
return `busy`. A blocked output write expires after ten seconds and disconnects
that client. Bound queues: state latest wins per topic, events 256 per client,
and at most 64 pending replies. No unbounded queue is permitted.

Every message has string `t`; unknown optional fields are ignored. Identifiers
and method/topic names are bounded to 128 UTF-8 bytes. IDs are strings or integers
in JavaScript's safe range (absolute value ≤9,007,199,254,740,991); no fractional
IDs. Sequences and revisions are nonnegative safe integers. A daemon restart
changes `boot`; sequences are scoped to that boot.

## Handshake and versions

```json
{"t":"hello","proto":1,"min":1,"client":"qml-shell"}
{"t":"welcome","proto":1,"min":1,"boot":"00000000-0000-4000-8000-000000000001","topics":{"compositor":1},"methods":{"daemon.status":{"v":1,"class":"desktop","timeout":1000,"params":{},"result":{}}}}
```

`hello.proto` is the client's highest wire version; optional `min` defaults to
`proto`. The daemon selects the highest overlap and reports the oldest version
it supports in welcome.min. No overlap returns `unsupported_version` and closes.
V1 initially has min=1; once v2 exists the daemon serves v2 and v1. Topic versions
are a name→integer map. Methods are a name→descriptor map: `v`, capability `class`
(`ui`, `desktop`, `system`, `privileged`), default timeout in ms, parameter and
result JSON Schemas. Classes describe the operation; they do not bypass polkit.
`nacrectl call --list` prints this registry.

Within a version only optional fields may be added. Breaking changes require a
new version and owner approval. Topic/method versions are independently advertised;
a client must not assume an unadvertised method exists. Required structural
fields remain required. Unknown message tags return `invalid_request`.

## Subscriptions and consistent state

```json
{"t":"sub","topics":["compositor"]}
{"t":"snap","topic":"compositor","seq":0,"data":{"windows":{},"order":[]}}
{"t":"patch","topic":"compositor","base":0,"seq":3,"data":{"windows":{"w1":{"id":"w1","focused":true}},"order":["w1"]}}
{"t":"resync","topics":["compositor"]}
{"t":"unsub","topics":["compositor"]}
```

Sub is idempotent and sends an initial snapshot; unsub forgets that client's
baseline. Unknown topics produce `unknown_topic`. Resync sends fresh snapshots
for already subscribed topics. The data root is an object. Entity collections
are stable-ID-keyed objects; `order` is a separate list of IDs. Arrays are allowed
for scalar lists/order, not lists of entities.

Patch data follows RFC 7396: recursively merge objects, null deletes an object
member, scalar/array values replace. A stored null object member cannot be
introduced by a merge patch; send a snapshot when the new state requires one.
Diff/apply property tests cover that representability boundary explicitly.

Each topic has a global seq. A patch's `base` names the last seq actually sent to
that client, and `seq` names the new state. Apply only when base equals the
client's known seq; otherwise request resync and retain the last good state.
Coalescing may skip source revisions (0→3 above); this is a complete patch from
0 to 3, not a missing update. The explicit base distinguishes coalescing from a
gap. Ignore obsolete seq within a boot. A snap replaces the entire topic and
resets the chain. Reconnect/new boot always resubscribes.

Track the last state sent to each client. Coalesce changes for at least one
frame (16 ms default); limit title-only churn to about 10 Hz. Build each outgoing
patch against that baseline, never against an update the client did not receive.
Apply compositor event batches before publishing: no window may reference an
absent workspace/output. A transient inconsistent niri state is held until it
can be projected consistently, never exposed as an inconsistent snapshot.

## Calls, replies and errors

```json
{"t":"call","id":7,"m":"daemon.status","p":{},"timeout":1000}
{"t":"ret","id":7,"ok":{"safe":false}}
{"t":"call","id":"s1","m":"settings.set","p":{"store":"desktop","key":["frameSheen"],"value":false,"rev":4}}
{"t":"ret","id":"s1","err":{"code":"stale_revision","message":"Store revision changed","retry":true}}
```

`p` defaults to `{}`. Optional timeout is milliseconds, 1–120,000; otherwise the
method's default applies. Duplicate in-flight IDs are rejected. A ret contains
exactly one of `ok` (including null) and `err`. Completion order need not match
request order. Timeout is a caller-visible deadline, not a promise to roll back
an already committed side effect; retrying mutations requires re-reading state.
No generic command execution or arbitrary file-writing method is exposed.

Uncorrelated transport/subscription failures use
`{"t":"error","err":{"code":"invalid_request","message":"Invalid message","retry":false}}`.
Messages must be safe for UI display; errors/logs do not echo secrets or raw
payloads. Structured operation errors may identify a registered store and line.

| Code | Meaning | Retry |
| --- | --- | --- |
| invalid_request | Invalid message, ID, shape or operation | false |
| unsupported_version | No supported version overlap | false |
| too_large | Frame or bounded result exceeds limit | false |
| permission_denied | UID/path/privilege check refused | false |
| unknown_topic / unknown_method | Unadvertised name | false |
| unavailable | Service/provider disconnected or disabled | true |
| busy | Client, call or worker concurrency limit | true |
| timeout | Call deadline reached | true |
| stale_revision | Settings rev mismatch; read current state | true |
| validation | Schema or hand-edit validation failed | false |
| io | File or outside connection failed | true |
| internal | Sanitized unexpected service failure | false |

`retry` is always explicit; clients decide whether retrying an action is safe.
Optional `err.dispatched` is false only when the daemon proves that no UI provider
received the call. Provider replies, lost providers and UI deadlines are marked
true. Its absence means unknown. The one-release `qs ipc` fallback is allowed for
a missing default socket or unavailable UI with dispatched=false, never an
uncertain result. An explicit private socket override disables that fallback.
Malformed/oversized frames, failed handshakes and UID mismatches close the link.
An ordinary failed call leaves the link usable.

### QML stdio transport

`nacrectl attach` forwards one checked socket connection through stdin/stdout
pipes. The pinned Quickshell 0.3.2 Socket/SplitParser accepts valid traffic, but
replaces invalid UTF-8 and buffers an oversized unfinished line before a QML
callback can reject it. This native bridge enforces the same byte-level framing
in both directions before forwarding; unknown optional fields remain unchanged.
It reuses the endpoint/peer checks above and the ten-second output deadline.

Rust readers expose `retained_bytes()` for their line-buffer capacity and
`trim_idle(max_bytes)` to release a completed or empty buffer above that ceiling.
Trimming refuses unfinished frames, including cancelled reads; it does not
change wire data or promise a process RSS reduction. Underlying reader buffers
and payloads owned by callers are outside that accounting.

The UI sends hello and owns subscriptions, IDs, deadlines and reconnects. Closing
either UI pipe ends the bridge; daemon loss fails the connection. The bridge
never reconnects, queues work for a later connection, or falls back to another
executor. A lost reply leaves dispatch uncertain. No live UI or unit is enabled
by adding this command.

During foundation validation, `NACRE_DAEMON_CLIENT=1` opts the QML client in;
unset or `0` retains the legacy paths. Reconnects use 50 ms–2 s backoff, retain
last snapshots and expose stale state after two seconds without a connection.
No pending call is replayed. Provider replies use the same connection without
waiting for the outbound activation they acknowledge.

## Events, liveness and UI providers

```json
{"t":"ev","topic":"notifications","name":"shown","seq":42,"data":{"id":"n1"}}
{"t":"lost","topic":"notifications","from":43,"to":46}
{"t":"ping","id":"health"}
{"t":"pong","id":"health"}
{"t":"register","provider":"ui","methods":{"ui.launcher":1,"ui.settings":1}}
```

Events have a per-topic event sequence separate from state sequences; they are
not merge patches. Each client's ring holds 256 events across topics. Overflow
drops oldest events, sends lost ranges before the next surviving event, and
clients refresh affected state. Durable notification history is stored separately;
the ring is never its persistence mechanism. Ping IDs are echoed unchanged.
Idle links do not need periodic pings; watchdog is a daemon/systemd concern.

Only one connection registers the UI provider at a time; another receives busy.
Registration lasts for that connection. `ui.*` calls are forwarded as call/ret
messages to its registered method allowlist with daemon-generated IDs; map the
reply back to the original caller. Provider loss returns unavailable to waiting
callers. Unregistered UI methods never execute. F3 adapts existing QML IpcHandler
targets without redesigning them; `nacrectl ui TARGET METHOD [ARGS]` encodes their
target/method/argv parameters for the registered UI dispatcher.

UI clients keep last state while disconnected, mark it stale after two seconds,
and reconnect with 50 ms→2 s exponential backoff. On a new boot clear transport
IDs/sequences, resubscribe and replace state with fresh snapshots. Do not replay
pending mutations automatically. After a daemon restart UI freshness is measured
from daemon readiness to the new snapshot rendered, with a one-second target.

## Generated contract and acceptance

F1's Rust serde types are authoritative for schemas in `protocol/schema`, Luau
types in `protocol/luau/nacre.d.luau`, and JSONL goldens in `protocol/fixtures`.
Generation is deterministic and CI rejects stale files. Goldens cover handshake,
snapshot/coalesced patch/resync, calls/errors, events/lost, UI routing and restart.
Clients replay the same committed transcripts. Property tests cover patch
round trips and recovery after gaps; fuzz targets cover framing and merge patch.
JSON object key order is not part of the wire contract; clients compare decoded
messages. The protocol library enables only Tokio's I/O utilities and leaves JSON
map ordering to its consumer. Export tests alone enable deterministic ordered maps;
CI also builds a separate consumer to catch dependency-feature leakage.
Nested niri and Hyprland tests subscribe, kill -9, restart and verify resync.
Nightly jobs and the full three-hour soak remain deferred to the later rails step.

## First services and native commands

`compositor` v1 is read-only. Its root has `backend`, `available`, `error`,
`outputs`/`outputOrder`, `workspaces`/`workspaceOrder`, `windows`/`windowOrder`,
`focus {windowId,workspaceId,outputId}`, `keyboard {names,currentIndex,activeName}`,
`capabilities`, and overview/configuration status. Collections use string keys;
Hyprland numeric IDs are normalized, with native IDs retained as metadata.
Unresolved references are held out until a consistent projection is possible.
Keyboard names are scalar lists. Title-only updates are limited to 10 Hz.
Niri requires one shared event socket and one persistent request socket;
Hyprland has one event socket and closes each request socket by protocol.

On niri, `compositor.events` v1 carries the one-release event-command bridge.
Its snapshot is `{backend,available,connectionId}`; `connectionId` is an opaque
local socket identity, never a path. Subscribe to it and `compositor` together.
Its `ev` messages use name `niri` and the existing normalized event data. The
daemon sends the corresponding state before each event; a slow client gets the
latest consistent state rather than every intermediate snapshot. Title-only
notifications may coalesce. On `lost` or a state-chain gap, refresh both topics
before processing more events. Events before subscription are represented by
the initial snapshot. Unresolved desktop entities/references are omitted/cleared,
including transient startup, window-close and disabled-output observations.
These are intended differences from the direct event stream.
The native compatibility command checks that the daemon serves its selected niri
socket. A missing default daemon or an older daemon without this advertised lane
uses the existing direct runtime; an explicit override or a session mismatch
never falls back. Disconnection retains the command's existing exit/restart policy.

`session` v1 has `available`, a last-good `record`, and an optional service error.
The record keeps the existing event, time, readiness, wallet, Brain-window count
and configuration-error fields. Its monitors are keyed by name with a separate
`order`. `session.check {event?}` writes fresh evidence; `session.ready {}` keeps
the existing fifteen-second readiness policy; `session.watch {}` acknowledges the
daemon-owned observer. The compatibility `session-watch` command restores its
old JSON shape and files. Its `watch` mode exits after acknowledgement instead
of keeping another process alive; the on-failure compatibility unit remains for
one release. A missing default/older daemon uses the direct runtime; explicit
overrides never fall back, and safe mode never starts a second observer.
These are intended lifecycle differences. Resume and output settling delays
remain 500 and 300 ms, respectively, using the same policy as direct mode.
The service reads login1 and Secret Service asynchronously, consumes the existing
compositor event stream and shares its native request transport. UI state goes
through the registered provider, with the same before-dispatch legacy fallback.
Its bounded internal event ring reports overflow/resync instead of adding a
polling loop. Session methods are advertised only when that service is declared.
Zbus uses the daemon's existing Tokio runtime. Legacy blocking helpers default
their process-local fallback pool to one worker; explicit caller settings and
the shell supervisor's exported environment are preserved.

`daemon.status` returns boot, safe-mode, client and service status.
`daemon.ping` returns null without outside work. `daemon` is the service-health
topic; safe mode also exposes one stable record on `daemon.notice`.
Settings and other desktop services arrive in F2–F4; the launcher engine now
precedes the remaining F2 fold-ins. Notification hosting remains deferred. Wire schemas cover structural shapes/ranges;
the decoder also enforces UTF-8 byte bounds and cross-field constraints.

The same ELF dispatches `nacre-helper daemon` / `nacre-daemon`, and
`nacre-helper ctl` / `nacrectl`. Native commands are `status`, `call [--timeout MS]
METHOD [JSON]`, `call --list`, `watch TOPIC`, and `ui TARGET METHOD [ARGS...]`.
Watch reconnects and resubscribes; it exits when its output pipe closes.
`NACRE_DAEMON_FRAME_MS` configures the advertised coalescing interval (default 16).
Standalone tests set an absolute `NACRE_DAEMON_SOCKET` and private HOME/XDG/buses.
User units are staged; enabling/starting them requires the owner's decision.


## Launcher methods v1 — additive payload contract

`nacre-protocol::menu` defines menu.query/menu.activate payloads. It registers no
method; advertise these descriptors only when the engine actually serves them.
The existing wire version, core registry and seven envelope transcripts do not
change. New schemas, separate Luau definitions and four method transcripts are
committed from the Rust types, including a populated320-app result. A fifth
transcript exercises optional conversion results and provider guidance; the older
method and envelope transcripts remain byte-identical.

Queries carry raw query text plus mode/optional route/limit. Prefix interpretation,
catalog loading, filtering, ranking, provider work and history belong to Nacre's
engine. Results echo query/mode/route and catalog_rev; clients drop superseded
query replies. items is a stable-ID-keyed object and order is authoritative.
Kinds/modes are open strings; older clients ignore unknown optional fields.
Wire keys use the existing stable/favorite ID function; navigation routes retain
the original menu IDs. Missing or duplicate result IDs fail visibly instead of
silently dropping records from the entity map.
Display fields are name/kind plus optional description/breadcrumb/icon/emoji/
brand/status/checked/reason/disabled. Icons are theme names or local paths; no image/base64 payload.

An optional activation string is an opaque engine descriptor, not a command or a
permission grant. Clients send id/activation/catalog_rev back to menu.activate;
the engine validates current revision and capabilities, and owns all side effects.
Never replay an ambiguous activation after disconnect/timeout. A successful result
can keep the view open and optionally update its query/mode/route. Structured
stale_revision/unavailable errors retain their normal wire meanings.

The migration owner reviewed this additive contract; engine availability is
advertised through the welcome method registry when implemented.
The daemon advertises these methods when its launcher service is hosted. Safe
mode leaves the launcher off. `menu` publishes availability, catalog revision,
indexed entry count and diagnostics; a successful query proves its own completed
result count. Empty `route: "apps"` returns the application catalog; empty
`route: "home"` returns saved favorites. Clients must not rank or filter results.

The implementation retains the original JavaScript ranking/provider model in
`nacre-menu`; the UI fallback stays until F3. Application directories are indexed
from absolute XDG data roots and watched with inotify, including newly created
roots and directory replacement. No-op scans preserve the revision. Invalid hand
edits keep the last good index; writes to corrupt preferences fail visibly.
Registry locks do not trigger index refreshes.

Application records are shared immutably between the parsed catalog and its
visible/all-app lists. Prepared emoji rows reuse a bounded 256-entry cache while
typing; the existing idle-worker retirement event releases that cache before
allocator trimming. The full CLDR search index remains in the worker and exits
with it. These caches do not change result ordering, payloads or revisions.

No-op scans queue no metadata work. Metadata refreshes coalesce to one in flight
plus one pending latest refresh, sharing the existing two-worker cap and ten-second
deadline. `menu.providers_rev` starts at zero and advances after a completed
metadata load; the `menu` topic then notifies subscribers even if `catalog_rev`
is unchanged. Clients can refresh the current query on this notification.
Concurrent calls can still return `busy` with `retry:true`: keep the old view and
retry only the latest idempotent query within its original deadline. Activation
is never automatically retried.

`menu.preferences` accepts `{action:"load"}` or
`{action:"favorite"|"hide"|"record",value:{id,enabled}}` and returns the existing
store shape. Legacy CLI/serve calls use this owner when present; an absent default
endpoint retains standalone behavior. An explicit endpoint never falls back.
F4 will bring this writer under the common settings registry.

Applications activate through a bounded native worker and the user manager's
`StartTransientUnit`, preserving literal argv, working directory, terminal wrapper,
clean launch environment and existing history. Intended difference: the former
`systemd-run --scope` path becomes a transient `Type=exec` user service. No shell
or `systemd-run` process participates. Other existing actions temporarily forward
the daemon-selected row to `ui.menu.activate`; that registered provider preserves
the existing confirmation/polkit flow and replies with `MenuActivated`. Clients
cannot supply a command or a confirmation bypass through the activation payload.

`MenuUiActivate` in `menu-ui-activate.json` defines this **daemon-to-provider**
payload: `{row,catalog_rev}`. A provider registers `ui.menu.activate:1`, but that
route is not advertised as a public method. Direct client calls are refused with
`permission_denied`, `retry:false`, `dispatched:false`; only the engine's checked
ticket path can forward a selected row. `ui.dispatch` remains public. This adds
no generic command execution API.

The row keeps the existing `id`, `kind`, `name`, `operation` and optional literal
`value` (text or a safe integer), `disabled`, display fields and action-specific
fields. `copy` requires literal text; `copy-clip` identifies the existing clipboard
entry, including images, and must not copy its display label. Command arguments
and working directory remain literal daemon-selected data. Open operation names
allow newer clients, but an unknown operation returns `unavailable` without doing
anything. Fields are bounded, and unknown optional fields are ignored. There is
no caller-supplied `confirmed` field: the existing UI still owns confirmation,
capability checks and polkit. Its acknowledgement may mean the existing dialog
accepted the action; `keep_open:true` retains that view while confirmation waits.
The inbound provider handler must not await the outbound `menu.activate` call,
or the two directions would deadlock.

`menu-ui-actions.jsonl` supplements the original immutable menu transcripts with
calculator-copy tickets, clipboard IDs, confirmation, literal argv, refusal of
public row injection and an unsupported operation. Arithmetic now runs directly
in the Rust engine, matching 217 frozen Quickshell outputs and 96 separately
frozen numeric/whitespace/power cases. The optional `conversion` field carries either
arithmetic or unit-conversion results; successful results issue literal-copy
tickets. Only lowercase whole-word `to`/`in` queries enter the existing conversion
worker. Invalid arithmetic returns its original error without a worker. The QML
cutover remains a separate F3 step; its JavaScript fallback stays for one release.

Files, packages, unit conversions, CLDR emoji and catalog discovery reuse native
providers in at most two nice-10 workers with a ten-second response deadline and
30-second idle exit. Cancelled worker I/O closes that worker instead of reusing a
partial response. Conversion results and nonfatal guidance are optional result
fields. Dynamic desktop inputs follow their F2/F3 service moves; absent inputs
retain the model's disabled/unavailable state. This does not claim all desktop
services have moved, nor establish a UI-frame or physical-desktop latency budget.
The initial service uses English IDs/fallbacks; locale and pseudo-language
adaptation must be complete before the F3 client cutover.

## Shared niri width policy

`niri.widths` v1 publishes `{available, opening_width, relaxed}`. The service uses
the existing width policy and X11 property driver. Native niri window events come
from the compositor owner's bounded internal tap, preserving nullable title and
floating fields; the public compositor snapshot and legacy event output keep
their existing shapes. There is one niri event stream and one shared persistent
request connection regardless of compatibility observers.

`niri.widths.configure` v1 accepts the generated `WidthsConfigure` payload:
`{connection_id, display, opening_width}`. The compositor socket identity and X11
display must match the daemon's session before the percentage changes. Percentages
must be finite, greater than zero and at most 100. Mismatches fail closed without
opening a second compositor connection. The result is a `WidthsStatus` snapshot.

The legacy `niri_widths` command keeps its options, help, session check and silent
foreground behavior. An absent default broker or an older default daemon without
this service retains the standalone implementation; an explicit endpoint never
falls back. Connected observers reconfigure and resubscribe after a restart.
Intended lifetime difference: stopping a compatibility client stops that view;
the daemon owns policy lifetime and restores saved hints when its service stops.
Height and maximum-size hints, later application sizing, and the single 150-ms
opening correction retain the frozen policy rules. Compatibility units remain
for one release and are not enabled or started by this development step.

## Hosted helper lanes v1

`helper.execute` retains the six legacy `serve` tools and their arguments,
operation status and stdout string. Inline parameters are
`{request:{id,tool,arguments,input}}`; the result is `{code,output}`.
The compatibility command retains the legacy ready packet, IDs, per-tool FIFO,
64 pending requests, 4 MiB request and 8 MiB response limits. It uses one link;
it never replays an action after a lost response. An absent default endpoint
or an older daemon permits the standalone command. An explicit endpoint or
safe mode fails closed.

`helper.transfer {}` returns `{directory}`: a private directory leased to that
connection. Large calls use `{file:"packet-..."}` instead of `request`, and large
results use `{file:"packet-..."}` instead of `code/output`. These basenames refer
only to that lease; no arbitrary path is accepted. Files are same-UID regular
files, mode 0600, without links. The lease is removed on disconnect; an exclusive
endpoint owner cleans its own stale leases after a crash. Normal packets remain
below the private link's 1 MiB limit. Types, schemas and examples are generated
alongside the other protocol artifacts.

Launcher preferences join the engine's existing writer. Other heavy operations
share its two bounded, nice-10, idle-exiting provider workers. Intended transport
difference: a worker operation has a ten-second deadline and the compatibility
call has a 120-second deadline including queueing; failures are returned rather
than retried. A running daemon retains its last good launcher preferences after
an invalid hand edit, as required by the sole-writer policy.


## Network change trigger v1

`network.watch` reports the health of the daemon's NetworkManager signal reader,
not NetworkManager availability, an AP list, connectivity or an action API.
Its typed data is `{available,generation,error?}`; `generation` is a nonnegative
JavaScript-safe integer and `error`, when present, is a sanitized message of at
most 256 bytes. See `network-watch.json`, `network.d.luau` and `network.jsonl`.
The generation increases for the initial subscribed reader and every matched
NetworkManager or owner-change signal, survives service restarts within a boot,
and resets with a new daemon boot. Snapshots coalesce at the normal frame rate.
There is no timer or active Wi-Fi scan. Network identities never enter this topic.

The one-release `network-state watch` adapter prints one initial `changed` line,
then one per generation increment, including increments covered by a coalesced
patch. It fails when its selected service/link fails; it never replays into a
second direct watcher. An absent default/older non-safe daemon permits the
existing standalone reader. An explicit missing endpoint or safe mode fails
closed. Closing the output pipe releases the idle adapter without polling.
These are added transport/lifecycle protections; frozen normal change lines stay
exact. Full network device snapshots/actions and other bar services remain deferred.

## Wallpaper filesystem monitoring

`wallpaper.watch` v1 is `{available,generations,error?}`. It reports debounced
filesystem changes, not wallpaper selection, decode or palette publication.
`generations` maps opaque 64-character lowercase SHA-256 source IDs to nonnegative
safe integers; at most 32 entries. IDs delimit the raw absolute root, scheme,
configuration and engine path bytes. They preserve symlink identity and avoid
publishing personal path strings. Unknown optional fields remain ignorable.
Errors are sanitized and at most 256 bytes. See `wallpaper-watch.json`,
`wallpaper.d.luau` and `wallpaper.jsonl`.

One inotify source reuses the standalone filter, recursive subscriptions without
following directory links, and 500 ms debounce. It observes the running helper,
installed helper and legacy bridge as distinct engine paths where needed. Common
library/scheme/preferences changes advance all relevant counters; engine changes
advance only their own counter. Counters survive a service restart within a boot.
No timer runs while idle. New directories are subscribed before their later file
events; creation alone does not produce a notification.

The one-release `wallpaper-watch` client emits no initial line, then the exact
legacy `{"changed": true}` line per generation increment, including increments
covered by a coalesced patch. Missing default/older non-safe daemons fall back
before accepting a source. Custom watch paths without a matching default-daemon
source retain standalone behavior. An explicit socket or safe mode fails closed
for missing capabilities or mismatched sources. Once accepted, source failure or
connection loss ends the client with an error; it never replays through a second
watcher. Closing the output pipe exits without an idle polling timer.

## QML compositor, helper packets and launcher views

`niri.source {socket}` returns `connection_id` only when the selected socket has
exactly the daemon compositor's device/inode identity. `niri.action
{connection_id,argv}` uses the existing compositor action allowlist and shared
request connection; it rechecks identity after waiting for that connection.
A session mismatch is refused before dispatch. A timed-out or lost action reply
is uncertain and must not be replayed. These methods are advertised only on niri.

`helper.transfer {}` retains its original directory response. Optional
`reserve:true` additionally returns an empty owner-only `file`; optional
`release:[names]` removes consumed packets from that same connection's lease.
Names, ownership, mode0600, regular-file/link count and existing size limits are
checked for reads and release. Reserve/release retain the128-packet cap; disconnect
removes the lease. QML uses FileView only with these pre-reserved input files.

`menu.query` additionally accepts optional `filter`, `category`, and
`presentation`. Omission retains the old response and ranking behavior.
`presentation:true` adds existing preview/context metadata to each item; it is
read-only display data, never execution authority. A selection sends only its
engine-issued activation ticket, item ID and catalog revision. The internal
selected-row route reuses the UI's existing confirmation and action handlers.
Unknown provider operations and unregistered UI methods are refused.

Intended text difference: the shared engine keeps its frozen ECMAScript final-sigma
and U+0085 whitespace behavior; the frozen actual-Qt394 cases differ at161,162,
211,212,213,216. Both references remain unchanged; the other388 cases agree.

Default UI bindings are generated from the neutral compositor contract. Niri
emits direct argv for `nacrectl ui` commands; their operands stay literal, including
empty strings and shell metacharacters. Other generated commands retain their
existing transport. The old binding fixture is unchanged; its15 intended Hyprland
UI command swaps are compared explicitly, with all keys and flags still exact.

### Revisioned Settings ownership (F4)

`settings.get {store}` returns `SettingsStore`: `store`, `rev`, `exists`, `data`,
`dirty`, and an optional `error {file,line,message}`. The same object is published
on `settings.store.<id>`. Revisions belong to one daemon boot. Invalid hand edits
retain the last good data; the error uses the registry-relative filename.
The existing pseudo-language is accepted only with `NACRE_DEVELOPER_MODE=1`,
matching the desktop controller; it is not added to the public language enum.

`settings.set {store,key,value,rev}` validates registered writable keys before
changing memory. A stale revision returns `stale_revision`. Writes become durable
after 150 ms quiet; `dirty:false` confirms publication. `settings.desktop
{arguments,rev}` uses the existing controller and compositor connection; reads
may omit `rev`. Display rollback journals remain durable before physical changes.
Caller loss never replays an admitted mutation or starts a fallback writer.

`settings.publish {store,value,rev}` preserves the three existing QML publishers:
`control-center`, `dashboard-extras`, and `battery-alerts`. It accepts their
current document version and owned fields, retains unknown existing fields, and
rejects other stores. It grants no arbitrary path, provider, or privileged write.
The native `state-file` client uses the same revisions and waits for a durable
snapshot before reporting success. An absent default socket permits standalone
compatibility; connection, validation, and dispatched errors do not.

`settings.provider {store,value,rev}` admits only the existing colors, wallpaper
picker, dashboard preferences, CLI accent, default-app initialization, and launcher
workflow providers. Their exact store IDs are `colors`, `wallpaper-picker`,
`dashboard`, `cli`, `apps`, and `launcher-workflows`. Registered settings and
explicit provider fields are validated; unknown existing fields survive unchanged.
No other provider, root-owned store, or arbitrary filename is writable. Native
read/modify/write operations check both the originating boot and revision, wait
for durable publication, and route rollback through the same owner.

Intended difference: with the daemon present, legacy `state-file save-id` bulk
writes outside the three QML publisher stores are refused; validated `set-id`
remains available. Provider-owned/read-only stores and unported tools retain
their existing privilege boundaries; this step does not migrate all 72 stores.

An accepted mutation advances the in-memory revision; it is not, by itself, a
durable-success claim. A client that reports durable success must observe the
same boot and store, a revision at least as new as the accepted revision,
`dirty:false`, no read/publication error, and the operation's owned values still
present. `settings.set` checks the requested key even for a no-op. A publisher
checks every field it supplied, not unrelated retained data. A provider
replacement also expects omitted provider-owned fields to remain absent; unknown
retained fields are outside that comparison. For `colors.appThemes`, only the
eight registered theme keys are checked. Values compare as JSON structures, so
object member order is irrelevant and absence differs from JSON null. A mismatch
is a visible superseded/conflict result, never a replay or fallback write.

`menu.preferences` favorite, hide and record mutations preserve their legacy
durable-success reply: the daemon waits for a clean matching publication of
favorites, hidden, recent and usage before returning success. The load action is
read-only. The native controller response's `rev` identifies its admitted store
revision; a later `settings.get` with another revision cannot be used as that
operation's receipt. Native clients compare changed/deleted operation-owned
paths against the exact receipt before waiting for durability.

The Settings actor owns only the 19 registry entries with Settings metadata
(16 writable and 3 read-only), plus the separately owned `battery-alerts` state
publisher. Its six narrow provider adapters are `colors`, `wallpaper-picker`,
`dashboard`, `cli`, `apps` and `launcher-workflows`; `settings.publish` remains
limited to `control-center`, `dashboard-extras` and `battery-alerts`. The other
53 registry records retain their named provider/root/tool owners; this is not a
generic 72-store write API. `profile` permits the session-owned `aurReview`
setting only. Whole-profile replacement is an offline installer/deployment
operation and is unsupported while the daemon owning the destination is active.
The current destination writers are the explicit `nacre setup` path and the
explicit release deployment path (`nacre deploy`, or `tools/manage.py --apply`)
through `tools/releases.py`; neither has an implemented stopped-daemon guard in
this stage. `tools/releases.py` reads whether selected desktop services are
active, but that snapshot does not prove destination ownership or exclude a
socket-activation/startup race. `tools/prepare.py` writes only
into a separately selected empty staging tree. Inotify and fingerprint checks
detect some external edits but do not establish exclusive ownership. No ordinary
session/UI caller of replacement was found; active-daemon installation remains
unsupported pending a separate owner-approved, destination-scoped enforcement
or handoff specification that covers socket activation and startup races.

`launcher-menu` is marked `config/write` in the registry but has no enabled
Settings descriptor. The two `NacreMenuContext` FileViews at lines 1677–1697
only read, parse and watch the defaults and user extension; they have no write
call, and both paths are empty in shared mode. The daemon menu engine reads the
merged configuration. No competing daemon-mode application writer was found;
standalone user-edited files do not establish one. `settings.set` does not admit
this store, and no generic setter or descriptor is implied by the registry tag.
