# Software roadmap: Phase 2

Status: approved Phase 2 implementation in progress, 2026-10-15. The two
[theme bundle pilots](theme-bundles.md) and registered monospace/Kitty font choice
are implemented in source. Remaining proposals below describe their own scope;
the [general-profile AUR review flow](aur-review.md) is implemented in source.
This document does not enable an extension feed. Phase 1 is described in
[software-phase-one.md](software-phase-one.md).

## Named theme bundles

A theme bundle should combine a complete Orient dark/light palette pair with a
small, explicitly licensed wallpaper pack. It should have a stable namespaced
ID, display name, version, upstream palette source/revision, author/license
credits, per-asset hashes and thumbnails. Wallpaper and palette licenses must be
recorded independently: a palette project's license does not license unrelated
images found in a community gallery.

The launcher should show both modes and the included images before Apply.
Applying should offer palette only, wallpaper only or both, retaining the
existing wallpaper collection and Colors favorites. A managed transaction should
retain the previous selection and restore it if palette validation or wallpaper
activation fails. Imported bundles must not execute installation hooks or shell
commands. Keep named bundles separate from wallpaper-derived color extraction;
both continue to use Orient for complete roles and contrast validation.

These eight familiar themes are candidates, not a ranking or a promise of
redistribution rights. Upstream mode names are proposed starting points; derived
light adaptations must be named and credited as Nacre adaptations.

| Candidate and upstream | Proposed dark/light starting points | Wallpaper direction to commission or license |
| --- | --- | --- |
| [Catppuccin](https://github.com/catppuccin/catppuccin) | Mocha / Latte | Quiet pastel landscapes and abstract forms |
| [Tokyo Night](https://github.com/folke/tokyonight.nvim) | Night or Storm / Day | Restrained evening city and daylight architecture |
| [Nord](https://github.com/nordtheme/nord) | Polar Night / a Nacre light adaptation using Snow Storm roles | Snow, water and northern terrain |
| [Gruvbox](https://github.com/morhetz/gruvbox) | Upstream dark / light | Warm paper, earth and subdued illustration |
| [Dracula](https://github.com/dracula/dracula-theme) | Upstream dark / an explicitly derived Nacre light adaptation | Violet geometric and nighttime imagery |
| [Everforest](https://github.com/sainnhe/everforest) | Upstream dark / light | Forest, moss and muted natural textures |
| [Rosé Pine](https://rosepinetheme.com/palette/) | Main or Moon / Dawn | Soft botanical and architectural images |
| [Solarized](https://github.com/altercation/solarized) | Upstream dark / light | Balanced blue, cream and low-detail scenes |

The two pilots are Catppuccin Mocha/Latte and Rosé Pine Main/Dawn. Validate
readability, selected/disabled states, terminal colors, external app exports,
multiple wallpaper aspect ratios and offline operation. Add the remaining
candidates only after the role mapping and asset rights are reviewed. Avoid
claims of pixel-identical upstream rendering when Orient derives additional roles.

## Curated, read-only extension catalog

Ship a small catalog as reviewed data, not arbitrary remote code loaded into the
main shell. Each record should identify its publisher, repository, immutable
revision, Nacre manifest/schema compatibility, supported kinds, hardware/tools,
license, screenshots and review date. Catalog updates should follow the normal
source/package review path. Browsing and searching the catalog should work
offline and must not fetch repositories, resolve mutable branches or execute
anything.

The initial UI should provide descriptions, source/manifest links and supported
capabilities. An explicit Install action should delegate to the existing staged
extension importer; it should preserve the importer’s size/path checks and its
disabled-by-default behavior. Enabling remains a separate decision. The first
catalog should prioritize a few independently reviewed launcher packs and palette
themes. Standalone panel entries must explain that they run as the user's normal
applications; a separate process is not a filesystem sandbox. Embedded bar or
dashboard widgets need a separate architecture proposal.

Acceptance requires immutable provenance, malformed-record handling, duplicate-ID
rejection, missing-capability explanations, no browsing-side network/code
execution, offline fixtures and a nested test demonstrating that catalog changes
cannot silently replace an enabled checkout. Define a removal/revocation policy
that disables a problematic recommendation without deleting user edits.

## General-profile AUR review

Implemented: see [the policy, review boundary and validation](aur-review.md).
The proposal below records the design intent.

The general profile should offer a readable PKGBUILD before installing an AUR
package. The review must also expose accompanying `.install` files and relevant
local patches; displaying only `.SRCINFO` is insufficient. Fetch into an owned
staging checkout, identify the exact reviewed commit, then ask whether to continue
with the native package-manager workflow in a visible terminal. A changed recipe
or dependency recipe must trigger another review before it is built. No fetched
recipe should be sourced or executed merely to produce its preview.

Official repository packages keep their ordinary package-manager confirmation.
The owner's personal profile retains its current behavior. Implement the policy
as an explicit, registered profile setting with a clear default and provenance,
not a hostname/account-name check. The general path must not silently inherit a
personal skip-review flag or use `--noconfirm` to cross the review boundary.
Preserve the existing personal path and its tests while adding general-profile
coverage. Do not change shared packaging pins as part of this policy work.

Before implementation, decide the review UI (terminal pager first is sufficient),
dependency-recipe policy, recipe-change comparison and failure/cancel behavior.
Tests should prove cancel has no build/install effects, the reviewed revision is
the built revision, updated recipes are re-reviewed, missing AUR tooling remains
actionable, and personal/general profiles keep distinct declared behavior.

## Implementation gates

Implement these proposals as separate changes after approval: bundle schema and
two pilots; catalog data model and offline browser; general-profile AUR review.
Use the existing settings/state registry for new preferences and retain global
launcher search. Validate in nested sessions with normal protections, immutable
fixtures, `nice -n 10` and bounded build parallelism. Source commits and CI do not
authorize live installation, package publication or a release.
