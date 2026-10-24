# N0059 — Audio, battery and power-profile state producers

Status: Implemented; pending lead review. Switch blocker from the topic audit.

## Scope and contracts

The daemon publishes the existing reserved `audio`, `battery` and
`power.profiles` service-v1 contracts. Bead's control center consumes health,
default audio IDs, ordered input/output nodes, volume/mute readback and ordered
available power profiles. Device IDs are PipeWire `node.name`, matching N0051's
`audio.set_default` and volume/mute methods. No consumer changes are needed.
PipeWire documents [`node.name` as its identifying property](https://docs.pipewire.org/devel/page_man_pipewire-props_7.html);
Pulse names are resolved as aliases, and [monitor sources](https://www.freedesktop.org/software/pulseaudio/doxygen/structpa__source__info.html) are excluded.

- `audio`: health, nodes (name, kind, linear volume, mute, readiness and explicit
  linear/perceptual readback), default output and input. `pactl` JSON sink/source
  lists provide channel volume and `properties["node.name"]` identities; Pulse
  aliases resolve the defaults, and sink monitor pseudo-sources are excluded.
  Entries without PipeWire identity are omitted. The largest channel matches
  the existing volume readback policy. Pulse's normalized volume is perceptual;
  its cube is linear volume. The existing OSD `pactl subscribe` process now serves
  either audio or OSD interest. Sink/source/server events debounce a state refresh;
  a failed subscription clears audio availability and reconnects with backoff.
  No audio work runs without either lease. Lists and commands are bounded.
- `battery`: UPower DisplayDevice percentage and charging flag, extended additively
  with UPower state (0–6), seconds to empty/full and `on_battery`. Nonpositive time
  estimates are unknown. Absent/non-battery DisplayDevice is unavailable.
- `power.profiles`: active ID and ordered profile entities from power-profiles-daemon,
  extended additively with `degraded` (PerformanceDegraded reason) and structured
  `holds` (profile, reason, application_id). Both current and legacy service names
  use the existing backend helpers. Unsupported optional properties stay unknown.

Power state uses the existing behaviours D-Bus proxy and signal helpers.
PropertiesChanged and service owner changes refresh the state; healthy operation
never polls. Disconnected buses reconnect after a bounded delay. No method or
policy is duplicated, and these producers never set audio or power state.
No backend implementation is copied. Existing QML audio/power state readers
remain for the daily driver: delete 7 days after the replacement is trusted at
the core switch, as part of owner-approved QML retirement.

## Startup and failure policy

Initial snapshots are synchronous unavailable values, without backend probes.
Daemon supervisors start all external discovery in background tasks. D-Bus
connection/setup and reads have deadlines; audio commands use async subprocesses
with deadlines and kill-on-drop. None use the blocking pool reserved for settings
and compositor readiness (N0053). Missing services publish unavailable, without
an error loop. Safe mode retains its existing service suppression.

## Acceptance evidence

Private fake pactl and private D-Bus tests cover topic shape and event propagation.
The N0053 hung-backend guard additionally leases all three topics, verifies their
unavailable snapshots, starts a hanging pactl and checks that status stays responsive.
It retains the READY deadline of two seconds with nonresponding buses and tools.
A second black-box test provides a functioning private bus with a UPower property
getter that never replies, verifies the getter was invoked and requires READY
within two seconds. All child processes and buses are task-private.
The full CI run also exercises existing dashboard fixtures. Their startup wait
now observes method readiness instead of sleeping 200 ms, and their paused focus
countdown checks the measured pause interval instead of assuming calls take under
two seconds. Neither changes product behavior or the daemon READY deadline.
Validation runs the full desktop job in an Arch CI container, including all Rust,
source, parity and native gallery checks. Exact commands and results are recorded
in the unit report. No laptop builds or live-device tests are permitted.

## Deferred audit findings

`dashboard.calendar` and `dashboard.media` remain degraded reserved-only rows.
They are queued as N0060 and are outside this unit. Weather belongs to N0057.
