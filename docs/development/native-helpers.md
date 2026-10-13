# Native helper architecture

Nacre's Cargo workspace is GPL-3.0-only. `nacre-helper` will host the existing
JSON-lines helper service and command dispatch; callers move together as their
frozen contracts pass. It is currently a library foundation, with no installed
command or helper replacement.

Orient stays an independent MIT dependency through the pinned `deps/orient`
submodule. The helper links `orient-core` and `orient-decoder` directly; it does
not spawn the Orient CLI. The direct library gallery test reads the existing
frozen Python references and checks 210 palettes exactly. Engine provenance
comes from Nacre's tested pin, and a source change during extraction is refused.

`Paths` keeps the helpers' current HOME-relative configuration/state/cache and
installation locations. JSON insertion order preserves existing state file
ordering; Orient's golden suite also passes with this feature enabled. Production
UI, command names and state remain unchanged until each helper is switched.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --quiet
```

Builds need Rust, CMake, Make, a C compiler and LCMS2, as the native Orient decoder
does. Every crate is non-publishable; this workspace does not publish packages.
