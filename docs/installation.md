# Install Nacre

The intended user path is **install the binary packages, then run `nacre setup`**.
The Arch package recipes are currently being validated; they have **not been
published to the AUR**. Obtain tested binary packages from the maintainer. An AUR
helper builds from source and therefore requires build tools; installing an
already built `.pkg.tar.zst` with pacman does not.

## Arch / CachyOS packages

Use a working, updated Arch-based system with a Wayland session. The currently
supported desktop is Hyprland with its Lua configuration API and UWSM; the tested
source stack is Hyprland 0.56.2, Quickshell 0.3.2 and Qt 6.12. Update the desktop
and Qt packages together.

Install the separately maintained `python-orient-colors` package first, followed
by the Nacre packages supplied for the same tested release:

```sh
sudo pacman -U python-orient-colors-*.pkg.tar.zst nacre-0*.pkg.tar.zst
# Optional; install either or both before selecting them in setup:
sudo pacman -U nacre-ai-*.pkg.tar.zst nacre-brain-*.pkg.tar.zst
nacre setup
```

Pacman resolves repository dependencies. It installs shared code, fonts and
compiled Dolphin/Thunar integrations system-wide. The optional packages provide
AI and Brain code independently. Setup runs as your ordinary desktop user,
without sudo, gcc, npm, Ruff, pip downloads or provider sign-in.

The terminal wizard detects the system XKB keyboard settings, lets you confirm
or change layout, variant, model and options, then selects the compositor,
optional apps and profile. It prints the private files to deploy and asks before
applying. Hyprland is available now; niri appears only when a complete packaged
setup backend is ready. A custom profile is a validated JSON file; see
[configuration](configuration.md).

For unattended use, explicitly confirm the printed plan:

```sh
nacre setup --non-interactive --profile default --compositor hyprland \
  --layout us --variant '' --yes
nacre setup --non-interactive --profile default --with ai --with brain \
  --layout no --plan
```

`--plan` changes no configuration. `--yes` applies the plan. Without an explicit
layout, non-interactive setup accepts the detected system layout, falling back
to US when system settings are unavailable. An existing private keyboard override
is always preserved. When no profile is specified, an installed profile is
retained, including its assistant access preference; fresh profiles use
workspace-only AI access. Selecting `--profile default` explicitly selects the
public default profile.

Setup uses the existing configuration conflict checks and release snapshots.
Local edits and foreign runtime links stop deployment before cutover. A failed
apply restores the snapshot. It does not start, stop or reload your running
desktop: the result is recorded as **prepared**, not live-validated. Log out and
back into the Hyprland/UWSM session, then open Welcome (`nacre-welcome`) and check
displays, audio, network, keyboard and locking. Open the launcher with **Super+A**
and Settings with **Super+S**.

The default profile has five workspaces and no assigned apps or personal startup
programs. Neither optional package signs into accounts, downloads models or
creates personal notes during setup. See [optional apps](optional-apps.md).

## Package updates and recovery

Upgrade the matching packages, then run `nacre setup` to review per-user changes.
Existing keyboard, palette state, accounts, conversations and notes stay private.
Use `nacre status` to inspect the current setup and `nacre rollback` to restore
its private-file snapshot. Rollback refuses to overwrite later edits.

System package files remain pacman-owned. A setup rollback restores per-user
configuration and runtime references; it cannot undo a pacman upgrade. Keep the
previous package archives and downgrade matching packages with `pacman -U` if
system code must also be restored. Source-based live deployment still uses its
existing live health gates and rollback behavior.

## Source path for contributors

Contributors need the [source-build tools](contributing.md#build-tools), including
native compilers and development-only formatters. This path is also useful for
reviewing changes before a package release:

```sh
git clone --recurse-submodules https://github.com/nacre-desktop/nacre.git
cd nacre
npm ci --ignore-scripts
python3 nacre/shell-tools/provision.py
./install.sh --profile default
./install.sh --profile default --prepare /tmp/nacre-preview
```

Use an empty prepare destination. From a running supported Hyprland session,
apply a **committed, checked** candidate with:

```sh
./install.sh --profile default --apply
# Optional apps:
./install.sh --profile default --with ai --with brain --apply
```

Source deployment checks the source and live keyboard/desktop behavior before
accepting an update. Failure restores the previous release. See
[configuration](configuration.md), [usage](usage.md),
[updates and recovery](recovery.md) and the
[Arch package maintainer guide](../packaging/arch/README.md).
