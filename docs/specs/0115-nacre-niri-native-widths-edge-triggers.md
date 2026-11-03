# N0115: native Nacre niri widths and edge triggers

Nacre already gates `nacre-niri-widths.service` with the live
`proportional_x11_widths` capability; the service's `ExecCondition` suppresses
startup only when the running compositor confirms the native policy. Keep that
fallback for stock niri and fork builds without the capability.

The fork overlay `nacre-niri.kdl` configures `edge-triggers` and requests an IPC
event for each edge using Nacre's saved hover delay (350 ms by default). The
generated stock `config.kdl` is unchanged. The Nacre helper daemon owns the
`NacreEdgeTriggerStream` after the capability reply and publishes its accepted
state plus events on `compositor.events`. QML daemon mode and Bead apps consume
that event lane. Without a daemon source, the QML helper tries the daemon lane
first, then uses the direct native stream as the one-release compatibility path.
Rejected or lost streams set active false and restore QML/Bead hover fallback.
Frame clicks remain available. Bead commit
`0dbb1536bfb3b50579f8c72cf5fc5454d66e8499` adds the generic `external_source`
policy only while the daemon publishes an acknowledged stream, retaining
reserved geometry while omitting interactive lip surfaces.

The event position is output-local logical coordinates. QML maps left, right,
and top events to assistant, control center, and dashboard openings after
hit-testing the actual configured lip at the event position; it ignores
bottom and suppresses triggers while fullscreen, in overview, or in click-only
mode. Missing or older capability fields default to false.

The native stream reports completed triggers only, so the UI cannot render the
pre-trigger hover progress cue supplied by the invisible-surface path. Live and
visual interaction acceptance is unverified; source tests and desktop CI do not
establish cue or rendered behavior parity.

Tests cover old and new capability replies, malformed edge payloads, stream
handshake, fork-only overlay generation, stock configuration preservation,
QML fallback/activation, and Bead source fallback on actual daemon socket EOF.
Full Bead and Nacre desktop CI on dev-1 is pending.
This branch depends on N0113's version identity fix and NN0010's IPC contract.
