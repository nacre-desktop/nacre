# N0406 — Tier 1 nacre-niri package and setup

Status: implementation; validation pending. Base: `fe04e6535`.

Pin: `1028c5b919abac907c06b63c6610584e42bded0f`, `nacre` branch,
`26.04.r3079.g1028c5b9`. Fork history (`git rev-list --count`) gives 3079;
the retained NN0027/NN0028 install note identifies the same package/revision.
`nacre-brain search "r3079"` returned only older r3073 notes, so history and
the retained package note provide the exact pin evidence.
The pin lives in `packaging/arch/niri/recipe.py` as `SOURCE`. Bump it only after
reviewing the new fork commit and confirming its own `packaging/arch/PKGBUILD`
and companion wrapper files with the same build/runtime proof; update this hash
and the recorded fork version together.

The niri package component stages the fork's own PKGBUILD, session wrapper and
desktop entry at that pin. Only `_commit` is added, using the recipe's existing
standalone override. No fork recipe logic is copied into Nacre. The fork depends
on `niri>=26.04`, has no provides/conflicts, and owns separate binary/session
paths. Core split-package builds inject the fork archive into their build chroot.
Repository artifact validation includes the fork; signing/publication is unchanged.

Setup recommends nacre-niri first; existing niri/Hyprland selections and combined
sessions remain valid. Fork sessions use the existing Niri integration and protocol
backend. No package replacement or transaction planner redesign is needed.

The fork supersedes nacre-niri-hover. Delete its recipe, staging, private builder,
patch, prepared-session receipt selection, obsolete tests and trust fixtures.
Stock Niri keeps its existing stream fallback; fork hover comes from native IPC.
No temporary duplicate implementation remains. No live session or fork changes.

Acceptance: focused setup/compositor/package tests, pinned clean Arch package
build/metadata/namcap and transaction proof, and the complete desktop CI job in
the Arch image on dev-1 under the release slot lock. Use the existing fresh-user
and signed-delivery harnesses for nacre-niri alongside stock cases.

Evidence and final line delta will be recorded after validation. Raw output stays
in task-private scratch; no package is published or uploaded.
