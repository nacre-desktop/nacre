# 0231 Warm panels: the launcher and the dashboard park instead of rebuilding

The Shell half of Bead B0392 (Bead `4e25ba4b`, spec 0245 "warm sources"). Queue rows 73 (beat
Noctalia on interaction CPU and memory) and 12 (smooth 120 Hz opens).

## Goal

Before this unit, the launcher and the dashboard were rebuilt cold on every open after a view switch
or after the 120 s keep-alive: a new worker, a Luau compile, every daemon subscription and a full
raster. Bead measured that at 30.9 ms per open against 0.48 ms resident, and 301 raster builds
against 0 (0245). With B0392 a closed docked panel parks in Bead's warm cache 2 s after its close
settles, if its source declares a retained-byte bound and the host sets a warm budget. Nacre opts in
for exactly two panels: the launcher and the dashboard on Today (`dashboard-today`, the view the
dashboard toggle and its edge lip open). Nothing else parks.

## Interface (no new Bead API)

- `Runtime::warm_retained_bytes()` (`reload.rs`): `Some(WARM_PANEL_BYTES)` for the launcher and for a
  dashboard showing Today, `None` for every other view. A dashboard closed on another tab carries that
  page's view name (`PanelNavigated`), which only `nacre tab N` opens, so it keeps the keep-alive and
  cold reopen instead of holding memory. The bound is a fixed admission charge, not a measurement;
  Bead adds the worker's stack (at least 2 MiB) and its retained rasters in `Mirror::read`.
- `Runtime::resume()`: Bead stages `PanelVisibility(true)` first; for the dashboard that is the
  existing re-lease path (be3d40187: fresh leases through the page factory, the parked values until
  they have data). `resume` then
  1. polls producers: `Session::sources_current()`, else `Err` (Bead drops the parked worker and runs
     the factory);
  2. dashboard: `invalidate_projection()`, so the page projects again (clock, relative times);
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
| `WARM_SOURCE_BUDGET` | 64 MiB (4 x bound) | the two charges, 51.5 MiB measured |

Each charge is at least 18 MiB (bound plus stack), so "2 x bound" (32 MiB) cannot admit both; as the
Bead lead asked, the budget is raised, not the bound. Only two view names opt in, so the cache never
holds more than these two. The bound stays at 16 MiB for real menus and dashboard data (more rows,
the Today poster), which the fixtures lack.

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

Expected after-first-use level (row 73): the live 158 MiB PSS + GPU with both panels released, plus
both parked: about 158 + 21 = 179 MiB from the heap measured here, at most about 186 MiB if each
parked panel keeps its released live cost (5 to 14 MiB, lead). Noctalia is at 210 MiB, so 24 to
31 MiB of headroom remain. CPU panels hold no GPU memory while parked. The live PSS check is the
lead's: these numbers exclude the worker stacks' touched pages, real icons and the Today poster.

## What evicts a parked panel

Bead evicts a parked source on any payload or protocol event for its epoch (`warm.rs`); payload-free
model hints are dropped. The tests assert that only hints arrive while parked.

- Dashboard: nothing that evicts. Its close drops every daemon lease (be3d40187); the preview gate and
  the poster thread send hints only, and `awake` is false while closed.
- Launcher: the menu topic and the query actor send hints. The quick controls (`controls`,
  `notifications`, `dashboard.updates`, `capabilities`) keep their leases while parked and send hints
  only. The prefs subscription delivers a payload, so a settings change evicts a parked launcher; its
  next open is cold with the new settings.
- Both: a language switch (`Reload`) and a shell source change (file watcher) evict.
- `resume` fails, and Bead runs the factory, when the accepted sources or the palette file changed.

## Live markers to check

- `WARM_SOURCE_CACHE count=N admission_bytes=B` (`BEAD_VERBOSE_LOG=1`): count 1 about 2 s after the
  launcher's close settles, 2 once the dashboard closes too; it drops by one on each reopen. If it never
  appears, a resource job without a later mirror read is keeping the worker busy (B0392).
- `PANEL_OPEN_LATENCY name=launcher cold=false` (and `name=dashboard-today`) on a reopen after more
  than 2 s, where a switch or the keep-alive gave `cold=true`; `PANEL_SLIDE dir=open first_frame_ms`
  below the cold open's.
- `WORKER_BUSY session=panel:launcher` / `panel:dashboard-today`: no raster rebuild on a warm reopen.
- No `UI_ERROR`. The first reopened launcher frame has an empty search, no row of the last visit and
  the first row selected; the dashboard shows its last values, then fresh data.

## Rollback

`WARM_SOURCE_BUDGET = 0`: Bead keeps the 120 s keep-alive, the cold switch and both allocator trims, as
before (B0392), and never parks or resumes. The memory write that skips unchanged state is the one
behaviour left; it writes the same values as before whenever the launcher's state changes.

## Tests

- `only_the_launcher_and_the_dashboard_on_today_opt_in_to_warm_parking`: a plain Runtime and a
  dashboard on Media return `None`, Today returns the bound.
- `a_parked_launcher_reopens_fresh_without_a_factory_call`: open, search, close, park (dormant, a
  charge, only hints), the trigger's fresh state, reopen through `resume`: empty search, no row of the
  search on the first resumed frame, one factory call.
- `a_parked_dashboard_reopens_on_fresh_data_without_a_factory_call`: the daemon's count changes while
  parked; the first resumed frame draws the fresh count, one factory call.
- `parked_panels_keep_less_than_their_charge_and_both_fit_the_budget`: the table above (printed as
  `WARM_PARKED` / `WARM_BUDGET` on stderr); it fails if a parked panel keeps more heap than its charge
  less the stack, or the two charges exceed the budget.

The host in these tests models Bead's CPU panel host (stage, one CPU prepare per Ready frame that sends
missing resources to the worker, ack, commit, park, resume). `warm::Cache` and the 2 s park timer are
Bead-private and covered by Bead's tests at the pin; the live open path is the lead's check.
