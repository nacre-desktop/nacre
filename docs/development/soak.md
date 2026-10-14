# Desktop soak testing

`tools/soak.py` exercises Niri and Hyprland sequentially in nested sessions with
public fixtures and disposable HOME/XDG, D-Bus, network and PID namespaces.
The host filesystem is read-only; only the parent Wayland endpoint is exposed.
It does not install anything, restart the live shell or change user settings.
Use a task worktree, keep its source frozen during both runs, and run in tmux:

```sh
nice -n 10 .venv/bin/python tools/soak.py \
  --hyprland-headless --wayland-debug --seed 20261013 \
  --orient-binary deps/orient/target/release/orient \
  --output /tmp/nacre-soak-before
```

Use the [development environment](../contributing.md), Bubblewrap, Quickshell,
both compositors, D-Bus tools, `notify-send` and `wl-copy`. FFmpeg optionally
creates a synthetic video. Build prerequisites with `-j2`. The driver also lowers
its own priority. It retains the public Qt toolkit policy. Hyprland uses a guest
virtual output because its nested Wayland-backed output reproduced a presentation
stall; this does not establish physical-output performance.

Defaults: ten-minute warmup, three measured hours per compositor, 60-second
samples and five-second seeded actions. Confirmation adds `--hours 1` and a new
output directory. Actions include launcher queries, settings/dashboard pages,
pearl detach/dock, notifications, clipboard, windows/workspaces, wallpaper/video
and palettes; Niri also changes scale. At most four detached pearls accumulate.
`--actions settings close --action-interval 0.5` selects a supplemental remapping
probe; `--notification-images` selects raw D-Bus image payloads and requires
`dbus-next`. Keep these supplemental probes separate from matched comparisons.

Artifacts: `samples.csv`, `events.jsonl`, `report.json`, logs and a bounded 4 MiB
Wayland trace. Copy captures into ignored `.research/soak/` before reboot; `/tmp`
is temporary. Reports record the revision and source fingerprint, duration,
workload health, exits, failed actions/probes and helper lifetimes. No restart
hides a crash. Two minutes of failed IPC ends an invalid workload. Example:

```sh
.venv/bin/python tools/soak_analyze.py \
  .research/soak/before/niri .research/soak/before/hyprland
NACRE_RUN_SOAK_REGRESSIONS=1 .venv/bin/python -m unittest discover \
  -s tools/tests -p test_soak_settings.py
```

Analysis excludes warmup and separates PID/start-time lifetimes. Resident PSS
apportions shared pages; swapped PSS counts paged-out memory. Use their sum on a
busy host: falling resident memory alone does not prove release. Missing memory
coverage remains unknown. The summary prints full/first/final-hour trends and
mixed-action correlations, never raw captures. Sustained growth is an investigation
signal, not proof of infinite growth; finite runs cannot prove an eventual limit.
These counters omit unmapped GPU allocations. Short helper lifetimes and one
zombie observation do not establish unreaped processes. Dashboard redesign and
Python helper internals remain report-only; port acceptance is tracked in
[rust-migration](rust-migration.md#fix-during-port).

## Results and fixes

Corrected baseline: seed 20261013, revision `b17827f7`, public Qt policy, default
render loop, guest Hyprland headless output. Both three-hour measurements completed
with zero shell exits and zero action/probe failures. Long-lived helpers stayed
flat. Shell growth continued in the final hour; no plateau was observed.

| Compositor/run | Measured minutes | Shell PSS + SwapPss slope, MiB/h (R²) | Dismissal keys at end | Result |
| --- | ---: | ---: | ---: | --- |
| Niri before | 180 | +119.9 (.816); first hour +163.3 (.576) | 711 | Healthy workload; continuing shell growth |
| Hyprland before | 180 | +144.7 (.880); first hour +58.3 (.228) | 718 | Healthy workload; continuing shell growth |
| Niri after | Pending | Pending | Pending | Confirmation required |
| Hyprland after | Pending | Pending | Pending | Confirmation required |

Fixed: dismissal tombstones now protect only the initial asynchronous load;
removed entries drop native references, snapshots/images and timers immediately.
Native expiry no longer depends on popup visibility: long-timeout notifications
and DND notifications can expire while their text remains in history. Permanent
zero-timeout notifications remain permanent. Expired history drops live images.
Regression coverage includes load/reentrant races, repeated clear cycles, image
release, hidden long-timeout/DND expiry and hidden launcher focus requests.
An isolated real D-Bus/Quickshell check passed 30 raw-image cycles plus automatic
hidden/DND expiry; a nested image probe also returned tracked/referenced/image/key
counts to zero. Baseline notifications had no images, so this does not attribute
its large memory signal to image retention.

Settings remapping now leaves native rendering enabled and lets Qt suppress
unexposed frames. Toggling Quickshell's update gate explicitly requests a frame;
failed traces identified Settings attaching a buffer before its initial configure.
The native rapid-remap regression and long confirmations must establish the fix's
practical limits. Earlier protocol failures and an interrupted warmup remain
preserved failed attempts. Repeated Qt Wayland text-input focus warnings remain
unresolved; warnings are not suppressed. Wallpaper/video and pearl operations
have weak mixed-action associations with memory changes, insufficient attribution.
