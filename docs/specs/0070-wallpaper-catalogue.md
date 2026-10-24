# N0070 — Wallpaper picker catalogue

Status: Implemented for Bead B0100; lease-only daemon method.

`wallpaper.catalogue {}` is a lease-only, request-lifetime method: each call does one bounded helper request, and the daemon retains no catalogue rows, subscription, worker, or timer after the reply. It returns `{entries: [...]}` with up to 2,048 entries containing `id` (the exact local source path), `kind` (`image` or `video`), `title`, `thumbnail`, `preview`, and `active`. The existing helper creates thumbnails at most 640×400/256 KiB and previews at most 1600×1000/2 MiB. The thumbnail cache keeps at most 128 entries (32 MiB maximum); preview pairs are pruned with their thumbnail. Paths are returned only from that cache, or as empty strings. The method is advertised with `wallpaper.active`.

The picker subscribes to `wallpaper.active` for the current revision and calls `wallpaper.catalogue` with `{}`. To select, it calls `wallpaper.activate` with `{revision, id}`; the daemon rejects a stale revision or an id absent from the current catalogue, invokes the existing `wallpaper-media select` helper, refreshes `wallpaper.active`, and returns its current `ActiveWallpaper`. Existing `{revision}` calls retain their prior read/confirmation behavior. Selection is guarded by the active revision both before catalogue lookup and immediately before the helper side effect; concurrent selections return `busy`.

The catalogue call runs only on demand through the daemon's bounded helper lane, uses `nice -n 10`, and is bounded to 12 seconds and 8 MiB of output. Selection is bounded to 15 seconds. N0053 startup behavior is unchanged: READY does not wait for wallpaper enumeration or previews; the method becomes available with the active wallpaper topic. Catalog data and selection do not add a settings store; existing picker preferences remain in `wallpaper-picker`.

The source UI uses static/dynamic filters, search over name and path, and carousel/spotlight/hexagon layouts. Its selected item previews while browsing and commits on close; static and motion media share the catalogue, with cached still thumbnails/posters. B0100 owns the Bead UI and parity evidence.
