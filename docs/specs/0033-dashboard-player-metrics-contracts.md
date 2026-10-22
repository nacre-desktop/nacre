# N0033 — Dashboard player (MPRIS) and system metrics contracts

Status: **Implemented; unpublished**. Consumer: Bead B0024 blockers B1 and B3
(`docs/specs/0024-dashboard-extras-parity.md` on Bead origin/main). N0033 was the
next free number on origin/main at commit time; N0030-N0032 are other worktrees.
The QML shell is unchanged: `NacrePlayers.qml`, `NacreSystemUsage.qml`,
`NacreDashboardSystem.qml` and `NacreGpuMemory.qml` remain the shell's own sources.

## Published interface (MIT OR Apache-2.0 types in `nacre_protocol::core_services`)

Both topics are advertised outside daemon safe mode. A subscriber's first snapshot
with `health.available=false`, no error and no players or readings is the starting
state; the producer fills it within about a second.

### `dashboard.player` v1

`{health, active?: EntityId, players: Entities<Player>}`. IDs are daemon-issued
(`player-N`), never reused within a boot, at most 16 players. `Player`:
`identity`, `status` (`playing|paused|stopped`), `title`, `artist`, `album` (bounded
text, empty when unknown; clients keep the QML fallbacks "Unknown track" and
identity), optional `art_path`, optional `position_ms` + `position_updated_ms`
(always together), optional `length_ms`, and `can_control`, `can_play_pause`,
`can_next`, `can_previous`, `can_seek`. Unknown position or length is **omitted,
never zero**. `can_*` are already gated: false unless `can_control`; `can_play_pause`
follows pause while playing and play otherwise (Quickshell `canTogglePlaying`);
`can_seek` also needs a track ID, a position and a length (QML `canSeek`).

`position_ms` is the reading at `position_updated_ms` (Unix ms). While `playing`
clients extrapolate locally; the daemon republishes only on seek, status, track and
capability changes, never per frame. `active` is the player QML would show: the
latest to start playing while it still plays, else any playing player, else that
latest, else the first. The QML manual override and its preference for players with a
local PipeWire stream stay client-side (needs the `audio` topic).

`art_path` is an absolute path in the private daemon cache
(`$XDG_STATE_HOME/nacre/shell/player-art`, 0700, digest-named PNG re-encodes via the
notification `ImageCache`). Sources: local `file://`/absolute path, `data:` base64
URI, or `https://` (8 s, 4 MiB, redirects stay https). Everything is decoded and
re-encoded (4 MiB, 8192 px side, 16 MP limits); remote URLs never cross the
protocol, failures leave the art unknown, URLs are never logged. Only art of listed
players is kept; the cache is emptied when the last subscriber leaves.

Methods (desktop capability, advertised only while the producer runs, params are the
generated `CoreCall` variants, result `{changed:true}` after the player accepted the
D-Bus call; topic state stays authoritative):
`player.play_pause|next|previous {player}`, `player.seek {player, position_ms}`.
Errors: unknown/vanished ID `stale_revision`; an ability the topic reports false
`permission_denied`; seek beyond length `validation`; producer idle or bus failure
`unavailable` (retryable, `dispatched:true` after the call left).

### `dashboard.system` additive members (compat transcript `core-compat-system-metrics`)

Old fields are unchanged. New optional members, omitted when unknown (patches use
null): `cpu_percent` (0-100, needs two readings), `memory {used,total}` bytes
(`MemAvailable` based, QML fallback formula), `gpu {percent?, memory_used?,
memory_total?}` for the busiest GPU (percent 0-100; VRAM from amdgpu sysfs or NVML,
Intel and unknown stay omitted), `network: [{down_bps, up_bps}]` oldest first, at
most 32 samples. **`_bps` means bytes per second** (names fixed by B0024). Network
follows the default-route interface (QML prefers the Wi-Fi interface while connected;
they coincide on a Wi-Fi default route) and clears its history when that changes.
`AccessPoint.frequency_mhz?` (`network` topic, compat transcript
`core-compat-network-frequency`) is the NetworkManager `Frequency` in MHz, omitted
when unreadable.

The topic now has a daemon producer: processes (top 12 by memory, same user) and
disks reuse the helper `dashboard-system` snapshot (`pid:start` identity, consumer
derives per-process CPU from `cpu_seconds`). Process termination stays out of scope.

## Producer behavior

- Lazy: both producers wait on a hub interest watch and do nothing until a client
  subscribes. On the last unsubscribe they stop and the topic returns to its
  starting state (no retained rows, art or history).
- Player: no polling. One D-Bus connection per subscription lifetime; the
  `NameOwnerChanged` and `/org/mpris/MediaPlayer2` signal streams mark players dirty
  and a 75 ms coalescing window re-reads only those players (`GetAll`). Art fetches
  run one at a time on the blocking pool.
- Metrics: first sample immediately, second after 500 ms (CPU/network rates need two
  readings), then every 2 s (`/proc/stat`, `meminfo`, `net/dev`, process and disk
  scan in the blocking pool, skipped while the previous scan runs). GPU every 10 s.
  Memory is bounded by the 32-sample ring, 12 processes, 12 disks and 16 players.
- Failures: a listing failure marks health unavailable (stale when rows exist) with a
  sanitized retryable error; a lost session bus keeps the last list stale and retries
  every 5 s while subscribed.
- `resource_state` gained optional `memoryUsed`/`memoryTotal` NVML members on GPU
  records (`resource-state` helper output gets two extra keys; QML ignores them).

## Verification

- `cargo test -p nacre-protocol`: typed replays of `core-dashboard-player`, the three
  new compat transcripts and impossible-value rejection (generated with
  `cargo run --example export`, `--check` clean).
- Private-bus test: two fake MPRIS players, art copy, controls and refusals,
  property-signal updates, vanishing player, idle reset, no cache leftovers.
- Synthetic `/proc` and `/sys` tests: CPU/memory/network deltas, bounded history,
  route change, GPU members, lazy start/stop with a no-sampling-while-idle check.
- Full desktop job on a build host (see README entry).

## Temporary duplicates and expiry

The QML services above and the helper `dashboard-system`/`resource-state` tools are
untouched duplicates of these producers. Delete the QML sampling paths one week after
the QML shell reads these topics and Bead B0024 is trusted on the owner desktop.
Remote art needs live HTTPS and is untested (no network in tests).

## Not done (one line each)

Seek track-ID race guard, shuffle/loop/raise (QML-only today), CPU/GPU temperature,
host card, wiring QML to the topics.
