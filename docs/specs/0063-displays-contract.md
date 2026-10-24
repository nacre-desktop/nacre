# N0063 — Displays: `displays` topic and the confirm-or-revert methods

Status: **Implemented; pending lead review** (Claude worker, branch `task/n0063-displays-20261024`). Consumer: Bead B0089 (Settings
Displays page). Additive v1, outside daemon safe mode. QML unchanged. Types are `nacre-protocol::displays` (MIT OR
Apache-2.0); schemas, Luau types and the golden transcript are generated (`protocol/schema/{displays-state,display-apply}.json`,
`protocol/luau/displays.d.luau`, `protocol/fixtures/displays.jsonl`). The daemon side is `crates/nacre-helper/src/daemon/displays.rs`.
**Safety:** every test uses fakes (a fake niri socket that keeps its own layout, a fake systemd on a private session bus). No
unit, test or measurement ever touched a real output of any owner machine.

## What the QML does today (read from `origin/main` 06a75b85)

`NacreDisplaysPage.qml` (Settings > Displays), driven by the `DesktopSettings` singleton, i.e. the helper `desktop-settings`
(or, with the daemon flag, the daemon method `settings.desktop` with the same arguments):

* **Shown.** Primary-display chips (one per output, only with more than one output). "Extend right", "Extend left", "Mirror"
  (only with two or more outputs; "Mirror" hidden when `monitorMirroring` is false, which is niri). Per output a section titled
  `{name} · {width}×{height} · {round(refreshRate)} Hz` with the note "Display changes revert after 20 seconds unless you keep
  them.", a **Scale** select (100, 125, 150, 175, 200, 250, 300 %) and a **Resolution and refresh rate** select listing the
  output's `availableModes` strings (searchable above 8 entries; the current one is the entry that starts with `WxH@` and has a
  rate within 0.01 of `refreshRate`). While a change is pending: "Keep this display change?" with **Keep** and **Revert**.
  A caption shows the controller's `message`/error. The page text says "Display transactions are unavailable on this compositor"
  when `monitorConfiguration` is false and, on niri, "Output mirroring is unavailable on niri".
* **Not in QML** (so not in the contract's UI): no arrangement canvas or dragging, no rotation, no enable/disable, no VRR control,
  no countdown (the 20 s is a fixed sentence; the page does not tick). The outputs' make/model and `transform`/position are
  not shown. They are in the topic because the compositors report them (the task's list) and the Bead page may use them.
* **Blocked** (all controls disabled) while `monitorConfiguration === false`, a change is pending, or the controller is busy.
* `NacreControlDisplays.qml` (control centre) is **brightness only**: a slider per output (`NacreBrightness.monitors`) and the
  keyboard backlight slider. Both already exist as daemon contracts (N0036 `brightness.*` / `keyboard_light.*`, N0046
  `brightness`/`keyboard_light.set`, the `controls` topic); N0063 adds nothing for it.
* `DisplayRecovery.qml` + `display-recover.py` are **not output configuration**. They rebuild the QML shell's per-screen surfaces
  after the output geometry or device pixel ratio changes (settle 700 ms, save and restore panel state, fallback = restart
  `nacre-shell` and re-open the saved views). Bead has no QML surfaces to rebuild: its layer surfaces follow `compositor` output
  events natively. No contract is needed; `display-recover` stays a QML-shell helper that dies with QML.

### How a change is applied (existing code, reused unchanged)

The page calls the controller `desktop_settings::Settings` (`crates/nacre-helper/src/desktop_settings.rs`):

* `display <extend-right|extend-left|mirror> <primary>`: `display_plan` puts the primary at (0,0) and the other outputs next to
  it using logical width (`size / scale`, rounded half-even, height for odd `transform`), to the right or left; mirror sets
  `mirrorOf` the primary at x=0.
* `display-edit <output> <scale> <mode|"">`: scale must be one of the seven values; `mode` must be one of the output's
  `availableModes` and `WxH@RHz`; sets width, height, refresh rate, then scale. Other outputs keep their positions.
* Both: refuse while a journal exists ("Keep or revert the current display change first"); write the journal
  `display-pending[-niri].json` `{token, previous (saved displays), monitors (live records before)}` with the fsynced writer;
  **arm** the independent rollback (`Native::arm`: transient user unit `nacre-display-revert-<token>.service` running
  `nacre-helper desktop-settings rollback-after <token>`, which sleeps 20 s and then `revert <token>`, through the daemon when it
  runs and directly otherwise; if arming fails the journal is removed and nothing is changed); then `configure_monitors` through the
  compositor backend and persist.
* **Compositor backends** (runtime IPC *and* files): niri sends `Output` IPC actions per output (`Mode`, `Scale`, `Transform`,
  `Position`) and, if any is refused, re-sends the previous values; Hyprland runs `hyprctl eval` with one `hl.monitor({...})` line
  per output. Both then persist through `Settings::persist`: `desktop.json` (`displays`, per-backend `compositorDisplays`) and the
  compositor's generated monitor configuration (niri desktop config KDL, Hyprland desktop Lua), so the layout survives a restart.
* `confirm` removes the journal (the armed unit then finds nothing to do). `revert [token]` re-applies the journal's `monitors`,
  restores the saved displays and removes the journal; a wrong token does nothing. A failure after the journal was written makes the
  controller revert by itself. `init` (installer) also reverts a leftover journal.

## The contract

Capability class `desktop`; method timeout 30 s (the settings owner's); advertised only while the `settings` service runs, the
compositor topic is available and `monitorConfiguration` is not false (niri says true, Hyprland says nothing, QML treats both as
configurable).

### Topic `displays` (`DisplaysState`)

`health`, `can_configure` (`monitorConfiguration` is not false), `can_mirror` (`monitorMirroring` is not false: niri false),
`scales` (`[1,1.25,1.5,1.75,2,2.5,3]`, so no client hard-codes them), `outputs` (at most 16) and `pending` (absent when idle).
`DisplayOutput`: `name`, `make?`, `model?`, `enabled`, `width`/`height` (current mode, physical px), `refresh_hz`, `mode?` (the
entry of `modes` that is current, resolved by the daemon with QML's rule), `modes` (the `WxH@RHz` strings, at most 256, malformed
ones dropped), `scale`, `x`/`y` (logical), `transform` (0..7), `vrr?`.
`pending {seconds:20, deadline_ms}`: unix ms at which the armed rollback fires (journal mtime + 20 s, so it survives a daemon
restart; a client counts down against its own clock, same machine).

Sources: the outputs are the controller's live `monitors` (what `settings.desktop state` returns, exactly what QML showed) joined
by name with the `compositor` topic for make, model and VRR; before the first live answer they come from the `compositor` topic.
The daemon asks the controller for a fresh live answer when the compositor outputs change (hotplug, a change made elsewhere) and
after every method here; the armed unit's revert (which goes through the same controller) republishes the topic, so the countdown
ends on every client. Make/model/vrr: niri rows gain `make`, `model`, `vrr` (`vrr_enabled`) only when niri reports them (existing
frozen fixtures do not, and are unchanged); Hyprland's `monitors -j` already has them. Disabled outputs are not listed by either
backend's `monitors` (QML ignored them too), so `enabled` is true in practice; it is there for compositors that list them.

### Methods

* `display.apply {output, arrangement? | scale?, mode?}` -> `{pending {seconds, deadline_ms}}`. Exactly one of `arrangement`
  (`extend-right|extend-left|mirror`, `output` = the primary) or `scale` (one of `scales`; `output` = the target, optional `mode`
  from its `modes`). It is the controller's `display` / `display-edit` and therefore **always** a transaction: it returns with the
  new layout applied and persisted and the rollback armed. Unless `display.confirm` arrives within 20 s the unit reverts it.
* `display.confirm {}` -> `{changed}`: keep the pending layout (`changed:false` when nothing was pending).
* `display.revert {}` -> `{changed}`: revert now (QML "Revert").
* Errors: `validation` for a malformed request or a controller refusal that keeps its fixed sentence (disconnected display,
  unavailable mode, unsupported scale, "Output mirroring is unavailable on this compositor", fewer than two displays for an
  arrangement), `busy` while a change is pending ("Keep or revert the current display change first"), `unavailable` when the
  rollback could not be armed (nothing was changed) or the compositor/settings service is down, `stale_revision` is retried once
  internally, any other controller failure is `io` with one generic message ("The display change failed and was reverted"; no
  system error text or path is relayed).
* The page's "blocked while busy/pending" rule is the client's: `pending` in the topic and `busy` errors.

### Why not more

The contract does not invent features QML lacks (rotation, enable/disable, VRR toggles, free positioning). Those need new
controller code with the same arm/apply/persist discipline and an owner decision; `transform`, `x`, `y`, `vrr` are readable so
a later unit can add the writers without a topic change.

## Startup (N0053)

The `displays` service is spawned beside the other Settings-page services, publishes an empty `health.available:false` topic at
once and then waits on topic notices; it never gates READY. Its first backend work (a `settings.desktop state` call, which only
queries the compositor) starts only after the compositor topic is available, i.e. after the daemon's own READY condition. It
spawns no process and uses no `spawn_blocking` of its own; the controller call runs on the pool the settings owner already uses
for compositor work (the same class N0053 left there on purpose).

## Tests

* `crates/nacre-protocol/tests/displays.rs` + unit tests: transcript replays through the mirror and validates, schemas/types are
  published, the apply shape (arrangement xor edit, scale list, mode syntax, name bounds), methods stable.
* `daemon::displays::tests` (9): niri and Hyprland-shaped rows -> outputs (current mode, malformed/long mode lists, control
  characters, disabled), live-over-compositor precedence and stale handling, method advertisement gates (safe mode, missing flag,
  Hyprland without the flag), output signature (windows ignored), request -> controller arguments, error mapping, deadline.
* `crates/nacre-helper/tests/displays.rs` (real daemon, private HOME, fake niri socket with its own mutable layout, fake systemd on a
  private session bus): topic contents; refusals (scale, mode, unknown output, mirror on niri, mixed shape) change nothing; apply
  (journal, unit `nacre-display-revert-<token>.service` with `desktop-settings rollback-after <token>`, niri `Output` actions seen,
  topic pending with the same deadline); busy while pending; a wrong-token revert does nothing; the right-token revert (the exact
  command the armed unit runs after its wait) restores the layout and clears the countdown on the client; manual revert; confirm
  persists to `desktop.json` `compositorDisplays`; extend-left positions; a refused arming leaves the fake niri untouched.
* The Hyprland apply path is the unchanged controller code covered by its frozen references
  (`desktop-settings-controller-reference.json`, both backends); N0063 only adds Hyprland row parsing to its unit tests. A live
  Hyprland check was **not** done (no owner machine is touched).
* The 20 s wait itself is not slept in tests: the unit's argv is asserted and its action is run directly.

## Temporary duplicates and expiry

None added. `settings.desktop` with `display*`/`confirm`/`revert` stays (QML uses it); it and the `display-pending` helper paths
are removed with QML after the Bead switch is trusted (the existing Group F cleanup), not before.

## Known gaps

No countdown tick in QML (the Bead page adds one from `pending.deadline_ms`). No enable/rotate/VRR/free-position writers (above).
After a daemon restart *during* a countdown the topic shows the journal as pending from its mtime, but the daemon does not itself
revert; the armed user unit does, and a leftover journal (e.g. the whole session died) blocks new changes until Keep/Revert, as in QML.
