# F4 memory acceptance — 2026-10-20

Status: memory acceptance PASS under the owner-approved debt decision. Source
integrated on main at dfcedea76e968bd71c62d7c9ea8c1b81dd68e542; exact-head CI
39393677394 is running. No published-head CI pass is claimed yet.
Selected source is original `8a1df35c6fec44e660530643ffef367e6b57ef89`,
baseline `5f4c0bf3025d5f2944b50ff5546ed7f8a3dcfe57`. Release helper hashes:
A `4a65f4ec887bc85e93b5b862f1786fe6f1b7a811875333765ee8f8c052cce0aa`;
B `439dd355fca276334c2faccbd0992d1b23eac6c6caab311ca1c1c61d3672e92c`.
Neither rejected optimization is included. No live switch occurred.

## Owner decision and method

The final bounded fix attempt ended without meeting group<=8MiB and QML<=2MiB.
The owner approved retaining original8a as tracked Niri memory debt, conditional
on five Hyprland alternating A/B pairs with group median increase<=12MiB in
each phase. Use unchanged standardv14 at2880x1800/scale1.5, Niri60Hz and
Hyprland120Hz, 600s warmup and three60s idle windows per arm. Whole-group sums
are calculated per sample; then per-arm medians and independent condition
medians. Component medians are independent and need not sum to group median.

Safety remains private12GiB scope, swap.max0, verified held-child containment,
>=4GiB MemAvailable before release, abort below2GiB, processSwapPss0 and strict
whole-arm host swap-in<1MiB. Exact original host swap capacity and used<64MiB
are checked before each arm. Owner-approved resets occur only between arms,
with no desktop heavy job and available memory>swap used+8GiB; free-h and
restored capacity are logged. Pre-arm stops do not create a measurement and
do not consume quiet-failure retry allowance. Preserve all accepted outputs
and immutable prior ledgers through hash-bound contiguous-prefix continuations.
Only host-swap-in quiet failures may retry once at batch end, max two arms.

## Niri five-pair reference

MiB, independent candidate-minus-baseline condition medians:

| Phase | Group | QML | Daemon | Other helpers |
|---|---:|---:|---:|---:|
| Fresh static | +9.499 | +5.643 | +2.415 | +0.799 |
| Panels closed | +10.098 | +8.173 | +1.438 | +0.777 |
| Static after latency | +6.679 | +4.799 | +1.388 | +0.794 |

Ten valid Niri arms, no quiet retries. Private report SHA256
`226ec4f6fd14ac7f5faa7811da68a385c74b783ea9924f983980a47f059412e9`.
This missed the former threshold and is approved debt only under the owner's
conditional decision, not a claim that the old gate passed.

## Category investigation and final rejected fix

The original5f4/original8a panels-closed category probe observed named Qt JS GC
heap resident PSS81,616→87,056KiB (+5.3125MiB). Qt allocator capacity grew
82,384,896→87,890,176bytes (+5.2502MiB), while live bytes after naturalGC were
18,399,648→18,395,264, essentially unchanged. This suggests retained JS capacity
or allocation churn rather than a demonstrated growth in live objects.
QV4_MM_STATS=1 was set, but no literal output was emitted; actual evidence is
qt.qml.gc.allocatorStats logging and kernel named-map categories.

Surface inventory matched: one64x48 QML window and six layers; layer pixel sizes
were not exposed. No extra surface was observed. Pss_Shmem stayed8KiB; file-backed
PSS fell199KiB. Both used jemalloc and had no [heap] VMA, so malloc was not
separately measurable by that label. Unknown anonymous mappings are not all JS,
and memfd is not synonymous with graphics. Total snapshot PSS fell528,663→
527,008KiB because other anonymous mappings offset JS growth. Thus this one
probe does not causally explain the five-pair regression; no graphics-buffer,
extra-surface, malloc or file-backed growth was established.

Final attempted fix67e0ef0c removed JSON round trips for two owned projections.
Forty focused daemon/QML tests passed without skips. Four valid fresh Niri arms
then produced group/QML deltas: fresh+7.211/+5.434MiB, closed+5.219/+1.595MiB,
after latency+13.747/+9.795MiB. It failed the required whole-screen limits, so it
is parked, not integrated. A panels-closed improvement alone was insufficient.
Raw archive SHA256175d8c5422aeec29326b67ecf30e5a1d73719d6defbe308bd90d70715785c944.
The earlier Colors optimization94866a37 also failed and remains excluded.
Cross-batch variance means these results do not prove the fixes caused growth.

## Hyprland result

All ten primaries valid, zero quiet-failure retries. Independent condition
medians, MiB (candidate minus baseline):

| Phase | Group | QML | Daemon | Other helpers |
|---|---:|---:|---:|---:|
| Fresh static | +8.266 | +6.324 | +1.413 | +0.658 |
| Panels closed | +6.868 | +3.692 | +0.971 | +0.638 |
| Static after latency | +0.713 | -0.276 | +0.892 | +0.622 |

Each whole-group increase is <=12MiB; the fallback has no separate QML gate.
Root independently recalculated medians and checked all ten raw source/helper
bindings, wrapper swap-in<1MiB, SwapPss0, actual full idle paging_window.elapsed_s
>=60, exact12GiB/swap0 limits and zero max/OOM events. The legacy duration_s
field ends at the last sample and can be slightly under60; it is not the full
window timer. Cleanup confirms all payload identities and cgroups absent, all
guardian return codes0, bench lock available.

Group paired-delta medians/ranges: fresh+8.266[-1.038,+16.232], closed+6.071
[-6.169,+25.910], after+0.389[-7.430,+9.738]. These descriptive spreads show why
one pair cannot decide a tight memory gate. No failed arm was substituted.

Several pre-arm stops occurred at swapused>=64MiB. Each preserved all accepted
outputs and stopped before the next payload. Approved resets restored original
capacity/zero use, then hash-bound continuations resumed the next unaccepted arm.
There was an overnight coordination stall after the first arm; it was not a
measurement failure. Continuation now proceeds on pre-agreed checks without a
lead approval wait. A failed readiness-smoke path and two pre-payload continuation
pin errors are preserved but did not create measurement arms.

Aggregate SHA256:
14c62595981c5eef0e690195da66454f534c1b0aa6e389738021a58ec774820d.
Full private raw archive SHA256:
381212ebe33655793348ad30b26ccc956d737b8ec700c58b2db8cd2d588508bf.
Private mirror: n0011-fallback-hypr-5pair-20261020T0100Z-final-review-mirror
under ~/.cache/nacre/rust-migration-validation/.


## Follow-up and release boundary

[N0012](../specs/0012-f4-qml-memory-debt.md) records the low-priority debt task.
Later phases may not add QML residency beyond the recorded debt without fixing
it. After measurement acceptance, integrate F4 normally and require exact-head
CI. N9/N10 installer handoff and managed opt-in/rollback remain separate source
work; acceptance is not authorization for a live switch, package or unit action.
