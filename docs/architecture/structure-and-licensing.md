# Structure and licensing

Accepted owner direction (2026-10-21): Bead remains MIT OR Apache-2.0 as a
permissive general framework; Nacre remains GPL-3.0 as the flagship desktop
built on Bead; Orient remains MIT. The daemon and Nacre-specific integrations
(including compositor bindings, media backends, and daemon implementation) stay
on Nacre's GPL side behind neutral adapters. The `nacre-protocol` schemas,
generated types, golden transcripts, and protocol documentation are MIT OR
Apache-2.0, so third-party Bead shells can speak to the daemon without taking a
GPL dependency. See the [protocol license notice](../../protocol/README.md).

No daemon repository split is planned. Revisit only if it becomes a general
services daemon with outside users. Nacre-first priorities remain unchanged;
generalization is deferred until after Bead's core switch on Nacre. Keep
Nacre-specific appearance, Orient coupling, daemon assumptions, and compositor
details out of `bead-*` crates; record existing assumptions in Bead's
Generalization list without fixing them during this phase.

The corresponding Bead decision is [ADR 0004](https://github.com/nacre-desktop/bead/blob/main/docs/decisions/0004-structure-licensing-generalization.md).
