# N0091 — Keep wallpaper activation responsive during background warming

Status: candidate; final warm/cold Bead acceptance and Nacre desktop CI pending.

## Problem

After N0089 removed the publication lock from the daemon event loop, a foreground
`wallpaper.activate` still launched `wallpaper-media select` at nice 10. B0173's
stable-cache warm test showed first-per-revision p95 of 135.385 ms, despite valid
palette and display-raster cache markers. A bounded private diagnostic changed
only that foreground child to nice 5; all 15 selections were below 120 ms, with
per-cycle maxima 114.30, 84.26, and 101.36 ms. Catalogue and prewarm helpers
remained at nice 10, and Bead raster prefetch remained at nice 19.

The trace found no publication-flock wait in the selection children. Their
runtime overlapped repeated catalogue and prewarm subprocesses, so giving the
direct user action a modest priority advantage lets background work yield while
preserving its existing execution path and generation checks.

## Change

Run only the `wallpaper-media select` child from `wallpaper.activate` at nice 5.
Catalogue and prewarm helpers remain at nice 10. Bead's cancellable raster
prefetch runs at nice 19 after the first frame and remains limited to one decode.

No additional daemon or worker is kept warm. The priority experiment used the
desktop TEST machine, a private HOME with stable wallpaper source identities,
and a normal-day cache seed; it is diagnostic evidence, not final acceptance.

## Acceptance

| Check | Result |
|---|---|
| Bead warm-cache selection-to-presented aggregate p95 <120 ms | Pending final `--wallpaper-live` rerun |
| Bead first-content frame: warm <150 ms, empty cache <300 ms | Pending final paired arms |
| Bead shell CPU returns to idle within 2 s after close | Pending final TID-summed run |
| Nacre desktop CI | Pending |

No cache schema, protocol, or security boundary changes.
