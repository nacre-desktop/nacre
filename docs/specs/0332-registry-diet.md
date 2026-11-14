# N0332 — `daemon` topic and method-registry diet

**Status:** Implemented (D1, D2, D3 from the N0327 findings)

N0327 found (code reading, origin/main 86433e4a4) that `Hub::service()` rebuilt
the whole method registry, every params/result schema included, on every call,
and republished it as part of the `daemon` topic. The compositor producer calls
`service()` after every compositor publish (`compositor.rs` `publish`), and
focus, notifications, tray, widths and others call it repeatedly with an
unchanged entry. Shell reads no `daemon` field: it uses the topic only as a
"registry changed" trigger and takes methods from Welcome and `daemon.status`
(Shell lead confirmation, 2026-11-15).

## D2. Rebuild only when the method set can change

`service()` returns early when the service's entry is unchanged. Before, those
calls only rebuilt the registry; any method change they surfaced came from
other state, which now republishes `daemon` itself through
`Hub::registry_changed()`:

| What gates `fixed_methods()` | Path that republishes |
| --- | --- |
| `services` entries | `service()` when the entry changed |
| topic presence (`niri.events`, `wallpaper.active`, `i18n`, `audio.spectrum`, `updates`, `weather`, `brain`, `compositor`) | `publish_state`: first publish of a topic |
| topic data: `available`, `health.available` (audio, BlueZ, notifications, time, network ...), `compositor` `backend`/`capabilities`/outputs empty, `controls` `night_light` | `publish_state`: `gates_methods` sees the field change |
| UI provider methods | `register`, `retire_provider` |
| BlueZ readiness (`bluetooth.rs` `set_ready`) | explicit `registry_changed()` |
| network capability bits (`controls::Requests::enable`) | explicit `registry_changed()` when the bits change |
| `hub.settings/compositor/dashboard/calendar/network/ai_chat/...` set at startup | each is followed by a `service()` entry change |

`gates_methods` sits in `hub.rs` beside `fixed_methods`; a new gate on topic
data must be added there. Known limit: tool probes inside the gates
(`program_available("ddcutil")`, `brightnessctl`, `wf-recorder`, `grim`,
`hyprpicker`, keyboard backlight, login-theme env) are re-run only when
something above republishes, not on the next compositor event. Calls are not
affected: each call checks its backend when it runs.

## D3. Size snapshots with a counting writer

`publish_state`, `event` and `enqueue` sized each frame with
`framing::encode(..)?.len()`, allocating the whole frame. The new
`framing::encoded_len` validates the same way, serializes into a byte counter
and applies the same `MAX_FRAME` limit (test: equal to `encode().len()`).

## D1. Smaller `daemon` topic

The topic is now `{boot, safe, services, topics, generation}`. `methods` (every
method with its schemas) and `clients` are gone from it; `daemon.status` still
returns the full status with both, and Welcome carries the methods.
`generation` changes exactly when the method set does: `registry_changed`
hashes the serialized registry and bumps the counter when the hash changes, so
every method-set change still republishes the topic (Shell N0251). No protocol
schema covers the topic, so the exported contract is unchanged
(`export --check`). `docs/architecture/protocol.md` describes the fields.

## Measurement

`hub::tests::registry_is_rebuilt_only_when_a_service_or_method_gate_changes`
scripts 50 compositor publishes, each followed by the producer's
`service("compositor", "running", 0)`, then one nacrectl connect (Welcome) and
disconnect. It counts `methods()` builds and `daemon` publishes:

| | registry builds | `daemon` publishes |
| --- | --- | --- |
| before (origin/main aba6a02c5, from the code: one build per `service()` call + Welcome) | 51 | 2 |
| after | 3 | 2 |

The same test pins that a gate change (compositor availability removing the
display methods), provider registration and retirement each bump `generation`,
and that a service change with an unchanged method set does not.
`daemon` snapshot size and the Shell's `daemon.status` round trips per patch
were not re-measured on a live session; the N0327 harness can do that.

## Not done

D4 (needs-full-snapshot detection while diffing) and the Shell/Bead items S1-S3,
B1-B2 from N0327 are separate units.
