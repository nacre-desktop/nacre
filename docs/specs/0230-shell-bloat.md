# N0230 Delete dead shell bundles and never-opened goldens

Shell items from the 2026-11-13 Platform repo bloat audit (audit C).

## Bundles

Deleted `shell/legacy-ui/`, `shell/layout-example/`, `shell/kit/` and `shell/backend/`
(9 files, 255 lines, 29 KB). No Rust view loader, Luau `require`, tool, packaging file,
CI step or doc names them; Bead's own `bead-ui` tests cover JSON bundles and the kit
fixture. The only caller was the first half of
`checked_in_ui_bundle_and_assets_are_complete`, which loaded the retired N0054 JSON
fallback bundle. That test now checks only the real `ui/main.luau` bundle.

## Goldens

One traced run of the whole nacre-shell test binary on dev-1
(`strace -f --seccomp-bpf -e trace=open,openat,openat2` around
`cargo test -p nacre-shell --bin nacre-shell -- --test-threads=6`; 893 passed, 0 failed,
4 ignored; none of the ignored tests touches a golden) opened 310 of the 321 files in
`shell/tests/goldens`. The audit's guesses (`welcome-*`, `bluetooth-*-1.5`,
`wallpaper-picker-*`) were all opened and stay.

The 11 never-opened files were all `placement/pair-*-1.5.png` (2.36 MB): side-by-side
QML|Bead review images that `placement_pixels_tests.rs` writes only in
`BEAD_UPDATE_GOLDENS=1` mode and never compares. No tool or doc references them. They
are deleted, and update mode now writes them to the temp artifacts directory
(`bead-placement-pixel-diffs`) so the next golden update does not put them back.
The `qml-*` reference masks they were built from stay: they are the parity reference.

## Validation (dev-1, `n0200-build:latest`)

Before and after the golden deletion: `cargo fmt --all --check` and
`cargo clippy -p nacre-shell --all-targets -- -D warnings` passed; the whole nacre-shell
binary passed both times (893 passed, 0 failed, 4 ignored); after the deletion 310
goldens remain and none were regenerated. No Python changed.

Net: −256/+3 lines, −2,388,226 bytes (−2.28 MiB) in `shell/` and `crates/nacre-shell`.
