# CI jobs and owners

Correctness checks run on every main push and pull request. Benchmarks and GPU
acceptance are advisory. During direct integration, agents run the complete local
gate before pushing and inspect the published checks; the main ruleset does not
require status checks before a push (see [community settings](community-settings.md)).

| Job                                     | What it checks                                                                                                                                                           | Owner                                                                 | Blocking                                                                          |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Source checks / `desktop`               | Syntax, lint, generated config/translations, Rust fmt/Clippy/tests, Python/native Qt regressions, every frozen helper parity case and harness controls, native galleries | Maintainers; Rust/helper and shell contributors fix their regressions | Yes, correctness                                                                  |
| Source checks / `fresh-install`         | A fresh default install, private bus/X display, nested Niri, shell loading and panel IPC assertions                                                                      | Install/runtime maintainers                                           | Yes, correctness                                                                  |
| Headless shell performance / `shell`    | Full nested Niri panel/video cycle, PSS/USS, frame latency and idle CPU budgets; JSON evidence                                                                           | Shell performance maintainers                                         | No; nightly/manual, shared hosted CPU and software rendering are noisy            |
| GPU compositor smoke / `hyprland`       | Fresh default install and strict shell assertions in disposable headless Hyprland                                                                                        | Compositor maintainers / GPU lab runner owner                         | No; manual, Hyprland requires a DRM render node/DMA-BUF unavailable on hosted VMs |
| Publish development preview / `release` | Existing development tag/version and changelog, then explicit publication                                                                                                | Release maintainers                                                   | No; manual publication, outside correctness CI                                    |

## Parity sandbox

All cases remain sandboxed and mandatory. The Arch correctness container runs on
Ubuntu 22.04: the Ubuntu 24.04 trial failed creating Bubblewrap's network namespace
(`RTM_NEWADDR: Operation not permitted`). A separate full Source run verified
that Ubuntu 22.04 needs neither added `SYS_ADMIN` nor `NET_ADMIN`: those grants
are removed. Outer seccomp/AppArmor restrictions are disabled for this job;
the inner user namespace supplies the sandbox's namespace permissions.
Bubblewrap still supplies a disposable
filesystem, UID 1000, private PID/network/IPC namespaces, a masked executable
search path and strict fixture peers. There is no non-sandboxed CI fallback.

The container retains the pinned Arch Python/native-engine environment. Running
parity directly on the Ubuntu VM would require a matching interpreter and native
runtime; the tested container now works with no added capabilities.
See [fixture isolation](../../tests/parity/helpers/FORMAT.md).

Niri's validator, Fastfetch and FFmpeg are installed in the correctness job so
their six existing configuration/video regressions execute rather than skip.
The [capability probe](https://github.com/nacre-desktop/nacre/actions/runs/38467056106)
passed all 834 existing suite tests, 12 harness controls and 349 frozen cases,
with no dependency skips, plus the strict fresh-install smoke test.

CI scripts run at `nice -n 10`; Cargo builds/tests use `-j2`. GPU jobs must not
load VKMS, request broad device privileges or turn an unavailable compositor into
a passing correctness check. The manual job targets an explicitly provisioned
`nacre-gpu` Linux runner with the documented [check dependencies](../contributing.md),
Hyprland and an accessible DRM render node. It uses a task-local Python environment
and the smoke harness's disposable HOME/session; it does not install system
packages or attach to the runner's desktop. No GPU runner is required for source CI.

## Overnight incidents

Record the introducing commit, failing check, reason and resolution here when a
concurrent change cannot be repaired with a small, clear fix. Do not revert another
task's work or regenerate frozen expectations to make the check green.

| Fix / evidence | Check / cause | Status |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `2753707f` | Earlier Ubuntu 24.04 runner denied Bubblewrap namespace setup                                                                      | Runner moved to Ubuntu 22.04; sandbox integrity passes                                                          |
| `6416d052` | Root-created fixture UNIX sockets denied UID 1000 connections; uncontrolled Python patch version changed one maintenance reference | Socket ownership and version input made deterministic; all 349 references retained; Source passed on `ec9ed146` and the capability probe |

Private overnight observations and the morning status belong in `.git/handoff-ci.md`,
not public test logs or personal machine inventories.
