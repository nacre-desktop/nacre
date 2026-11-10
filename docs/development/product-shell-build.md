# Build and stage the product shell

Nacre's native shell is in `crates/nacre-shell/`; the Bead-based UI is in
`shell/`. The Cargo manifests pin the framework revision used by this checkout.

Build the three product executables:

```sh
nice -n 10 cargo build --locked --release -j2 -p nacre-shell --bin nacre-shell --bin bead-wallpaper-video --bin nacre-wallpaper-decode
```

Stage an isolated preview/package tree:

```sh
stage="$PWD/target/product-shell"
mkdir -p "$stage/bin" "$stage/shell"
install -m 755 target/release/nacre-shell "$stage/bin/nacre-shell"
install -m 755 target/release/bead-wallpaper-video "$stage/bin/bead-wallpaper-video"
install -m 755 target/release/nacre-wallpaper-decode "$stage/bin/nacre-wallpaper-decode"
cp -a shell/. "$stage/shell/"
```

For the complete Arch source gate, see the [desktop workflow](../../.github/workflows/check.yml).
