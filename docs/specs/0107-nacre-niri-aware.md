# N0107: Nacre niri-native capability awareness

Status: Implemented; pending lead review and exact-head dev-1 desktop CI.

When Nacre runs with nacre-niri, it should stop installing or generating only
the workarounds whose behavior the running compositor reports as native. Stock
niri and older nacre-niri builds must retain the current generated config and
helper behavior. A capability query is authoritative because native features
may be disabled by configuration; a version suffix alone is insufficient.

The `(nacre-niri)` version suffix identifies which compositor binary to launch
before its socket exists. Active-feature decisions use the JSON unit variant
`"NacreCapabilities"` over `NIRI_SOCKET`. Nacre accepts only a well-formed
`Ok.NacreCapabilities` object with all three known boolean fields. Missing
socket, unsupported request, IPC error, malformed or unknown response means no
native capabilities and preserves stock behavior.

Feature handling:

- `overview_hover`: launch the identified nacre-niri binary and negotiate the
  hover event stream; its supported event handshake falls back to stock events
  when the flag is disabled. Skip companion installation for nacre-niri. Keep
  the private hover acceptance probe.
- `workspace_collections`: do not suppress Nacre workspace visitors, named
  visitor workflows, monitor actions, origin ledger, adoptions, or hotplug
  reconciliation; collections do not replace those behaviors.
- `proportional_x11_widths`: keep the current helper until the native feature
  includes continuous X11 size-hint repair and enforcement, not just initial
  widths. NN0002's initial-width rule is not equivalent, so the capability
  remains false until the dynamic policy moves.

No owner paths, secrets, custom KDL, or theme data move into nacre-niri. The
user-owned `custom.kdl` remains included last, including the N0103 pearl rule
ordering fix. No live session, installed config, or unit is changed.

No current workspace-generation or width-helper policy is suppressed because
the reported native features do not replace those behaviors. Tests cover native
feature maps, disabled/unknown responses, the version suffix identity and stock
fallback. Validation is delegated to the lead's dev-1 full desktop CI run. This
worktree does not run local builds or tests.

## Completion record

Pending exact-head CI evidence.
