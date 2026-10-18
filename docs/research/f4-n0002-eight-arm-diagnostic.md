# F4 N0002 diagnostic — 2026-10-18

**Publication blocked; geometry noncompliant.** Eight v12/v13 arms passed cell
validators, full windows, process SwapPss0, strict collector/whole-wrapper
paging and scope OOM checks. Independent source/raw audit found actual
1920×1080/scale1/60Hz, not required2880×1800/1.5 H120/N60. The frozen collector
hardcodes the former at lines815/823/849; driver transforms did not override it.
These are diagnostics, not N2 target-profile acceptance. No runtime push/live action.

Sol+Terra accepted v12first4/v13last4 comparability: memory reads/inclusion/
arithmetic/collector transforms/windows unchanged; invalid fifth replaced with
fresh v13 arm, never retrospectively accepted. Raw evidence remains private.
Archive SHA25610e9603e31e0ce7b13d130d0e9c9344396556984f38998a798683eef8a1e70fb;
162-file transfer index SHA256d27dbc41e9adf6ca7d4a4131720c391871debb4926dca7c27dd79d525be29855.
Controller ended22:31:55UTC, owned scopes/PIDs/session gone, lock released.

## Matched group median deltas

Candidate minus baseline MiB; whole-group samples[].all_pss_mib includes every
emitted process row including private D-Bus. No pooling across different phases.

| Pair | Fresh | Panels closed | After latency |
| --- | ---: | ---: | ---: |
| Hyprland1 | -7.897 | -8.438 | -16.127 |
| Hyprland2 | +23.565 | +20.790 | +6.612 |
| niri1 | +10.277 | +4.453 | +0.352 |
| niri2 | +9.798 | +10.942 | +16.692 |

Niri fresh repeats above5MiB. QML median deltas+9.604/+6.373MiB, daemon
+0.255/+2.767, other Nacre helpers+0.460/+0.691. Independent component medians
are not additive. QML end-of-phase anonymous mapping deltas+12.410/+3.223MiB,
shared buffers+3.594/+4.660, file categories−0.296/−0.048. These single endpoint
mappings do not decompose whole-phase medians. Pacman appeared3versus2 of195
fresh samples, which does not explain the median difference.

Cold CLI single-observation matched deltas(ms): H1+15.730,H2−9.680,N1−14.835,
N2+36.796; no endpoint acceptance inferred from this spread. Exact raw per-run
values/min/p95/max/components remain in private eight-arm-phase-review.json.

## Source leads, not proven causes

Candidate has11 initial topics versus baseline3; added Settings payloads total
onlyabout2.3KB. DesktopSettings serializes/reparses snapshots and subscribes plus
refreshes; NacreStateFile also serializes and initializes with subscription+get.
These may affect heap/rebinding highwater, not yet allocation proof. Twenty
always-live daemon stores and Hub snapshots explain some service state, not
proof of the QML/shared-buffer difference. Don't optimize blindly at wrong target.

Observed pacman-Sichatgpt child matches software_ai::snapshot on software-library
load. Standalone QML refreshes15s whileactive; menu worker load also constructs
that snapshot. Physical-session cause/CPU contribution remains unproven.

## Next required unit

Prospective geometry correction only, independently reviewed. Real readiness
must validate actual physical2880×1800, logical1920×1200, scale1.5, H120/N60 and
QMLscreen dimensions before any600s warmup. Config requests alone aren't proof;
missing/unsupported modes fail closed. Then a newly granted full8arm profile
capture,>=104min fixed+overhead. Prior8 remain diagnostic. F4 still requires
standing whole-group/endpoint gates and exact published-head CI; no waiver.
