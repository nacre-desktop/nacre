# N0405 — v0.1 user docs and compositor matrix

Status: docs complete; recheck at the cut.

The matrix reflects the 2026-11-11 source audit. Cited compositor source files
have no changes between `fe04e653` and current `origin/main`; launcher mapping
changes do not affect generic app launch or workspace placement. Source cannot
confirm Hyprland overview hover/highlight, Hyprland detach behavior in-session,
or launcher placement fallback on stock niri and Hyprland.

The VM spec records install, session and render passes for all tiers on
2026-11-12, but purge failures before the N0402 state fix. N0402 records a
passing niri purge rerun only. All-tier purge status needs confirmation at cut.

Checks: relative documentation links resolve; no TODO remains in the changed
user docs. No temporary duplicate or product code was introduced.
