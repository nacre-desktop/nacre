# N0094: Event driven daemon idle work

## Goal

Keep the daemon asleep when its inputs have not changed. Repeated status and
method catalogue construction must not scan PATH or rebuild protocol schemas.

## Findings and changes

The supplied 60-second profile reports 103‰ of a core (~10.3%) and 51 context
switches/s while idle, with hot samples in index lookup, menu normalization,
software rows, JSON schema work, and executable `statx` checks. The Niri event
path invalidated the complete prepared launcher index whenever visible window
titles or workspace names changed. That index includes static app and software
rows, although only the window action rows need those compositor fields. Window
actions now join the query-dynamic candidates, so compositor updates no longer
discard/rebuild the static index. Executable availability is cached by program
and PATH value, shared by controls and status-pill methods; a PATH change clears
the cache. Core and launcher method catalogues are initialized once and cloned
when requested. Schema validation and method contents remain unchanged.

## Validation and acceptance

Before: owner-provided private daemon profile, 60 s, ~10.3% core, ~51 process
context switches/s. Candidate: private nested Niri daemon on prod, 60.19 s,
0 `/proc/<pid>/stat` CPU ticks at 100 Hz (<0.017% core resolution), 61 voluntary
and 1 involuntary context switches (1.03/s total), five stable threads, zero swap PSS.
Debug trace over the idle capture logged zero launcher watch changes and zero
"Launcher inputs changed; refreshing index" events. The measured rate is below
the <0.5% core and <5 switches/s targets. The benchmark now accepts any stable
thread count when checking sleep/wake cycles and reports both switch counters.
No live session or daemon is part of this measurement.

Run all suites and the complete `desktop` job from
`.github/workflows/check.yml` in an Arch container. Passed on the final source:
`cargo fmt --check`; `cargo clippy -j2 --all-targets -- -D warnings`;
`cargo build -j2 --locked --examples`; `cargo test -j2 --quiet` (814 passed,
2 ignored); the protocol minimal consumer; the release helper build; helper
parity tests (28 passed and 534 cases, 0 failures); `tools/check.py` (tools 508,
AI 120, Brain 52, shell-tools 491; all passed); translation coverage/audits
(244 and 185 states per language); and the ignored daemon memory regression
(1 passed). No temporary duplicate paths or new product lines are introduced.
