# N0031 — Semantic bar action routes

Status: **Done**. N0030 is already reserved for Brain daemon hosting in
N0024 and the queue; N0028/N0029 are concurrent AI units. Consumer: Bead B0022
(`docs/specs/0022-bar-composition-lifecycle.md` on Bead origin/main).

## Boundary

The daemon publishes semantic mutations, never a shell-command-catalog executor.
Bead owns its own transient views: launcher/dashboard, calendar, status detail,
peek, controls and the session menu are local view operations. Their local
arguments include the originating compositor output name and an anchor rectangle
in that output's logical coordinates. Anchor rectangles describe presentation,
not daemon state. Hover, dismissal and popup handoff stay local; output removal
closes that output's views. No Nacre-internal view name becomes a protocol name.
Opening a power menu is local; executing the selected session action is a daemon
mutation. The QML shell and its current command catalog remain unchanged.

Output arguments use the exact output names from the compositor snapshot, not
surface indexes or whichever output happens to be focused. Calls validate live
output and target membership. Workspace activation accepts either a published
opaque ID string or a numeric slot in 1..255. An ID string is never interpreted
as a slot. On niri, a slot resolves to the existing workspace on that output;
on Hyprland, an empty numeric slot can be created on that output. An existing
workspace belonging to another output is refused. Consumers discard pending actions on output loss
or transport-generation change and never replay mutations after reconnect.

## Service audit

At the starting revision only `network.set_wifi`, `network.connect` and
`network.disconnect` have registered core-service mutation handlers. Bluetooth,
audio and power-profile payloads in `CoreCall` are reserved schemas; they are
not runtime capabilities. Battery is readback-only. Bead must inspect the welcome
method registry and hide/disable unavailable mutations. No schema alone grants
authority or proves a backend exists.
The registry is per connection: after a service-readiness change in the `daemon`
topic, reconnect to obtain a fresh welcome before enabling newly available
methods. Never replay pending mutations on that reconnect.

## Consumer method map

All new methods are version 1. IDs come from the current daemon topic; do not
construct IDs from a native compositor address or a D-Bus service name.

| Bar action | Method / parameters | Scope |
| --- | --- | --- |
| Workspace click | `workspace.activate {output, workspace}` | Exact output; published ID string or numeric slot 1..255 |
| Workspace/title scroll | `workspace.scroll {output, direction:"next"\|"prev"}` | Adjacent workspace on that output |
| Window focus | `window.focus {output?, window}` (`output` optional since N0040) | Window ID; optional exact-output guard |
| Execute session choice | `session.request {action, confirmed}` | `lock`, `logout`, `suspend`, `reboot`, `shutdown`; global session |
| Tray primary/secondary/context | `tray.activate`, `tray.secondary_activate`, `tray.context_menu` with `{item,x,y}` | Current tray item; global logical anchor coordinates |
| Tray scroll | `tray.scroll {item,axis,delta}` | Current item; `horizontal` or `vertical`, signed bounded delta |
| Brightness click/scroll | `brightness.set {output,percent}`, `brightness.adjust {output,delta}` | Percent / percentage-point change on that output |
| Audio scroll | `audio.adjust_volume {delta}` | Current default sink, normalized change in [-0.2,0.2] |
| Audio toggle | `audio.toggle_mute {}` | Current default sink |
| Bluetooth power | `bluetooth.set_enabled {enabled}` | Current default controller |
| Wi-Fi power | Existing `network.set_wifi {enabled}` | Existing NetworkManager ownership/admission rules |

Default-device audio/Bluetooth actions deliberately do not accept node or adapter
identifiers: the reserved per-device contracts require topic owners that do not
exist at the starting revision. These bar actions express the default-device
intent directly. Device selection, pairing, routing and power profiles retain
their reserved contracts and are outside the bar action inventory.

Success `{changed:true}` records accepted action execution, not authoritative
state readback. Observe service/compositor updates to render the resulting state.
After a timeout or any `dispatched:true` error, refresh state rather than retrying
a toggle or relative operation. Unavailable owners and stale IDs fail closed.

Tray IDs bind the session-bus GUID, current unique D-Bus owner and object path.
Restarting a bus cannot revive IDs when unique connection names are reused. Each action checks
the published item and resolves the watcher's current registration again, then
addresses the unique owner directly. An existing SNI watcher remains untouched;
the daemon offers a bounded fallback watcher only after a non-queuing name
request succeeds. Registration verifies the caller owns the advertised service.
The daemon registers its own connection as a host against either watcher; the
fallback reports actual live host registrations and clears them on owner loss.
Owner loss clears registrations and the daemon snapshot; no watcher takeover or
notification-server ownership change occurs. Context-menu requests invoke the
application's SNI `ContextMenu`; this unit does not add a D-Bus-menu renderer.
Tray scroll accepts raw signed SNI deltas in [-1200,1200], with either axis.

Brightness discovers its device from current compositor outputs and native
light-device discovery; callers never supply a sysfs path, DDC bus or device
descriptor. DDC writes reuse connector/EDID identity checks. Internal-panel
backlight mapping requires one internal output. Audio and brightness mutations
reject concurrent admission rather than queueing relative actions. Tool work
runs off the daemon event loop; input/output limits and deadlines remain bounded.
Brightness's default method deadline is 30 seconds to cover DDC discovery and
identity-checked writes; other new methods default to 10 seconds.

Hyprland workspace focus uses `on_current_monitor=false` after checking the
target's owning output, preserving the workspace on that output. The legacy
QML path retains its existing default. The upstream
[dispatcher contract](https://wiki.hypr.land/configuring/core/dispatchers/)
defines monitor focus and workspace focus as separate variants. Scrolling uses
the nearest existing ordinary workspace in index order on the requested output,
skips index gaps and special workspaces, and does not wrap; the adapter reads
the active workspace afresh. A shell that filters or synthesizes its visible
row computes the next visible target locally and uses activation by ID or slot.

This unit adds missing bar action contracts and reuses existing native compositor,
session-command and light-device implementations. Per-method runtime ownership,
safe-mode refusal, strict bounded parameters and sanitized errors are required.
Non-lock session actions require explicit confirmation after the local menu
selection; an omitted/false confirmation never dispatches them. No arbitrary
executable, D-Bus destination, device descriptor or compositor command is accepted.

## Acceptance and expiry

Verify generated schemas/Luau freshness and golden call transcripts; output and
target mismatch, malformed/oversized/unknown fields, confirmation refusal,
safe-mode/absent backend, owner replacement and timeout uncertainty. Run the
whole desktop job from `.github/workflows/check.yml` in an isolated Arch container
on the remote CI host, then check the exact pushed revision's GitHub CI. No live installation
or physical session action is authorized.

Temporary duplicate: legacy QML tray/device readers and mutation entry points
coexist with the semantic daemon routes; delete 7 days after the replacement is trusted. Trust condition:
stable Bead core-switch use and owner-approved retirement of the QML shell.
There is no second settings writer or generic command dispatch route.

## Verification

The complete desktop job from `.github/workflows/check.yml` passed with exit 0
in an isolated `ghcr.io/archlinux/archlinux:latest` container, including the
notifications integration's clang/libpipewire dependencies. Rust formatting,
clippy, all native suites, release builds, protocol export freshness and the
minimal consumer passed. The helper library passed 571 unit cases; all 534
frozen helper parity cases passed without reference changes. Source/QML checks
passed (tools suite: 457 tests with one existing skip; shell tools: 479 tests).
The regular translation gallery passed 244 states per language, wide-font
gallery 185 states per language, and AUR pager capture passed. Private-bus tests
cover SNI method delivery, host registration, owner loss and stale identities;
bus GUIDs fence identity reuse after a bus restart. No live/hardware test or
installation was performed. Exact published-commit CI is recorded in the unit report.

The ephemeral CI checkout also needs canonical Git file modes for the sandbox's
unprivileged fixture user. A checkout-only helper restores tracked regular files
to 0644/0755, leaving Git metadata, untracked files and symlinks untouched. A
restricted-checkout reproduction and all 28 sandbox-integrity tests pass after
restoration; namespace and sandbox permissions are unchanged.

Product Rust delta: +2309/-1 lines, including exporters and excluding test
modules, generated artifacts and documentation. No new dependency was added by
this unit; the incoming notifications unit's dependency pin is retained.
