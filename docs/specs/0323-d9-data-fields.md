# N0323 — Platform fields for the D9 dashboard pages

Status: implemented. Answers platform asks 4 and 5 of the D9 review (N0242,
[0237](0237-design-review.md#d9-dashboard-pages-n0242)). All fields are optional and absent
when unknown, so older producers and consumers stay valid. No new topic, method, task or poll.

## Contract

| Topic | Where | Field | Source |
| --- | --- | --- | --- |
| `brain.recent` | state | `path` (absolute vault root, ≤ 4096) | the configured vault root the producer already reads |
| `brain.recent` | `notes.items.*` | `world` (1..64 chars) | first name in the note's `Worlds:` header (`brain_discovery::header_fields`), read from the text the producer already loads |
| `brain.recent` | `notes.items.*` | `modified_ms` (Unix ms) | the file mtime from the metadata the producer already reads |
| `dashboard.player` | `players.items.*` | `desktop_entry` (1..255 bytes, no `/`, no control chars) | MPRIS root `org.mpris.MediaPlayer2.DesktopEntry`, as given (basename without `.desktop`) |

```json
{"health":{"available":true,"stale":false},"path":"/home/user/Documents/Nacre-Brain",
 "notes":{"items":{"n0a1b2c3d4e5f6071":{"title":"Public fixture note",
   "path":"/home/user/Documents/Nacre-Brain/projects/fixture.md","world":"Nacre","modified_ms":1800000000000}},
   "order":["n0a1b2c3d4e5f6071"]},"tasks":{"items":{},"order":[]}}
```

```json
{"player-1":{"identity":"Fixture Player","status":"playing","…":"…","desktop_entry":"org.fixture.Player"}}
```

- The vault has no `world:` frontmatter; a note records `Worlds: A, B` in its header block
  (`brain_data` writes it). The first listed world is the note's world; a name over 64 chars is
  dropped, not truncated.
- `desktop_entry` is an untrusted bus value: the producer drops one that fails
  `core_services::desktop_entry_valid`, and a client resolves it against installed desktop
  entries before launching anything. It is re-read with `Identity` on every root or player
  `PropertiesChanged` (the existing per-player `GetAll` reads).
- `path` is kept from the last snapshot while the vault is unavailable.

## Daemon topic invariant (ask 3)

Shell's N0251 re-reads the method registry only when the `daemon` topic changes. That holds for
service status changes (`Hub::service` republishes `daemon`), but not for every method-set
change: a UI provider `register` or disconnect, and methods gated on a topic's presence or
content (for example `dashboard.weather.refresh` when `dashboard.weather` first publishes,
brightness methods on compositor outputs, `time.*` on the `time` topic's health), change `Hub::methods()` without a
`daemon` publish until the next service change. Probe tests run on dev-2 and never committed
(`Hub` unit tests comparing the `daemon` snapshot's `methods` with `Hub::status()`): service change
passes; provider register, provider disconnect and the `dashboard.weather` first publish fail.
No pinning test was added and nothing was fixed; the lead decides.

## Validation

dev-2, `nacre-ci/arch-nacre:current` (rustfmt 1.10.0), read-only cargo cache, `--network none`:
export regenerated and `--check` clean, `cargo fmt --all --check`, `cargo clippy -p nacre-helper
-p nacre-protocol --all-targets -D warnings`, `cargo test -p nacre-protocol`, the minimal consumer,
and every nacre-helper test target (lib 1071 passed, including the private-bus player contract
that now covers `DesktopEntry` and a root-only `PropertiesChanged`; `dashboard_extras` covers
`world`, `modified_ms` and `path`).
