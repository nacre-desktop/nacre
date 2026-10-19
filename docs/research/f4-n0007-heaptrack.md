# F4 allocation profiling — 2026-10-19

F4 remains unpublished. The valid uninstrumented source8a screen failed the
whole-group memory rule: fresh/closed/after deltas were +5.756/+6.121/+0.488 MiB.
Profiling below is diagnostic only and does not replace that result.

## Inputs and execution

Baseline source: `5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57`.
Candidate: `8a1df35c6fec44e660530643ffef367e6b57ef89`; native source/helper unchanged
from accepted `1382a28ab544743e32e906f7542bcecdfdfca083`.
Standard heaptrack/heaptrack_print1.5.0 were used on private seeded instances at
2880×1800, scale1.5, Niri60Hz. Original600s settling, three60s phases and eight
scene actions were retained. Native supervisor was replaced solely by a private
profiler lifecycle, so this is not an acceptance-equivalent process group.

The initial standard wrapper launch timed out before readiness and its output
was truncated by acceptance cleanup. A private lifecycle instead terminates only
its recorded QS process, waits for standard heaptrack/compression completion,
checks integrity, then cleans supporting processes. Frozen acceptance tools were
not changed. An external timeout caused a separate stopped-job launch failure;
that extra wrapper was removed and its evidence retained.

Standalone baseline completed. Plain candidate stopped before configuration
loaded; its allocation trace spans1.46s despite60s readiness wait, not a full
workload. A separately reviewed startup backtrace sampled heaptrack/libunwind
inside an existing Qt.md5/OpenSSL allocation; IPC subsequently answered after
detach. This suggests profiler interaction but does not establish its mechanism.
The16 Qt.md5 callsites are identical between baseline and candidate. No hash or
product change is justified by this observation.

Candidate then completed the full workload with one brief debugger attachment
before settling. Debugger detachment was verified before workload. This A/B
instrumentation asymmetry is retained. All scene checks, six-config/96-catalog/
12-favorite input parity, trace drainage and exact-owned cleanup passed.
Both full traces and standard parses are complete. All jobs are released.

## Results and limits

Whole-run standard CLI output, preserving its formatted byte suffixes:

| Quantity | Baseline | Candidate |
| --- | ---: | ---: |
| Trace duration |833.90s|835.91s|
| Allocation calls |39,883,974|40,091,957|
| Peak heap |165.12M|165.89M|
| Trace-end unfreed bytes |106.58M|104.47M|

The standard B-minus-A diff exited0, stderr empty. Its largest reported positive
per-stack peak delta is524.29K in Qt string/URL/QML conversion; this is not an
application source attribution. Independent peak subtraction is not a comparison
of simultaneous states. Trace-end unfreed bytes do not establish a leak or PSS.
The diff does not identify a multi-megabyte retained owner explaining the valid
uninstrumented regression.

Massif text contains dense interval-peak totals but zero-valued detailed owner
trees. Tagged1.5.0 source explains the writer/pass ordering; changing thresholds
or merge flags is unsupported as a remedy. Parent launch and recorder time origins
are not aligned exactly; final boundary mismatch is about0.28s in candidate.
No exact endpoint coverage or phase owner claim follows from those text rows.

The standard GUI prefix filter[0,T] can retain all allocations/frees from startup
until T. Its Leaked column then represents bytes still held at T; Peak remains
a prefix maximum. See the tagged [replay implementation](https://github.com/KDE/heaptrack/blob/v1.5.0/src/analyze/accumulatedtracedata.cpp)
and [filter defaults](https://github.com/KDE/heaptrack/blob/v1.5.0/src/analyze/filterparameters.h). A phase-only nonzero-start selection cannot give the same held
set. Private GUI feasibility is being prepared, not a new capture or tool parser.
No source fix, new acceptance retry or publication is granted by this report.

## Safety and evidence

The original held-child12GiB/swap-zero scope and independent guardian applied;
host start>=4GiB/abort<2GiB retained. No OOM events occurred in completed runs.
Cleanup records cover exact owned processes, scope, tmux and lock release.
No physical-session UI, Nacre package release, units or session opt-in was changed.

Full baseline archive:363e5358e21fb5e00b1acbd19b8d0645593ca0a1b7c9045553f20259c8911051.
Full candidate archive:a72c25efd0408b75d6ea81afb0fd83f1940372353999c79919186a80dbfce448.
Raw traces: A08b0f59c1fdad3ab2310a7bba2446d48ceea646dc9e713aced7180ba33482b1e;
B2b014e60d0aeb09c640d973271673e220b49d196aa4735802c36678adcea0766.
Private reports preserve failed startup traces, backtraces, tool/source pins,
standard output, resource events and cleanup records. None are acceptance data.

## Private GUI analysis boundary

Luna prepares and coordinator reviews routine analysis under the latest owner
Luna-first workflow. Root accepted final launcher after bounded process shutdown,
Xauthority stdin/absence and intentional143 shutdown handling corrections. One
30-minute private Xvfb session is granted, original capped guardian unchanged.
Initial loaded A summary must be reviewed before prefix selection or B. Explicit
private DISPLAY/Xauthority and exact GUI window/PID checks gate all input.
No physical display, app configuration, profiling capture or tool parser changes.
The prefix must visibly begin at0 and sort by Leaked; a nonzero start or Peak sort
is not held-state evidence. Time selection precision remains explicitly limited.
