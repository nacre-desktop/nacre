# N0160 desktop CI results

Every check in `.github/workflows/check.yml`'s `desktop` job passed on dev-1 in
the task-private Arch Linux container (8 CPUs, 16 GiB, no swap). This note
summarizes the run; detailed commands and logs were temporary task evidence and
are not part of the repository. The [N0160 spec](../../specs/0160-daemon-interaction-cpu.md)
records the measured behavior and product change.

Formatting, workspace Clippy with warnings denied, examples, all workspace
tests, minimal protocol consumer, release helper, trace-memory policy tests,
feature checks, and shell/wallpaper builds passed. The pinned Luau analyzer
reported `LUAU_ANALYZE_OK 189 modules`; product checks and private
compositor/wallpaper smoke passed. Cargo-deny advisories, bans, licenses and
sources passed. `npm ci`, translation coverage (1,787 messages), and generated
assets passed.

The first CI pass stopped at the source guard because temporary benchmark
scripts under `.lead` were inside the checkout. Moving that scratch outside the
source tree resolved the environment issue. The source/UI check and every
remaining workflow step were rerun in order and passed: Python/JSON/shell/Lua/
QML checks, compositor boundary, palette CLI, JavaScript/Prettier, four Python
suites, capture review, and the ignored daemon raster-memory test. The memory
test passed in 25.56 seconds (peak anon 261,532 KiB; settled anon 20,476 KiB;
RSS 54,364 KiB; clean file PSS 19,723 KiB). The completed rerun exited zero.

An earlier run at nice 19 had four shell-test failures; the tests passed in
isolated runs and in the complete nice-10 workspace rerun without source or
assertion changes. Container-only portal/FUSE warnings did not fail the checks.
No test or build ran on the laptop or occupied desktop TEST host. Owner/lead live
audit remains separate from this private-container evidence.
