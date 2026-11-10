# N0159 — Repository root and evidence cleanup

Shipped app configuration now lives in `config/{fastfetch,fish,hypr,kitty,niri,uwsm}`
and profiles in `config/profiles`. Installer, session staging, package recipes,
generators and checks read those sources; installed `~/.config/<app>` and the
private session backend layout stay unchanged. The active protocol fuzz harness
moves to `tools/fuzz` with its crate path adjusted. A package symlink preserves
the documented `/usr/share/nacre/uwsm/sessions/nacre-niri-managed.desktop`
template path without copying it.

## Root audit

| Kept path | Current consumer |
| --- | --- |
| `ai/`, `brain/`, `bin/` | Optional installers, native wrappers and `packaging/arch/stage.py`. |
| `deps/` | Only the pinned Orient submodule; desktop CI builds its palette engine. |
| `protocol/` | Generated protocol schemas/types and MIT/Apache license notices. |
| `install.sh` | Source-install entrypoint in configuration and optional-app guides; delegates to `tools/manage.py`. |
| `package.json`, `package-lock.json` | Pinned Prettier for `brain/web/app.js`; `tools/check.py` and desktop CI run it. |
| `dependencies.lock.json` | Python/Orient pins consumed by CI, packaging and `tools/check.py`. |
| `VERSION`, `CHANGELOG.md` | Package version and release workflow validation. |

Reference searches found active consumers for each retained root entry. No
runtime or dependency was deleted merely because its directory was old.

## Evidence

Raw N0154/N0160 CSVs, JSON captures, per-pair directories, temporary replay
harnesses and older command/result logs are removed. Human-readable methods,
summary tables and CI outcomes remain, and links point to those summaries.
N0156 JSON reference fixtures remain because Rust tests consume them directly.
There were no tracked screenshot images under `docs/` to retain or prune.
Future raw runs belong in ignored `.lead/` or task-private host storage.

## Validation

Every check in `.github/workflows/check.yml`'s `desktop` job passed on dev-1
in the task's Arch Linux container (8 CPUs, 16 GiB). Rust tests used a bounded
4 GiB `/tmp` tmpfs, now also explicit in the desktop workflow: shared-disk
timing noise affected the existing 60-second Brain migration gate; the threshold
and test code were unchanged.

- `tools/check.py`: 406 tools tests (one intentional skip), 138 AI, 54 Brain and
  377 shell-tools tests passed, plus syntax, formatting and generated-config checks.
- Native gallery and the isolated daemon raster-memory regression passed.
- Both workflow package stage/install tests passed, including offline repeat,
  private-state preservation, rollback, all seven config trees and native artifacts.
- Disposable-HOME configure/map/helper-install suites passed (16/4/5 tests),
  including update from root app directories, stable installed paths and private files.
- `cargo check --manifest-path tools/fuzz/Cargo.toml --locked -j2` passed after
  refreshing its stale lockfile with the protocol's existing base64 dependency.

Latest-main protocol and dashboard-metrics tests passed after the final rebase.
N0155's documentation removals/rewrite and N0166's dashboard changes are retained.
No live installation was touched. Raw check output stays in ignored `.lead/`.
