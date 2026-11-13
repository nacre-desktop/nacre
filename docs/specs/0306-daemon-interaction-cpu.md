# N0306 — daemon interaction CPU

**Status:** Implementation complete; focused dev-1 validation passed; hosted CI pending.

## Goal and change

Reduce the Nacre daemon's CPU during the N0300 six-cycle interaction while
preserving daemon topics and lock-screen pixels. The largest sampled cost was
the lock-content behavior redrawing and PNG-encoding the complete lock
dashboard after relevant topic changes. It now uses the `image` PNG encoder
with a fixed Sub filter and fast compression instead of Orient's adaptive
per-pixel filter scoring. PNG decoding produces byte-identical RGBA pixels.

The behavior is driven by the 750 ms topic debounce in
[lock_content.rs](../../crates/nacre-helper/src/daemon/lock_content.rs:275),
then calls [lock_prepare.rs](../../crates/nacre-helper/src/lock_prepare.rs:159)
and [lock_dashboard.rs](../../crates/nacre-helper/src/lock_dashboard.rs:1223).
Perf attributed 45.3% of daemon cycles to Orient's adaptive PNG filter;
dashboard compositing and polygon rasterization added another 7.6%.
This work is debounced and may run after an interaction's action window. The
profile attributes it to the whole-workload lock-content path; it does not show
that closing the launcher or dashboard directly invokes the encoder.

## Measurement

Nacre `74baf744d`, the N0300 B arm, stock niri, private headless labwc/niri at
1920×1080 with Pixman/software rendering on dev-1. Three fresh daemon runs per
phase; each run contains six 10-second cycles. Per-thread CPU ticks were sampled
every 10 ms. The valid before/after runs used the same source, shell, workload,
and container limits.

| Daemon CPU | Before | After |
| --- | ---: | ---: |
| Whole 60-second workload, % of one core (median [min, max]) | 6.55 [6.53, 6.60] | 2.62 [2.62, 2.65] |
| Reduction | — | 60% (3.93 percentage points) |

| Action CPU across six cycles | Before | After |
| --- | ---: | ---: |
| Launcher open / close | 0.21 s / 0.56 s | 0.20 s / 0.08 s |
| Dashboard open / close | 0.38 s / 0.65 s | 0.36 s / 0.03 s |
| Workspace focus (2→1→3→1) | 0.12 s | 0.12 s |
| Four notifications per cycle | 0.07 s | 0.07 s |

Action values are the median of the three run-level accumulated CPU totals,
using per-thread tick deltas at action boundaries. Whole-workload values are
the median of three full 60-second runs.

One run generated 13 private lock-dashboard PNGs. Their combined disk size
increased from 1.48 MB to 3.96 MB (+2.48 MB) with the faster filter. This does
not change decoded pixels or topic payloads. N0305's `resource_state.rs` and
OSD timer changes were left untouched.

## Validation

- Focused pixel-equivalence test passed (1/1):
  `fast_dashboard_png_encoding_preserves_raster_pixels`.
- `cargo test --release --locked -j2 -p nacre-helper --lib`: 1,025 passed,
  3 ignored.
- `cargo clippy --release --locked -j2 -p nacre-helper --lib -- -D warnings`:
  passed with no warnings; `cargo fmt --all -- --check`: passed.
- Hosted CI run: pending.
