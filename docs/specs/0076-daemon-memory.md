# N0076 — release daemon raster working memory

Status: implemented and validated; no live deployment.

The reported daemon retained 362 MiB anonymous memory after startup with daemon
notifications/behaviours, wallpaper picker, weather photo and audio spectrum.
The anonymous mappings alone do not identify their owner allocations.

## Attribution and change

The catalogue uses a bounded-output, short-lived helper process; it does not keep
decoded thumbnails in the daemon. Its on-disk thumbnail/preview caches remain
bounded by N0070. Spectrum has fixed FFT buffers. Weather photo validates one
size-limited image at a time and retains a path/metadata, not decoded pixels.
Lock content is different: its serial blocking task renders in the daemon.
Its renderer, fonts and temporary decoded/resampled/encoded images are dropped
on return, but glibc can retain freed pages in thread arenas after thread exit.
With real wallpaper decoding, trimming alone still left roughly 78 MiB anonymous
in the preliminary candidate, above the 60 MiB target.

Keep glibc's mmap threshold at its initial 128 KiB during single-threaded daemon
startup. This disables its dynamic growth, so later multi-megabyte raster buffers
use independently releasable mappings rather than expanding persistent arenas.
The tradeoff is more mmap/munmap calls for medium/large allocations. This affects
only the daemon process on GNU libc. See the
[glibc allocator manual](https://sourceware.org/glibc/manual/latest/html_node/Malloc-Tunable-Parameters.html).

Release source/canvas/crop/resized images as soon as their next stage exists,
without changing sampling, resolution or pixels. After the preparation function
returns (including errors), call glibc `malloc_trim(0)` on the blocking thread,
after the renderer and its buffers have been dropped. Weather photo similarly
reclaims after its validator returns, including failures; validation borrows the
encoded bytes instead of cloning them. Other allocators keep their normal
behavior. No periodic trim timer, persistent raster cache or extra worker is
introduced. Lock preparation already runs serially, outside the daemon readiness
path; the blocking pool is capped at two.

## Regression method

`cargo test -j2 -p nacre-helper --test daemon memory:: -- --ignored --nocapture` starts an
actual daemon with `--notifications=daemon --behaviours=daemon`, private HOME,
fake compositor socket, private fake logind, 20 synthetic 3840×2160 JPEGs and a
generated video, plus a 6000×4000 RGBA wallpaper used after warmup. The client
requests metadata then hydrates two image rows and the video, verifies the
generated files, leases spectrum using synthetic PCM, and activates a validated
cached 1400×1400 weather photo. The empty weather-location setting prevents
external weather refreshes; the photo loader uses the matching validated cache.
No live bus, desktop or configuration is used. The catalogue is request-scoped,
not leased.

The sampler reads RSS/anonymous memory every 50 ms; swapped samples fail. Named
phases include startup, lock completion plus three seconds, catalogue metadata,
hydration, photo readiness, and warm idle. Then change to the 24-megapixel
wallpaper and wait for the generated dashboard to identify that actual file,
followed by three seconds of settling. This prevents startup cleanup or fallback
backgrounds from masking retention. Assert daemon peak and settled bounds:
sampled peak anonymous memory <512 MiB, lock and final settled anonymous memory
<=60 MiB. Startup is reported separately for attribution.
`NACRE_MEMORY_BINARY` selects a control executable for matched comparisons;
the acceptance measurement runs five alternating pairs and phase medians using
the same test harness. This comparison is a task measurement, not a CI control
build. CI runs this guard explicitly as its last step, using its freshly built release
executable, after concurrent test/build work has ended. Ordinary `cargo test`
skips this one isolation-sensitive guard. Rendering is CPU only,
without compositor/GL/DRM buffers. These container regression tests are not a
physical desktop capture or the laptop reference gate.

## Evidence

Five alternating pairs used the same private Arch container, default dev build
profile, CPU renderer and fake 1280×720 scale-1 output. Its cgroup memory limit was
12 GiB with swap disabled. All samples had zero SwapPss and shared-memory PSS;
there were no DRM buffers. Raw per-run logs, medians, ranges and binary hashes
are in the private task handoff, outside the public repository.

| Phase | Control anon MiB | Fixed anon MiB | Median change MiB |
| --- | ---: | ---: | ---: |
| Startup | 16.59 | 15.95 | -0.64 |
| Initial lock render | 81.00 | 24.54 | -56.46 |
| Spectrum leased | 81.00 | 24.54 | -56.46 |
| Catalogue metadata | 81.00 | 24.54 | -56.46 |
| Catalogue hydrated | 76.42 | 23.93 | -52.49 |
| Weather photo ready | 76.74 | 23.43 | -53.30 |
| Warm idle | 76.74 | 23.43 | -53.30 |
| Late 24MP lock render, settled | 89.27 | 24.01 | -65.26 |
| Sampled peak | 261.73 | 258.60 | -3.14 |

Every control failed the 60 MiB guard and every candidate passed. Late-render
settled ranges were 84.99–128.73 MiB control and 23.97–24.10 MiB fixed. The fixed
process settled within three seconds of publishing the prepared wallpaper.
Settled RSS medians were 162.36→99.37 MiB. File-backed PSS, reported separately
and excluded from the unreclaimable-memory gate, was 57.32→59.01 MiB. All phase
medians satisfy the +0.5 MiB relative gate.

Control source: `a6da43500b67d4a5bc9589965a09d10447c902f3`.
Dev executable SHA-256:

```text
control   79fc4326479015800e0e7f6920df8ee9c3f5c80e510c506fd2ce47be985cb1d4
candidate f28fe94aa3e2e7bf472353e3e9097a29b27e32db3127245b4bbd4d0cd4d49af8
```

The complete desktop CI job passed with `CI=1`, including formatting, Clippy,
all Rust suites, minimal protocol consumer, release builds, sandbox integrity,
translation/source/QML checks, 500 tool tests (one opt-in soak skipped), 120 AI
tests, 52 brain tests, 490 shell-tool tests and 534 helper-parity cases with zero
failures. Both translation galleries passed (244 and 185 states per language),
as did the AUR pager capture and the final release-memory guard.

The release daemon settled at 20.65 MiB anonymous / 51.70 MiB RSS after the late
render, with a sampled 253.39 MiB anonymous peak. Release executable SHA-256:
`60e2fd13c35d916aaa31671ab727c59b25ae16a05ba00624bb333d6b7cec24f3`.
This is an absolute guard result, separate from the matched dev-build comparison.

The constrained replay uses Cargo -j2 and two test threads; an initial
parallel run hit the existing AI-chat fake-peer two-second deadline, which then
passed in isolation. No assertion or timeout was relaxed.

The fixture has not reproduced the complete reported 362 MiB laptop footprint;
do not present the reported value as this fixture's measured control.

No temporary product duplicates are introduced. The saved control executable is
test evidence only: delete 0 days after the replacement is trusted (after the
final measurements and CI), with the private remote build directory.
