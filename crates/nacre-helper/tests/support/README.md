# Font parity renderers

The frozen lock text masks and panel PNGs remain byte-exact with FreeType 2.14.3.
FreeType 2.15 changed TrueType v40 hinting, so the font files alone do not fix the
renderer output. See the [upstream release notes](https://sourceforge.net/projects/freetype/files/freetype2/2.15.0/).

On another FreeType version, the two frozen font tests run themselves in a child
process using the checksum-pinned reference renderer. The test-only Python
bootstrap builds it with `make -j2` under the Cargo target's `test-deps` directory;
it needs a C compiler, make, pkg-config and HarfBuzz development files. It installs
no system package and changes no system configuration. Its source URL and SHA-256
are in `font-renderer.json`. A cached build works offline. Missing build tools,
download/checksum errors, a wrong child renderer or zero selected tests fail.

A separate test compares all 56 text inputs between Rust and Pillow using the
current process's FreeType, including when Pillow has a bundled library. This
preserves frozen correctness while checking current system compatibility. The
installed helper keeps the system renderer; this harness is never installed.
Neither font fixtures nor expected pixels are regenerated.
