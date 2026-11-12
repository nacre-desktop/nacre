# Install Nacre

Nacre 0.1.0 is a preview for updated Arch Linux and CachyOS systems. Install it
from the signed binary repository; an AUR helper or source build is not needed.
Kitty is the only required default app. Fish, Fastfetch, Dolphin and other app
integrations are optional extras.

## Install

Open the [package index](https://nacre-desktop.github.io/nacre/) and follow its
current bootstrap instructions. The installer configures a separate signed
repository and installs Nacre and its Orient dependency. Save and review the
installer before running it. Run it as your ordinary user; it requests sudo for
the repository setup and package transaction, then starts `nacre setup` as you.

Setup offers three compositor choices:

1. **nacre-niri** — recommended; Nacre's Niri fork with all shell features.
2. **Stock niri** — the core shell, with fork-only features hidden or reduced.
3. **Hyprland** — the core shell, with fork-only features hidden or reduced.

You can prepare more than one session. The setup wizard asks which sessions to
use and whether to enable the optional AI and Brain extras. It keeps an existing
profile and prepares the next login; it does not restart your current desktop.
Log out and choose the Nacre session at the login screen. See the
[compositor compatibility table](compatibility.md) before choosing a tier.

The first setup also offers the optional app integrations. You can add them
later. AI and Brain are optional too; enabling either does not sign in, download
models or create notes.

## Updates

Run:

```sh
nacre update
```

This runs the normal system update, including packages and configured AUR or
Flatpak updates. AUR recipe review is enabled by default. After package changes,
Nacre shows the private setup plan and asks before applying it. The bar's Updates
item uses the same updater. Review the package summary and log in again if the
compositor or shell needs a restart.

## Uninstall

Review the removal plan, then purge Nacre's private settings and state if you
want a clean removal. Remove Nacre's installed packages afterward:

```sh
nacre uninstall --plan
nacre uninstall --purge
```

The plan lists files that will be removed, restored and kept. Purge removes
Nacre's settings, state and cache. It does not remove your AI chats or Brain
data. Remove any other installed Nacre packages through pacman if you want all
session components removed. Remove the selected session and extras packages
alongside the core package. Repository configuration and its trust key can also
be removed separately.

See [recovery and troubleshooting](recovery.md) if setup or removal reports a
problem.
