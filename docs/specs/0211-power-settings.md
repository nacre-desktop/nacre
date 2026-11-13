# N0211 — Power settings

Status: review fixes implemented; renewed focused checks pending. Local commits only.

The five previously planned power settings become visible on supported Niri and
Hyprland sessions with the managed daemon behaviours enabled. Battery saver also
requires a system battery. Settings remain
in the registry and `power.json`; no root configuration or new system service.

- Saver acts only while enabled and on battery: still wallpaper (including the
  dashboard preview), reduced shell/app motion, and a same-resolution mode near
  60 Hz when advertised by the output. Niri validates generated output config;
  Hyprland commits only the mode through output-management.
  Saved display and animation preferences are not rewritten. A
  session-scoped rate journal restores the prior rate on AC/disable and survives
  daemon restarts; a new compositor session uses its normal display configuration.
- Idle dim and sleep use Hypridle. Zero means never and remains the default to
  preserve the previous no-timeout policy. Nonzero dim must precede nonzero sleep.
  Dimming reuses guarded backlight/DDC controls, saves the previous brightness,
  and restores it on activity. Displays without writable brightness remain unchanged.
  Existing private idle listeners and sleep-lock policy are preserved; there is
  no separate stored lock-on-idle boolean in this checkout (the row opens private policy).
  Niri idle controls (source and packaged) stay hidden until the existing
  password-unlock acceptance gate passes. This unit keeps packaged locking
  explicit-only; enabling its idle/sleep hooks remains an owner decision.
- Short power-button actions use compositor bindings behind the existing
  power-key inhibitor. Lid "suspend" stays with logind (so HandleLidSwitchDocked
  keeps a docked laptop awake); only "lock" and "nothing" make the daemon hold a
  `handle-lid-switch` inhibitor over D-Bus while the compositor binding acts.
  A new inhibitor is acquired only when compositor control exists and any
  necessary reload succeeded; otherwise logind retains lid ownership.
  Before the daemon writes a policy, the seeds equal the default (button locks,
  lid left to logind). Hyprland deliberately has no base suspend lid binding:
  without a lid inhibitor, a compositor suspend would bypass logind's docked rule.
  Hyprland matches switch binds by device name; "Lid Switch" is the ACPI lid name. Suspend/lock remain available when locked;
  "menu" falls back to suspend while hyprlock is running, since the menu cannot
  open over the lock screen. No logind configuration is edited.
- The idle service restarts (and the compositor reloads) only when the generated
  file content changed, because hyprlock lives in the idle unit's cgroup; a failed
  restart is started again rather than left stopped. Power loads before host so
  host overrides still win. Saver failures retry only when their inputs change,
  Hyprland saver mode changes use `wlr-randr>=0.5.0` mode-only transactions,
  retaining the complete configured HDR, bit-depth and VRR rule (including
  description/wildcard matches), rather than reconstructing it from live rows.
  Stale dim records for unplugged displays are dropped.

Settings apply live; installing the new session include/unit wiring requires a
session restart. The lead owns deployment and physical/live validation. No
performance claim or live-desktop change is part of this unit. Additional steady
state budget: at most 1 MiB (subscriptions and small journals; not measured here).
No temporary duplicate implementation is introduced. Fixed lock bindings remain
the safe startup fallback until the managed policy loads.

Primary contracts: [Niri switch events](https://niri-wm.github.io/niri/Configuration%3A-Switch-Events.html),
[Hyprland switch bindings](https://wiki.hypr.land/configuring/core/binds/switches/),
[systemd inhibitor locks](https://github.com/systemd/systemd/blob/main/docs/INHIBITOR_LOCKS.md).
Mode-only preservation follows [wlr-randr 0.5.0 changed-field commits](https://gitlab.freedesktop.org/emersion/wlr-randr/-/blob/v0.5.0/main.c)
and [Hyprland's monitor-rule overlay](https://github.com/hyprwm/Hyprland/blob/v0.56.2/src/config/shared/monitor/MonitorRuleManager.cpp).

Original implementation validation, before these review fixes (prepared container): helper library **1,045 passed / 3 ignored**;
whole shell binary **887 passed / 4 ignored** with six threads. Generator/setup
Python suites: **79 passed**; Niri 26.04 validated all nine hardware-policy pairs.
Cargo fmt, touched-crate Clippy, compositor boundary and Ruff 0.16.10 passed.
The before/after and difference images were inspected; only the power golden changed.
Test infrastructure now uses a same-process cancellable fake and a 30-second
Wayland startup hang guard; protocol assertions remain unchanged.

Review follow-up: failed profile writes are remembered until their inputs change;
regressions cover failed idle restart recovery and reload, failed profile writes
across compositor events, unplugged dim records, and manual-only packaged locking.
Owner question (10): verify real password unlock before enabling packaged/source
Niri idle and sleep-lock; packaged locking remains explicit-only in this unit.
No live validation or deployment is authorized here.
