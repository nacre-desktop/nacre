# Compositor support

Nacre runs on [nacre-niri](https://github.com/nacre-desktop/nacre-niri), Nacre’s own fork of niri. It is the compositor for Nacre, and the Bead shell uses its native layer-promotion and workspace-affinity capabilities where available.

The Nacre codebase also includes a Hyprland backend. Backend-specific behavior is selected at the compositor boundary; shell layout and settings remain in Bead and Nacre’s shared services.

The SDDM greeter remains a separate Qt/QML application. The desktop shell itself is Bead, implemented by `crates/nacre-shell/` and `shell/`.
