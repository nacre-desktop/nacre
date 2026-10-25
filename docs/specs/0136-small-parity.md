# B0136 producer — daemon wallpaper previous

Status: implemented; full Arch desktop CI equivalent passed. Bead B0136
consumes this additive v1 contract after publication.

`wallpaper.active.previous_available` is optional in v1 and defaults false. It
is QML `rotationReady`, shared by manual Next and Previous: the configured
pool has at least two items and the wallpaper producer is current, awake,
unlocked and not selecting. It does not require existing history. The daemon
behaviour worker owns pool and session readiness; it reuses `catalog_pool` and
publishes the field on active wallpaper, settings, library or session changes.
With automatic rotation off, it starts the existing lightweight session watcher
only while an active wallpaper and a multi-item pool make navigation possible.
Readiness retains only the pool count result; the full pool is dropped before
idle waits and reloaded for an actual automatic rotation.

`wallpaper.previous {}` is a Desktop method returning `{changed:boolean}`. It
uses the existing wallpaper rotation plan, pool and selector. The history is a
private in-memory deque capped at 100 source paths. The active wallpaper
source paths are capped at 4 KiB; one in-flight traversal may copy a filtered
stack of at most 100 entries (~0.8 MiB maximum path bytes plus small container
overhead). The active wallpaper
producer updates it only after a valid presentation is observed. Previous
filters history through the current pool and skips the current source; it uses
the latest valid entry, otherwise wraps to the preceding pool item. Failed
selections leave both current source and history unchanged. The field reports
QML `rotationReady`, including when no history exists, so Previous and Next
share one correct readiness signal.

Validation: deterministic history cases cover ordinary changes, repeated
backwards traversal, pool filtering, wrap with empty history, singleton pools,
failed selection and the 100-entry bound. Protocol transcript coverage records
the optional readiness field and method. Generated schema, Luau and transcripts
are current. Full Arch desktop CI equivalent passed: Rust checks/tests and
minimal consumer, helper sandbox parity, i18n/source/native UI suites (including
490 shell-tools tests and 534 helper parity cases), galleries (244 default and
185 wide-font states per language), AUR review capture and daemon raster memory.
The disposable runner used explicit toolchain homes and scoped submodule Git
trust for the checker's isolated HOME; its font packages matched the CI job.

Temporary duplicate: QML's in-memory history remains for the QML daily driver;
delete that path 7 days after the Bead core switch is owner-approved and trusted.
