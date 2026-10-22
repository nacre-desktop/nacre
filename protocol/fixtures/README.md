# Protocol v1 transcripts

Generated from `nacre-protocol` Rust types; refresh with
`cargo run -p nacre-protocol --example export`. Add `-- --check` for a read-only
freshness check. Cargo tests check the committed exports automatically.
These are new link contracts, separate from immutable Python helper references.
All messages are exact JSONL wire objects; names and content are synthetic.

| Transcript | Replay |
| --- | --- |
| handshake | Client hello (with unknown optional field), daemon welcome |
| state | Welcome, initial snap, valid coalesced patch, wrong-base patch, client resync, replacement snap |
| null-snapshot | Two daemon snapshots; a new null object member cannot be introduced by RFC 7396 |
| calls | Alternating client calls and daemon success/unknown-method/timeout replies |
| events | Daemon event, lost range, surviving event; client ping and daemon pong |
| restart | Welcome/snap, new-boot welcome, client resubscribe, fresh sequence-zero snap |
| ui | Provider registration, caller call, forwarded provider call, provider ret, caller ret |
| notifications-lifecycle | Producer state transitions, action replacement, hover timing, suppression, dismissal, forget and expiry |
| notifications-controls | Typed notifications method requests, acknowledgements and state changes |
| notifications-errors | Sanitized method failures and retry classifications |
| notifications-loss-restart | Lost range, explicit resync, fresh boot and snapshot |
| notifications-ownership | Capability refusal when the daemon does not own the bus |
| notifications-storage | Unknown history before load, storage failure and later readback |
| shell-actions | Output-scoped workspace activation/scroll/window focus and confirmed session execution |
| bar-services | Current-owner tray actions and output-scoped brightness updates |
| pill-controls | Default-sink volume/mute and default-controller Bluetooth power |

For state replay, a wrong base must not mutate the last snapshot. `base` matches
the last delivered sequence even when coalescing skips source revisions. A new
boot retains old display data until fresh snapshots, but invalidates its chain.
Requests are never automatically replayed after a deadline or disconnect.

Launcher method-v1 goldens are separate from the seven unchanged wire
transcripts: menu-apps (320 keyed items), menu-empty, menu-results (display/prefix
shape examples), and menu-activation (opaque activation/navigation/errors).
Their query strings illustrate shapes; the Nacre engine defines interpretation.
No daemon method is advertised/implemented by generating these fixtures.
The six notifications transcripts are contract examples generated from the
producer types. They do not claim that a runtime adapter is implemented or that
the daemon advertises the methods; advertisement remains conditional on usable
handlers and bus ownership.

The bar-action methods are implemented by N0031; availability still depends on
the daemon's safe mode and backend readiness. Use IDs from the current topic
and inspect the connection's method registry. Views and anchors remain local
to each shell; see [N0031](../../docs/specs/0031-shell-action-routes.md).

The `i18n` transcript (N0035) shows the language topic, a paged `i18n.catalog`
and the `stale_revision` failure after a language change; see
[N0035](../../docs/specs/0035-i18n-publication-contract.md).
