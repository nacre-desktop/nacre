# N0169 — idle shell memory

Status: implementation and full CI verified; final measurements await the
existing-host-swap gate decision. Physical-session acceptance remains owner-held.

Measure the real `shell/` tree and a private daemon in the task's bounded Arch
container on dev-1: two physical outputs, 2880×1800 at scale 1.5 and 3440×1440
at scale 1, static and video wallpapers. The desktop TEST host was unreachable.
Use identical release flags, ten-minute idle checkpoints and dashboard/launcher
open/close phases. Report CPU/GL backend, anonymous, shared and clean file memory
separately. Raw profiles and harnesses remain untracked.
Launch only after the private daemon publishes healthy displays and widget-store
snapshots. A premature launch produced undeclared-store errors before the first
bar generation and was excluded; no error lines are filtered from valid runs.

The supplied live summary (~182 MiB PSS, ~113 MiB anonymous) differs from the
provided smaps excerpt (103.2 MiB PSS, 68.2 MiB anonymous, 20.9 MiB shared,
14.1 MiB file-backed). Neither is a matched control/candidate measurement.

| Checkpoint, MiB | Before | After |
| --- | ---: | ---: |
| Static, ten-minute idle | Pending | Pending |
| Static, after dashboard/launcher close | Pending | Pending |
| Video, ten-minute idle | Pending | Pending |
| Video, after dashboard/launcher close | Pending | Pending |

Supporting 130-second static diagnostic (Nacre `3fc0d4f5`, actual Bead Git
revisions `7ea19860` → `4979d7fc`; not the final ten-minute acceptance):

| Main process, MiB | Before | After |
| --- | ---: | ---: |
| Anonymous PSS | 123.4 | 75.6 |
| Shared PSS | 20.7 | 20.7 |
| Clean file PSS | 79.7 | 79.1 |

Both arms had zero UI errors, swap PSS and swap-in; picker body pixels matched
on both outputs at first open and reopen. These diagnostics did not check the
current global pre-used-swap condition and do not replace the planned five pairs.

Targeted process-wide cache charges reproduce the anonymous-memory increase with
the real retained wallpaper picker and 17 generated 1920×1080 JPEGs: about
52.3 MiB of CPU rasters and 13.7–21.6 MiB of warm decoded images. These charges
follow shared allocations, so shared frame references are not counted twice.
Individual reported Luau heaps are about 0.6–1 MiB; markup build-byte counters
are historical build measurements, not additional live heap charges. Allocator
free arenas and remaining uninstrumented allocations are reported separately;
this fixture does not establish an exact breakdown of the original live PID.

The two full physical RGBA wallpapers require 38.7 MiB before sharing. Static
wallpapers use at most two SHM buffers per surface; active ordinary surfaces
briefly use three, then compact while idle. Hidden panel SHM returns near idle.
Static decoding temporarily holds output pixels in the decoder and parent;
the parent drops its decoded vector after SHM attachment. It does not retain
both the original source raster and scaled copies at steady idle.
Ordinary panel runtimes expire after 120 seconds; the wallpaper picker retains
its runtime for smooth reopen. Software video decoding adds a separate helper's
roughly 80–90 MiB anonymous heap, not memory in the main shell PID.

GPU resources retire before CPU snapshot admission to free their shared budget.
The framework candidate reuses the accepted close-snapshot lifecycle for CPU
fallback panels. It compresses display-sized snapshots off the UI thread and
releases matching raw frames only after compatible frozen pixels are admitted.
The generic `SceneSource::release_prepared` hook retires matching frame identities
without changing models or pending transactions. Failed, stale, reveal and
budget-denied snapshots keep their existing ownership. Cached first-frame
pixels remain available for reopen; changed content prepares asynchronously.
Active source caches stay warm. The generic `Renderer::resume_prepared` hook
lets a host-confirmed terminal open draw import
an existing dense frame and discard the compact snapshot without a new decode
or UI-thread flatten. Early diagnostic comparisons used dependency-checkout
overlays that Cargo could reset; they do not count as acceptance evidence.
Acceptance builds use actual Git revisions, with source hashes checked before
and after compilation.

The measurement plan is five alternating control/candidate pairs per wallpaper
mode, each with 600 seconds idle before and after the dashboard, launcher and
picker sequence. Checkpoints are process samples rather than 60-second sampling
windows; report medians and ranges. The diagnostic hover pair found stable cache
owners; a cold backdrop bake confounded its first-input comparison. Headless
presentation events are too sparse to establish the physical 120 Hz target.

CI also exposed a launcher shutdown race: consuming a cancelled query could
leave its worker waiting while teardown joined it. A cancellation check before
the wait closes that race; both comparison builds include this common fix.

Full Bead CI (`ubuntu:24.04`, all `ci.yml` steps) passed at `311b4136`.
Full Nacre desktop CI (Arch, including `tools/check.py`, gallery and daemon raster
memory) passed at `c5184888` with that pin. The launcher regression passed 50/50
repetitions and within the full suite. Both authors/committers use the required
identity; neither repository has been pushed yet.

The final matched executables use Nacre `c5184888`, Bead `7ea19860` versus
`311b4136`, identical helpers, release debug=1/strip=false and `-j2`. All Bead
source files match Git before and after compilation; executables are independent
regular files. Final ten-minute captures are held because existing host swap
(~2.2 GiB) exceeds the skill’s 64 MiB precondition despite ~96 GiB available RAM.
A task-local exception was requested; no swap or kernel state was changed.
Snapshot size depends on image entropy; generated `testsrc2` JPEG/video fixtures
are reproducible, not a claim that every photograph compresses equally.
No temporary product duplicates are planned.
