# N0012 — Reduce tracked F4 QML memory debt

Status: Queued, low priority; F4 memory debt accepted after the approved Hyprland
five-pair condition passed. No new investigation work is authorized now.
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

## Category probe and rejected final fix

One private Niri session each for original5f4 and original8a, same geometry,
600s warmup and full idle windows, at panels closed. This is category diagnostic
evidence, not a replacement acceptance comparison. Surface inventory matched:
one64x48 QML floating window and six layer surfaces (background1, border exclusion4,
drawers1). Layer pixel sizes were not exposed; no extra surface was observed.

Named Qt JS GC heap VMAs grew in resident PSS/Anonymous from81,616 to87,056KiB
(+5.3125MiB). Qt GC allocator stats showed allocated capacity82,384,896 to
87,890,176bytes (+5.2502MiB), chunks1251 to1335, while live bytes after a natural
GC were18,399,648 versus18,395,264 (essentially unchanged). The lead was retained
JS allocator capacity/allocation churn, not demonstrated extra live JS objects.
QV4_MM_STATS=1 was set but produced no literal stats output; these are actual
qt.qml.gc.allocatorStats logging plus kernel named-map measurements, not a
claim that QV4_MM_STATS itself worked or an explicit GC was forced.

The overall point-in-time QML PSS actually fell528,663 to527,008KiB; other
anonymous mappings offset the named JS heap increase. Kernel Pss_Shmem stayed
8KiB, Pss_File fell199KiB. Both processes mapped jemalloc and had no [heap] VMA,
so malloc could not be isolated by a [heap] label. Unknown anonymous maps are not
all JS, and memfd mappings are not all graphics buffers. No graphics/surface or
file-backed growth was demonstrated by this probe. One snapshot cannot causally
explain the five-pair residency regression; this limitation must stay in reports.

The final fix67e0ef0c removed JSON round trips for two consumers through a
default-off object projection path. It passed40 focused daemon/QML tests, no
skips, then four valid fresh Niri arms with zero retries. Independent condition
median deltas, MiB (group / QML): fresh+7.211/+5.434; closed+5.219/+1.595;
after latency+13.747/+9.795. It failed the required group<=8/QML<=2 screen and
was parked as evidence, not integrated. The panels-closed improvement alone was
insufficient; other phases exceeded the limits. Cross-batch differences do not
prove this optimization caused the increase. Raw archive SHA256:
175d8c5422aeec29326b67ecf30e5a1d73719d6defbe308bd90d70715785c944.

The earlier Colors lazy-store attempt94866a37 also failed its four valid-arm
screen: group+14.687/+15.400/+18.709MiB and QML+12.115/+14.829/+15.278MiB
across fresh/closed/after-latency. It too remains parked, not part of selected8a.
Owner's final-attempt allowance is exhausted; no more profiling/fix hunt here.

## Accepted Hyprland reference

Original8a versus5f4, five valid pairs, zero retries: group increases fresh
+8.266MiB, closed+6.868MiB, after+0.713MiB (all<=12); QML increases
+6.324,+3.692,-0.276MiB respectively. Daemon+1.413,+0.971,+0.892MiB;
other helpers+0.658,+0.638,+0.622MiB. Root recalculated all medians and audited
raw quiet/full-window/containment/cleanup proofs. Full evidence and limitations:
[acceptance report](../research/f4-memory-acceptance-20261020.md).

This is accepted debt, not a claim that the former QML<=2MiB gate passed.
Source integrated asdfcedea7; published-head CI39393677394 pending at recording.
