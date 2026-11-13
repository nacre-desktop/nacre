# Nacre Shell UI

This is the Nacre product UI: Luau views, tokens, widgets, i18n and goldens
hosted by the Bead framework through `crates/nacre-shell`. Everything here is
GPL-3.0-only, retaining the upstream terms and any asset-specific notices.
Build and preview steps are in
[docs/development/product-shell-build.md](../docs/development/product-shell-build.md).

A hosted view can set or compute `view.frame = false` to draw its own floating
composition without the shell's joined frame artwork. The default keeps the frame.
