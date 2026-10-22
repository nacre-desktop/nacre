# N0027 — Active wallpaper producer contract

Status: **Implemented and verified locally; unpublished**. Rebased on
origin/main `507fd68c`; the complete Arch desktop CI replica passed with exit 0
on 2026-10-22. Lead publication is required before Bead adopts the contract.
N0027 was free on origin/main at start; N0026 is reserved by another worktree. Consumer: Bead
B0021 (`docs/specs/0021-static-wallpaper-surface.md` on Bead origin/main).

## Published interface

`wallpaper.active` version 1 is advertised only outside daemon safe mode.
MIT OR Apache-2.0 types live in `nacre-protocol::wallpaper`; JSON Schema,
Luau types and golden frames live under `protocol/`. No private-state file
names are part of the consumer interface.

Snapshot fields: `available`, `stale`, `revision`, nullable `current`, `outputs`
(output-name to pair), optional bounded `error`. Each pair contains `revision`,
`palette_revision` (equal), absolute local `path`, `file_identity`, `presentation`, and `palette`
with `mode: dark|light` and `colours` containing `#rrggbb` values. Current Nacre
policy is global: every output receives the same pair. Output names match the
published compositor topic. Hotplug changes assignments without changing the
image/palette revision; no connected outputs leaves an empty map and retains
`current`. Revisions are monotonic within a daemon boot; reconnect using the
welcome boot identity and resubscribe, never compare across boots.
An unavailable/malformed compositor snapshot retains the last output assignments
and marks the wallpaper state unavailable/stale; it is not an output-removal event.

`presentation` defines `crop` (preserve aspect ratio, cover output, centred
crop), `fit` (preserve aspect ratio, contain and centre), `fill` (stretch), and
`centre` (unscaled, centred). The producer currently emits only `crop`, matching
`NacreWallpaperScene.qml`'s `Image.PreserveAspectCrop`. Decode at each output's
pixel dimensions; source-sized decoded images are not required.

`wallpaper.activate` version 1, desktop capability, 1000 ms default timeout:
request `{ "revision": N }`, success is the complete matching pair. A superseded
revision returns `stale_revision`, unavailable/stale source returns `unavailable`,
both retryable. This is a read-only presentation activation gate, mirroring
QML's `NacrePresentation.activate`, not a new settings or wallpaper writer.
The consumer prepares the image, calls the gate, and commits the decoded image
and returned palette together if still applicable. If a newer topic revision
arrived while the call was pending, discard the older result. Decode failure,
disconnection, malformed data, unavailable/stale state and failed activation
preserve the last successfully displayed image **and** palette. Do not update
colours independently from the pair. No acknowledgement persists selection.

Wallpaper selection and palette generation remain in the existing picker,
`wallpaper-media select --stream-presentation`, settings registry and classic
publisher. Their atomic presentation publication is the daemon's source of
truth. Bead is a passive consumer and never invokes private helpers or writes
selection/configuration. A wallpaper change becomes a new published pair;
Bead's published activation route is the gate above.

## Producer, bounds and safety

Reuse the `wallpaper-watch` service and its single inotify reader. Presentation
changes have a separate 500 ms debounce bit and do not increment legacy source
generations. Selected-image ancestors share that same reader, so external image
replacement/removal/restoration also refreshes the pair; these events do not
increment legacy generations unless they also belong to the wallpaper library.
Compositor notifications update output assignments. Nothing polls
while idle. The service keeps its existing supervisor/health reporting; failure
marks the active topic unavailable and retains the last good pair as stale.
Before any good pair, `current` is null, revision zero, and stale is false.
Invalid/missing/oversized presentation or image does not publish a replacement.
Recovery of identical inputs clears stale without changing the revision.

The daemon reads only an owner-validated, bounded presentation record and
publishes its full required QML palette. It accepts only absolute local regular
files owned by the daemon user, with no group/other write permission. Every
component is opened from `/` relative to a held directory descriptor with
`O_NOFOLLOW`. Ancestor directories must belong to root or the daemon user and
must not be group/other writable, except root-owned sticky directories such as
`/tmp`. Symlinks (including aliases), `..`, NUL and remote URLs are refused.
User-owned images on other mounted directories are supported under these rules.
Only the picker's PNG/JPEG/WebP/GIF/TIFF image headers are accepted, at most
100 million source pixels and 128 MiB encoded data. This is a static poster contract, including static posters
for media; motion/video playback is outside this unit.
Consumers decode only the first static frame/page of GIF/TIFF inputs; this
contract carries no animation timing or page selection.

The private local path was chosen over a daemon decoded handle because consumers
already decode at output size and the daemon need not retain another image cache.
A path is not an immutable file lease: consumers must securely reopen relative
to a descriptor for `/` with `openat2(RESOLVE_BENEATH|RESOLVE_NO_SYMLINKS)` (strip
the leading slash), or an equivalent descriptor walk. Both approaches must apply
the ancestor checks above, verify regular-file type, uid and permissions on the
open descriptor, and decode from that descriptor. A convenience URL/image loader that follows links
does not satisfy this contract. Changed/deleted/unreadable files retain the last
good display. No consumer should infer or read any sibling state files.

`file_identity` carries decimal-string `device` and `inode` (u64, avoiding JSON
precision loss), encoded byte `size`, and signed Unix `mtime_sec`, `ctime_sec`
plus `mtime_nsec`, `ctime_nsec` (0..999999999). Compare every field against fstat
on the consumer's open descriptor before and after decoding; reject mismatches.
Rust consumers can use `WallpaperFileIdentity::from_metadata`. Keep the descriptor
through decoding so replacing a pathname cannot replace the opened image.

Bounds: 128 KiB source record, 4096-byte path, 32 outputs with 256-byte names,
128 colours with 64-byte keys, 256-byte error; integer revisions are at most
2^53-1. Schema string maxLength counts Unicode characters; Rust validation also
enforces the stricter UTF-8 byte bounds. Standard framing and patch/resync limits
apply. Native validation enforces pair/output revision relationships in addition
to JSON Schema structural checks. Errors never include private path details.

## Validation and temporary duplicates

Acceptance: generated contract freshness and replay, initial pair, palette-only
switch, invalid source and recovery, stale activation, output add/remove,
symlink/escape/permission/ownership/size guards, Rust fmt/clippy/tests, protocol
minimal consumer, and the entire desktop CI job in a private dev-1 Arch container.
No installer, live desktop or physical-session verification is required.

Temporary duplicate: QML still consumes the existing presentation file beside
the new daemon projection: delete 7 days after the replacement is trusted.
Trust condition: stable Bead core-switch use, subject to the owner's QML retirement
decision. `wallpaper.watch` remains the compatibility generation topic: delete 7
days after the replacement is trusted. Trust condition: all supported legacy
watcher clients have migrated.
There is no second persistence writer or decoded-image store.

Validation evidence: the full desktop job from `.github/workflows/check.yml`
passed in a fresh `ghcr.io/archlinux/archlinux:latest` container with SYS_ADMIN,
unconfined seccomp/AppArmor and `/dev/fuse`. This includes Rust fmt/clippy,
480 helper unit tests, 19 daemon integration tests, protocol goldens and minimal
consumer, release builds, sandbox integrity, all source/QML and frozen parity
suites, and both native translation galleries plus AUR capture. The focused
producer suite passed all four tests, including compositor-loss/recovery and
GIF/TIFF acceptance. `tools/tests` retained its one existing skip.

Product Rust delta: +699/-9 lines, excluding tests/docs/generated artifacts
(includes 52 exporter lines); tests +509/-0; generated protocol artifacts +386/-0.
The QML consumer and persistence writer are unchanged. No image, package or Git
revision was published; no live desktop/configuration action was taken.
