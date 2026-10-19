# N0012 — Reduce tracked F4 QML memory debt

Status: Queued, low priority; source/investigation work is not authorized now.
Activation depends on F4 acceptance and a separate lead assignment. Owner decision:
Oct19 final fix attempt, then move on. No switch-on blocker if the approved
Hyprland acceptance condition passes.

## Recorded cost

Original candidate `8a1df35c6fec44e660530643ffef367e6b57ef89` versus baseline
`5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57`, five valid Niri A/B pairs,
unchanged standard v14 geometry and quiet/safety gates. Independent condition
medians, MiB (whole group / QML increases):

| Phase | Group | QML |
|---|---:|---:|
| Fresh static | +9.499 | +5.643 |
| Panels closed | +10.098 | +8.173 |
| Static after latency | +6.679 | +4.799 |

Private report SHA256:
`226ec4f6fd14ac7f5faa7811da68a385c74b783ea9924f983980a47f059412e9`.
This is the measured Niri debt reference, conditionally approved by the owner;
F4 still needs five valid Hyprland pairs with group increase <=12 MiB per phase.
Record the final Hyprland report when available; do not label F4 accepted early.

## Future bounded unit

Reduce QML residency without protocol/schema changes or behavior regressions.
Start from saved smaps/Qt GC/surface evidence and rejected optimization results;
do not repeat broad profiling without a concrete new hypothesis. Preserve all
original sources, raw measurements and failed optimization branches.

Correctness must cover affected consumers, structural equality, owned fields,
publication failure, lifecycle release and daemon-off behavior. Measurement uses
matched whole-group and per-process medians at the recorded geometry, the same
phases, and enough alternating pairs to distinguish run-to-run variation. A
single pair cannot decide a 5 MiB threshold. New capture requires a lead grant,
private capped scope, verified containment and the standing quiet/safety gates.

Later phases must not increase QML residency beyond this recorded debt without
fixing the increase. Do not silently move the reference or reclassify QML growth
as daemon cost. No live action, package publication or installation in this spec.
