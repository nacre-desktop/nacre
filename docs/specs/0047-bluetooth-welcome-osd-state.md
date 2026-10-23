# N0047 — Bluetooth Welcome fix, OSD resilience, Bluetooth state and controls

Status: **Slice 1 implemented (urgent fix); slices 2 and 3 in progress; unpublished.** Consumers: Bead B0051 (OSD), B0055 (Bluetooth UI).

## Slice 1 — Welcome refused whenever BlueZ was on the bus (urgent)

N0045 advertised `bluetooth.pair` with a 155 000 ms timeout, but the protocol accepts method timeouts of `1..=120000`
(`Message::validate`). With BlueZ on the system bus the daemon's Welcome therefore failed validation for every client.
CI has no BlueZ, so nothing caught it.

- Fix: `PAIR_TIMEOUT` is 110 s; the advertised timeout is `PAIR_TIMEOUT + 5 s = 115 000 ms`. The user-visible prompt
  timeout (60 s) is unchanged.
- Guard tests: `hub::tests::every_static_method_set_passes_welcome_validation` validates a Welcome built from every
  module that advertises methods without a backend; the Bluetooth fixture validates the full `Hub::welcome()` once the fake
  BlueZ is present, and the network fixture's `advertised()` helper validates every Welcome it receives (all network
  backends faked). Any advertised timeout outside `1..=120000` fails these tests.
