# Niri monitor-workspace acceptance

`tools/tests/verify_workspace_hotplug.py` uses the real niri headless backend,
private HOME/XDG roots, a private D-Bus, native IPC and six owned Wayland clients.
It rejects any compositor socket outside its temporary runtime directory. The
extended `monitor_workspaces_headless.rs` driver accepts `add N...` and
`remove N...` on stdin; one remove command can undock several outputs together.
Only the shell event process opts into policy writes with
`NACRE_NIRI_WORKSPACE_POLICY=1`; ordinary event queries remain read-only. The
Bring command and event owner share one lock across ledger reads and writes.
The driver includes niri's normal configuration watcher, since named-slot recreation must
exercise actual asynchronous reloads rather than a mock acknowledgement.

Build the driver against the tested Niri 26.04 library and its matching
`niri-config` and Smithay dependencies (the same features and toolchain as the
library). Reuse those compiled libraries when available. Build Nacre's helper
with `nice -n 10 cargo build --locked -j2 -p nacre-helper`, then run:

```sh
nice -n 10 python3 tools/tests/verify_workspace_hotplug.py \
  --driver /path/to/headless-driver \
  --helper target/debug/nacre-helper \
  --output /path/to/private/hotplug-proof.json
```

Both policies cover 1→2→1, 1→3→1, removing two outputs in one dispatch, reconnecting
in reverse order, immediate remove/re-add blips, and stable per-monitor active
workspace/selected window. The proof also checks exact migration backups,
pruning only empty visitors, appending merged tiles as columns, preserving every
window, keeping merges after reconnect, restoring recreated empty named slots,
adopting the selected dynamic visitor, and restoring an empty dynamic view using
Niri’s existing spare. Native Qt tests separately verify
three-second presentation debounce, occupied-only visitor groups, monitor badges
and click identity after reindexing. The earlier monitor-actions proof continues
to cover send/swap/rotate and failure rollback in fixed mode.

Physical acceptance remains necessary for USB-C/dock and DRM connector events,
EDID/connector identity changes, lid close/open and suspend/resume, mixed refresh
rates/scales and output placement, and hardware focus/input feel. The headless
proof does not test those electrical, driver or presentation paths.

Hyprland retains global workspace numbering. Its existing backend contract and
workspace bar tests must stay green; Niri's per-output policies, visitor origin
tracking and merge action are intentionally capability-gated there. Do not claim
matching Hyprland hotplug restoration from a Niri headless proof.
