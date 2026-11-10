# N0156 — Bead shell recovery and QML retirement

Owner authorization: 2026-11-10, “remove all the quickshell stuff and previous
shell, we are using bead now.” No live installation is authorized by this unit.

## Delivery gates

Stage A changes failure recovery and installer migration before removing any
QML source. Stage B removes the QML shell and its exclusive tooling only after
Stage A's disposable-HOME proofs pass. Before pushing any QML deletion, fetch
origin and require this exact check to succeed:

```sh
git cat-file -e origin/main:docs/research/bead-vs-qml-shell-benchmark-20261110.md
```

The check failed at the start of N0156 and passed after N0154 landed on
`origin/main` at `2a1d6d786`. Recheck before pushing deletion. Stage A was held while the legacy QML checks failed; both stages are integrated
after the final Bead-only gates pass. Each commit contains
`[no-publish]`; integration uses a normal fast-forward push to main.

## Recovery design

Use the product's versioned copies under `~/.local/share/nacre/bead/` and its
`~/.local/state/nacre/ui-shell/` transaction journal. A copied executable and
matching Luau tree are a candidate, not a known-good release. Only a completed
live health check establishes a recovery candidate. Bind that checkpoint to the
systemd InvocationID: daemon-reload changes cached ExecStart without changing
the running process, so a newly prepared launcher must restart and pass health
before it can be certified. Recovery tries each distinct copy once and rejects
all journal references to a failed copy. A source update without fresh build
output may reuse a validated installed copy. Keep previous healthy copies
through updates and rollback. The lead's private `stable-bead` and
`good-bead-nacre-*` copies are outside the product's recovery contract.

`nacre-ui-shell-fallback.service` must select a previously validated Bead,
validate its executable, restart the shell and check process stability, provider
registration and daemon health. An exhausted recovery attempt stops the shell
and reports a TTY recovery command in the journal; it must not repeatedly
restart the failed candidate or invoke QML.
When an unrelated release holds the lock, the recovery controller exits 75;
the existing fallback unit retries only that exit status after two seconds.
Successful recovery, exhaustion and other errors do not restart the fallback.
This avoids losing a failure notification during a configs-only release.
Ordinary scheduled restarts must
not trigger recovery before the service's start limit is exhausted. Cut over
the fallback before restarting even an identical Bead build; comparing desired
unit bytes keeps repeat requests idempotent.
Migration also stops and removes a managed legacy `nacre-qml-windows.service`
before restarting an identical Bead build; an unmanaged unit is refused.
Manual
rollback refuses without changing a healthy running shell when no previous copy
exists; if older copies are damaged, its current known-good copy is the final
recovery option. Automatic failure recovery excludes the failed current copy.

Direct source migration holds `releases/release.lock`, the same lock as the
switch/recovery controller. It must not acquire the daemon's config writer
lease: the running daemon owns that lease for its lifetime. An inherited writer
descriptor from an enclosing release is still validated. Legacy-file archiving
runs under that same release lock only after health succeeds, with the config
root pinned and validated; it touches retired QML files, never daemon settings.
The real-session code-only proof exposed and corrected this lock conflict.

The real-session rollback proof also exposed shared UI crash history: a failed
copy could put an older healthy copy into Bead safe mode before its provider
registered. Each installed launcher now exports its immutable release ID;
Nacre passes a separate crash-history filename to the existing Bead recovery
primitive. Panel state stays shared. The three-crash protection stays active for
each copy. Legacy launchers retain their original history file. UI startup blocks
SIGINT/SIGTERM before spawning workers so the existing Calloop signal source can
complete a normal service stop and write the clean-shutdown marker. CLI client
cancellation is unchanged. No Bead framework API or dependency revision changes.

## Scope and retention

Keep root `shell/`, `crates/nacre-shell`, `nacre/login` (SDDM QtQuick), and shared
runtime/deployment helpers in `nacre/shell-tools`. `bead-shell/` is owned by N0157
and is outside this deletion. Inventory consumers before removing helpers,
translation IDs, package dependencies or test fixtures.

Temporary QML runtime and switch paths retained for Stage A: delete 0 days after
the Bead migration/recovery replacement is trusted and Stage B passes the
benchmark and full CI gates. Individual `bin/*.rollback` copies use
the rule “delete 7 days after the replacement is trusted”; commit age alone
does not prove a replacement has been trusted.

Stage A temporarily retains the existing QML transaction code alongside the
Bead recovery transaction so the two deliveries can be tested and integrated
independently. Its controller additions are not a second service or a background
job; they reuse the existing fallback unit, release lock, versioned copies and
health checks. Stage B removes the old transaction and QML installer branches.

## Validation and migration evidence

Initial Stage A focused checks ran in the task's Arch container on dev-1 with disposable
HOMEs. The common environment was
`PYTHONPATH=tools:nacre/shell-tools:nacre/shell-cli/src`, with
`NACRE_HELPER_BINARY=/work/target/release/nacre-helper` and
`NACRE_ORIENT_BINARY=/work/deps/orient/target/release/orient` for setup tests.

| Exact command (from `/work`, prefixed `nice -n 10`) | Result |
| --- | --- |
| `python -m unittest discover -s tools/tests -p test_ui_shell_switch.py` | 41 tests, OK |
| `python -m unittest discover -s tools/tests -p test_releases.py` | 21 tests, OK, 1 skipped |
| `python -m unittest discover -s nacre/shell-tools/tests -p test_installer_dropin_guard.py` | 31 tests, OK |
| `python -m unittest discover -s tools/tests -p test_setup.py` | 46 tests, OK |

Pinned Ruff 0.16.10 formatting and lint passed on the original eight Python files
and the six Python files changed in the safety review. The full-check environment
exposed inherited `XDG_CONFIG_HOME` directing offline preparation outside its
explicit HOME; preparation now pins that HOME's config root, and the setup
rollback test verifies the inherited foreign directory is not created.
The source installer tests exercise fresh/update deployment in temporary HOMEs
with external services mocked; package setup tests exercise real private-file
preparation and rollback with executable fixtures.

The initial full desktop run reached the legacy launcher's 64-case QML suite
and exceeded its 40-second aggregate wrapper limit. Individual measured cases
took roughly 5–20 seconds. Keep its original shared-engine execution and all
assertions; allow 900 seconds for the aggregate suite. The two-context suite
passed alone in 19.513 seconds against its former 20-second limit and now has
40 seconds. These test wrappers are retired with their QML surfaces in Stage B.

Real-systemd command: as UID 1000 `tester` in a task-only Arch container booted
with systemd PID 1, run
`nice -n 10 python /work/tools/tests/rehearse_ui_shell_systemd.py`. Result: exit 0,
`PASS: fresh migration → old fallback update → start-limit recovery → rollback → next update → exhausted-copy stop and journal hint`.
The script uses stub Bead/daemon programs and the actual release controller and
systemd user manager. It proves immutable copy selection, actual
`StartLimitBurst`/`OnFailure` execution, rollback/update convergence and an inactive
shell with unchanged `NRestarts=0` after exhaustion. It does not prove rendered
Bead UI health. The fixture copies its controller into the disposable HOME and
the failure unit executes that persisted copy.

The initial standalone Stage A full check was not green: its legacy shared-engine
QML aggregate still exceeded the enlarged 900-second limit and the dual-context
wrapper exceeded 40 seconds. Those QML-only
suites are removed in Stage B, as requested; their failures are not reported as
passing Stage A CI.

The final native workflow passed on the final-compatible Stage B sources:
`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -j2 -- -D warnings`,
`cargo build --locked --examples -j2`, `cargo test --workspace --locked -j2`, the
minimal protocol consumer, release helper build and every imported product
feature/build command from `.github/workflows/check.yml`. The product Rust suite
reported **867 passed, 0 failed, 4 intentionally ignored**. Native source hashes
were unchanged when the clean test/consumer corrections were committed.

On clean `9e124324a1`,
`dbus-run-session -- xvfb-run -a /tmp/check-env/bin/python tools/check.py` passed:
404 tools tests (one skipped), 138 AI, 54 Brain, 374 shell-tools, 13 Bead proof
suites/179 tests, six package Python tests and 14 product Python suites/185 tests.
Palette CLI, generated configuration, SDDM parser and JavaScript checks passed.
The palette guard and its two regression cases were then validated on clean
`250f4f641a`: pinned Ruff passed, all 16 B installer cases passed, and the full
`tools/check.py` command passed again with the same discovered-suite totals.
The remaining desktop steps also passed: Luau analyzer 0.742 (189 modules),
native-only proof suites, cargo-deny 0.20.2 (advisories/bans/licenses/sources), npm,
translation coverage (1787 messages), generated assets and the native AUR gallery.
The existing CPU daemon-raster memory test passed: settled RSS 54272 KiB,
anonymous 20540 KiB and file PSS 19032 KiB, with transient peak anonymous
259152 KiB (22.87 seconds). This is the existing CI raster-cache gate, not a
laptop UI measurement or a relative-renderer performance claim.

Package staging/setup used the repository's `package-stage-install` equivalent:

```sh
python -m unittest \
  test_setup.PackageSetupTests.test_apply_is_offline_repeatable_preserves_private_state_and_rolls_back \
  test_setup.ArchPackageLayoutTests.test_split_packages_have_no_file_collisions_and_ship_native_artifacts
```

Result on clean `250f4f641a`: two tests, OK (0.130 seconds), using the actual B
helper and pinned Orient engine. The workflow now uses the correct layout-test
class. Those unit cases deliberately use Bead executable fixtures; a supplemental
real-artifact call to `packaging/arch/stage.py::stage_ui_shell` staged the three
real Bead release binaries below with the real shell tree and product/font
notices. All hashes matched before/after; binaries were ELF, mode 0755, owner
0:0; palette/bar and all four license notices were present, with no QML runtime
staged. Actual execution of these same artifacts is covered by the B fresh proof.
No pacman transaction or package publication was performed.

The actual Bead proof (recorded as proof29) ran in the task-only systemd container:

```sh
# Inside n0156-ci-desktop, UID 1001; fresh /home/beadproof, runtime /run/user/1001:
nice -n 10 python3 /tmp/n0156-real-bead-recovery.py
# Container root, using RUN_STARTED printed by that exact invocation:
journalctl --since "$RUN_STARTED" _UID=1001 --no-pager
journalctl --since "$RUN_STARTED" _UID=1001 --identifier=nacre-ui-shell-fallback --no-pager
```

Result: exit 0. It used all three real Bead executables, the actual daemon and
Niri under Xvfb. The initial legacy service was a harmless placeholder, not a
QML UI health claim. Real migration, stale-fallback `install.py --code-only`,
rollback/update, failed-B-to-good-A recovery and exhausted-copy shutdown passed.
The live daemon held the config writer lease during the direct installer run.
A normal SIGTERM left `ui-health-<release>.json` with `running:null` and unchanged
empty failures. Each restored Bead answered the real `nacre.hide` route (`null`,
exit 0). The root journal contained seven real OSD prewarm markers, zero
`UI_ERROR` entries, and the required no-good-copy hint. Shell and fallback restart
counts were stable for five seconds after the terminal fallback completed.
Completed oneshot units may lose their InvocationID through systemd collection;
the fresh, UID-scoped journal proves that execution instead.

The final real-manager stub rehearsal also passed, with a fresh UID 1000 HOME:
`nice -n 10 python /work/tools/tests/rehearse_ui_shell_systemd.py` (exit 0).
While `releases/release.lock` was held, fallback `NRestarts` advanced 0 to 3;
after unlocking it recovered the known-good copy. Rollback/next update and
terminal shutdown passed, with the shell inactive and `NRestarts=0` stable.
The system journal query scoped to `_UID=1000` since `2026-11-10 15:14:08`
confirmed the recovery hint. These stub checks prove actual systemd wiring and
contention; proof29 separately establishes real provider/daemon health.

All commands above ran with nice level 10, Cargo `-j2`, and task containers capped
at eight CPUs and 16 GiB. No installer ran against the owner's HOME; no laptop
session, units or installed files were changed.

## Bead-only fresh install and artifact provenance

The B proof used an empty `/home/beadproof` (UID 1001), a restarted private user
manager, and its own Niri/Xvfb processes. Neither the HOME nor the source checkout
contained the old QML tree or configuration link. The old A clone's canonical
`/work/nacre/shell` was moved aside so it could not mask a missing resource.
It ran the real source pipeline, not a preseeded runtime:

```sh
NACRE_HELPER_BINARY="$B_ROOT/nacre/shell-tools/nacre-helper" python3 "$B_ROOT/tools/configure.py" --apply
python3 "$B_ROOT/nacre/shell-tools/install.py"
# Then, after injecting owned old-runtime paths and the old managed fallback:
python3 "$B_ROOT/nacre/shell-tools/install.py" --code-only
python3 "$B_ROOT/tools/releases.py" ui-shell rollback
python3 "$B_ROOT/nacre/shell-tools/install.py" --code-only
```

Commands ran at nice level 10 inside `n0156-ci-desktop`, with
`B_ROOT=/opt/n0156-stage-b`. The manual proof's subprocess log records exact
arguments and results. Result: exit 0. Fresh install activated Bead and its actual
provider; code-only archived the injected old source/config link and replaced
the old fallback. Rollback followed by a real installer update stayed healthy.
An injected B start failure recovered A with the provider responding; corrupting
all retained copies stopped the shell, emitted the recovery hint and left stable
restart counts. The final UID-scoped system journal contained ten real OSD prewarm
markers and zero `UI_ERROR` entries.

This exposed two Niri-only bootstrap issues: shortcuts and palette validation
incorrectly required a Hyprland loader. Each check is now scoped to Hyprland;
Niri still receives all shared palette/shortcut setup. Focused checks passed:
`test_install_extras.py` two tests; installer guard 34 A tests and 16 B tests.
The Hyprland missing-loader refusal remains tested.

The final proof ran on clean, committed `250f4f641a`, using the tracked
[`rehearse_bead_recovery.py`](../../tools/tests/rehearse_bead_recovery.py).
Its local, uploaded and executed file hashes were identical:
`4a6348e6f248779f8b85cb685d35a639e7a77436936a18b7510a8d45d71877c4`.
The exact invocation inside the container's UID-1001 scope was:

```sh
N0156_SOURCE_ROOT=/opt/n0156-stage-b \
N0156_ARTIFACT_ROOT=/opt/n0156-stage-b \
N0156_ARTIFACT_COMMIT=2f5ba7f5073cd4e502eda57a939224dc509d73c3 \
N0156_FRESH_B=1 nice -n 10 python3 /tmp/n0156-real-bead-recovery-b.py
```

The executed `/tmp` copy was byte-identical to the tracked manual rehearsal.
Root queried `journalctl --since "$RUN_STARTED" _UID=1001 --no-pager`, required
actual Bead prewarm output and the no-good-copy hint, and rejected `UI_ERROR`.
The wrapper returned zero. The source and artifact identities were:

| Input | SHA-256 |
| --- | --- |
| B `nacre/shell-tools/install.py` | `13cc8c741780522b1607a8bf432aba77d663d76b255df36f0254a266a20c4254` |
| B installer guard test | `1d474fc33fb9530858d8b02ad89504aeff2b90780d2ed3fdecc9e5b3b87efc55` |
| `nacre-shell` | `1b62a189b37041567ca6fab51fc2de1a14d143f32469b324fd1caf87b079fa85` |
| `bead-wallpaper-video` | `23030f3f47bd6105fcee1ab504283bd7e3744608d1d942d1282fe648baebaa6b` |
| `nacre-wallpaper-decode` | `3a4e5c11963bf814fdc43bc7304821b24129cdbf31733b22d644627c3dd230be` |
| B helper | `eb5a92cba561abc4664f89827bcb54e2c538a7556a2506a7fc0830273db5ef49` |

The three Bead release binaries were built from `2f5ba7f50` with
`cargo build --locked -j2 --release -p nacre-shell --bins`; later B commits changed
only tests, developer dependency metadata and the Python installer guard. The
runtime Rust and its compiled product inputs were unchanged. The helper was the
actual B release build. These artifacts are identified separately from the source
checkout, rather than claiming they were rebuilt from a later commit.

## Owner-run live migration (procedure only; not executed here)

Use a committed, checked checkout and its matching prebuilt Bead executables
(`nacre-shell`, `bead-wallpaper-video`, `nacre-wallpaper-decode`) in
`target/release/`, or the matching installed package bundle under
`/usr/lib/nacre/bead-shell/`. Save work before the shell restart. From that
checkout the lead will run:

```sh
git status --short
systemctl --user show nacre-shell.service -p ExecStart -p DropInPaths -p ActiveState
journalctl --user -u nacre-ui-shell-fallback.service --no-pager -n 50
systemd-run --user --wait --pipe nice -n 10 python3 "$PWD/nacre/shell-tools/install.py" --code-only --dry-run
systemd-run --user --wait --pipe nice -n 10 python3 "$PWD/nacre/shell-tools/install.py" --code-only
nacre ui-shell status
systemctl --user is-active nacre-shell.service nacre-daemon.service
journalctl --user -u nacre-shell.service -u nacre-ui-shell-fallback.service --no-pager -n 100
```

Stage A replaced the managed `nacre-ui-shell-fallback.service` action with
`ui-shell recover-bead --recover`, staged an immutable matched Bead copy, and
health-checked the activated shell. Existing Bead remains recoverable. It retained
QML files under `~/.local/share/nacre/shell/source`, `source.previous`,
`source.good` and `~/.config/quickshell/nacre` until Stage B. The final installer
archives those owned legacy files after Bead passes health checks, as below.
The shared `~/.local/share/nacre/shell/tools` and `shell/bin` paths remain product
runtime paths; their name does not make them obsolete QML code.

The way back for a Bead update is `nacre ui-shell rollback`, followed by the
status and journal commands above. A source release rollback uses
`nacre rollback` for the managed file snapshot; a direct install's helper/tree
rollback remains separate. Stage A retained explicit `ui-shell qml` as temporary
compatibility. Stage B removed that command; it was never the new automatic
failure action.
After Stage B, the same `--code-only` command keeps shared command/configuration,
palette, font, app-isolation and daemon setup, installs the matched Bead copy,
restarts under the managed unit, and waits for process/provider/daemon/settings
health. It replaces the old fallback action before activating the candidate.
Only after success, under the release lock, it moves owned `source`,
`source.previous`, `source.good`, and `source.good.next` out of
`~/.local/share/nacre/shell/` to
`~/.local/state/nacre/shell/backups/<UTC>/legacy-shell/`. It also moves
`~/.config/quickshell/nacre` only when its exact symlink target is an old product
source location, and moves `shell/bin/qs`, `shell/tools/launch.sh` and
`shell/tools/qt-check.sh` only when their bytes match the known old scripts.
Unrecognized config links/directories and modified scripts remain untouched. The adjacent
`manifest.json` records moves; `backups/latest-legacy-shell-backup` records its
parent. Shared `shell/tools`, `shell/bin/nacre_shell`, palette/user data and the
SDDM greeter remain. Legacy Python shell-supervisor code is retired with its
other already-ported helper originals. A prepared Niri session refresh retains
the live bind directory inode while removing obsolete staged source files and
rebuilding the generic/shared view.

Package setup removes the two old source/config links only when its existing
package-links receipt owns their exact targets; it stages a matched Bead copy
and recovery units without starting live services. The next login activates
those units. Package source trees no longer contain the old shell.

The supported way back is `nacre ui-shell rollback` to the previous healthy Bead,
or the release snapshot owner `nacre rollback` for an applied source/package
configuration transaction. Archive manifests retain old bytes for lead-led
inspection; no QML launch command is shipped. A standalone install's `--restore`
restores its private-file manifest and leaves the previous service restart to
the lead. No command above grants workers live access.

## Rollback-copy inventory

All 13 `bin/*.rollback` files currently have unverified replacement-trust dates:
`nacre-ai-account.py.rollback`, `nacre-ai-chat.py.rollback`,
`nacre-ai-claude.py.rollback`, `nacre-ai-context.py.rollback`,
`nacre-ai-memory.py.rollback`, `nacre-ai-remote.sh.rollback`,
`nacre-ai-skills.py.rollback`, `nacre-ai-tools.py.rollback`,
`nacre-ai-usage.py.rollback`, `nacre-ai.py.rollback`,
`nacre-brain-maintain.py.rollback`, `nacre-brain-sync.py.rollback`,
`nacre-brain.py.rollback`. Their last edits range from October 21 to November 5;
last-edit dates do not establish trust. Retain pending proof of the trust date;
delete 7 days after each replacement is trusted, with its test/i18n/configure
consumers. The three Brain copies import one another and require a grouped
consumer review.

## Stage B extraction and limits

The old daemon/menu benchmark adapters imported the retired QML session driver;
retire both adapters with that driver rather than add another nested-session
framework. Existing native daemon contract and memory tests remain. The generic
power sampler now fingerprints Bead Luau/Lua/JSON files. Historical development
guides identify their retired QML commands explicitly.

Standalone QML panel extensions are removed with their interpreter. Validation
rejects their manifests; an old `.qml` entry point is never executed as a program.
Launcher and theme extensions retain their shared validation and launch behavior.
Review-tree preparation no longer stages QML source, and successful releases no
longer promote `source.good`; Bead journals own known-good shell recovery.

Moved shared settings/navigation metadata, branding masters and the non-QML
English/context catalog into `nacre/shell-tools`. The daemon combines its
embedded Bead English with that installed shared catalog, so newer Bead strings
are not lost. Notification expiration defaults now live in the existing shared
notification policy rather than being parsed from QML source. Original shared
translation/launcher reference records and legal attribution remain historical
research; generic native tests remain, while QML UI/parity suites and drivers
are retired. The SDDM QtQuick parser/tests and its Qt dependencies remain. Its translation
engine is now a regular file containing the former shared engine bytes, replacing
a symlink into the retired shell tree. The Widgets application icon also remains
shared data because the Bead application desktop entry still consumes it.
The native Niri runtime test provider remains for host preferences, palette,
verified companions and commands; only its deleted hover-builder operation and
its callers are retired. JSON Schema remains a developer test dependency for
shared notification contracts. Frozen historical diagnostic fixtures are excluded
from the live compositor-dispatch scan only at their anchored archive path.

The obsolete private Niri auto-builder required a QML caption probe. That
builder and probe are retired together; existing receipt-validated companions
and native Nacre Niri remain selectable. Without one, the existing stock-Niri
fallback runs with hover streaming disabled. Fresh stock-Niri sessions therefore
lack companion overview-hover titles until a verified companion is supplied.
No unverified build is promoted in place of the deleted proof.

`bead-shell/` is untouched because N0157 owns its mirror/proofs. Necessary
migration code/tests still spell old configuration paths explicitly. These
are exceptions to a literal global `git grep -il quickshell` requirement:
removing those references would prevent safe old-install migration, and editing
`bead-shell/` would cross this task's boundary. The final inventory must list
all residual paths, rather than obscuring names or removing provenance.

Ordinary Niri application rounding remains a shared setting. Its generated rule
now excludes Bead's client-rendered pearl app IDs, replacing the retired QML
app-ID exclusion; the geometry/clip rule and validation bounds are retained.

The shared `nacre start` dispatcher requests the managed service. If the base
service invokes it without a Bead drop-in, it reports the missing activation
and exits with EX_CONFIG (78); the existing service prevents restart for that
status. This avoids self-start recursion and a missing-configuration loop.

## Remaining literal-name inventory

The exact remaining file set from `git grep -il quickshell` is listed below.
The migration references are removal-only or negative/provenance tests; no
shipped shell launch path invokes that runtime. This inventory includes the
explicit N0157 exclusion rather than silently treating it as deleted.

### Historical documentation and reference evidence (39)

- `assets/screenshots/settings-review/README.md`
- `docs/architecture/foundation.md`
- `docs/architecture/protocol.md`
- `docs/development/compositors.md`
- `docs/development/detachable-panels.md`
- `docs/development/native-helpers.md`
- `docs/development/niri-parity.md`
- `docs/development/niri-testing.md`
- `docs/development/performance-session-results.json`
- `docs/development/performance.md`
- `docs/development/power-baseline.md`
- `docs/development/rust-migration-results.json`
- `docs/development/settings-architecture.md`
- `docs/development/soak.md`
- `docs/development/testing.md`
- `docs/research/bead-vs-qml-shell-benchmark-20261110.md`
- `docs/research/bead-vs-qml-shell-benchmark-20261110/provenance.csv`
- `docs/research/f4-n0007-gc-diagnostic.md`
- `docs/research/launcher-engine-notice.md`
- `docs/research/n0156-reference/diagnostics/case-support-collect.json`
- `docs/research/n0156-reference/diagnostics/case-support-malformed-devices.json`
- `docs/research/n0156-reference/diagnostics/expected-support-collect.json`
- `docs/research/n0156-reference/diagnostics/expected-support-malformed-devices.json`
- `docs/research/n0156-reference/diagnostics/expected-support-markdown.json`
- `docs/research/n0156-reference/launcher-base-menu.json`
- `docs/research/n0156-reference/launcher-reference.md`
- `docs/research/n0156-reference/qt-translation.json`
- `docs/specs/0007-f4-qml-memory-fix.md`
- `docs/specs/0011-f4-qml-residency-investigation.md`
- `docs/specs/0019-weather-updates-workers.md`
- `docs/specs/0033-dashboard-player-metrics-contracts.md`
- `docs/specs/0046-control-center-action-contracts.md`
- `docs/specs/0048-cli-ui-routes.md`
- `docs/specs/0055-daemon-updates-count.md`
- `docs/specs/0073-retire-qml-windows-companion.md`
- `docs/specs/0101-install-tiers.md`
- `docs/specs/0156-bead-only-shell.md`
- `docs/specs/0158-launch-workspace.md`
- `docs/specs/README.md`

### Migration and its regression/installed-package proofs (8)

- `nacre/shell-tools/install.py`
- `nacre/shell-tools/tests/test_installer_dropin_guard.py`
- `packaging/arch/test-installed.py`
- `tools/package_runtime.py`
- `tools/releases.py`
- `tools/tests/rehearse_bead_recovery.py`
- `tools/tests/test_releases.py`
- `tools/tests/test_setup.py`

### N0157-owned mirror, proofs and records (outside this unit) (100)

- `bead-shell/FEASIBILITY.md`
- `bead-shell/README.md`
- `bead-shell/docs/decisions/0004-structure-licensing-generalization.md`
- `bead-shell/docs/research/00-scope.md`
- `bead-shell/docs/research/02-rendering.md`
- `bead-shell/docs/research/05-prior-art.md`
- `bead-shell/docs/research/baseline-summary.md`
- `bead-shell/docs/research/feature8-gate-c.md`
- `bead-shell/docs/research/qml-retirement-inventory-20261023.md`
- `bead-shell/docs/research/scope-index.md`
- `bead-shell/docs/research/scope-inventory.json`
- `bead-shell/docs/research/source-pins.json`
- `bead-shell/docs/research/spike-setup.md`
- `bead-shell/docs/specs/0029-packaging-coexistence-rollback.md`
- `bead-shell/docs/specs/0033-switch-systemd-validation.md`
- `bead-shell/docs/specs/0037-workspace-launcher-parity.md`
- `bead-shell/docs/specs/0051-osd-kinds-pin.md`
- `bead-shell/docs/specs/0056-visual-parity.md`
- `bead-shell/docs/specs/0059-welcome-app.md`
- `bead-shell/docs/specs/0083-weather-settings-i18n-harness.md`
- `bead-shell/docs/specs/0084-trial-switch-routes.md`
- `bead-shell/docs/specs/0090-live-window-previews.md`
- `bead-shell/docs/specs/0092-palette-mode.md`
- `bead-shell/docs/specs/0095-sound-wifi-parity.md`
- `bead-shell/docs/specs/0099-wallpaper-hwdec.md`
- `bead-shell/docs/specs/0168-smoothness-audit.md`
- `bead-shell/docs/specs/0197-launcher-actions.md`
- `bead-shell/docs/specs/0208-widgets-editor.md`
- `bead-shell/docs/specs/0213-launcher.md`
- `bead-shell/docs/specs/evidence/0076/metrics.json`
- `bead-shell/docs/specs/evidence/0103/README.md`
- `bead-shell/docs/specs/evidence/0124/validation.json`
- `bead-shell/docs/specs/evidence/0136/launcher-favorite-stars.json`
- `bead-shell/docs/specs/evidence/0136/notification-app-icons.json`
- `bead-shell/docs/specs/evidence/0136/notification-history-popout.json`
- `bead-shell/examples/hyprland-backend.lua`
- `bead-shell/examples/niri-backend.kdl`
- `bead-shell/scripts/rehearse-ui-switch.py`
- `bead-shell/scripts/systemd-validation/Dockerfile`
- `bead-shell/scripts/systemd-validation/install.sh`
- `bead-shell/scripts/systemd-validation/validate.py`
- `bead-shell/spikes/backend-reference.py`
- `bead-shell/spikes/backend-windows.qml`
- `bead-shell/spikes/bar-matrix.py`
- `bead-shell/spikes/bar-reference.py`
- `bead-shell/spikes/bar-ui.py`
- `bead-shell/spikes/blank-outputs.qml`
- `bead-shell/spikes/blank.qml`
- `bead-shell/spikes/calendar-reference.py`
- `bead-shell/spikes/detach-ghost-proof.py`
- `bead-shell/spikes/fake-bluez.py`
- `bead-shell/spikes/fake-power.py`
- `bead-shell/spikes/fake_system.py`
- `bead-shell/spikes/fullscreen-lips.py`
- `bead-shell/spikes/launcher-port.qml`
- `bead-shell/spikes/launcher-reference.py`
- `bead-shell/spikes/measure.py`
- `bead-shell/spikes/model-proof.py`
- `bead-shell/spikes/notification-reference.py`
- `bead-shell/spikes/panel-reference.py`
- `bead-shell/spikes/panel-reload.qml`
- `bead-shell/spikes/panel.qml`
- `bead-shell/spikes/parity-compare.py`
- `bead-shell/spikes/placement-reference.py`
- `bead-shell/spikes/reference/nacre-placement/README.md`
- `bead-shell/spikes/results/baseline.json`
- `bead-shell/spikes/status-reference.py`
- `bead-shell/spikes/stress.py`
- `bead-shell/spikes/tests/test_fake_system.py`
- `bead-shell/spikes/wallpaper-video-bench.py`
- `bead-shell/ssh-placeholder/shell/shell-routes.json`
- `bead-shell/ssh-placeholder/shell/sound/body.luau`
- `bead-shell/ssh-placeholder/spikes/backend-reference.py`
- `bead-shell/ssh-placeholder/spikes/backend-windows.qml`
- `bead-shell/ssh-placeholder/spikes/bar-matrix.py`
- `bead-shell/ssh-placeholder/spikes/bar-reference.py`
- `bead-shell/ssh-placeholder/spikes/bar-ui.py`
- `bead-shell/ssh-placeholder/spikes/blank-outputs.qml`
- `bead-shell/ssh-placeholder/spikes/blank.qml`
- `bead-shell/ssh-placeholder/spikes/calendar-reference.py`
- `bead-shell/ssh-placeholder/spikes/fake-bluez.py`
- `bead-shell/ssh-placeholder/spikes/fake-power.py`
- `bead-shell/ssh-placeholder/spikes/fake_system.py`
- `bead-shell/ssh-placeholder/spikes/fullscreen-lips.py`
- `bead-shell/ssh-placeholder/spikes/launcher-port.qml`
- `bead-shell/ssh-placeholder/spikes/launcher-reference.py`
- `bead-shell/ssh-placeholder/spikes/measure.py`
- `bead-shell/ssh-placeholder/spikes/model-proof.py`
- `bead-shell/ssh-placeholder/spikes/notification-reference.py`
- `bead-shell/ssh-placeholder/spikes/panel-reference.py`
- `bead-shell/ssh-placeholder/spikes/panel-reload.qml`
- `bead-shell/ssh-placeholder/spikes/panel.qml`
- `bead-shell/ssh-placeholder/spikes/parity-compare.py`
- `bead-shell/ssh-placeholder/spikes/placement-reference.py`
- `bead-shell/ssh-placeholder/spikes/reference/nacre-placement/README.md`
- `bead-shell/ssh-placeholder/spikes/results/baseline.json`
- `bead-shell/ssh-placeholder/spikes/status-reference.py`
- `bead-shell/ssh-placeholder/spikes/stress.py`
- `bead-shell/ssh-placeholder/spikes/tests/test_fake_system.py`
- `bead-shell/ssh-placeholder/spikes/wallpaper-video-bench.py`

## Stage B focused validation

The final isolated dev-1 snapshot passed shared i18n coverage (1,787 messages;
2,503 obsolete IDs removed), pinned Ruff 0.16.10 format/lint for 29 Python files,
`cargo fmt --all --check`, and `git diff --check`. Exact focused commands:

```sh
nice -n 10 /tmp/check-env/bin/python tools/i18n/check.py
nice -n 10 /tmp/check-env/bin/python -m unittest discover -s tools/tests -p test_ui_shell_switch.py -v
PYTHONPATH=nacre/shell-tools nice -n 10 /tmp/check-env/bin/python -m unittest discover -s nacre/shell-tools/tests -p test_installer_dropin_guard.py -v
nice -n 10 cargo fmt --all --check
```

Results: i18n PASS, switch 18 tests OK, installer 13 tests OK, Rust format exit 0.
The installer tests cover shared fresh/code-only setup and preservation of old
files when Bead activation fails. These checks are supplemental; full desktop
The final full desktop, real Bead and package results supersede these initial
focused checks and are recorded above.

The menu engine's frozen JavaScript/provider tests retain their original catalog
input under historical research. Only the shipped launcher catalog loses the
retired documentation entry; original generic reference inputs and compressed
expected outputs remain unchanged.
