# N0160 private headless measurements

See [the spec](../../specs/0160-daemon-interaction-cpu.md) for interpretation,
limits, root cause and checks. `run-summary.csv` derives CPU from summed raw
ticks/actual elapsed seconds; `memory-summary.csv` records unreclaimable memory.
`identities.json` pins the binaries, source trees, replay and temporary probes.
Each numbered arm contains the original per-second counters and completion,
renderer, UI-error and raster-cache records. PIDs and paths are private container
identities, not host session processes. Before/after arms alternate in each pair.

The primary daemon number is `daemon_self_cpu_pct` (the complete process/thread
group). `daemon_cpu_pct` also includes waited-child CPU. Child accounting can
be charged when reaped, after the child's work occurred; it is a conservative
cross-check rather than exact attribution to a one-second window. Thread/TID
and live-tree columns are diagnostics, not the authoritative daemon counter.

## Replay provenance

`replay/*.txt` preserves the exact temporary scripts as data, not installed
tools. Copy them to `.lead/`, removing only the `.txt` suffix, inside a new
task-private checkout/container. The runner refuses to operate outside a
container. Never run these on a live desktop. Its historical compositor Arm
imports these three files from Nacre commit
`2f33e1ffad5be0b517e720ab74bb7e74449e441d` into `.lead/tmp/n0160-harness/`:

- `bead-shell/spikes/parity-compare.py`
- `bead-shell/spikes/fake_system.py`
- `bead-shell/spikes/capture_safety.py`

The runner changes that private Arm's parent renderer to pixman and enables
animations. The archived Arm hash in the identities is after that renderer
substitution. `n0160-harness-check.py` validates exact replay timing and exited
worker accounting before captures. `n0160-repro.py` supplies the sampler; its
standalone CLI was not the measured topology. `n0160-run.py` drives the real
shell and verifies focused workspaces and all action counts.

Both preserved helper arms were release builds at source `eb0525b3e`, with
`CARGO_PROFILE_RELEASE_DEBUG=1`, `CARGO_PROFILE_RELEASE_STRIP=false`, and the
same `n0160-instrument.py` overlay. The candidate adds only the spec's
`lock_dashboard.rs` change. `cargo build -j2 --release --locked -p nacre-helper
--bin nacre-helper` produces each arm; preserve as
`.lead/tmp/n0160-{baseline,candidate}-helper` before the next build. Build the
same real `nacre-shell`, `nacre-wallpaper-decode` and `bead-wallpaper-video`
release binaries under `target/release`. Restore the six probe-modified files
before final CI; probes are not part of the product patch.

The lab adds labwc, wlr-randr and perf to the desktop CI Arch dependencies.
Use a private 8-CPU/16-GiB container, no swap, no host graphics/audio mounts,
SYS_NICE and the workflow's namespace/security options. `nice -n 10 python
.lead/n0160-pairs.py --start 3 --end 7` runs the five pairs; do not apply a
second nice adjustment to its Python subprocesses. The sampler guard checks
available memory and all snapshots verify zero SwapPss. The runner sets the
private environment and creates/removes only recorded child processes.

The output compositor is distribution Niri 26.04 (`8ed0da4`), not the patched
Nacre compositor used by N0154. Labwc uses pixman; nested Niri uses software
Mesa EGL. A startup check requires 60,000 mHz at 1280×720/scale 1. The hashes
pin this environment more precisely than a rolling package version alone.

Failed harness attempts and early nice-19 pairs were excluded before accepting
the five pairs here. No builds ran alongside these captures. The final profile
uses the same validated six-cycle replay, separate from the unprofiled pairs.
Raw `perf.data` is disposable; its digest and flat symbol report preserve the
diagnostic evidence without committing a 26-MB binary.

Profile command: `perf record -e cpu-clock:u -F 99 --call-graph dwarf,16384
-p <private-daemon-pid> -o perf-final.data -- sleep 75`, through a privileged
execution in the task container only. The daemon retains the normal container
capability set; only the profiling execution gets that override.
The profile replay uses 10 seconds warmup, 5 seconds idle before, 60 seconds
interaction, and 5 seconds idle after; the unprofiled matched pairs above use
the longer idle windows. Recording begins immediately after private daemon
startup. Flat report: `perf report -i perf-final.data --stdio --no-children
--call-graph none --percent-limit 1`. The data SHA-256 is
`bddcb908758b5a29bd92f2645b8c3627a01dbcd7f4a83d8046bd205089a7c022`.
See `profile-final/interaction.json` for its completed 60.060-second replay and
`profile-final/ui-errors.json` for zero errors. DWARF unwinding was incomplete;
the reported percentages are flat instruction samples, not inferred call counts.

## Desktop CI recipe

`replay/full-desktop-ci.sh.txt` reproduces every check command in the workflow's
`desktop` job. Launch its Bash at the container's default priority; each command
already applies nice 10. Adding nice 10 to that Bash as well makes children nice
19. The final benchmark pairs did not have this wrapper error.

The first source-copy CI attempt was cancelled while integrating main's peer
changes. A later nice-19 workspace run had four shell-test failures; all four
passed isolated and in the complete nice-10 workspace rerun without source or
assertion changes. That rerun passed through translation generation, then the
compositor-boundary guard also scanned untracked benchmark `.py` files under
`.lead`. The clean workflow checkout has no such scratch. Moving it outside the
source tree resolved the environmental failure. `replay/ci-completion.sh.txt`
reruns the source/UI check and every remaining workflow check in order, with the
same context. The job's evidence therefore spans the main and completion logs;
the initial script's nonzero exit is not represented as a clean pass.
