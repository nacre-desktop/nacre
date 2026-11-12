# N0406 — Tier 1 nacre-niri package and setup

Status: integrated 2026-11-12. The package runtime proof is still open; it needs the Orient release N0408.

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

## Evidence (dev-1, 2026-11-11/12)

- Fork package: a clean Arch container built `nacre-niri-26.04.r3079.g1028c5b9-1`
  (sha256 `2b72f42e…`). namcap reported warnings only, and `pacman -U --print` resolves.
- The complete desktop job of `check.yml` ran in the Arch image. Every step passed at
  least once on this branch. Later runs failed only in tests this unit does not touch,
  and those tests passed when run alone:
  - the nacre-helper wallpaper watcher test;
  - two nacre-shell wallpaper catalogue tests;
  - `launcher::tests::latest_same_scope_query_replaces_rows_without_an_empty_frame`,
    which hung under the full parallel suite.

  The Shell lead fixes these in N0208. On the final commit, the focused setup,
  package, repository and niri-session suites pass (63 tests, 2 skipped).
- Not yet proven: the fresh-runtime install of Core, all sessions and the fork. The
  signed Orient release `arch-0.1.0dev6-1` was built from Orient `17bf36ef`, but the
  engine lock pins `3efe8d1e`, so `fetch-orient.py` rejects it. A matching Orient
  release (N0408) comes first. The package workflow then runs this proof for all
  four session choices when it is dispatched.
- Product lines: about +300 and −2,390, most of it the retired companion and its
  patches.
