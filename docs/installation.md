# Install Nacre

Use an updated Arch or CachyOS system. Setup can install the selected Wayland
compositor if it is missing, or prepare Nacre alongside an existing session. Arch package
recipes are available, but **Nacre packages are not yet published to the AUR**.
Obtain matching, tested binary archives from the maintainer. The tested Hyprland
stack uses Hyprland 0.56.2 (Lua configuration), Quickshell 0.3.2, Qt 6.12 and UWSM.
Upgrade the desktop and Qt together; avoid partial system upgrades.

## Arch packages and setup

Install the separate Orient engine and Nacre core archives from the same tested
release. In the directory containing the archives:

```sh
sudo pacman -U orient-*.pkg.tar.zst nacre-0*.pkg.tar.zst nacre-session-niri-*.pkg.tar.zst
# Optional, if you want both apps:
sudo pacman -U nacre-ai-*.pkg.tar.zst nacre-brain-*.pkg.tar.zst
nacre setup
```

Pacman installs shared code and resolves repository dependencies. Run setup as
your ordinary user, without sudo. Binary installation needs no compiler, npm or
Python package downloads. The wizard confirms keyboard layout, compositor,
optional apps and profile, then shows the private files it will deploy and asks
before applying. A custom profile is a validated JSON file; see
[configuration](configuration.md).

For an explicit preview or unattended application:

```sh
nacre setup --non-interactive --profile default --compositor hyprland --layout us --plan
nacre setup --non-interactive --profile default --compositor hyprland --layout us --yes
```

`--plan` writes no configuration and never installs packages. `--yes` applies
the file plan. Interactive setup offers the missing packages through pacman;
non-interactive setup requires `--install-deps` to do that. The package transaction
uses a full system upgrade and retains pacman's normal confirmation prompt.
Configure the Nacre package repository before asking setup to install Nacre
session or optional packages; they are not in Arch's official repositories. Without a layout, setup uses
system XKB settings, falling back to US. Existing private keyboard overrides are
preserved. Omitting `--profile` retains an installed profile; `--profile default`
selects the public default with five workspaces and no startup apps or app routes.
Optional app installation never signs in, downloads models or creates notes.

Local edits and foreign runtime links stop deployment before cutover. A failed
apply restores its snapshot. Package setup prepares the next login; it does not
restart your running desktop or certify it as tested. Save work, log out and
select **Nacre (Hyprland)** or the existing UWSM-managed Hyprland session. Open
Welcome with `nacre-welcome`, the launcher with Super+Space and Settings with
Super+I. Check keyboard, displays, audio, network and lock/unlock.

## Choosing Hyprland or niri

The packaged setup supports **Hyprland** and **niri 26.04**. It defaults to the
current desktop when supported; you can choose explicitly:

```sh
nacre setup --compositor niri
# Prepare both logins, with Niri as the primary selection:
nacre setup --compositor niri --also-compositor hyprland
```

The wizard offers a primary compositor and optional additional compositors.
`nacre-session-niri` and `nacre-session-hyprland` contain their login entries and
depend on the corresponding compositor and portal backend. Core does not force
installation of another compositor. Pacman recognizes installed compatible
packages/providers, so an existing Niri installation is reused. Orient remains
a separately installed engine package required by Nacre.

Niri setup creates a separate configuration under `~/.config/nacre/niri` and the
package supplies a **Nacre (niri)** entry at the login screen. It preserves the
existing `~/.config/niri` configuration, so a plain Niri/Noctalia session remains
available. No prior Nacre/Hyprland installation, source checkout or compiler is
required. Read [TTY recovery](recovery.md#recover-from-a-tty), save work, log out
and select **Nacre (niri)**. UWSM owns this session; do not start a second Niri
service alongside it.

The wizard validates the new Niri configuration before applying it and keeps a
rollback snapshot. It does not restart the current desktop. Nacre services start
only in the selected Nacre Niri session. Put private Niri overrides in
`~/.config/nacre/niri/custom.kdl`; repeat setup preserves that file, the keyboard
choice and existing palette/display settings. It leaves common Fish, Kitty,
Fastfetch and UWSM configuration alone when preparing Niri.

Niri remains experimental: installation preparation is not a complete hardware
acceptance test. Check first login, launcher, keyboard, displays, audio, network,
manual lock/password unlock and logout. Explicit lock requests are handled by a
session-owned Hypridle service; automatic idle/suspend locking is not enabled by
this preparation. Validate manual unlock before adding that policy. Keep the
plain Niri session available and see [Nacre on niri](../README.md#nacre-on-niri)
for compositor feature limits. The source-only [separate login preparation
guide](development/niri-testing.md#full-nacre-login-preparation-2026-10-13) remains
available for development experiments.

### Niri hover companion

The optional `nacre-niri` package adds window titles when hovering in native
niri overview. Install its matching tested archive before preparing niri:

```sh
sudo pacman -U nacre-niri-*.pkg.tar.zst
```

It installs under `/usr/lib/nacre/niri`, alongside `/usr/bin/niri`, without
replacing the distribution's compositor. The source development preparation tool
selects it automatically. The package setup currently uses stock Niri; installing
the companion does not switch the packaged login to it. Without a packaged or already verified private
companion, source preparation builds and verifies one, requiring Rust and niri
development libraries. Its `--stock-niri` option skips the companion; overview
still works, but hover titles do not. The companion does not establish a complete
login, unlock, idle or screen-sharing test. Package details are in the
[Arch guide](../packaging/arch/README.md#niri-companion).

## Updates

Upgrade matching packages, then run `nacre setup` to review per-user changes.
`nacre status` shows the current setup; `nacre rollback` restores the previous
private-file snapshot and refuses to overwrite later edits. It cannot undo a
pacman upgrade: retain previous archives if a matching package downgrade is
needed. See [recovery and troubleshooting](recovery.md).

## Source installation

Contributors need the [source build tools](contributing.md#build-tools). Clone
the repository and initialize its pinned Orient dependency:

```sh
git clone --recurse-submodules https://github.com/nacre-desktop/nacre.git
cd nacre
npm ci --ignore-scripts
python3 nacre/shell-tools/provision.py
./install.sh --profile default
./install.sh --profile default --prepare /tmp/nacre-preview
```

The first install command prints a plan. `--prepare` stages an empty destination
without changing the running desktop. From a supported Hyprland session, apply a
committed, checked candidate with `./install.sh --profile default --apply`.
For optional apps, add `--with ai` and/or `--with brain`. Source deployment uses
live health checks and rollback; package setup only prepares files for next login.
