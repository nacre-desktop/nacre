# License boundaries

Nacre’s first-party product code, including `crates/nacre-shell/` and `shell/`,
is GPL-3.0-only; the full license is in [LICENSE](LICENSE). Individual manifests,
source notices and separately licensed assets retain their own terms.

The Bead framework is an MIT dependency fetched by Cargo at the pinned revision.
The root `vendor/fontconfig/` and `vendor/system-tray/` patches retain their
upstream MIT licenses and patch provenance. Bead's generic test fonts and plugin
examples remain in the dependency checkout, with their original notices.

The shipped shell binaries include Bead framework code; its MIT notice ships
from `packaging/licenses/LICENSE-Bead-MIT`. Nacre’s GPL license ships from `LICENSE`.
Bundled font, image and icon notices remain adjacent to their assets, including
the CC0 welcome demo provenance.
