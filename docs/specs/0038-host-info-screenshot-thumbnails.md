# N0038 — Host info in `dashboard.system` and screenshot thumbnails

Status: **Implemented; pending lead review; unpublished**. Consumer: Bead B0031
(`docs/specs/0031-dashboard-media-system-finish.md` on Bead). The QML shell is
unchanged; its host card keeps reading `/etc/os-release` and `/proc` itself.

## Published interface (additive, optional)

`dashboard.system.host` (`SystemHost`, omitted when nothing is known; every member
independently optional, omitted when unknown, patches use null) covers exactly what
`NacreHostCard.qml` shows:

| Member | Meaning |
| --- | --- |
| `user` | login name of the daemon user (QML: `$USER`) |
| `os` | os-release `PRETTY_NAME`, else `NAME` (QML falls back to "Linux" itself) |
| `kernel` | `/proc/sys/kernel/osrelease` |
| `uptime_s` | seconds since boot when the subscription started |
| `boot_ms` | Unix ms of boot; clients count uptime as `now - boot_ms` |

Strings are 1-64/128 chars without control characters. The block is read once when a
subscription starts (not per tick) and cleared on the last unsubscribe. Memory total is
already `memory.total`. Not published, because the host card does not show them:
hostname, CPU model; the "Hyprland" line is a QML constant.

`screenshots` items gain optional `thumb_path`: absolute PNG, longest side at most
256 px (smaller captures keep their size), in the private cache
`$XDG_STATE_HOME/nacre/shell/screenshot-thumbs` (0700 dir, 0600 digest-named files, via
the same `ImageCache` as player art). Only the at most six listed captures get one;
sources are opened by symlink-free descriptor walks, up to 32 MiB and the shared pixel
limits. Undecodable captures simply have no `thumb_path`.

## Producer behavior

Thumbnails exist only while the `screenshots` topic has a subscriber: the producer
watches topic interest, builds thumbnails on the first refresh after a subscribe, and
on the last unsubscribe deletes the cache files and drops the topic's thumb paths.
A capture is decoded once per (mtime, size) identity (failures included); unlisted
thumbnails are removed after each refresh.

## Verification

- `cargo test -p nacre-protocol` (host members, rejections, compat transcript).
- Synthetic `/proc` + fake `/etc` tests for the host block and the lazy lifecycle.
- Private-daemon test: real PNGs scaled to 256 px, small image unscaled, broken image
  without thumb, modes 0700/0600, cache emptied after the client leaves.
- Full desktop job on a build host (README entry).

## Temporary duplicates and expiry

QML host-card file reads stay until Bead B0031 is trusted; delete them one week
after (same rule as N0033).
