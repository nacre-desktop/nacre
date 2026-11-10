# Nacre foundation

Nacre is a wallpaper-themed Wayland desktop running on nacre-niri, Nacre’s niri compositor fork. Its native shell uses Bead: reusable Rust primitives in `crates/nacre-shell/` and user-arrangeable Luau views in `shell/`. The daemon owns desktop state and outside connections; the shell reads snapshots and sends actions over the private protocol. Only the daemon writes registered settings.

Hyprland support is provided by the compositor backend in `crates/nacre-shell/`. The SDDM greeter remains a separate Qt/QML application in `nacre/login/`.

See [protocol](protocol.md) for wire contracts, [core services](core-services.md) for reserved schemas and [structure and licensing](structure-and-licensing.md) for repository ownership.
