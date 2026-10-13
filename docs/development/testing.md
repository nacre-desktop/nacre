# Desktop test infrastructure

Run the normal gate before integrating changes:

```sh
PATH="$PWD/.venv/bin:$PATH" python tools/check.py
```

The checker creates a disposable HOME and separates its configuration, state,
data and cache directories. Child checks have closed stdin. Native rendering
needs a virtual X display, as in CI's `dbus-run-session -- xvfb-run` invocation.

## Production QML and dependency fixtures

`nacre/shell-tools/tests/qml_fixtures.py` registers unchanged production QML in
its original directory layout, preserving relative JavaScript and QML imports.
`run_qml` verifies the staged QML/JavaScript bytes against the checkout before
loading it. Do not remove production objects, replace root types, inject test
properties, or edit production imports to make a test load.

The test import path supplies Quickshell, Io, Wayland, Hyprland, Widgets,
Bluetooth and the used Services modules. Processes record commands/writes and
expose controllable started/exited/output signals; they execute no commands.
File views expose synthetic text and loaded/failed/changed signals without host
file access. Device collections, screens and environment values are mutable
fake state. Fixture providers override dependencies through module manifests.
A small Qt plugin supplies attached layer-shell properties and native
notification property names that plain QML cannot declare. It is built in a
temporary directory and is never installed with the desktop.

Use `stage_qml(source, destination)` for a component under test. A production
singleton stays a singleton: refer to it from the test, rather than instantiating
it through a Component. `run_qml(..., isolate=True)` starts a fresh QML engine
for each service test function so cases retain independent initial state.
Use typed PersistentProperties/ShellScreen inputs when the production API
requires them. Observe actual child objects through objectName or a distinctive
property instead of injecting aliases into the component.

Prefer `tryCompare`, `tryVerify`, signals, files and IPC replies to startup
sleeps. Bounded waits for absence or a documented minimum interval are still
useful when testing a temporal contract. Keep a finite failure deadline.
The standalone watcher test proves subscription readiness with file-change
replies, including a newly created subdirectory. `NACRE_TEST_HELPER_BINARY`
lets that CLI test use a replacement executable with the helper name as its
first argument; its assertions depend on output/files, not Python imports.

## Repeated checks under load

```sh
PATH="$PWD/.venv/bin:$PATH" python tools/tests/check_load.py \
  --jobs 4 --rounds 3 --output /tmp/nacre-check-load
```

This owns its virtual display and private buses and runs the entire gate in
parallel. Each run has separate state. `results.json` records elapsed wall time,
exit status and the corresponding log. Only owned process groups are stopped.

## Fresh-install smoke test

```sh
PATH="$PWD/.venv/bin:$PATH" NACRE_ORIENT_BINARY=/path/to/candidate/orient \
  python tools/tests/smoke_install.py --output /tmp/nacre-install-smoke
```

CI runs this in a clean Arch container. It prepares the default profile into an
empty staging directory with a separate empty installer HOME, then uses the
staged installation as the runtime HOME. Preparation intentionally installs no
dependencies: the candidate SDK environment and the shipped palette bridge are
supplied as runtime prerequisites, matching the source/package installer.

The runner owns a private D-Bus, Xvfb display and nested niri compositor. It
starts the unchanged staged shell, drives launcher/dashboard/settings/wallpaper
IPC, waits for resulting state, captures a screenshot, and rejects QML load,
binding and file-view warnings/errors. Optional hardware services are not a
substitute for hardware/session testing. Logs, IPC state, timing and screenshot
are uploaded even when the smoke fails.

CI also tries Hyprland's headless backend. It may report unavailable only before
the shell starts; once a compositor starts, all shell assertions remain strict.
Cached Arch packages still undergo normal package database refresh/update;
venv/Cargo cache keys include interpreter/compiler and source/dependency inputs.
The normal installation/build commands still run after cache restoration.
