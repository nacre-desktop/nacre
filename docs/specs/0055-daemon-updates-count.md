# N0055 — Daemon update count

Status: Implemented; pending lead review. Date: 2026-10-24.

The daemon owns `dashboard.updates` using the existing `updates.rs` counter.
The original implementation started a background check immediately and every
1,800 seconds, independent of subscribers. N0095 supersedes that schedule: the
automatic check begins with the first topic subscription and repeats every
1,800 seconds while subscribed. Settled package-database changes and updater
completion are retained while unsubscribed and trigger a refresh when a dashboard
subscribes. Explicit `dashboard.updates.refresh` remains on-demand. The method
accepts an empty object and immediately returns `changed: false`; the topic
carries completion. A concurrent request returns `busy`. Safe mode does not
start or advertise this service.

Parity: the QML status worker invokes `nice -n 10 nacre-helper updates` immediately
and every 30 minutes. A cache older than 30 minutes launches a refresh, which
counts nonblank lines from `checkupdates` (fallback `pacman -Qu`) plus `paru -Qua`
(fallback `yay -Qua`). The daemon reuses the same selection, counting and exit
rules: checkupdates accepts 0/2; other tools accept 0/1, with stderr on 1 an error.
Each package command retains its 90-second timeout and namespace routing through
`systemd-run --user`. Checks may contact package mirrors and AUR; no new consent
setting or IP lookup is introduced. No upgrades, installs or Flatpak operations
are performed by the daemon.

The initial topic has unavailable health and no count or checked time. Success
publishes available health, count (including zero), text and Unix-second `checked`.
Failure/timeout clears count, marks unavailable/stale, reports a retryable error
and retains the last successful checked time. Additive `refreshing` defaults to
false for older producers. While refreshing, existing data is marked stale.
The worker runs via N0053 `process::offload`, never the readiness/blocking-file
pool; its thread sets nice to at least 10 and direct children inherit it,
without adding a second nice increment to an already low-priority daemon. A single lane remains locked inside the
blocking worker even if its caller disappears. The existing cache file lock also
prevents overlap with the retained standalone refresh process. READY does not await this work.

The daemon publishes the count topic without rewriting `updates.json`; legacy
cache-age readers still follow the standalone cache. Interactive Quickshell/Qt preflight remains
in the explicit upgrade flow unchanged, including `paru -Syu --skipreview`.
`Updates.qml` is not already a daemon consumer, so it remains untouched under the
unit's fallback instruction. Temporary duplicate: standalone QML timer/cache
refresh path; delete 7 days after the daemon replacement is trusted (successful
count/refresh/error operation and producer CI), coordinated with the core switch.
The retained standalone Python entrypoint continues to delegate to Rust.

Validation: fake checkupdates covers zero, multiple lines, command error and
bounded timeout. Topic tests cover unknown, zero, 129, failure and checked-time
retention; the N0053 hung-backend READY guard now includes hanging checkupdates.
The first full CI run exposed a dashboard-settings validation-test race with
the quiet-period flush normalizing version metadata. The test now waits for
the durable revision and uses returned revisions; stale-write rejection remains
asserted.

Full desktop CI steps passed in a disposable Arch container: Rust formatting,
strict all-target clippy, examples, workspace tests, minimal protocol consumer,
release helper, 28 sandbox tests, translation generation/coverage, and full
`tools/check.py` (491 tools tests, one skip; 120 AI; 52 Brain; 487 shell-tools;
534 helper parity cases, zero failures). Protocol export `--check` passed.
Both native translation audits passed (244 and 185 states per language); the
AUR pager gallery passed. Temporary CI scaffolding was moved outside the scanned
checkout before the source/gallery steps were rerun. No live session was touched.
Simplicity review: one existing counter, one timer/lane, and the existing file
lock; no second package provider or cache writer. Product Rust: +188/-10 lines,
excluding tests and generated artifacts.
