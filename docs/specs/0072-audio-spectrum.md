# N0072 — leased audio spectrum

Status: implemented and validated. Producer for Bead B0115;
design context: Bead B0113, Music widget. No UI or live installation in this unit.

## Contract

The additive `audio.spectrum` v1 topic uses existing protocol v1 snap/patch
framing and coalescing, with compact base64 bytes rather than JSON float arrays.
No new transport framing or client-wide subscription fields are required.
Bead's `nacre_protocol::Message`/topic mirror can transport it unchanged; B0115
must pin this producer revision, decode the new type and implement rendering.
Inspect welcome before subscribing; safe mode does not advertise the service.
Generated schemas, Luau declarations and fixture come from `nacre-protocol`'s
`export` example, including the configuration call's parameter schema.

1. Subscribe normally: `{"t":"sub","topics":["audio.spectrum"]}`.
2. Optionally call `audio.spectrum.configure` with
   `{"bands":"32","rate_hz":60}` on that connection. Bands are string enum
   values `"16"`, `"32"`, `"64"`; omitted fields default to 32/60.
   Requires an existing subscription; unknown fields and rates outside 1..60 fail.
   Reply is `{"changed":true}`. Reconfiguration resets that client's baseline.
3. Unsubscribe or disconnect to release. A repeated subscription resets defaults.

Example topic data (32 silent bands):

```json
{"health":{"available":true,"stale":false,"error":null},"bands":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","level":0.0,"peak":0.0,"onset":0,"beat":false,"paused":true}
```

`bands` is padded standard base64, decoding to exactly 16, 32 or 64 unsigned
bytes, ordered low frequency to high. Divide by 255 for normalized 0..1 values.
Band edges are `20 * 1000^(i/N)` Hz (20 Hz to 20 kHz). Each is the maximum
FFT amplitude in its interval, mapped from -60..0 dBFS to 0..1, with 25 ms
attack and 200 ms decay. Analysis at 64 bands is shared; smaller leases take
the maximum of adjacent groups. Frequency resolution is 11.72 Hz; the lowest
bands can share bins. RMS `level` and absolute `peak` cover the latest 16.67 ms
PCM block, normalized to full scale and rounded to 0.001.

`beat` flags an energy onset (>1.6 times a slowly tracked RMS baseline,
RMS >0.01, 180 ms refractory interval). `onset` is a cumulative counter for
the daemon lifetime, retained across sink switches, capture retries and lease
cycles. Compare it across delivered frames: coalescing may hide `beat` pulses.
A fresh subscription takes its first value as baseline. This is an onset
detector, not tempo/BPM inference. `paused` means silence persisted for 3 s.
Unavailable states publish zero levels/bands with `available:false`.

## Implementation and budgets

The daemon starts one `parec` default-sink monitor capture on the first lease,
using PipeWire's Pulse compatibility backend, 48 kHz mono s16le, 10 ms requested
latency/process fragments. It never captures the microphone or plays audio.
The last lease drops and reaps only that child. The shared N0059/N0061 OSD
audio backend observes sink changes while spectrum is leased; its existing
`audio.default_output` changes trigger reopening `@DEFAULT_MONITOR@`.

A 4096-sample Hann window and RustFFT transform run inside the daemon, at the
highest requested lease rate capped by the lead-configured hub frame interval
and 60 Hz. Each client's delivery also has its own cap and coalesced baseline.
After 3 s below RMS 0.0001, FFT/smoothing stop; bounded PCM reads and cheap
level checks continue so sound wakes analysis without another external probe.
No capture, FFT plan, timer or audio connection exists when unleased. Only an
unavailable static snapshot and an interest waiter exist at startup (N0053).

RustFFT 6.4.1 is MIT OR Apache-2.0; new FFT dependency licenses must remain
permissive (no advisory/license exception added). No temporary duplicate paths.
The microphone `audio.peak` topic retains its separate purpose and lifecycle.

## Acceptance evidence

Fake PCM tests cover sine frequencies 60..12000 Hz, RMS/peak, periodic onsets,
silence transform skipping/resume, independent lease projection/rate caps,
live configuration, capture sharing/release and sink changes. No real sink is
used in CI. N0053's actual-daemon hung-backend READY <2 s test also checks
spectrum advertisement and absence of an unleased capture launch.

CPU gate: <=2% of one core at 60 Hz/32 bands; unleased capture/FFT CPU is zero
by construction. An explicit release-mode fake-PCM benchmark includes ingest,
FFT, JSON topic publication, per-client projection, hub diff and frame encoding.
It processes 3600 changing frames (60 simulated seconds), using process CPU
ticks rather than wall time. This excludes capture subprocess/audio-server
cost and real scheduling; record that limitation with results.

Remote Arch desktop CI completed, including Rust format/Clippy/tests, minimal
consumer, sandbox integrity, Ruff/QML formatting, all source suites, 534 parity
cases with zero failures, and English/pseudo galleries. The tools suite's one
opt-in soak test remained skipped, as in the job definition. Source checks were
resumed after fixing task-container Git trust for the checked-out submodule.
The new dependency graph passed `cargo deny check advisories licenses bans sources`.

Release fake-PCM CPU evidence (one 32-band lease, 60 Hz):

| Phase | Daemon CPU, one core | Capture CPU | Delivery |
| --- | ---: | ---: | --- |
| Unleased, 2 s | 0 ticks (100 Hz accounting) | no process | none |
| Changing tones, 10.001 s | 0.200% | 0.200% (fake) | 600 topic updates |
| Paused silence, 10.001 s | 0.300% (cheap PCM checks) | 0.700% (fake) | 0 topic updates |

The offline 3600-frame pipeline consumed 0.0200 CPU seconds for 60 simulated
seconds (0.0333% of one core). Both explicit release tests passed. Real-time
measurement includes capture I/O, analysis, projection, diff and frame encoding;
the Python fake's CPU is reported separately. Shared OSD backend, actual
PipeWire/Pulse server and hardware costs were not measured. No real audio
endpoint was opened. CPU accounting resolution is 0.01 s, so small percentage
differences between music and silence are not significant.
