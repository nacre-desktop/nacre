# Shell memory and launcher progress

## Done

- Final branch `perf/shell-memory-budget`, isolated worktree, rebased on merged
  main `fa37a28` (helper/compositor/hardware work included). Normal push guard
  passes; no hook or history changes. Earlier `perf/shell-memory` is a checkpoint.
- Added tested rollup PSS/USS/SwapPss accounting and sample coverage; retained
  merged performance tests/documentation during the add/add rebase.
- Five-second launcher reuse, floating-app content unloading with saved view
  state, old-poster idle trimming, and lazy video file loading implemented.
- Floating window identity/geometry and backend ownership stay intact. Search
  IPC remains safe after content release. No visual/private preference changes.
- Full local `tools/check.py` passes 621 tests plus lint/format/native checks.
- An isolation audit found inherited XDG_DATA_HOME made demo favorites absent.
  No live preferences were changed. Superseded captures are retained privately;
  public draft tables explicitly marked superseded until corrected runs finish.
- Now explicitly set private XDG_DATA_HOME and public system XDG_DATA_DIRS.
  Shell-owned fixtureCheck verified 48 demo IDs and 12 visible favorites.
  Corrected before/after panel/static/video/output matrix running.

## Next

- Finish final main-based static/video/two-output matrix and valid launcher
  component/catalog/icon breakdown; profile remaining native allocations.
- Record final tables and a scoped memory budget in performance.md.
- Push documentation milestone, create one PR, verify CI, rebase and merge.

## Open questions

- How much native allocator/renderer memory remains after video destruction?
- What memory cost does five-second launcher reuse have under a populated catalog?

All raw fixtures/logs are private disposable artifacts; no live deployment.
