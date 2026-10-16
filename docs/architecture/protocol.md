# Nacre private protocol v1

Contract for nacre-protocol and all Nacre clients. See
[foundation.md](foundation.md). Committed schemas, Luau definitions and transcripts
live under `protocol/`; the daemon and native client share the Rust wire types.

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
Settings and other desktop services arrive in F2–F4; `menu.query` follows F2 and
notification hosting remains deferred. Wire schemas cover structural shapes/ranges;
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
committed from the Rust types, including a populated320-app result.

Queries carry raw query text plus mode/optional route/limit. Prefix interpretation,
catalog loading, filtering, ranking, provider work and history belong to Nacre's
engine. Results echo query/mode/route and catalog_rev; clients drop superseded
query replies. items is a stable-ID-keyed object and order is authoritative.
Kinds/modes are open strings; older clients ignore unknown optional fields.
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
It implements no helper/daemon/menu behavior and makes no latency acceptance claim.
