# N0008 — Cache the software-library package availability probe

- ID: N0008
- Repo: nacre-desktop/nacre
- Implementer: future built-in worker
- Status: Blocked — parked, do not start
- Depends on: F4 and owner priorities
- Date: 2026-10-19

## Goal and context

Later, remove unnecessary repeated pacman-Sichatgpt probes without stale or false
software availability. Source map: software_ai.rs26–38 invokes pacman-Sichatgpt
on each software-library snapshot; software_library.rs624 calls it. Standalone
NacreSoftwareLibrary.qml90–96 refreshes every15s while active; menu worker load
also builds the snapshot. A private captured child matched that command and had
~78MB PSS. This does not prove owner's physical-session cause or CPU totals.

## Deferred scope

When assigned, define a bounded freshness/invalidation policy, keep blocking or
risky probe work in an exiting worker, preserve availability/error behavior, and
add fake-command tests for call count, invalidation and failure. No live pacman,
package install or desktop operation. Full concrete steps/acceptance and a
worker assignment must be written before status Ready. Owner explicitly says
save this small task for later; DO NOT implement during current F4 work.
