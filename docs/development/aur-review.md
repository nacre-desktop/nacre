# AUR recipe review

The general profile requires terminal-pager review before AUR builds or installs.
Software app offers, font offers and package search all use this same installation
boundary. Official repository packages keep ordinary pacman confirmation and do
not require AUR tools. AUR review requires `git`, `less`, `pacman` and `makepkg`
plus the standard base-devel build environment. Missing review tools produce an
explanation rather than a silent bypass. Missing base-devel is included in the
official dependency plan and installed only after all reviews and final consent.

1. Resolve missing dependencies from the tracked `.SRCINFO`, without sourcing
   PKGBUILD. Fetch each AUR package base into an owned, detached checkout.
2. Show dependency recipes one by one, then the requested recipe, in `less`.
   Each document identifies the exact Git revision and contains PKGBUILD,
   `.install` files, patches and every other tracked text file. Binary assets
   show hashes and sizes. Escape control characters and bidirectional overrides.
3. If a recipe changed since its previous review, show a unified diff followed
   by the complete current checkout. Unchanged recipes still require review.
4. After closing each pager, type `reviewed` to approve that exact recipe. After
   all reviews, type `install` to permit builds and administrator install prompts.
   Cancelling any review or that final approval starts **no build or install**.
5. Install repository dependencies with ordinary administrator/pacman prompts,
   then build approved AUR recipes in dependency order and install their requested
   package archives. Recheck tracked contents and HEAD before each build.

Git hooks, credential helpers, external/file protocols, filters from global Git
configuration, submodule updates, pager shell/editor commands and LESSOPEN hooks
are not used. Review never calls makepkg, including its metadata-printing modes:
those can source PKGBUILD. The approved build uses normal checksum/signature and
dependency checks; there is no `--noconfirm` review shortcut. No AUR helper may
fetch a replacement recipe after approval. Common build parallelism is bounded
with MAKEFLAGS=-j2, CARGO_BUILD_JOBS=2 and CMAKE_BUILD_PARALLEL_LEVEL=2, under nice.

After the final `install` approval, normal package transactions and build failures
can leave installed build dependencies or completed packages. Review cancellation
is the effect-free boundary; this is not a rollback system for arbitrary approved
PKGBUILD code. A reviewed recipe still executes as the user, and source URLs can
refer to moving upstream versions. Exact-recipe review does not imply immutable
upstream source or a filesystem sandbox.

## Registered policy and provenance

Settings → System → Package review exposes `system.packages.aurReview`, stored as
`profile.json` → `aurReview`, with values `required` or `native`. Changing it asks
for confirmation. Global launcher search finds **AUR package review policy**.
The Settings provenance action runs `nacre-helper aur-review policy` in a terminal,
showing the effective mode, profile, registered store/key and source.

An absent/default general profile resolves to `required` with provenance
`general-profile-default`. Older custom profile declarations retain their prior
native package-manager flow, with `legacy-custom-profile-default` provenance.
An explicit setting overrides either default and reports
`registered-profile-setting`. No hostname, account name or personal ID is matched.
A personal consumer using the shared default profile must explicitly declare
`aurReview: native` in its source overrides to retain that behavior on future
installation. This source declaration does not edit an existing live profile.
The unchanged personal/native command path and frozen tests remain in place.
Software front ends use the policy-aware `software-package install PACKAGE`
dispatcher; the existing `menu-backend install` native compatibility contract is
retained for older consumers. It is not the general-profile Software entry point.

Review receipts use the state registry's `aur-review-recipe` schema at
`$XDG_STATE_HOME/nacre/aur-review/recipes/*.json`, recording the reviewed revision,
content digest/document, time and policy provenance. Newer/corrupt state refuses
review instead of bypassing it. A cancelled later review may retain earlier review
receipts and fetched cache files; neither is a build or package installation.

Cycles, ambiguous AUR providers, missing/inconsistent metadata, symlinks and
oversized recipes fail with an explanation. Choose a concrete provider or resolve
such a case manually; do not disable dependency checks. The planner limits a
request to 64 AUR bases, 512 files/base and 4 MiB of reviewed content/base.

## Evidence

Native fixtures cover dependency order, per-recipe/final cancellation, changed
recipe diffs, unreadable inputs, policy provenance and preserved native behavior.
The real dispatcher is checked with private tools and HOME/XDG state.
`tools/capture-aur-review.py` captures actual Kitty/less against inert local
recipes on an authenticated private Xvfb display. It never captures the live
desktop or builds/installs a package. `aur-review inspect CHECKOUT` is the same
read-only pager/document path without review approval or installation.

References: [Arch makepkg manual](https://man.archlinux.org/man/makepkg.8.en),
[PKGBUILD manual](https://man.archlinux.org/man/PKGBUILD.5.en),
[pacman manual](https://man.archlinux.org/man/pacman.8.en),
[AUR metadata RPC](https://aur.archlinux.org/rpc/v5/info?arg%5B%5D=google-chrome).
