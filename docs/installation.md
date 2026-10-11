# Installing the development preview

Supported starting point: an existing CachyOS/Arch system and a compatible
Hyprland Lua / UWSM graphical session. The repository does not replace the OS
installer. Tested extraction baseline: Hyprland 0.56.2, Quickshell 0.3.2, Qt 6.12.
Native package ABI compatibility matters; avoid partial Qt updates.

1. Install the desktop/system dependencies listed below. Python 3.13+ and the
   native Qt tools are required. Pillow and the pinned Orient package are installed
   into Nacre's isolated environment; FFmpeg/multimedia support enables animated
   wallpapers and wtype verifies launcher keyboard focus.
2. Clone recursively, check out the desired development tag and initialize the
   pinned Orient submodule. Inspect `dependencies.lock.json` and `VERSION`.
3. Use `--prepare` to inspect a core-only or optional-component installation into
   an empty directory. This performs no live or account operations.
4. Run `python3 nacre/shell-tools/provision.py` to build/test the isolated engine
   environment. It installs the local pinned Orient checkout and desktop adapter.
5. In a running Hyprland session, inspect `./install.sh --profile default`, then
   apply the managed transaction. Source must be clean and committed. Existing
   unfamiliar/local config changes are preserved/refused rather than overwritten.
6. Check shell/IPC/input readiness, log out/in for actual startup acceptance, and
   verify connected displays, lock/resume, audio/network and both wallpaper types.

The default selection is configs+shell (including Settings, Colors and Welcome).
`--with ai` installs assistant code/services and launchers, preserving existing
provider sessions. It does not install or sign into Codex/Claude automatically.
`--with brain` installs the local Brain UI/server. Neither creates a populated vault,
copies the maintainer's accounts or downloads a semantic model. Optional model /
provider setup remains a separate user choice, documented in the app guides.

Generic assistant sandbox policy defaults to workspace-write; explicit personal
profiles can set `aiFullAccess:true`. Review/choose that profile preference when
setting up the optional AI component. Renderer deployments never restart a busy backend.

Use a clean test account to exercise general defaults on hardware. Use an
accelerated VM for install/upgrade/recovery tests; VM performance is not desktop
performance evidence. Keep your ordinary user's accounts/vault in their existing
locations. A shared-user, simultaneous desktop/profile switch is not implemented;
select/install profiles at a session boundary so there is one notification server,
one palette publisher and coordinated services.

System packages are pacman-owned; runtime snapshots roll back Nacre's owned code /
configuration, not distribution packages or personal data. See [recovery](recovery.md)
for the existing drift/refusal and rollback behavior. Initial public packaging is
source-based; AUR/PyPI/distribution publication is not claimed by this preview.


## Required packages

Start with a working Hyprland Lua session, UWSM, PipeWire/WirePlumber, NetworkManager
and BlueZ. Provisioning checks for the following Arch packages:

```sh
sudo pacman -S --needed librsvg quickshell qt6-imageformats qt6-multimedia \
  qt6-multimedia-ffmpeg ddcutil cpptrace libdwarf wtype papirus-icon-theme
```

The source checks also need `git`, `python`, `python-pip`, `ruff`, `lua`, `fish`,
`qt6-declarative`, `gcc`, `pkgconf`, `gtk3`, `kconfig`, `kcolorscheme`, `breeze`,
`nodejs` and `npm`. Run `npm ci --ignore-scripts` for the source formatter, then
`python3 tools/check.py` to check the source before applying. Some optional native
application integrations need their application's development packages.

Everyday desktop tools include `kitty`, `dolphin`, `hypridle`, `hyprlock`, `grim`,
`slurp`, `wl-clipboard`, `libnotify`, `brightnessctl`, `ffmpeg` and `fontconfig`.
Recording and other optional launcher actions indicate missing tools in the UI.
Keep the compositor and Qt stack compatible as a whole; installing the package
list alone does not establish a working first graphical login.
