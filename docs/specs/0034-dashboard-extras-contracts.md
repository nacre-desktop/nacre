# N0034 — Dashboard extras daemon contracts

Status: **Implemented and verified on a dev-2 Arch replica; unpublished** (see
Validation). Consumer: Bead B0024 (`docs/specs/0024-dashboard-extras-parity.md` in
Bead) items B2 focus, B4 extras and B5 customization writes. The number was the
next free one after the in-flight N0029-N0033 reservations; renumber freely before
publication. Protocol types are MIT OR Apache-2.0 (`nacre-protocol::dashboard_extras`);
JSON Schema, Luau types and golden transcripts are generated under `protocol/`.

## Published interface (all version 1, outside daemon safe mode)

Every state has `health {available, stale, error?}`; last-good data stays with
`stale:true`. Collections are `Entities<T>` (`items` + `order`, newest first).
Methods are class `desktop`; success is `{changed}`; no optimistic update, state is
authoritative.

| Topic | Fields |
| --- | --- |
| `dashboard.focus` | `status` idle/running/paused/done, `total_s` (0..10800), running: `deadline_ms` (Unix ms), paused: `remaining_s`, `owns_dnd`, `previous_dnd` |
| `clipboard` | `entries`: `{kind: text/image, preview (<=200 chars, no control chars), created_ms}`, at most 50 |
| `screenshots` | `items`: `{path (absolute), created_ms}`, at most 6 |
| `ai.usage` | `providers` `codex`/`claude`: `{name, lines (<=5, <=256 chars), observed_ms}` |
| `brain.recent` | `notes` `{title<=100, path}` (<=5), `tasks` `{text<=160, path}` (<=6) |

Methods: `focus.start {minutes 1..180}`, `focus.pause|resume|stop {}`,
`clipboard.copy {id}`, `clipboard.clear {}`, `screenshots.open {id}`,
`ai.usage.refresh {}`. IDs are opaque daemon-issued (`c<cliphist id>`, `s<hash>`,
`n<hash>`/`t<hash>`); callers never construct them. Errors: `validation` (bad
parameters), `invalid_request` (`focus.start` while a session is active),
`stale_revision` (clip or screenshot no longer exists), `unavailable`, `io`.
`pause/resume/stop` with nothing to do return `{changed:false}`.

### Focus (B2)

The daemon owns the timer. `running` publishes `deadline_ms` only; consumers count
down locally, so nothing publishes per second. A 30 s wall-clock recheck while a
session runs prevents suspend from oversleeping the deadline; there is no idle timer.
State persists in the existing `state/dashboard-focus.json` shape plus
`owner:"nacre-daemon"`, so the helper `dashboard-data` reader still works.
DND follows the QML rules: `previous_dnd` is the setting at start, `owns_dnd` is
`!previous_dnd`, DND is set only through the settings writer (the notifications
producer observes that store; no second writer), and stop/completion restore
`previous_dnd` only if the session owns DND and it is still on. Ownership is cleared
before the restore. The user turning DND off ends ownership. A failed DND change at
start rolls the session back. Completion by the daemon sends the existing
"Focus complete" notification after the restore.
A session written by the legacy helper (no owner marker) is mirrored (file watch) and
can be controlled, but the daemon never auto-completes or notifies for it, so QML and
the daemon cannot both notify. Damaged state reports unavailable and is never reset.

### Clipboard, screenshots, AI usage, Brain (B4)

- Clipboard rows come from the N0026 native index (cliphist IDs and capture times,
  seven-day retention, pins preserved). Refresh is driven by the cliphist database
  close-write event with a 300 ms quiet period. `clipboard.copy` runs the existing
  `wl-copy` path; `clipboard.clear` deletes every unpinned clip and its thumbnail
  (pins are the existing content fingerprints). Previews are the label QML shows;
  clip content never enters a topic, event or log.
- Screenshots list the newest six regular, user-owned, unlinked `png/jpg/webp`
  files directly in `Pictures/Screenshots` (QML lists the same). `screenshots.open`
  re-checks the file and starts `xdg-open`. The daemon never deletes or rewrites the
  files. The "30-day expiry" mentioned in planning belongs to the composer screenshot
  cache (`composer.rs`); the dashboard folder has no expiry today and none is added.
- AI usage is read only on `ai.usage.refresh` by running the existing
  `nacre-ai-usage read --agent codex|claude` as a bounded (12 s, 64 KiB) child of the
  same helper. Codex needs a provider connection, so nothing reads it unprompted,
  matching the QML page that reads on open/Refresh. Before the first read the topic is
  empty and unavailable without an error. A failed provider keeps its last rows stale.
- Brain recent reuses the N0020 vault root and the dashboard scanner
  (`brain_context_at`), refreshed by inotify on the vault and two directory levels
  (2 s quiet). Hardening: only regular files are read, so a symlinked note can no
  longer be followed out of the vault (an intended difference from the old reader).

### Customization writes (B5)

`dashboard-extras` is writable through the existing `settings.set {store,key,value,rev}`
for `enabled`, `order` and `weatherPhotos` (it already has write access and
registered keys; no new method and no change to `SettingsProviderWrite`). The same
revision fence, durable-after-150 ms write and publication apply. New guard: `enabled`
and `order` accept at most 32 lowercase page identifiers (`[a-z0-9-]`, 1..64 bytes) in
both `settings.set` and `settings.publish`; the frozen registry check, which accepts any
array, is unchanged. Bead reads `settings.store.dashboard-extras`.

## Bounds and safety

Frame, patch and topic limits are the standard ones. Heavy work (clipboard,
screenshots, AI usage, Brain) shares one slot and uses the two-thread blocking pool.
Strings are control-character-free and bounded in Unicode characters; paths are
absolute with no `..`, NUL or newline. Errors contain no paths or content. One
supervised service `dashboard-extras` owns all producers; the focus timer, clipboard
and screenshot watchers sleep in inotify when idle.

## Temporary duplicates and expiry

QML still reads `dashboard-focus.json` through the helper `focus-*` actions, the
clipboard through `desktop-extras clips`, screenshots through `FolderListModel` and
usage/Brain through helper reads, beside the new topics. Delete the QML/helper
duplicates 7 days after Bead's B0024 consumer is trusted (the focus file shape stays
until then). No `.rollback` copies and no second writer exist.

## Validation

Protocol: schema/Luau/transcript freshness, replay of five topic transcripts and the
settings transcript, bounds and call validation (4 tests). Daemon: 10 private-daemon
integration tests (temporary HOME, fake niri socket, fake `cliphist`, `wl-copy`,
`xdg-open`, `notify-send` and usage helper) cover advertisement, focus lifecycle,
DND ownership and release, expiry/restart, legacy mirroring and damaged state,
clipboard copy/clear/pins/privacy, screenshot filtering/open/stale, usage stale
rows, Brain symlink/hidden filtering with live refresh, and the B5 fence. The whole
`desktop` job is the acceptance gate; see the lead's report for exact results.
