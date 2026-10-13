# Overnight soak testing

`tools/soak.py` runs seeded shell actions in nested Niri, then nested Hyprland,
one compositor at a time. It uses public generated demo data and a disposable
HOME, session bus, network and PID namespace. Only the parent Wayland endpoint
is exposed; live compositor IPC, user services and configurations are hidden.
The host filesystem is read-only. No installation or desktop restart is needed.

Use the development Python environment described in [contributing](../contributing.md).
Runtime prerequisites are Bubblewrap, both compositors, Quickshell, D-Bus tools,
`notify-send` and `wl-copy`. FFmpeg optionally supplies a small synthetic video.
An existing Wayland session supplies the parent display. Pass the pinned Orient
binary to exercise palette changes without building another engine:

```sh
nice -n 10 .venv/bin/python tools/soak.py \
  --orient-binary deps/orient/target/release/orient \
  --output /tmp/nacre-soak-before
```

Run it in tmux for unattended use. The default is ten minutes of warmup followed
by three measured hours **per compositor**, with one sample every 60 seconds.
For confirmation, repeat with `--hours 1` and a new output directory. Short
preflights can use `--hours 0.025 --warmup 10 --interval 10`.
The tool also lowers its own scheduling priority. Build prerequisites with `-j2`.

Each compositor writes `samples.csv`, `events.jsonl`, `shell.log`,
`compositor.log` and `report.json` under its output directory in `/tmp`.
The report identifies the Git revision, seed, action failures, warnings,
helper lifetimes and observed exits. Helper-service and wallpaper-watcher
start/exit probes are added only to staged QML. Shell/compositor exits end that
run rather than silently restarting it. Other short helper lifetimes are
observations, not proof of crashes; they can be legitimate jobs. CSV PIDs belong
to the private namespace. Helper stderr is shared with the shell, so per-helper
QML warning attribution is unknown rather than duplicated across helper rows.

Actions cover launcher queries, dashboard tabs, settings pages, launcher pearl
detach/dock, wallpapers, palettes, notification bursts, bounded clipboard owners,
workspace switches and test windows. Niri output scale changes every five minutes.
At most four intentionally detached pearls accumulate before docking; otherwise
the workload itself could resemble a component leak.

The analysis excludes warmup and separates PID/start-time lifetimes. It flags
PSS/USS slopes above 5 MiB/hour or descriptor/thread/child slopes above 0.5/hour
when the least-squares fit has R² ≥ 0.6 and at least five samples. These are
investigation thresholds, not memory budgets. Missing memory is unknown, never
zero. PSS apportions shared resident pages; USS counts private resident pages.
Neither measures all graphics-device allocations. Host swap counters accompany
samples because a busy machine can distort resident-memory trends.

Startup warnings about unavailable hardware services are expected in this
sandbox. Repeated warnings still need review. Preserve failed actions, crashes
and missing coverage in reports; a finished run is not automatically a pass.
Dashboard redesign defects and Python helper internals are report-only during
the concurrent redesign and Rust migration.

## Results

The overnight comparison is in progress; no long-run conclusion is claimed yet.

| Run | Niri | Hyprland | Result |
| --- | --- | --- | --- |
| Before | Pending: 3 measured hours | Pending: 3 measured hours | Baseline not complete |
| After | Pending: 1 measured hour | Pending: 1 measured hour | Confirmation not complete |
