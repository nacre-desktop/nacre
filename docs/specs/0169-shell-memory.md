# N0169 — idle shell memory

Status: implemented; final dev-1 batch running. Laptop acceptance is held
by the coordinator; performance follow-ups belong to Platform.

The real `shell/` and a private daemon run under headless labwc/pixman with
2880×1800 at scale 1.5 and 3440×1440 at scale 1. Panels use CPU fallback;
llvmpipe is loaded, with no DRM device. The desktop TEST host was unreachable.
Main-process smaps values exclude the separate video helper and compositor.
Both arms use identical helpers and release debug=1/strip=false flags.

## Attribution and fix

The fixture reproduces about 113 MiB anonymous PSS after opening the wallpaper
picker: about 52.3 MiB retained CPU rasters, 14–22 MiB decoded warm images, and
the remaining baseline, Luau, metadata, native libraries and allocator arenas.
Individual Luau heaps are about 0.6–1 MiB; font/glyph counters are small rather
than the dominant cost. These targeted counters explain the fixture, not an
exact partition of the owner's original PID: the supplied smaps excerpt itself
has 68.2 MiB anonymous PSS rather than the reported 113 MiB.

Bead reuses its guarded close-snapshot lifecycle for CPU panels, compresses
snapshots off the UI thread and releases matching prepared frames only after
acceptance. GPU resources retire before CPU snapshot budget admission. Generic
`SceneSource::release_prepared` and `Renderer::resume_prepared` hooks preserve
pending transactions and active animation ownership. The CPU preparation hands
its already-baked dense cache to the source actor, preserving the original frame
identity used by close-release guards; matching imported image rasters are reused.
The handoff retains the existing 128 MiB snapshot scratch ceiling, adding no cache.

Static wallpaper decoded vectors are dropped after SHM attachment. Wallpapers
use at most two buffers per surface; active ordinary surfaces briefly use three
and compact at idle. Hidden-panel SHM returns near idle. The retained picker
runtime stays warm; ordinary panel runtimes expire after 120 seconds. Video
software decoding costs roughly 80–90 MiB in a separate helper, not the main PID.

## Diagnostic, dev-1 nested

Valid short diagnostics, **not ten-minute acceptance**, use common Nacre
`c5184888`, Bead `7ea19860` → `8a34629e`; the pushed `ed03c8ee` preserves both
framework patches exactly after a trivial CI/dead-code-annotation rebase.
The current main also includes later launcher/product changes outside this unit.

| Main process, MiB | Anon before → after | SHM before → after | File before → after | PSS before → after |
| --- | ---: | ---: | ---: | ---: |
| Static, ready | 36.6 → 36.5 | 20.7 → 20.7 | 23.0 → 23.3 | 80.3 → 80.4 |
| Static, picker reopen closed | 112.7 → 64.1 | 20.7 → 20.7 | 78.1 → 78.7 | 211.5 → 163.4 |
| Video, ready | 37.6 → 37.9 | 20.7 → 20.7 | 23.2 → 23.0 | 81.5 → 81.5 |
| Video, 30 s after interactions | 114.9 → 69.9 | 20.7 → 20.7 | 78.7 → 78.5 | 214.3 → 169.0 |

First-hover control/candidate is 7.36/12.07 ms (+4.71 ms); reopen is
7.68/11.42 ms (+3.74 ms). Steady inside-hover medians stay near 2 ms; picker
body pixels match. The coordinator accepted this trade-off on 2026-11-12.
**Follow-up:** verify live journal markers after deploy: 120 Hz target, gaps
≤12 ms. Headless timings do not certify physical presentation smoothness.

The final batch targets five alternating pairs per wallpaper mode, with 600 s
idle before and after dashboard, launcher and picker interactions. It samples
checkpoints rather than sixty-second windows. Current final-batch result: pending.

| Ten-minute checkpoint | Before | After |
| --- | --- | --- |
| Static, initial idle | Pending | Pending |
| Static, after interactions | Pending | Pending |
| Video, initial idle | Pending | Pending |
| Video, after interactions | Pending | Pending |

## Guards and validation

The coordinator's final dev-1 waiver permits pre-existing swap and **≤8 MiB
global host swap-in per arm**, provided measured SwapPss=0 and RAM/OOM guards
hold. Other tenants' swap-in is outside the unit's scope. Desktop TEST captures
retain their strict <64 MiB pre-used-swap rule. RAM must be ≥4 GiB before launch
and ≥2 GiB continuously; missing coverage or gaps invalidate an arm. Fresh
init-managed containers use six CPUs, 12 GiB and 2,048 PIDs, under the remote
slot runner. The monitor reads host `/proc` once per second without periodic
Docker daemon calls. No swap reset, cache drop or kernel setting was used.

| Valid short arm | Measured SwapPss, KiB | Global swap-in, bytes |
| --- | ---: | ---: |
| Static control | 0 | 77,824 |
| Static candidate | 0 | 8,192 |
| Video control | 0 | 53,248 |
| Video candidate | 0 | 90,112 |

Short-arm minimum available RAM was 105,998,432 KiB; UI_ERROR=0, failed routes=0,
static pixels matched and both video outputs produced frames. Final-batch guard
values: pending. Earlier strict-1-MiB failures and missing-monitor captures are
excluded; they are not final medians. Generated JPEG/video entropy is a fixture
choice, not a guarantee that every photograph compresses equally.

Hosted Bead CI run `43158252740` passed on `ed03c8ee`; the complete Ubuntu
workflow and strict renderer feature checks are green. Full Nacre
Arch desktop CI passed on `c23bb121` with the final pin. The launcher cancellation
race found by CI was fixed with a one-line check before waiting; its regression
passed 50 repetitions and the full suite. Hosted main CI is the final gate.
Actual Git source files were verified before/after compilation; stale-target and
checkout-overlay attempts were excluded. No raw logs or profiles are committed.
Product diff: Bead +367/−144 lines, plus the one-line Nacre cancellation fix
(net +224); tests and pin metadata excluded. No temporary product duplicates remain.
