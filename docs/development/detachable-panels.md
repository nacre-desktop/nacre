# Detachable panels

Bead panels can be detached as ordinary windows. On nacre-niri, the compositor’s `bead_layer_promote_manager_v1` capability promotes the existing layer surface in place and handles floating placement, movement, focus and decorations. Nacre does not create a second window on that path. Stock niri uses the prepared-window fallback.

The [user guide](../detachable-panels.md) covers gestures and shortcuts. The compositor and shell share ownership of placement; view state stays with the Bead panel during a detach. Nacre’s implementation is in `crates/nacre-shell/` and `shell/`.
