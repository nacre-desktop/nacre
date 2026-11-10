# N0153 — recover transient plugin markup timeouts

## Problem and root cause

After a loaded login, the updates bar item disappeared on both outputs. The
plugin host had saved `runtime error: markup exceeded 50 ms limit` in
`plugins.json` as a permanent fault. Restarting the shell therefore excluded the
plugin again. The plugin form's protected call swallowed the timeout, recorded
the fault and returned an invisible node instead of using view recovery.

## Fix

Framework side B0229x belongs to this unit (no separate spec number). Bead
classifies budget overruns before recording permanent faults and propagates
them through the form wrapper to the existing view transaction/retry path.
The last accepted tree survives a failed build. Loading state ignores timeout
faults written by older hosts, so the next shell start retries those plugins.
Other plugin faults remain visible on the Plugins page and recoverable with
its existing enable action. The 50 ms CPU budget is unchanged.
The product registry also lets index-load failures reach view recovery instead
of treating a failed index as an empty plugin list. All four registry bundles
explicitly mount plugins, so the missing-index fallback was unnecessary.
The shell, helper and product-proof runner Bead pins move together to retain
one framework revision; the helper's service behavior is unchanged.

The separate drag-preview `unknown binding bar.collections` error is a real
schema/projection omission: workspace previews read the collection list and
current id when popouts are enabled. Both preview schemas now declare those
stores and the shared preview projection forwards them.

No new surface, layout, service, background job or duplicate implementation is
introduced. No live state is changed; deployment and live acceptance belong to
the lead.

## Checks

The framework regression runs a real infinite Luau loop inside a mounted
plugin render. With the old callback it fails with “transient plugin timeout
unexpectedly replaced the scene” (exit 101); the fixed callback passes, retains
the accepted scene, writes no fault state and renders healthy content on the
next update. A second test loads old timeout faults while retaining genuine
faults. The existing permanent-fault/re-enable checks remain in place.

Full Bead Ubuntu 24.04 CI passed on dev-1 (8 CPUs, 16 GiB), including all 165
bead-ui tests, both minimal-host proofs, private service/tray/clock lifecycles,
and dependency/license checks for all 12 manifest roots. Bead main is
`7ea19860ed1e5f0157d47f7b27c92ee035c21978`.

The [hosted Bead run](https://github.com/nacre-desktop/bead/actions/runs/42777098595)
failed before checkout: the existing workflow requests 8 CPUs, but the hosted
runner has 4. No code checks ran there. That CI configuration issue is outside
this plugin unit; the required full workflow was replicated on dev-1.

The Nacre workspace suite passed, including all 866 shell tests (4 existing
ignored checks), saved-updates-timeout recovery and the workspace drag preview
with collection defaults and projected selection. The protocol consumer proof
also passed. Full Arch `desktop` CI is still running through the release builds,
product proofs, source checks, galleries and raster-memory checks on dev-1.
