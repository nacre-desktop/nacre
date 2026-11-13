# N0211 — Power settings

Status: implemented; focused checks passed; ready for lead review. Local commits only.

The five previously planned power settings become visible on supported Niri and
Hyprland sessions with the managed daemon behaviours enabled. Battery saver also
requires a system battery. Settings remain
in the registry and `power.json`; no root configuration or new system service.

- Saver acts only while enabled and on battery: still wallpaper (including the
  dashboard preview), reduced shell/app motion, and a same-resolution mode near
  60 Hz when advertised by the output. Existing compositor generators validate
  mode changes. Saved display and animation preferences are not rewritten. A
  session-scoped rate journal restores the prior rate on AC/disable and survives
  daemon restarts; a new compositor session uses its normal display configuration.
- Idle dim and sleep use Hypridle. Zero means never and remains the default to
  preserve the previous no-timeout policy. Nonzero dim must precede nonzero sleep.
  Dimming reuses guarded backlight/DDC controls, saves the previous brightness,
  and restores it on activity. Displays without writable brightness remain unchanged.
  Existing private idle listeners and sleep-lock policy are preserved; there is
  no separate stored lock-on-idle boolean in this checkout (the row opens private policy).
  Source Niri idle controls stay hidden until its existing password-unlock acceptance
  gate passes; packaged Niri uses its installed lock service.
- Lid and short power-button actions use compositor bindings with the existing
  user-session hardware-event inhibitor. Suspend/lock remain available when locked.
  Menu opens the existing power menu. No logind configuration is edited.

Settings apply live; installing the new session include/unit wiring requires a
session restart. The lead owns deployment and physical/live validation. No
performance claim or live-desktop change is part of this unit. Additional steady
state budget: at most 1 MiB (subscriptions and small journals; not measured here).
No temporary duplicate implementation is introduced. Fixed lock bindings remain
the safe startup fallback until the managed policy loads.

Primary contracts: [Niri switch events](https://niri-wm.github.io/niri/Configuration%3A-Switch-Events.html),
[Hyprland switch bindings](https://wiki.hypr.land/configuring/core/binds/switches/),
[systemd inhibitor locks](https://github.com/systemd/systemd/blob/main/docs/INHIBITOR_LOCKS.md).

Validation (prepared dev-1 container): helper library **1,045 passed / 3 ignored**;
whole shell binary **887 passed / 4 ignored** with six threads. Generator/setup
Python suites: **79 passed**; Niri 26.04 validated all nine hardware-policy pairs.
Cargo fmt, touched-crate Clippy, compositor boundary and Ruff 0.16.10 passed.
The before/after and difference images were inspected; only the power golden changed.
Test infrastructure now uses a same-process cancellable fake and a 30-second
Wayland startup hang guard; protocol assertions remain unchanged.
