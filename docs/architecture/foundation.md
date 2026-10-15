# Nacre foundation

One persistent Rust daemon owns desktop state and outside connections.
Every Nacre UI reads snapshots and sends actions over one private socket.
Only the daemon writes registered settings; existing data and backup rules stay.
Heavy or risky work runs in bounded workers that leave memory when idle.
The migration proceeds through measured, tested steps without changing the live desktop.

Approved 2026-10-15. This charter redirects Wave 4 through F1–F4 first.
The wire contract is in [protocol.md](protocol.md); implementation status is in
[rust-migration.md](../development/rust-migration.md). This is the target contract,
not a claim that all services or measurements are already implemented.

## 1. One service process

`nacre-daemon` is `nacre-helper daemon`, installed under another name. Its systemd
user `.socket` and `.service` belong to `graphical-session.target`; the service uses
`Type=notify`, `WatchdogSec=20`, `Restart=on-failure`, `RestartSec=0.5`. It also
runs standalone with `NACRE_DAEMON_SOCKET` for nested tests and CI.

It owns desktop state and outside connections: compositor (one connection per
compositor), settings/state (sole writer), notification server and history,
launcher engine, session/display policy, niri widths, network, dashboard/media,
theme/wallpaper publication; later audio/tray/bluetooth/power/clipboard from
bead-services when the Bead shell needs them. The shell supervisor stays the UI's
parent, so a daemon crash does not take the UI down.

There is one async runtime thread (`tokio` current-thread), a blocking pool of at
most two threads, and a separate PipeWire thread. Each service is one task owning
its state. A failing service restarts alone with backoff from 0.25 to 30 seconds,
and stays off after five failures in two minutes. Three daemon crashes in two
minutes trigger safe mode: compositor, settings and notifications only, plus one
notification. Heavy or risky work (image/album-art decode, palette analysis,
fetches, software actions, AI, Brain) runs in daemon-managed worker processes at
nice 10, with timeouts and a concurrency cap; they exit after 30 seconds idle.
Use events; timers exist only where a source has no events. Crash reports contain
no window titles, notification text or personal paths. Errors remain visible.

## 2. One link: nacre-protocol

The private link is `$XDG_RUNTIME_DIR/nacre/daemon.sock`, in a 0700 folder, with
same-UID checks through `SO_PEERCRED`. Each line is one JSON object, at most
1 MiB. Blobs travel as file paths, never base64. Messages are hello/welcome
(protocol/minimum versions, boot ID, topic and method versions), sub/unsub,
snap/patch (RFC 7396, per-topic sequences and resync on a broken chain), call/ret
(ID, method, parameters, timeout; errors have code, message, retry), ev (256 events
per client, lost on overflow), ping/pong.

Collections are objects keyed by stable ID; ordering is a separate `order` list.
State delivery coalesces latest wins per client and topic, comparing against the
last state sent to that client, never faster than a frame. A client unable to
read for ten seconds is dropped. Within a version only optional fields are added
and clients ignore unknown fields; breaking changes get a new version, serving
current and previous. Rust types generate committed JSON Schema, Luau definitions
and golden transcripts tested by the daemon and every client. Compositor
snapshots are consistent even though niri's event stream is not atomic.

## 3. One settings writer

Only the daemon writes the 65 stores in `nacre-state-files.json`.
`settings.set {store,key,value,rev}` checks the revision, validates through the
Settings schema registry, updates memory, publishes a patch, and atomically writes
after 150 ms quiet using the existing backup rules. Inotify observes hand edits;
invalid edits keep the last good value and report file and line. Existing
privilege boundaries, root-owned operations and provider-owned credentials stay
intact. The daemon does not acquire arbitrary privileged file-writing authority.
See [versioned-state](../development/versioned-state.md) and
[settings-architecture](../development/settings-architecture.md).

## 4. UIs are views

The QML shell, Bead shell, Settings/Colors/Welcome windows, keybindings and scripts
are clients. They keep view state only. A disconnected UI keeps its last snapshot,
shows a small reconnecting mark after two seconds, retries from 50 ms to two
seconds, and resubscribes on a new boot ID. Keybindings use native `nacrectl`
(the helper under another name), with no shell or Python in that path. `ui.*`
calls go to whichever shell registered as the UI provider. Existing `nacre`
commands continue working; `qs ipc` is a compatibility fallback for one release.

## 5. Services outside the view

MIT-licensed bead-services are headless libraries run by the daemon. Long-term,
the Bead shell keeps no independent notification server, tray, audio or compositor
backends, launcher catalog or ranking. The launcher engine is Nacre's menu code,
served as `menu.query`. If the hop costs too much of Bead's 8 ms per-key CPU
budget, Bead links the library in-process instead; measurement decides.

## 6. Repository boundary

Nacre (GPL-3.0-only) owns nacre-helper, nacre-protocol and, after Gate D,
nacre-shell. Bead (MIT/Apache) contains the framework only: code flows Bead to
Nacre, never GPL into framework crates. Nacre pins `deps/bead` like Orient when
the daemon starts hosting bead-services; then its minimum Rust becomes 1.99.
The migration agent owns Nacre and nacre-protocol. The Bead agent may add optional
fields, methods and transcripts through Nacre's contribution workflow. Breaking
protocol changes need the owner. Do not move nacre-shell between repositories
mid-feature; resolve its command-name collision at Gate D.

## 7. Validation rails

Golden transcripts, property tests and nightly cargo-fuzz cover decoding, merge
patches, settings validators, desktop entries and image headers. A nested
three-hour chaos soak repeatedly kills the daemon: UI fresh within one second,
no notification missing from history, no leftover processes, flat memory.
Budgets warn first and fail after two clean weeks. No self-hosted GitHub runner
on public repositories. Existing frozen parity fixtures and measurement gates
remain mandatory. F1 adds fuzz targets; nightly jobs and the full soak are a
later brief, not acceptance claims for F1–F4.

## 8. Budgets

| Measure | Target |
| --- | --- |
| Daemon idle PSS | Measured baseline +20%; flat over three hours (±2 MiB) |
| Idle wakeups | At most one per second |
| Local call p99, excluding work | At most 1 ms |
| UI fresh after daemon restart | At most 1 s |
| Always-on helpers | 7–9 to one, plus UI |
| Keybinding to launcher | Measure before/after; no bash or qs startup in path |

Nested results are labelled nested. Keep the frozen Wave 3 sources and pending
desktop-PC A/B/A/B plan unchanged; missing measurements are not zeros.

## 9. Scope and sequence

Keep Bead's renderer and Luau, hyprlock, versioned stores, parity fixtures and
measurement gates. Do not rewrite working QML merely to route it through the
daemon: move a feature when touched or needed by Bead. Do not move Quickshell's
PipeWire/tray now, build a native lock, use D-Bus/Varlink/gRPC/binary encoding for
the private link, or move nacre-shell mid-feature.

F0 records this charter and protocol. F1 builds the daemon/link and read-only
compositor service. F2 folds in always-on helpers, one commit each. The launcher
engine step below follows F2. F3 adds the QML client, launcher query client and
repository-default keybindings. F4 makes settings single-writer
and then stops for a report. CI must be green before each next step.

Owner sequencing update, 2026-10-15: port the current `services/app-search.js`,
`modules/launcher/menu-model.js` and `app-browser.js` ranking/provider behavior to
a Rust library hosted by the daemon immediately after F2, before F3. Publish
`menu.query` request/result schemas and golden transcripts in nacre-protocol as
the first commit, so Bead can build its client. Freeze current JS ordered results
for query/catalog fixtures before porting; match them exactly. Index application
folders with inotify, keep frecency in the existing launcher-preferences store
with one writer, and reuse menu-backend's special searches as providers. Add
`menu.activate` only if needed. Measure daemon CPU/query and p50/p99 at realistic
catalog sizes, aiming well below 1 ms per keystroke. F3 switches the QML launcher
behind the same fallback setting; its JavaScript path stays for one release.

Bead proceeds independently with F5 headless services, deterministic testing,
crash safety, data sources and compositor blur. Hosting its notification server
and nightly fuzz/soak jobs are deferred.
The rest of the migration follows one rule: long-running or frequently called
work becomes a daemon service; rare, heavy or risky work becomes a worker.

Enabling live daemon units, switching notification ownership and changing live
keybindings each require an explicit owner decision. Tests use private HOME/XDG,
D-Bus and nested compositors; nothing here authorizes live deployment.
