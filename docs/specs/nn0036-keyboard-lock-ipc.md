# NN0036 — Caps Lock OSD from the compositor event stream

Status: implemented 2026-11-13.

## Problem

The daemon's OSD session read `/sys/class/leds/*::capslock/brightness` every
500 ms to show the Caps Lock OSD (N0305: one of the remaining idle wakers).
sysfs LEDs never notify. The coordinator's review of N0305 (118536a84) also
found a fallback bug: kernfs sends no inotify CREATE/DELETE, so once the picked
LED vanished (a USB keyboard unplugged), `capslock_path` became `None`, the
tick was dropped, and Caps Lock had no OSD until the daemon restarted.

## Design

- nacre-niri (`nacre` branch): event `KeyboardLockStateChanged { caps_lock,
  num_lock }`, kept in the IPC `KeyboardLayoutsState`, so a stream starts with
  the current state and then gets one event per change. Sources: smithay's
  `led_state_changed` seat callback, startup, and the two num-lock restores
  after keymap changes (`set_modifier_state` changes the LEDs without the
  callback). Capability `keyboard_lock_state` in `NacreCapabilities`.
- Only streams that include collection events (`EventStreamWithCollections`,
  `NacreOverviewHoverStreamWithCollections`) carry it. Plain `EventStream`
  clients built on stock niri-ipc reject unknown variants; nacre-shell's niri
  backend is one, and the initial snapshot would have broken its stream for
  good. Nacre's daemon always gets a collection stream from nacre-niri.
- Nacre daemon: the compositor service already reads that stream; it hands
  `caps_lock` to `osd::capslock`, a `watch` value on the hub, and resets it to
  `None` when its stream ends. No new connection, task or process.
- OSD session: while the value is `Some`, the LED is never read and nothing
  ticks (unless night light can't be watched). The first compositor value is a
  silent baseline; later changes show the OSD. When it goes back to `None`, the
  LED takes over from a silent baseline.
- The switch keys on the event, not on the capability: only capable builds
  send it, and a timed-out capability probe can then neither leave the OSD
  stale nor stop it.

## Fallback (stock niri, Hyprland)

- A known LED is read every 500 ms, as before. If the read fails, the same read
  looks for another LED.
- Without a known LED, a 2 s tick keeps looking (keyboard plugged in later).
- The LED is picked deterministically: the lowest `inputN::capslock` (the
  keyboard registered first, normally the built-in one), then other names in
  order. A plain name sort put `input12` before `input3`.
- Removed: the inotify watch on `/sys/class/leds` and its `cap_led_changed`
  path. kernfs never fires them; the night-light watch is unchanged.

## Checks

- nacre-niri: fmt (nightly), clippy, `cargo test -p niri-ipc`, `cargo test
  --lib` (new `tests::keyboard_lock`: one event per change, none on the plain
  stream; snapshot gating; state replication), in a dev-1 Arch container;
  hosted CI on the pushed commit.
- Nacre: osd tests (new: LED unplugged → rediscovery, LED order, compositor
  source replaces the LED, tick table), `cargo test -p nacre-helper`, fmt,
  clippy; hosted CI.

## Idle wakeups

dev-1 container, debug `nacre-helper daemon` with one client subscribed to
`osd`, a fake niri socket and a fake sysfs LED tree (`NACRE_SYS_ROOT`); arms
differ only in what the fake compositor sends. Daemon context switches per
second (all threads), 20 s settle, 3 alternating rounds of 60 s, median:

| Arm | ctx/s | Runs |
| --- | ---: | --- |
| nacre-niri: `KeyboardLockStateChanged` | 1.08 | 1.05, 1.08, 1.28 |
| stock niri, Caps Lock LED present (500 ms read, as before) | 7.22 | 7.13, 7.22, 7.23 |
| stock niri, no LED (2 s re-discovery, new) | 2.60 | 2.57, 2.60, 2.60 |

Each 500 ms LED read costs about 3.6 switches (timer, offload thread and
hand-back), so nacre-niri removes about 6 of the daemon's idle switches per
second. The no-LED re-discovery tick costs about 1.5 per second; before, that
case did not tick but never found a keyboard plugged in later.
