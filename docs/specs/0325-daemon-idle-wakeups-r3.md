# N0325 — daemon idle wakeups, round 3

**Status:** Implemented (daemon half); the bar's lease switch is handed to Shell
(`.lead/shell-bar-lease.patch` in the N0325 worktree)

Follows [N0305](0305-daemon-idle-wakeups.md) and [N0321](0321-daemon-scoreboard-fixes.md)
for finding 4 of the [scoreboard](../research/noctalia-scoreboard.md).

## Where the idle wakeups came from

dev-1, `tools/bench/noctalia` nacre arm in `nacre-ci/arch-nacre-session:current`, stock
niri, software rendering, a 300 s idle window 30 s after the bar maps (profile
`30/330/2/10`), per-thread voluntary + involuntary context switches. Attribution from
`strace -f -tt -T` over 120 s of the idle daemon (diagnostic arm, not in the numbers).

| Waker | Thread | Base rate | Cause |
| --- | --- | ---: | --- |
| Control-centre poll | main + `nacre-offload` | ~3.5/s | The bar leases `controls` for its lifetime to read `controls.recorder.recording` (B0199). A lease runs the control-centre session: every 2 s a `tokio` interval (two timer-wheel wakeups per tick) and two offload handoffs (light scan, then rfkill), every 10 s two child processes (`systemctl --user is-active`, `nacre-helper control-tools state`) with their reader threads. |
| Caps Lock LED readback | `nacre-offload` | 2.0/s | Stock niri and Hyprland send no lock-key state; the LED is read every 500 ms (N0321 moved it off the main thread). nacre-niri sends `KeyboardLockStateChanged` (NN0036) and nothing polls. |
| Brain git sweep | main + a blocking thread | ~0.1/s | 30 s interval: vault path, then `brain_git::flush` on a fresh blocking thread (the runtime keeps idle blocking threads 1 s). |
| Calendar clock | main + `nacre-offload` | ~0.05/s | 60 s interval re-projects "now" (events that ended, the next one). |
| Everything else | audio, fast lane, PipeWire `module-rt` | 0.04/s | No timers. |

Timer-wheel note: a `tokio` timer is placed by the bits its deadline shares with the clock,
so a 2 s interval is cascaded from the 64 ms level and, every other tick, from the 4.096 s
level: two or three main-thread wakeups per tick. A `std` thread blocked in
`recv_timeout` wakes once.

## Changes

1. **Control-centre fast ticks on one offload thread.** After a full or slow read, one
   offload job runs the next four 2 s ticks itself (`fast_ticks`: wait on a channel with
   the tick as timeout, scan lights, read rfkill) and returns only on a change or when the
   slow read is due. The main thread no longer wakes on unchanged ticks. A `changed()`
   refresh or a compositor output change ends the job at once (the session drops the
   channel's sender) and re-reads. The scan keeps N0067's guard: while an earlier scan is
   stuck the job keeps the prior lights, and a job held by a stuck scan is abandoned after
   the remaining ticks + 2 s and the slow read runs. Freshness is unchanged: fast members
   every 2 s, slow members (units, night light, appearance) every 10 s.
2. **`recorder` topic, event-driven** (lead decision (b), daemon half). `{recording}` from
   the user manager: the session subscribes to `org.freedesktop.systemd1`, reads the
   transient `nacre-screen-recording.service` unit's `ActiveState` and re-reads it on each
   `PropertiesChanged` on the unit's fixed object path. Snapshotted on subscribe, no work
   without subscribers, no polling; with no user manager on the bus it stays false until
   one takes the name. `controls.recorder` is unchanged. Protocol: `recorder.rs`
   (schema, Luau type, fixture) and the registry lines.
3. **No `systemctl` child without a user manager.** The slow read asked `systemctl` for
   the unit states even when their members are left out (no user bus); it now skips them.

Shell half, not committed here: `.lead/shell-bar-lease.patch` switches
`with_bar_recording` to lease `recorder` and reads `data.recording` (2 files, 4 lines).

## Results

Idle window 300 s, per-thread context switches per second, dev-1, two runs per column.
"Fix" is this unit's daemon with the base shell (the bar still leases `controls`);
"fix + bar patch" adds the Shell patch above (the bar leases `recorder`).

| Thread | Base 86433e4a4 | Fix | Fix + bar patch |
| --- | ---: | ---: | ---: |
| `nacre-helper` (main) | 2.76, 2.67 | pending | pending |
| `nacre-offload` | 3.66, 3.89 | pending | pending |
| other daemon threads | 0.04, 0.04 | pending | pending |
| **daemon total** | **6.46, 6.60** | pending | pending |
| shell (for reference) | 1.93, 1.99 | pending | pending |

Daemon idle CPU at the base: 0.054 and 0.062 % of one core.

## Remaining periodic wakeups

| Waker | Thread | Rate | Why it stays |
| --- | --- | ---: | --- |
| Caps Lock LED readback (stock niri, Hyprland) | `nacre-offload` | 2.0/s | No compositor event and no kernel notification for an input LED; one 500 ms read keeps the OSD latency. nacre-niri: none (NN0036). |
| Control-centre slow read while `controls` is leased | main + `nacre-offload` + 2 children | 1 per 10 s | Unit states and night light have no event source wired yet; next unit. With the bar patch the lease exists only while the control centre (or its 120 s keep-alive) is up. |
| Control-centre fast ticks while leased | `nacre-offload` | 0.5/s | sysfs lights and rfkill; next unit. |
| Brain git sweep | main + blocking thread | 1 per 30 s | Commits edits made outside Nacre (an editor in the vault); the queue's 120 s quiet period is the user-visible bound. |
| Calendar clock | main + `nacre-offload` | 1 per 60 s | "Now" moves: events that ended drop out at the next minute. |

## Next unit: event-driven control centre

The control-centre session still polls while any client leases `controls` (the open
control centre, or Bead's 120 s closed-panel keep-alive): sysfs every 2 s and two
children every 10 s. Event sources exist for most members: backlight `actual_brightness`
is `sysfs_notify`'d by the backlight core (inotify `IN_MODIFY` on kernfs), `/dev/rfkill`
reads block for events, UPower signals keyboard-light changes, the user manager signals
unit states (as `recorder` now does), and the night-light records are already watched by
the OSD. DDC stays on explicit refresh. `control-tools state` could become an in-process
call, but it runs compositor module calls that the child's 20 s timeout bounds today, so it
belongs with that rewrite, not here.

## Checks

- dev-2 (`nacre-ci/arch-nacre`, rustfmt 1.10): `cargo fmt --all` (three rewraps, applied) and
  `cargo run -p nacre-protocol --example export` (generated files unchanged) passed; the first
  `clippy -p nacre-helper -p nacre-protocol --all-targets -D warnings` found one compile error
  (fixed).
- Open, queued: clippy, `cargo test -p nacre-protocol`, `cargo test -p nacre-helper`
  (including `control_center::tests::recorder_follows_the_user_manager_without_polling`, a
  private `dbus-daemon` with a fake user manager), the "Daemon raster memory regression" step,
  and the bench "after" arms.
