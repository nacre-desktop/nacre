# Install Nacre

Nacre 0.1.0 is a preview for updated Arch Linux and CachyOS systems. Install it
from the signed binary repository; an AUR helper or source build is not needed.
Kitty is the only required default app. Fish, Fastfetch, Dolphin and other app
integrations are optional extras.

## Install

The installer adds a separate signed pacman repository and installs Nacre and
its Orient engine. It does not replace the Arch or CachyOS repositories. Save it
and read it first, then run it as your ordinary desktop user:

```sh
sudo pacman -Syu --needed curl gnupg
curl --fail --location https://nacre-desktop.github.io/nacre/install-arch.sh -o /tmp/nacre-install-arch.sh
less /tmp/nacre-install-arch.sh
bash /tmp/nacre-install-arch.sh --plan
bash /tmp/nacre-install-arch.sh
```

`--plan` only shows what will happen. The installer asks for sudo for the key,
the repository and the package transaction, then starts `nacre setup` as you.
It trusts only the signing key with fingerprint
`886CBE272BA30B44DBB55F66714CFAAC876C349E` and requires signed packages and a
signed database. The [package index](https://nacre-desktop.github.io/nacre/)
lists the current release.

Setup offers three compositor choices:

1. **nacre-niri** — recommended; Nacre's Niri fork with all shell features.
2. **Stock niri** — the core shell, with fork-only features hidden or reduced.
3. **Hyprland** — the core shell, with fork-only features hidden or reduced.

You can prepare more than one session. The setup wizard asks which sessions to
use and whether to enable the optional AI and Brain extras. It keeps an existing
profile and prepares the next login; it does not restart your current desktop.
Log out and choose the Nacre session at the login screen. Without a login
manager, start it from a TTY with `nacre session nacre-niri`, `nacre session niri`
or `nacre session hyprland`. See the
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

Review the removal plan first, then remove Nacre's files and the packages:

```sh
nacre uninstall --plan
nacre uninstall --purge
sudo pacman -Rns $(pacman -Qq | grep -E '^(nacre|orient$)')
systemctl --user daemon-reload
```

The plan lists every file that will be removed, restored or kept. Files you
changed stay unless you purge. `--purge` removes Nacre's settings, state and
cache. Your AI chats and Brain data stay unless you add `--purge-data`; Brain
notes and your own wallpapers are never removed. The repository entry and its
key stay in pacman's configuration until you remove them.

See [recovery and troubleshooting](recovery.md) if setup or removal reports a
problem.
