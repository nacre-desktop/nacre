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
that event lane. The matching `EdgeLeft` event clears the hover opened by an
edge trigger. Without a daemon source, the QML helper tries the daemon lane
first, then uses the direct native stream as a compatibility path until the
daemon edge source is trusted by Nacre clients. Remove that temporary direct
stream 30 days after the daemon edge source is trusted; the stock-niri surface
fallback remains permanent.
Rejected, disabled, or lost streams clear active hover routes and restore
QML/Bead fallback. Bead routes dashboard/control `hover-open` and `hover-leave`
through its owner-only command socket with the triggering output; the assistant
uses its existing app route. Frame clicks remain available. Bead commit
`41d1cc6b74a6aaaae2320ec531b6e49df3797a04` adds the generic `external_source`
policy only while the daemon publishes an acknowledged stream, retaining
reserved geometry while omitting interactive lip surfaces.

The compositor advertises edge-trigger availability only when at least one
enabled edge has an `Event` action. The generated fork overlay configures all
four edges this way; configurations without an enabled event action, including
stale or manually launched stock-like configurations, keep the invisible-surface
fallback. Nacre regenerates the overlay during setup/preparation, while session
launch reuses an existing overlay rather than silently rewriting user files.

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
The complete generated include tree validated with the final native Niri binary:
stock `config.kdl` and fork `nacre-niri.kdl` both exited 0. N0113's version
identity fix is merged at `6457215e`; NN0010 supplies the IPC contract. The
Bead CI report is in the B0219 task worktree at
`.lead/ci/final-41d1cc6b/REPORT.md`; Nacre's full desktop workflow log, stage
manifest, and source-tree proof are in `.lead/ci/final25/`.
