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

For state replay, a wrong base must not mutate the last snapshot. `base` matches
the last delivered sequence even when coalescing skips source revisions. A new
boot retains old display data until fresh snapshots, but invalidates its chain.
Requests are never automatically replayed after a deadline or disconnect.
