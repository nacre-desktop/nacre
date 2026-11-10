# N0160 measurement method

The [spec](../../specs/0160-daemon-interaction-cpu.md) contains the accepted
before/after summary tables, root cause, and limitations. This note preserves
the measurement method needed to interpret those results.

Daemon CPU is the complete process/thread group, derived from summed
`/proc/PID/stat` ticks over actual elapsed time. Waited-child CPU is reported
separately and included in a conservative total; it may be charged when a child
is reaped, after its work occurred, so it is not exact one-second attribution.
Live thread and process-tree samples are diagnostics, not the authoritative
daemon counter.

The five accepted pairs numbered 03–07 alternated before and after builds. Each
fresh private session warmed for 30 seconds, measured idle for 30 seconds, ran
the six-cycle 60-second interaction, then measured idle for another 30 seconds.
Both arms used the same release flags, symbols, timing overlay, real shell,
palette and static 1920×1080 wallpaper. No builds ran during captures. Failed
harness attempts and early pairs accidentally run at nice 19 were excluded.

Measurements ran in a private 8-CPU/16-GiB dev-1 container with no swap, host
graphics/audio mounts or live settings. Distribution Niri 26.04 hosted inside
labwc; labwc used pixman and nested Niri used software Mesa EGL at 1280×720,
scale 1 and 60 Hz. This is not the patched Nacre compositor used by N0154.
The N0160 build used source `eb0525b3e`, release debug symbols, no stripping,
and the same temporary timing instrumentation in both arms; the candidate also
included the lock-dashboard cache change. Those flags and topology matter when
comparing the measurements with other runs.

The final profile used the same validated replay but separate timing: 10 seconds
warmup, 5 seconds idle, 60 seconds interaction and 5 seconds idle. Its flat
`cpu-clock:u` report had 1,571 samples and zero lost samples; DWARF unwinding was
incomplete, so percentages describe flat instruction samples, not inferred call
counts. The spec records the resulting hot paths and measured memory deltas.
