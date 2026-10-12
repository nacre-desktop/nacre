# Shell memory and launcher progress

## Done

- Isolated `perf/shell-memory-budget`; rebased on main `9a38ff0`, preserving
  compositor/focus/shell-hide work. No live deployment or private preference edits.
- PSS/USS/SwapPss, unknown-rollup coverage, PID identity recheck and nearest-rank
  percentiles added to tools/performance.py, with regression tests.
- Settings/Colors/Welcome content and non-app launcher modes unload after five
  seconds; window identity/geometry and view state survive. App launcher is kept
  warm after its first use; search/selection/prompt reset still runs on open.
- Previous poster trims after five seconds. Video files load on demand. Welcome
  preloads four output-sized stills only on its visible colors tour page.
- Native tests cover lifetimes, state, real Welcome preload model, muted video,
  crossfade/reverse navigation and warm mode switching. Local full gate passes
  624 tests plus lint/format/native/parser checks; CI passed on the source head.
- Final matched measurements and sanitized JSON documented in performance.md:
  closed full cycle RSS/PSS/USS 446.7/359.0/340.8 -> 387.9/305.7/284.6 MiB.
  Apps synchronous warmed open ~54 -> 4 ms, after long idle 60 -> 3 ms; warm
  retained cost ~5.6 MiB PSS in the dedicated profile. First creation still costs.
- Static/video and one/two-output comparisons complete. Two active 1080p video
  outputs add ~114 MiB PSS in the candidate video-output sequence. Budget proposed.
- Captures with inherited XDG data paths were discarded. The final fixture is
  fully explicit, with 48 actual demo IDs, 12 favorites and ready local icons.
  Diagnostic GC/cache-release probes were small and are not shipped.

## Next

- Code, measurement, documentation and PR #10 review preparation are complete.
- Final rebase preserves the separately merged magnetic docking work. Repeat
  source/native checks and required CI on this head, then merge PR #10.
- No performance implementation remains; future limits below are follow-up scope.
- Final merge/cleanup evidence is recorded privately and in the linked PR status.

## Open questions / limits

- PSS excludes unmapped GPU allocations. Anonymous native allocation stacks remain
  unclassified; partial tracing is not claimed as evidence.
- PNG/H.264 budgets need calibration for real output/scale/codec/catalog sizes;
  large GIF frame caches and 4K/HDR video were not measured.
