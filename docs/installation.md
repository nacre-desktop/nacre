# Install Nacre

Nacre is currently a source-based development preview for an existing CachyOS/Arch
system running Hyprland with its Lua configuration API and UWSM. A guided installer
will come later. The current compatible stack is Hyprland 0.56.2, Quickshell 0.3.2
and Qt 6.12; keep your desktop packages updated together.

## 1. Install the requirements

Start with a working Hyprland session, PipeWire/WirePlumber, NetworkManager and
BlueZ. Install the desktop tools:

```sh
sudo pacman -S --needed git python python-pip python-pillow quickshell librsvg \
  qt6-imageformats qt6-multimedia qt6-multimedia-ffmpeg ddcutil cpptrace \
  libdwarf wtype papirus-icon-theme kitty dolphin kio-extras qt6ct ark hypridle hyprlock \
  grim slurp wl-clipboard libnotify brightnessctl ffmpeg fontconfig
```

This preview also builds small native integrations and checks the source before
installing. Install the [source-build tools](contributing.md#build-tools) once.

## 2. Download Nacre

```sh
git clone --recurse-submodules --branch v0.1.0-dev.2 https://github.com/nacre-desktop/nacre.git
cd nacre
npm ci --ignore-scripts
```

The recursive clone includes the exact Orient engine version Nacre uses.

## 3. Prepare and review

Build the isolated color-engine environment, then print the installation plan:

```sh
python3 nacre/shell-tools/provision.py
./install.sh --profile default
```

To inspect the files without changing your desktop:

```sh
./install.sh --profile default --prepare /tmp/nacre-preview
```

Use an empty destination directory. This preview does not install packages or
start services.

## 4. Install

From your running Hyprland session:

```sh
./install.sh --profile default --apply
```

Nacre checks the source and keyboard input before accepting the update. If a
check fails, it restores the previous working version. Existing configuration
changes are preserved; a conflict stops installation rather than overwriting them.
Keep your current configuration backed up while trying this development preview.

Log out and back in, then check your displays, audio, network and lock screen.
Open the launcher with **Super+A** and Settings with **Super+S**.

The default profile has five workspaces with no assigned apps. It does not start
browsers, editors, chat or music apps at login. The desktop services and first-run
Welcome are started separately.

## Optional apps

The default installs the shell, Settings, Colors and Welcome. Add either optional
app explicitly:

```sh
./install.sh --profile default --with ai --apply
./install.sh --profile default --with brain --apply
```

Neither signs into accounts, downloads models or creates personal notes. AI starts
with workspace-only access. See [optional apps](optional-apps.md).

## Keyboard and updates

The base keyboard layout is US. On first installation, Nacre reads the system's
X11 keyboard settings from `localectl` and saves a private override in
`~/.config/nacre/keyboard.lua`. Existing overrides are preserved. Edit that file
if you want another layout; desktop and host overrides still take precedence.

Your accounts, notes, conversations and wallpaper collection stay local.
Read [configuration](configuration.md), [usage](usage.md) and
[updates and recovery](recovery.md) for everyday use.
