<p align="center"><img src="assets/branding/nacre-avatar-transparent.png" width="140" alt="Nacre shell logo"></p>
<h1 align="center">Nacre</h1>
<p align="center">Mother-of-pearl. Colors shaped by your wallpaper.</p>

**Development preview · 0.1.0-dev.1.** Nacre is a Quickshell desktop for Hyprland.
This repository contains the general desktop. [Orient](https://github.com/nacre-desktop/orient)
is the independently versioned color engine.

Nacre includes a universal launcher, desktop controls, notifications, wallpaper
library with static/animated playback, Settings, Colors and a first-run Welcome
tour. AI and Brain are optional components. Generic defaults have numbered
workspaces, no personal app routing and no personal startup applications.

## Get the source

```sh
git clone --recurse-submodules https://github.com/nacre-desktop/nacre.git
cd nacre
./install.sh --profile default
```

The last command prints a plan. Inspect it before applying. The supported initial
target is CachyOS/Arch with the documented Hyprland Lua API and compatible Qt /
Quickshell packages; this is not a universal distribution installer or a bootable ISO.
See [installation](docs/installation.md) for dependencies and a clean-user workflow.

```sh
./install.sh --profile default --apply
# Explicit optional packages; no provider credentials or models are downloaded:
./install.sh --profile default --with ai --with brain --apply
```

Source must be committed/clean, and live deployment requires a running Hyprland
session plus the source/input validation gates. Existing managed changes are
checked for drift. Accounts, vaults, conversations and wallpapers are outside the
code snapshot; component installation does not copy them from the maintainer.

## Test a fresh installation without changing your desktop

```sh
./install.sh --profile default --prepare /tmp/nacre-core-preview
./install.sh --profile default --with ai --with brain --prepare /tmp/nacre-optional-preview
```

Preparation installs the selected file layout into an **empty** directory and
creates an inspectable manifest. It does not install system packages, start services,
change devices, create a vault or authenticate providers. It is a file-install
check, not proof of a real graphical login. Use a clean test user/accelerated VM
for complete first-install checks, then real hardware for performance/device tests.

## Personal profiles and updates

Profiles are validated JSON. They select components, workspace count/labels/icons,
startup argument arrays and application routes. The default is in
`profiles/default.json`; a consumer can supply its own:

```sh
./install.sh --profile /path/to/profile.json
```

Custom profiles can consume a pinned Nacre revision without duplicating its implementation.

The existing profile is retained when `--profile` is omitted. Selecting a profile
changes the desktop composition at the next managed deployment/login. Concurrent
full desktops on one user/session are not supported: notifications, publication
and services have one owner. Test users isolate account/config state; optional Brain
can use `NACRE_BRAIN_PORT` if a separate instance needs another port.

## Version and license

Nacre is in its0.x development-preview series. Version1.0 will be released when
installation and daily-use acceptance are ready. See [CHANGELOG](CHANGELOG.md).

Nacre-owned source is **GPL-3.0-only**: [LICENSE](LICENSE). Its system dependencies
retain their own licenses. The bundled landscape wallpapers are public domain.
Read [usage](docs/usage.md), [configuration](docs/configuration.md),
[optional apps](docs/optional-apps.md) and [updates/recovery](docs/recovery.md).
