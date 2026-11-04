# N0121: final Niri workaround sweep

The Nacre-Niri fork exposes workspace swap and workspace-set rotation as atomic
IPC actions, explicit hidden-window restore actions, force-close for the owning
Wayland client, and pin state/actions. Nacre queries `NacreCapabilities` and only
advertises or dispatches fork additions when the corresponding live feature is
enabled. Missing fields from older forks default off. Stock Niri retains its
existing configuration and helper paths. Standalone event snapshots reuse the
capability reply fetched for stream selection and refresh it on `ConfigLoaded`;
they do not poll capabilities on a timer.

Nacre's generated `nacre-niri.kdl` owns the fork-only force-close and restore
bindings. The stock `config.kdl` contains no fork action names. Hide, last-hidden
restore, and current-workspace restore use native actions when supported; explicit
hide targets retain the selected Niri window ID. The stock hide ledger remains
available as fallback. Pinning reads the live pinned state, sends an ID-targeted
toggle only when the requested state differs, and remains unavailable through the
generic Niri action API on stock Niri.

The Nacre fixed-slot monitor transaction uses one native selected-workspace swap
when enabled, persists the resulting ownership map, and applies the same
involution if persistence fails. Workspace-set rotation uses the native set
operation only when every configured named set is present, active workspaces are
configured members, and all unnamed spare workspaces are empty. The action
rehomes workspace identities with the set; Nacre verifies the resulting IDs,
active slots, focus, and output ring before persisting. Numbered sends, visitor
merges, and origin-output policy stay in Nacre. Stock Niri and older forks retain
the legacy multi-IPC swap/rotation with rollback.

Group toggle and previous/next group-cycle lower to stock Niri KDL actions,
removing the `spawn-sh`/`niri msg` round trip on either compositor. Force-close is
reported as a Wayland-client-scoped capability; generic `windowForceKill` stays
false because X11 clients are not supported and sibling windows on one Wayland
connection may close together. Unsupported rows are kept and annotated with the
fork capability required, preserving the stock-Niri gap report.

Validation includes capability parsing for old, disabled, and enabled responses;
native/stock snapshot and dispatch behavior; fork-overlay versus stock config
checks; group action lowering; swap/rotation eligibility, inverse recovery, and
fallback paths; and the repository's Arch desktop CI replica on dev-1.
