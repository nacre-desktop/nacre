# Implementation spec queue

Updated 2026-10-17. [Standing owner rules](../architecture/standing-rules.md) apply to every assignment.
Spec agent owns design, review, queue order and measurement grants. Implementers
must not infer permission from an old roadmap or reserve a measurement slot.

| Order | ID | Assignment | Status | Dependencies |
| --- | --- | --- | --- | --- |
| 1 | N0001 | [F4 settings completion audit](0001-f4-settings-completion-audit.md) | In progress | preserve current unpublished F4 |
| 2 | N0002 | [F4 whole-group acceptance/publication](0002-f4-whole-group-acceptance.md) | Ready | N0001 source-ready; slot grant |
| 3 | N0003 | [Core network state adapter](0003-core-network-state-adapter.md) | Ready | N0001/N0002 Done; green published F4 |

First Ready assignment is authorized after its listed prerequisites pass. Existing
work is preserved; send exact unpublished state before overlapping changes. Ready
with dependencies means fully specified, start only after dependencies are Done.

Status flow: Ready → In progress → Review → Done; Blocked names an exact missing
dependency or decision. Start the first Ready row whose dependencies are Done.
Report completion to the spec agent and continue to the next eligible Ready row.
Only the spec agent marks Done. Ask before deviating from scope.

Measurement slot: **released; no active grant**. The former F4 capture was
stopped and owned-worker cleanup confirmed. Partial evidence is preserved;
incomplete cells do not count. N0001 source audit may proceed. N0002 requires
source-ready review plus exact full artifact hashes, host scope and time window,
then a separate explicit grant. Neither implementer may start measurements from
this index. No live/physical testing is authorized.

Beyond this queue: any unresolved F4 ownership fixes from N0001, then network controls and Bluetooth / power,
followed by deps/bead hosted-service work. These are not yet assignments. Core
work cannot bypass unresolved F4 ownership. Charter65 is historical; the current
registry has72 records,19 with Settings metadata. Scope is explicit ownership,
not arbitrary writable access to all72 records.
