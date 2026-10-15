# Palette presentation witnesses

These developer-only programs run in the disposable HOME, Wayland compositor and
D-Bus session created by `tools/performance-session.py`. They are never installed.
Build them before measuring; never compile during a quiet comparison:

```sh
mkdir -p /tmp/nacre-palette-probes
nice -n 10 cc -Wall -Wextra -Werror tools/performance-probes/gtk-palette.c \
  -o /tmp/nacre-palette-probes/gtk-palette $(pkg-config --cflags --libs gtk+-3.0) -lm
nice -n 10 c++ -Wall -Wextra -Werror -fPIC tools/performance-probes/qt-palette.cpp \
  -o /tmp/nacre-palette-probes/qt-palette $(pkg-config --cflags --libs Qt6Widgets)
```

Add `--wallpaper-frame-smoke --palette-probes /tmp/nacre-palette-probes` to a
normal nested-session invocation with an exact prebuilt `--helper-binary`.
Use `--revision` and the corresponding pinned `--orient-binary` for the Python
baseline. Its CLI package and shared state modules are staged from that revision;
installed packages must not supply baseline code. The native publication path
calls Orient libraries directly; the engine executable is only a baseline tool.

The clock starts inside the copied shell immediately before `setWallpaper`.
The shell witness waits for its selected poster, palette and rendered frame.
GTK and Qt use ordinary themed widgets without loading CSS or setting colors.
Kitty queries its current background and displays ordinary ANSI-colored text;
it never sets terminal colors. Both compositor smokes capture pixels inside the mapped witness window bounds;
niri witnesses are arranged on the active workspace before the selection clock. An application paint callback or OSC
reply alone is not evidence of a presented frame.

`--lock-frame-smoke` additionally starts actual Hyprlock in the private nested
session. Capture evidence includes the lock-acquired event and new accent pixels.
Functional lock probes use `--immediate-render --no-fade-in --grace 0` in both
builds; immediate rendering avoids the software niri lab's screencopy preload
stall. These developer options do not measure default lock startup latency.
Cleanup terminates all process groups started by the harness, including the
nested lock; no password or live session interaction is needed.

The five-second observation and two-second lock startup waits are functional
checks, not latency estimates. A stale client produces an incomplete desktop
result. Do not quote a finite whole-desktop time from the shell-only timing or
file timestamps. Per-client crops use each compositor's mapped window geometry, at fixture scale 1.
Use the owner's separate quiet-window A/B/A/B policy for performance conclusions:
measured SwapPss zero and host swap-in below 1 MiB, with counters and page size.

The action observer samples SwapPss for all private harness descendants at 250 ms,
including witnesses and compositors. Reports retain raw host swap counters and
page size; `paging_gate_failures` rejects missing coverage and swapped processes.
Sampling does not establish coverage of processes shorter than its interval.
A paging-valid functional smoke alone does not establish a quiet measurement window.
