# Contributing

Start with [CONTRIBUTING.md](../CONTRIBUTING.md) for contribution policy, reporting
channels and PR expectations. This guide covers development tools and releases.

General desktop changes belong in Nacre. Color analysis and palette generation
belong in [Orient](https://github.com/nacre-desktop/orient). Keep user configuration
and account data out of source changes. Make small, focused changes and describe
what the user will see and how you checked it.

## Build tools

The current source installer runs validation and builds native desktop
integrations. These are build/check dependencies, separate from desktop runtime
tools:

```sh
sudo pacman -S --needed ruff lua fish qt6-declarative gcc pkgconf gtk3 \
  kconfig kcolorscheme breeze nodejs npm rust lcms2 cmake make
```

Use Python 3.13 or newer. Initialize the engine submodule and create an isolated
check environment rather than activating development code in your daily session:

```sh
git submodule update --init --recursive
npm ci --ignore-scripts
python3 -m venv .venv
cargo build --release --locked --manifest-path deps/orient/Cargo.toml -p orient-cli
export NACRE_ORIENT_BINARY="$PWD/deps/orient/target/release/orient"
.venv/bin/python -m pip install ./nacre/shell-cli ruff==0.16.10 pillow==12.3.0
```

The Pillow version above matches `dependencies.lock.json`; update this example
when that pin changes. Run `provision.py` only in the separate test account when
validating live installation: it activates the palette runtime for that user.

## Check a change

```sh
PATH="$PWD/.venv/bin:$PATH" python tools/check.py
./install.sh --profile default --prepare /tmp/nacre-test-install
```

The source check covers Python lint/format, shell/Lua syntax, native Qt/QML,
JavaScript formatting and regression tests. CI installs the pinned Pillow version
from `dependencies.lock.json`; Orient itself supports a range for library users.

Stage installation work in an empty directory before applying it in a separate
test account.
Test source changes before a live update. A normal desktop update should preserve
accounts, notes and busy assistant workers. Full graphical login, hardware devices,
lock/resume and performance need real-session checks in addition to unit tests.

## Packaging and dependencies

See [CI jobs and owners](development/ci.md) for automatic correctness checks,
advisory benchmarks and manual GPU acceptance.

Nacre pins Orient in `deps/orient` and `dependencies.lock.json`. Update the submodule,
lock and SDK engine fingerprint together. Regenerate the checked-in startup
palette with `python3 tools/generate-palettes.py` when the engine changes.

Nacre calls the pinned standalone Rust binary through its JSON interface; no
Python Orient package is installed. Pillow remains for desktop thumbnails/artwork,
with its exact desktop version in Nacre's lock. A dependency change must
invalidate the isolated runtime build cache. Keep template previews separate from palette publication.

Bundled wallpapers must permit redistribution. Preserve their source/license
metadata and update their checksums and dimensions after optimization. Public
screenshots use bundled images and isolated demo state, never personal accounts,
notes or wallpaper collections.

## Releases

Versions below 1.0 are development previews. Update `VERSION`, package metadata,
README and changelog together, pass checks, then create an annotated version tag.
The manually dispatched release workflow publishes that tag as a GitHub prerelease
under the automation account. PyPI publication is a separate future step.

## UI size scale

New and redesigned UI uses the shared scale in
[`size-scale.json`](../nacre/shell/widgets/size-scale.json), exposed by
`NacreTokens.space`, `radii`, `textSize` and `iconSize`. This is plain versioned
JSON so Bead's future component kit can consume the same values. Spacing and radii
use logical pixels; text and icon font sizes use points. `radii.full` creates a
pill/circle (Qt clamps it to the available geometry). Use the named sizes rather
than introducing near-duplicate literals. Structural dimensions, hairline borders,
animation timing and measured geometry are separate concerns.

The scale follows the dashboard/control center's common values: spacing
2/4/8/10/12/14/18/24/32, corners 8/16/20/full, text 10/12/14/18 and icons
12/15/18/24/32. This first pass converts shared `widgets/`; existing module
layouts move to it when redesigned by their owning tasks.

The existing `textPointSize`, `cornerRadius` and `spacing` aliases retain their
12/12/8 defaults for modules awaiting redesign. New UI uses the named groups.
