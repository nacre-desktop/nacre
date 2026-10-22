# CI jobs and owners

Correctness checks run on every main push and maintainer dispatch on main. Benchmarks and GPU
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

## Self-hosted source CI

The `nacre-ci` organization runner group permits only `nacre-desktop/nacre` and
`nacre-desktop/bead`, with public repositories enabled. Workflow access is further
restricted to `nacre/.github/workflows/check.yml@refs/heads/main` and
`bead/.github/workflows/ci.yml@refs/heads/main` under that organization.
The organization and both repositories require approval for **all outside
collaborators** before fork pull-request workflows run. Source CI uses only
`push` to `main` and maintainer `workflow_dispatch` on `main`; workflows using
these labels must never add `pull_request`, `pull_request_target`, or
`issue_comment`. Other workflows, including `compositor-hardware.yml`, keep
their existing runners.

Three ephemeral runners use the official Actions runner, pinned to a verified
release digest, in the dedicated `nacre-ci-runners` Compose project. Each
registration accepts one job, then the supervisor resets the runner state and
registers again. Labels are `self-hosted,linux,x64,nacre-ci`. Both runner and
self-hosted job containers have 10 CPU / 28 GiB limits. Container jobs require the host
Docker socket: this grants host-level Docker access, so ephemeral registration
is not a security boundary against malicious workflow code. Only trusted
maintainer main-branch code belongs here. Registration credentials stay outside
repositories and reports; the runtime obtains short-lived tokens via `gh api`.

Keep GitHub Actions' repository-scoped caches. No persistent host Cargo/pacman
cache is mounted into jobs; any future mounts must be isolated per repository.
New pushes cancel older Source checks for the same ref.

Hosted fallback (one command):
`gh variable set NACRE_CI_RUNNER --repo nacre-desktop/nacre --body '["ubuntu-22.04"]'`.
Delete that variable to restore the self-hosted default. The expression uses
`fromJSON`, so the variable must contain a JSON array. The Arch job images and
sandbox options remain in effect with either runner. Ubuntu fallback jobs use
the hosted VM resource limits, avoiding a Docker CPU quota above its CPU count. Bead documents its Ubuntu
24.04 fallback in its own CI guide.

## Parity sandbox

All cases remain sandboxed and mandatory. The Arch correctness container runs
on the `nacre-ci` self-hosted Linux runners. Outer seccomp/AppArmor restrictions
are disabled for this job; the inner user namespace supplies the sandbox's
namespace permissions. No added `SYS_ADMIN` or `NET_ADMIN` grants are needed.
Bubblewrap still supplies a disposable
filesystem, UID 1000, private PID/network/IPC namespaces, a masked executable
search path and strict fixture peers. There is no non-sandboxed CI fallback.

The container retains the pinned Arch Python/native-engine environment. Running
parity directly on the Ubuntu VM would require a matching interpreter and native
runtime; the container runs with no added capabilities.
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
