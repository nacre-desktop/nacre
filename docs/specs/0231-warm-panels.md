# 0231 Warm panels: the launcher, the dashboard, the control centre and Strand park instead of rebuilding

The Shell half of Bead B0392 (Bead `4e25ba4b`, spec 0245 "warm sources"). Queue rows 73 (beat
Noctalia on interaction CPU and memory) and 12 (smooth 120 Hz opens).

## Goal

Before this unit, the launcher and the dashboard were rebuilt cold on every open after a view switch
or after the 120 s keep-alive: a new worker, a Luau compile, every daemon subscription and a full
raster. Bead measured that at 30.9 ms per open against 0.48 ms resident, and 301 raster builds
against 0 (0245). With B0392 a closed docked panel parks in Bead's warm cache 2 s after its close
settles, if its source declares a retained-byte bound and the host sets a warm budget. Nacre opts in
for exactly two panels: the launcher and the dashboard on Today (`dashboard-today`, the view the
dashboard toggle and its edge lip open). N0231b (owner, 2026-11-25: memory for smoothness) adds the
docked control centre (`control`, a cold open was 64 to 104 ms before content) and Strand (`wallpaper`,
extras `Kind::Wallpaper`). Nothing else parks.

## Interface (no new Bead API)

- `Runtime::warm_retained_bytes()` (`reload.rs`): `Some(WARM_PANEL_BYTES)` for the launcher and for a
  dashboard showing Today, `None` for every other view. A dashboard closed on another tab carries that
  page's view name (`PanelNavigated`), which only `nacre tab N` opens, so it keeps the keep-alive and
  cold reopen instead of holding memory. The bound is a fixed admission charge, not a measurement;
  Bead adds the worker's stack (at least 2 MiB) and its retained rasters in `Mirror::read`.
  N0231b: `Some(WARM_CONTROL_BYTES)` for a Runtime with the dock's status pages (`dock_status`, only the
  `control` view: the notification history, its peek and the battery popout stay `None`), and
  `Some(WARM_PANEL_BYTES)` for extras `Kind::Wallpaper`; the other extras kinds stay `None`.
- Strand is a retained panel (`retain_when_closed`): Bead keeps it live while it is the panel slot's
  view (B0392 parks only non-retained panels 2 s after close), and parks it when another view replaces
  it, if its close has settled. Every reopen of an opted-in source runs `resume`, the kept one too.
- `Runtime::resume()`: Bead stages `PanelVisibility(true)` first; for the dashboard that is the
  existing re-lease path (be3d40187: fresh leases through the page factory, the parked values until
  they have data). `resume` then
  1. polls producers: `Session::sources_current()`, else `Err` (Bead drops the parked worker and runs
     the factory);
  2. dashboard: `invalidate_projection()`, so the page projects again (clock, relative times).
     Control centre and Strand need nothing here: PanelVisibility(true) already does what a cold
     factory does. The dock leases its own topics again (a4c7a110b) and opens on the overview
     (`cc_home_on_open`: page home, no detail, scroll 0; the close cleared the hover stores), and a
     status pill's `CompanionModel`, sent after the select, still picks its section. Strand leases its
     three topics again and starts a new visit (`wallpaper_opened`: empty query, selection on the active
     wallpaper, `visit` + 1 so the strand rests on its pearl, a new peek), with fresh rows once the
     active wallpaper's lease has data;
  3. launcher: applies `launcher::Memory`, the state the bar's trigger writes before it shows the view
     (`open_launcher`: empty search, apps mode, home route, no selection, scroll 0), which a cold
     factory reconciles too (`with_launcher_memory`). When the search changes, `Menu::forget_results`
     makes the query start from no rows, as a cold open does, instead of keeping the last visit's
     rows while the reply is pending; the projection waits up to 20 ms for it (`project_initial`) and
     the menu selects the first row;
  4. stages that as an ordinary model update and returns its scene, or the accepted scene when nothing
     changed (the visibility transaction already laid it out at the current viewport).
- `Runtime::commit` and the release write `launcher::Memory` only when the launcher's own remembered
  state changed (`launcher_remembered`). Before, every commit rewrote it, so the palette and visibility
  commits of a parked launcher's reopen replaced the trigger's fresh state with the last visit's
  search (the trigger's `CompanionModel` never reaches a parked view). The bar's `state` reply still
  reads the newest of the two.
- `main.rs`: `options.warm_source_budget = WARM_SOURCE_BUDGET` for the core views.
- `dormant_ready` stays Bead's default. Both panels' workers ran a PrepareCpu/PrepareImages job in the
  tests below and still parked (B0392: a mirror read follows the last finished job).

## Budget and bound

| Constant | Value | Why |
|---|---:|---|
| `WARM_PANEL_BYTES` (bound per view) | 16 MiB | the Runtime's own heap, measured at about 2.5 MiB with fixture data |
| `WARM_CONTROL_BYTES` (control centre, N0231b) | 8 MiB | a Runtime the launcher's size; estimated, see below |
| `WARM_SOURCE_BUDGET` | 112 MiB (was 64) | the four charges: 51.5 MiB measured plus about 18 and 30 to 38 MiB estimated |

Each charge is at least 18 MiB (bound plus stack), so "2 x bound" (32 MiB) cannot admit both; as the
Bead lead asked, the budget is raised, not the bound. Only four view names opt in, so the cache never
holds more than these four. The bound stays at 16 MiB for real menus, dashboard data (more rows, the
Today poster) and Strand's catalogue and encoded thumbnails (at most 640x400 and 256 KiB each, about
11 shown), which the fixtures lack. The budget is an admission ceiling, not resident bytes: over it,
Bead evicts the oldest parked panel, whose next open is cold again.

## Memory cost (measured)

Headless, through the real Bead worker (`reload/warm_park_tests.rs`), CPU renderer at scale 1.5 on a
1920x1200 output, fixture data, laptop debug build. "Kept" is the glibc in-use heap (`mallinfo2`
uordblks + hblkhd) while parked minus after dropping the parked worker (the cold retire), in an
isolated child process; "caches" is the same difference of `bead_core::cache::report().used_bytes`;
"charge" is the worker's `warm_retained_bytes`, the `WARM_SOURCE_CACHE` admission bytes.

| Parked panel | Charge (admission) | Kept heap (resident) | of which Bead caches | Runtime rest |
|---|---:|---:|---:|---:|
| launcher | 24.81 MiB | 9.80 MiB | 7.26 MiB | 2.54 MiB |
| dashboard-today | 26.71 MiB | 11.58 MiB | 9.09 MiB | 2.49 MiB |
| both | 51.52 MiB | 21.38 MiB | | |

N0231b, estimated from the code (the focused run could not start: night-run froze on battery); the
memory test below prints the measured rows as `WARM_PARKED view=control` and `view=wallpaper`:

| Parked panel | Charge (estimate) | Kept heap (estimate) | Why |
|---|---:|---:|---|
| control | about 17 to 18 MiB | about 8 to 11 MiB | a CPU panel like the launcher: Runtime about 2.5 to 4 MiB, text and icon rasters about 7 MiB |
| wallpaper (CPU test) | about 30 to 38 MiB | about 11 to 20 MiB | Runtime 3 to 6 MiB (catalogue rows, encoded thumbnails); five centre pearls bake at 544 px at scale 1.5 (1.2 MB each), six far ones at 163 px, one glow |
| wallpaper (product, GL) | about 22 to 26 MiB | about 6 to 11 MiB | Strand draws on femtovg GL, whose worker cache (`UploadOnly`) drops decoded pixels after upload; parking drops the GL textures with the renderer |

So the two add about 14 to 22 MiB of resident heap in the product, under the coordinator's 45 MiB
guard (147 MiB after first use with two parked, gate 204 MiB). Decoded thumbnails do not stay in a
parked GL Strand, so it keeps them only as encoded bytes (about 3 MiB) and re-decodes them for upload
on reopen.

Expected after-first-use level (row 73, N0231): the live 158 MiB PSS + GPU with both panels released, plus
both parked: about 158 + 21 = 179 MiB from the heap measured here, at most about 186 MiB if each
parked panel keeps its released live cost (5 to 14 MiB, lead). Noctalia is at 210 MiB, so 24 to
31 MiB of headroom remain. CPU panels hold no GPU memory while parked. The live PSS check is the
lead's: these numbers exclude the worker stacks' touched pages, real icons and the Today poster.

## What evicts a parked panel

Bead evicts a parked source on any payload or protocol event for its epoch (`warm.rs`); payload-free
model hints are dropped. The tests assert that only hints arrive while parked.

- Dashboard: nothing that evicts. Its close drops every daemon lease (be3d40187); the preview gate and
  the poster thread send hints only, and `awake` is false while closed.
- Launcher: the menu topic and the query actor send hints. Its close drops the quick controls' own
  leases and subscriptions (`controls`, `notifications`, `dashboard.updates`, `capabilities`; row 73),
  so they send nothing while parked; PanelVisibility(true), which Bead stages before `resume`, leases
  them again and the first frame draws the tiles it closed on. The prefs subscription delivers a
  payload, so a settings change evicts a parked launcher; its next open is cold with the new settings.
- Control centre (N0231b): its close drops its own leases and subscriptions (a4c7a110b: `controls`,
  `vpn`, `power.profiles`, `notifications`, `displays`, `dashboard.updates`, the tile store). The
  dock's status topics (network, Bluetooth, battery, audio) are the bar's (`BarDaemon`), so the daemon
  serves them anyway; their subscriptions and the session menu's send hints only, and the microphone
  meter's `audio.peak` capture is released.
- Strand (N0231b): its close drops its three topics' leases (`wallpaper.active`, the wallpaper-picker
  store, `compositor` off niri) and every subscription (`Extras::park`, the `Kept` wrapper); in the
  shell the bar's wallpaper warmer holds the first two, so the reopen's leases have data at once. Its
  close also ends the peek, the warm hold, the palette hold and the prefetch, and clears the thumbnail
  retries, so no timer is due (only the boot-refresh poll of a Strand built before the daemon boot was
  known, for at most 30 s). A parked source's timers never run anyway. The landing pick and the peek's workspace call reply with hints only.
- All: a language switch (`Reload`) and a shell source change (file watcher) evict.
- `resume` fails, and Bead runs the factory, when the accepted sources or the palette file changed.

## Live markers to check

- `WARM_SOURCE_CACHE count=N admission_bytes=B` (`BEAD_VERBOSE_LOG=1`): count 1 about 2 s after the
  launcher's close settles, 2 once the dashboard closes too; it drops by one on each reopen. If it never
  appears, a resource job without a later mirror read is keeping the worker busy (B0392).
- N0231b: count 3 once the control centre is closed for 2 s too, 4 once Strand is replaced by another
  view after its close (a closed Strand stays retained, not parked, while it is the slot's view).
- `PANEL_OPEN_LATENCY name=launcher cold=false` (and `name=dashboard-today`, `name=control`,
  `name=wallpaper`) on a reopen after more than 2 s, where a switch or the keep-alive gave `cold=true`; `PANEL_SLIDE dir=open first_frame_ms`
  below the cold open's.
- `WORKER_BUSY session=panel:launcher` / `panel:dashboard-today`: no raster rebuild on a warm reopen.
- No `UI_ERROR`. The first reopened launcher frame has an empty search, no row of the last visit and
  the first row selected; the dashboard shows its last values, then fresh data. A lip-opened control
  centre shows the overview and a status pill its section; Strand opens with no query, resting on the
  active wallpaper, thumbnails drawn on the first frame or arriving after it, never a blank strand.

## Prewarm after a shell start (N0231c)

Parking only helps from the second open on, so once per shell start a one-shot thread
(`prewarm_panels`, `main.rs`) sends Bead's panel command `prepare` for `launcher`,
`dashboard-today`, `wallpaper` and `control`, 3 s apart, starting 10 s after startup. Each
prepared panel builds without mapping and parks at Bead's 2 s release, so the first user open is
`cold=false` too. Guards, checked at send time; any failure ends the sequence for good:
`WARM_SOURCE_BUDGET > 0`, the daemon link connected, `session.state` not locked, sleeping or
preparing to sleep, and no panel targeted open (`bead.panel state`). Markers:
`PANEL_PREWARM view=<v>`, `PANEL_PREWARM stopped reason=<disconnected|locked|panel_opened>`, and
Bead's `PANEL_PREPARE epoch=` (a skipped prepare prints nothing). The thread ends with the sequence.
A user hover-prepare in the first ~20 s isn't detected; a prewarm prepare of another view retires it.

## Rollback

`WARM_SOURCE_BUDGET = 0`: Bead keeps the 120 s keep-alive, the cold switch and both allocator trims, as
before (B0392), and never parks or resumes. The memory write that skips unchanged state is the one
behaviour left; it writes the same values as before whenever the launcher's state changes.

## Tests

- `only_the_launcher_today_the_control_centre_and_strand_opt_in_to_warm_parking`: a plain Runtime, a
  dashboard on Media, the notification history (a control centre without the dock) and the other
  extras kinds return `None`; Today and Strand return the bound, the dock `WARM_CONTROL_BYTES`.
- `a_parked_launcher_reopens_fresh_without_a_factory_call`: open, search, close, park (dormant, a
  charge, only hints), the trigger's fresh state, reopen through `resume`: empty search, no row of the
  search on the first resumed frame, one factory call.
- `a_parked_dashboard_reopens_on_fresh_data_without_a_factory_call`: the daemon's count changes while
  parked; the first resumed frame draws the fresh count, one factory call.
- `a_closed_launcher_holds_no_quick_lease_and_reopens_on_its_tiles` (`control_center_tests.rs`, row 73):
  the close releases the four quick leases and wakes nothing, a closed model change leases nothing, and a
  cold or `resume` reopen leases them again and draws the closed-on tiles on its first frame.
- `a_parked_control_centre_reopens_on_its_overview_without_a_factory_call` (N0231b): close on the edit
  page, park (dormant, a charge, only hints) with none of its seven own leases, reopen through `resume`:
  the overview, leased again, one factory call.
- `a_parked_strand_reopens_on_a_new_visit_and_the_active_wallpaper_without_a_factory_call` (N0231b):
  twelve 640x400 thumbnails; search, close, park with no `wallpaper.active` or `compositor` lease; the
  daemon's active wallpaper changes; the first resumed frame has no query and a caption, the leases
  return, then the caption names the new active wallpaper; one factory call.
- `parked_panels_keep_less_than_their_charge_and_all_four_fit_the_budget`: the tables above (printed as
  `WARM_PARKED` / `WARM_BUDGET` on stderr); it fails if a parked panel keeps more heap than its charge
  less the stack, or the four charges exceed the budget.

The host in these tests models Bead's CPU panel host (stage, one CPU prepare per Ready frame that sends
missing resources to the worker, ack, commit, park, resume). `warm::Cache` and the 2 s park timer are
Bead-private and covered by Bead's tests at the pin; the live open path is the lead's check.
