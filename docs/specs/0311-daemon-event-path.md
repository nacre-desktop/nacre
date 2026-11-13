# N0311: Daemon event-path diet

**Goal.** Nacre's daemon CPU and wakeups at or below Noctalia's (owner,
2026-11-13). Audit A (daemon services) and review E (architecture, rec 3) found
that most per-event executor work was avoidable: file reads per compositor
event, 75 store re-reads per inotify event, double serialization per publish,
D-Bus fan-out per signal. This unit lands the verified items, the smallest
change each, one commit each.

## Changes

| Item | Before | After |
|---|---|---|
| A6 + E3 hub publish | `to_vec` + `encode(clone)` per publish, before the dedup check; one `patch::diff` per connection | Dedup first, one `framing::encode` (its length is the byte account); the last `(base, seq, diff)` per topic is reused by every connection at the same base |
| A7 method registry | Module method maps parsed embedded JSON schemas (`core-service-call.json` is 20 KB) on every call, `ui.*` forward and welcome | The nine modules with small schemas build their map once (`LazyLock`; the launcher already did); provider (`ui.*`) call deadlines skip the registry; `available_methods` reads the compositor snapshot by reference |
| A1 active wallpaper | `refresh` (6 file reads + image header sniff) on every `compositor` change | Refresh only when `(available, output keys, focus.outputId, collections)` changes; `collectionWallpapers` from the `settings.store.desktop` snapshot (file fallback without the settings service); file changes still refresh through the wallpaper watcher |
| A2 settings watcher | Every inotify event re-read all 75 stores and re-armed every watch | The event's file names select the stores; temp files of atomic writes are ignored; a directory event, a nameless event or an overflow re-reads all and re-arms |
| A8 observers | weather and weather photo cloned snapshots on every notice of any topic; the launcher cloned the compositor snapshot per change | Notices filtered by topic first (the weather photo reads its inputs once when a lease starts instead of waiting for an unrelated notice); the launcher reads the snapshot by reference |
| A3 network | A second publish per NetworkManager signal fed `network.watch` (`generation+1`); every AP `Strength` signal triggered a full `read_raw` | `network.watch` topic, its generation plumbing, the daemon-backed `network-state watch` client, protocol type, schema, Luau type, fixtures and docs removed (no consumer outside fixtures; the standalone `network-state watch` stays). AP `PropertiesChanged` carrying only `Strength`/`LastSeen` re-reads at most every 30 s |
| A4 tray | Every StatusNotifierItem signal re-read every item (2+5N D-Bus calls); every session-bus name change cost a round trip | Registered names and per-owner rows kept locally; an item signal re-reads that item with one `GetAll`; `NameOwnerChanged` matched on the watcher name (`arg0`) and on lost names (`arg2=''`); the owner-wait loop no longer re-polls on unrelated names |
| A5 NameOwnerChanged | 5 unfiltered subscriptions (behaviours/system power, bluetooth, player, notifications x2); system power did 11 property gets per signal | Matched on `arg0` (service name), `arg0namespace` (MPRIS) or `arg2=''` (lost names); system power does one `GetAll` per interface |
| A9 dashboard metrics | `niri msg version` spawned synchronously on the executor per `dashboard.system` subscription | Read on the first subscription through `process::offload`, kept once known |
| A10 control centre | 2 s tick: `systemctl --user is-active` child plus 2 fresh offload threads per tick | Units read on the slow tick (every 5th) and on every toggle (full refresh); `process::offload` hands work to an idle offload thread (kept 30 s) instead of spawning one per call; `OFFLOAD_LIMIT` still bounds concurrent work |
| Deletes | `nacrectl attach` (QML byte bridge), the Quickshell `qs` session-state fallback, the dead tray owner branch | Removed |

Kept, with reasons:

- The NetworkManager persistent signal stream stays as the core reader's restart
  trigger; it no longer publishes. An owner-only trigger would stop retries after
  a transient owner-check failure while NetworkManager keeps running.
- The `helper.preferences` alias: it is reached only through the
  `helper.execute` bridge, which waits for the owner's decision with
  `wallpaper.watch` and `niri.widths`.
- `active_wallpaper::is_busy`: the audit was wrong; `SelectionGuard::is_busy`
  gates `previousAvailable`.
- The nested `workspace` copy in `compositor` window rows:
  `menu.rs` `compositor_inputs` reads `window.workspace.id` and `.name` from the
  topic, so the field is not dead.
- The method registry is still assembled per call: its contents depend on live
  service state, and only shell-less clients (`nacrectl`, helper tools) send
  calls without a timeout. Bluetooth, network, player and spectrum embed the
  20 KB `core-service-call` `$defs` in every method, so their maps are not
  cached: a first run that cached them measured +0.6 MiB anon at idle and
  +1.1 MiB after interaction, over the +0.5 MiB gate.

## Measurements

Paired headless runs on dev-2 (Arch container): headless labwc with a nested
niri, one `foot` window whose title changes at 10 Hz during the interaction
window. The base (`fcd337c0c`) and N0311 daemons run at the same time against
the same compositor, each with a private HOME, XDG dirs and session bus (also
its system bus) and the N0300 static-wallpaper fixtures. Each round: idle
window t = 10–130 s; interaction window t = 150–215 s with 12 launcher calls,
18 topic subscriptions (`dashboard.system`, `dashboard.player`,
`dashboard.weather`, `controls`, `menu.providers`, `wallpaper.active`),
12 notifications and 12 workspace switches. CPU and context switches are
summed over all daemon threads from `/proc`. Three rounds, medians.

| Metric | Base | N0311 |
|---|---:|---:|
| Idle CPU, % of one core | 0.000 | 0.000 |
| Idle context switches/s | 0.2 | 0.0 |
| Interaction CPU, s per 65 s window | 0.77 (1.18 %) | 0.62 (0.95 %) |
| Interaction context switches/s | 20.1 | 17.4 |
| RssAnon after the idle window, MiB | 19.2 | 19.4 |
| RssAnon after the interaction window, MiB | 20.9 | 21.3 |
| Threads | 3 | 4 (one pooled offload thread) |
| `compositor` bytes per patch, one subscriber | 131 | 131 |

Interaction CPU drops 19 % and context switches 13 %. This headless session has
no idle activity for either daemon, so the idle CPU column cannot show the
per-event savings (wallpaper refresh, settings re-reads, NetworkManager, tray
and owner signals) that a live desktop pays at idle; the live laptop audit is
the acceptance. Memory stays within the +0.5 MiB gate. Patch bytes are
unchanged because the nested `workspace` copy stays (see above); encode-once
and shared diffs save daemon work, not wire bytes. Both arms predate N0309's
single malloc arena (N0311 was measured on the base `fcd337c0c`).

## Checks

On dev-2, in the Arch CI environment:

- every commit: `cargo check -p nacre-helper -p nacre-protocol --tests` passes;
- `cargo fmt --check`, Ruff 0.16.10 format and lint on the `tools/check.py`
  Python set, `tools/compositor_boundary.py`, and
  `cargo clippy -p nacre-helper -p nacre-protocol --all-targets -- -D warnings`
  pass;
- `cargo test -p nacre-protocol` passes except `generated_contract_is_fresh`:
  N0213 (`737acfd81`) added `MenuQuery.startup` without regenerating
  `protocol/luau/menu.d.luau`; this push regenerates it in its own commit. The
  protocol minimal consumer passes;
- `cargo test -p nacre-helper` in `nacre-ci/arch-nacre` (no network): the lib
  suite passes (1084), and the integration targets pass except two
  `lock_dashboard` cases that download their frozen font (no network in the
  job) and `palette_pipeline::whole_publication_…`, red on main since N0213 and
  fixed by `75b8ad434`, which this series is rebased on.

New or changed tests: the tray fixture checks the `GetAll` row and that a lost
registered name reaches the `arg2=''` match; the settings inotify test drives
the named refresh and checks that it is not a full re-read; the control-centre
backend test treats unit states as slow members; the existing weather-photo
location test caught a lease that waited for an unrelated notice. The `network.watch` daemon test, the attach
bridge test and their fixtures are deleted with the code.

No temporary duplicate paths were added. Net: −517 lines outside this spec
(product sources −69, tests −392, protocol artifacts and architecture docs −56).
