# Contributing

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
  kconfig kcolorscheme breeze nodejs npm
```

Use Python 3.13 or newer. Initialize the engine submodule and formatting tools:

```sh
git submodule update --init --recursive
npm ci --ignore-scripts
python3 nacre/shell-tools/provision.py
```

## Check a change

```sh
python3 tools/check.py
./install.sh --profile default --prepare /tmp/nacre-test-install
```

The source check covers Python lint/format, shell/Lua syntax, native Qt/QML,
JavaScript formatting and regression tests. CI installs the pinned Pillow version
from `dependencies.lock.json`; Orient itself supports a range for library users.

Use an empty staging directory or a separate test user for installation work.
Test source changes before a live update. A normal desktop update should preserve
accounts, notes and busy assistant workers. Full graphical login, hardware devices,
lock/resume and performance need real-session checks in addition to unit tests.

## Packaging and dependencies

Nacre pins Orient in `deps/orient` and `dependencies.lock.json`. Update the submodule,
lock and desktop-adapter dependency together. Regenerate the checked-in startup
palette with `python3 tools/generate-palettes.py` when the engine changes.

The library's broader Pillow requirement belongs in Orient; the exact desktop
version belongs in Nacre's lock. A dependency change must invalidate the isolated
runtime build cache. Keep template previews separate from palette publication.

Bundled wallpapers must permit redistribution. Preserve their source/license
metadata and update their checksums and dimensions after optimization. Public
screenshots use bundled images and isolated demo state, never personal accounts,
notes or wallpaper collections.

## Releases

Versions below 1.0 are development previews. Update `VERSION`, package metadata,
README and changelog together, pass checks, then create an annotated version tag.
The manually dispatched release workflow publishes that tag as a GitHub prerelease
under the automation account. PyPI publication is a separate future step.
