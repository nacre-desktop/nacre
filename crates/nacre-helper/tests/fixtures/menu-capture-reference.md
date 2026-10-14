# Capture reference

`menu-capture-reference.json` was frozen from the current Python menu helper
before implementing the native capture path. `freeze_menu_capture.py` uses a
disposable HOME, a fixed date and injected tools/compositor; no live capture runs.
The Python source SHA-256 is
`d74ca0b2a6d6d41a173b399fa5fbf876472d9ef8b18572d604ea8b20108eb958`;
the capture driver refuses changed sources and existing reference destinations.
The 11 cases compare results, arguments, image bytes and written filenames.

Intended security difference: captures are reserved at 0600 inside a repaired
0700 screenshot leaf directory; linked destinations and filename collisions are
refused, following `docs/development/security-review.md#fix-during-port`.
The reference's original 0644 modes remain unchanged and explicitly checked.

Intended transport difference: Hyprland uses its socket; niri uses `niri-ipc`
requests on one persistent socket and waits for close/rename file events instead
of polling. Unexpected socket errors surface; replies are capped at 2 MiB.
See [niri IPC](https://docs.rs/niri-ipc/26.4.0/niri_ipc/) for the wire contract.

Intended bound: the previously unbounded grim/clipboard calls have a 5 s deadline;
interactive selection keeps 180 s. Native niri completion retains its 5 s limit.
Native subprocess failure wording differs; successful outputs are unchanged.

Menu state replaces `systemctl is-active` with exact private-fixture GetUnit and
Properties.Get calls. Active/reloading/refreshing match systemctl's success
states; unavailable state remains false, with unexpected errors sent to stderr.
