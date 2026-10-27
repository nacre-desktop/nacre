# N0086 — Wallpaper activation latency

## Diagnosis

The Oct 27 owner laptop phase log records selection→presentation at 521–1123 ms. Cached decode/scale was 0.1–1 ms; upload/commit/presentation added 30–60 ms. Bead had serialized activation calls and gated the selected raster on daemon confirmation. Nacre's activation handler scanned a catalogue, invoked `wallpaper-media select`, refreshed its producer, and waited synchronously before replying.

## Implementation

Bead now submits every selection immediately and fences acknowledgements with a monotonically increasing selection revision. An exact cache hit (decoded raster and matching cached palette) can commit optimistically in one surface update; otherwise the already-cached picker preview leads and the full raster replaces it when ready. Stale acknowledgements cannot replace the latest image or palette. Catalogue palette metadata is optional, validated, and bounded to 64 entries with at most 64 roles each.

Nacre validates the selected path directly under the configured wallpaper library, checks the supported suffix and open-file identity, and does not build/scan the catalogue or spawn `wallpaper-media select` before its acknowledgement. It records a generation fence, acknowledges the current active state, and starts a replaceable background selection task. The task persists the selected media and extracts/publishes the palette only while its generation remains current. Writes and rollback are fenced under the existing publication lock. Cached palette metadata accompanies catalogue entries for same-commit client application.

Catalogue palette data is bounded to 64 attachments per response and Bead retains at most 64 palette entries, each with at most 64 colour roles (under 1 MiB at protocol limits). The active wallpaper palette contract remains capped at 128 roles. No product compatibility duplicate was introduced.

## Validation and acceptance

Bead's full B0156 CI recipe passed on final commit `5574b3fb41282d6f8fc642d50a078df27abc10c9` (`FULL_CI_OK`, exit 0); the bounded-cache follow-up was safely pushed. Nacre's full `desktop` job passed on final commit `60882418328c9212ca43133ff6d2122b57c60b63` (`FULL_CI_OK`, exit 0) in the Arch CI container with the workflow's namespace settings and nice priority; it was safely pushed.

On the desktop TEST machine, a private nested session at 2880×1800, scale 1.5, copied all 17 static owner wallpapers into disposable HOME directories, prewarmed palettes and raster caches, then recorded 20 sequential measured switches. Selection→first presented frame p95 was 62.882 ms (max 72.897 ms); daemon acknowledgements were 20/20, p95 7.977 ms (max 9.672 ms). The sample uses the first compositor-presented frame for each selection; later confirmation frames from that selection do not count twice. Cached palette is fused with the optimistic frame, covered by Bead's `catalogue_palette_is_validated_and_installed_with_the_selected_pair` test.

Paired QML/Bead screenshots cover idle, carousel, hover, pressed, spotlight, hexagons, dynamic, empty, warm and 20-switch states. The known visual gaps are the spotlight card size (about 960×540 in QML and 790×440 in Bead; RMSE 17.14%) and hexagon layout (RMSE 6.61%). Other captured states were at or below 3.08% RMSE. Private, untracked screenshot pairs and sanitized metrics are in the [N0086 evidence folder](../../.lead/evidence/n0086-parity/README.md); the 17 copied wallpaper files and desktop capture workspace were deleted after the run.

Product diff: Bead +1,722/−305 lines and Nacre +544/−107 lines. The owner laptop install and live remeasurement remain a separate lead action; this unit only used the desktop TEST machine.
