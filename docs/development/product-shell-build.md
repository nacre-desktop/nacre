# Building the product shell from Nacre

The shipping workspace pins every Bead package to the same frozen framework
revision. Cargo fetches Bead from GitHub when building from a Nacre checkout;
no Bead checkout is needed. `Cargo.lock` records the resolved graph.

Build the three executable targets, then stage the established preview/package
shape (`bin/{nacre-shell,bead-wallpaper-video,nacre-wallpaper-decode}` plus
`shell/`):

```sh
nice -n 10 cargo build --locked --release -j2 -p nacre-shell \
  --bin nacre-shell \
  --bin bead-wallpaper-video \
  --bin nacre-wallpaper-decode

stage="$PWD/target/product-shell"
mkdir -p "$stage/bin" "$stage/shell"
install -m 755 target/release/nacre-shell "$stage/bin/nacre-shell"
install -m 755 target/release/bead-wallpaper-video "$stage/bin/bead-wallpaper-video"
install -m 755 target/release/nacre-wallpaper-decode "$stage/bin/nacre-wallpaper-decode"
cp -a shell/. "$stage/shell/"
```

For local development only, temporarily add this example to the root
`Cargo.toml` and adjust each path to the developer's Bead checkout. Do not
commit this patch: shipping manifests must continue using the pinned git source.

```toml
[patch."https://github.com/nacre-desktop/bead"]
bead = { path = "/path/to/bead/crates/bead" }
bead-backends = { path = "/path/to/bead/crates/bead-backends" }
bead-core = { path = "/path/to/bead/crates/bead-core" }
bead-kit = { path = "/path/to/bead/crates/bead-kit" }
bead-render = { path = "/path/to/bead/crates/bead-render" }
bead-services = { path = "/path/to/bead/crates/bead-services" }
bead-test = { path = "/path/to/bead/crates/bead-test" }
bead-ui = { path = "/path/to/bead/crates/bead-ui" }
bead-wallpaper = { path = "/path/to/bead/crates/bead-wallpaper" }
bead-wayland = { path = "/path/to/bead/crates/bead-wayland" }
bead-widgets = { path = "/path/to/bead/crates/bead-widgets" }
```

The root `fontconfig` and `system-tray` patches vendor Bead's exact sources and
notices because patches declared in a git dependency do not apply to Nacre's
workspace. The release profile retains Nacre's thin LTO, single codegen unit,
and stripped output, while applying Bead's size optimization to dependencies
and full optimization to Bead workspace crates and its rendering, text, and
image hot-path dependencies.

The shell build script discovers the resolved `bead-test` manifest through
`cargo metadata --locked` and exports the pinned Bead checkout root to
the product crate. Product tests that need Bead's IBM Plex fixture font,
neutral UI fixture, or hello plugin use that root directly. This keeps those
tests runnable from Nacre alone without copying framework fixtures into the
product tree or setting a developer-specific environment variable.

Nacre retains `serde_json/preserve_order`. Feature unification makes Bead IPC
replies and the shell’s command payload larger than in Bead’s standalone graph.
The existing inline reply and queue contracts are retained with narrowly scoped
Clippy size allowances; no protocol pin, ordering contract or golden is changed.
