# N0080 — active compositor shortcuts

The shortcuts topic was empty because Niri discovery scanned a managed default
folder for filenames containing `bindings`, and parsing rejected private KDL.
The source is now the active loader: the Niri socket process's `-c` / `--config`
argument (or its default loader), then explicit `NACRE_NIRI_CONFIG` / `NIRI_CONFIG`,
then the niri-on-pc session
loader, standard XDG loader, or managed loader. Niri IPC does not expose its loaded
config path. Use the PID in its socket filename to read process metadata, without
opening extra compositor connections. No live session, installed files or configuration is changed.

Read KDL with the KDL v1 parser used by Niri's configuration syntax, following
relative, absolute and home-relative includes in order. Preserve overlay titles,
custom bindings, native actions and spawn arguments. Project the effective Niri
records directly so file metadata cannot undo include-order overrides, with canonical modifier
ordering and the existing QML projection/order/grouping. Session-generated
`Overridden by custom.kdl` comments remain excluded; repeated canonical keys
resolve to the final definition. Hyprland keeps its existing discovery and live
query. Bound traversal to 256 files / 8 MiB and reject malformed input or cycles;
missing/empty loaders publish an empty list, missing required includes fail,
optional missing includes remain watched, and failures retain stale good state.

Directory inotify watches cover included files, missing includes, symlink targets,
in-place writes and atomic replacement, without an idle polling timer. Re-read
and rebuild watches on config changes and explicit refresh. No protocol/UI changes.

Replaced the generated-only regex parser, frozen binding corpus and its obsolete
capture generator/tests with filesystem/parser/topic tests. No temporary duplicate
implementation remains. Existing Hyprland frozen capture/discovery tests remain.

Validation: full desktop workflow in an isolated Arch container on a host selected
by the standing slot policy; focused session-tree, empty/missing/invalid/cycle,
spawn/native/title and automatic config-refresh tests. Results recorded below.

Include/override semantics: [upstream Niri documentation](https://github.com/niri-wm/niri/blob/main/docs/wiki/Configuration%3A-Include.md).

The existing shared-stream daemon regression asserts exactly one event stream and
one persistent request connection; config discovery must open neither. Niri
encodes its PID in the [IPC socket name](https://github.com/niri-wm/niri/blob/main/src/ipc/server.rs).

The full remote workflow uses `RUST_TEST_THREADS=2` on the shared build host. An
unchanged Colors preview test exceeded its five-second wall-clock deadline with
default harness concurrency, then passed in isolation in 0.39 seconds. The
assertions and deadlines are unchanged; all suites still run.

Validation passed: the complete desktop workflow in the Arch CI image, including
format/Clippy, examples, all Rust suites, the standalone protocol consumer, release
helper, sandbox integrity (28 tests), `tools/check.py` (501 tools / 120 AI / 52
Brain / 490 shell-tool tests), native/frozen parity checks, both translation
galleries (244 / 185 states per language), and the CPU raster memory regression.
Focused checks also cover the daemon's two-connection invariant, config changes
and stale recovery, and include-order title/modifier overrides through the final
QML projection. Local/remote source manifests are retained with the worker report.

No live files, installed components, keybindings, or units were changed.
