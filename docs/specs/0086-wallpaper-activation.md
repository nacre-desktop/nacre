# N0086 — Wallpaper activation latency

## Diagnosis

The Oct 27 owner laptop phase log records selection→presentation at 521–1123 ms. Cached decode/scale was 0.1–1 ms; upload/commit/presentation added 30–60 ms. Bead had serialized activation calls and gated the selected raster on daemon confirmation. Nacre's activation handler scanned a catalogue, invoked `wallpaper-media select`, refreshed its producer, and waited synchronously before replying.

## Implementation

Bead now submits every selection immediately and fences acknowledgements with a monotonically increasing selection revision. An exact cache hit (decoded raster and matching cached palette) can commit optimistically in one surface update; otherwise the already-cached picker preview leads and the full raster replaces it when ready. Stale acknowledgements cannot replace the latest image or palette. Catalogue palette metadata is optional, validated, and bounded to 64 entries with at most 64 roles each.

Nacre validates the selected path directly under the configured wallpaper library, checks the supported suffix and open-file identity, and does not build/scan the catalogue or spawn `wallpaper-media select` before its acknowledgement. It records a generation fence, acknowledges the current active state, and starts a replaceable background selection task. The task persists the selected media and extracts/publishes the palette only while its generation remains current. Writes and rollback are fenced under the existing publication lock. Cached palette metadata accompanies catalogue entries for same-commit client application.

New palette cache metadata is bounded to 64 catalogue attachments × 64 colour roles (budget below 1 MiB at the protocol limits). The active wallpaper palette contract remains capped at 128 roles. No product compatibility duplicate was introduced.

## Validation and acceptance

Bead full CI passed on the exact pushed Bead HEAD `7343a6b368d18df4f0446272d317b93a98089ff`; Nacre full `desktop` CI and the private desktop capture are being run for this unit. Acceptance is at least 20 switches using the owner's 17 static wallpapers in a private desktop TEST session at 2880×1800, scale 1.5: selection→presented p95 ≤150 ms, daemon_ack ≤100 ms, and cached palette applied in the same frame. Results and the capture command will be recorded here after completion. The owner's laptop installation/remeasurement is a separate lead action.
