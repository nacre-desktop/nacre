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

**Decision: deferred, 2026-10-15.** Phase 2 adds no catalog implementation,
feed, schema or catalog browser. Existing manual extension import and the
first-party starter packs remain separate features.

**Trigger:** at least one real, independently maintained third-party Nacre
extension exists with a compatible manifest, reviewable immutable source and
clear licensing. At that point, return to the owner with a concrete catalog
proposal for separate implementation approval. Nacre's own starter packs and
the two theme pilots do not satisfy this trigger.

If approved later, the catalog should be reviewed, read-only data. Records should
identify publisher, repository, immutable revision, compatibility, capabilities,
license, screenshots and review date. Browsing and search must work offline and
must not fetch or execute code. Installation should reuse the existing staged
importer and remain disabled until separately enabled. Preserve user edits and
define a removal/revocation policy before shipping recommendations.

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

Approved decisions: terminal pager first; AUR dependency recipes reviewed one by
one in the same flow; diffs for changed recipes; cancellation before the final
build/install approval has no build or install effects.
Tests should prove cancel has no build/install effects, the reviewed revision is
the built revision, updated recipes are re-reviewed, missing AUR tooling remains
actionable, and personal/general profiles keep distinct declared behavior.

## Implementation and validation gates

The registered monospace font choice, two theme pilots and general-profile AUR
review are implemented as separate source changes. The extension catalog remains
deferred until its trigger and a new approval. No other theme candidates are
promised by this phase.

Source commits receive separate CI checks. Tests stay nested with private state,
normal protections, `nice -n 10` and build parallelism of two. Source pushes do not
authorize a deployment, package publication or release. The final laptop checks
are supervised, reversible Phase 1 actions; ask before each action and restore
its changes. Do not test DNS, SSH, firmware or the bootloader in that checklist.
