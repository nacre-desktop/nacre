# Lock image test fonts

The two renamed subsets and unmodified card font are test inputs and are never installed. Original URLs,
revisions, byte counts and SHA256 values are in `provenance.json`, copied from
Nacre's pinned `font-assets.json`. The two upstream licenses are retained here.

Subset glyph IDs, hinting and variable axes are retained. Latin ligature mappings
for the included Material Symbols also stay present: removing those mappings
changed rendered pixels despite equal outline coordinates. All 238 mask, offset
and advance checks against the original full fonts passed before reference capture.

The lock references freeze the current Python helper with these font files and
explicit public system/time data. They are immutable; do not regenerate them to
make a native implementation pass.
