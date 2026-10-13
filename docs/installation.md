# Install Nacre

Use an updated Arch-based system with a working Wayland desktop. Arch package
recipes are available, but **Nacre packages are not yet published to the AUR**.
Obtain matching, tested binary archives from the maintainer. The tested Hyprland
stack uses Hyprland 0.56.2 (Lua configuration), Quickshell 0.3.2, Qt 6.12 and UWSM.
Upgrade the desktop and Qt together; avoid partial system upgrades.

## Arch packages and setup

Install the separate Orient engine and Nacre core archives from the same tested
release. In the directory containing the archives:

```sh
sudo pacman -U orient-*.pkg.tar.zst nacre-0*.pkg.tar.zst
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

`--plan` writes no configuration; `--yes` applies. Without a layout, setup uses
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

Choose **Hyprland** for the packaged setup path. Nacre also has an experimental
**niri 26.04** backend with generated bindings and shell integration, but the
packaged niri setup is not ready: `nacre setup --compositor niri` is
currently rejected. Installing niri or selecting its session entry alone does
not prepare Nacre for it.

For niri, start from a working Nacre/Hyprland installation and a source checkout
with the [source prerequisites](contributing.md#build-tools). Follow the
[separate login preparation guide](development/niri-testing.md#full-nacre-login-preparation-2026-10-13)
and read [TTY recovery](recovery.md#recover-from-a-tty) before switching. This is
a source-only preparation workflow, not the packaged wizard. Keep a working
Hyprland session available. The prepared **Nacre (niri)** entry is managed
by UWSM; do not start a second niri service. See [Nacre on niri](../README.md#nacre-on-niri)
for feature limits and pending session checks.

### Niri hover companion

The optional `nacre-niri` package adds window titles when hovering in native
niri overview. Install its matching tested archive before preparing niri:

```sh
sudo pacman -U nacre-niri-*.pkg.tar.zst
```

It installs under `/usr/lib/nacre/niri`, alongside `/usr/bin/niri`, without
replacing the distribution's compositor. Nacre's separate preparation tool
selects it automatically. Without a packaged or already verified private
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
