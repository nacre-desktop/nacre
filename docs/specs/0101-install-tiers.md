# N0101: install tiers, opt-in modules and uninstall

Context: the 2026-10-29 install research rated Nacre's packaged setup "mechanism
8/10, footprint 4/10, uninstall 2/10". The owner approved a tiered install: a
non-intrusive default, opt-in extras that each list their files, and a clean
uninstall built on the existing hash receipt and snapshots.

## Tiers and modules
Every path that `tools/configure.py` manages belongs to one module
(`tools/nacre_modules.py`, `module_of(path)`, ordered path rules):

| Tier | Module | Paths |
| --- | --- | --- |
| 0 | `core` (always) | `~/.config/nacre`, `~/.local/state/nacre`, `~/.local/share/nacre`, `nacre-*`/`orient` commands, `nacre-*.desktop`, Nacre icon, `nacre-*` user units, Notifications D-Bus activation, `~/.config/quickshell/nacre` link |
| 1 | `session` (always) | `~/.config/nacre/niri/*`, `nacre-hyprland.target`/`nacre-niri.target`, first `~/.config/hypr/hyprlock.conf` (seed, never overwrites) |
| 2 | `hypr-config` | `~/.config/hypr`, `~/.config/uwsm`, `hypridle.service.d/nacre.conf`; forced on when Hyprland is a selected session |
| 2 | `theming-kitty`, `theming-fish`, `theming-gtk`, `theming-rofi`, `theming-fastfetch` | `~/.config/<app>` |
| 2 | `xdg-open` | `~/.local/bin/xdg-open` shim |
| 2 | `dolphin-override` | `org.kde.dolphin.desktop` override, FileManager1 D-Bus activation |
| 2 | `wallpapers` | `~/Pictures/Wallpapers/nacre-*.jpg` demo images |
| profile | `ai`, `brain` | `nacre-ai*`, `nacre-brain*`, `~/.local/share/nacre/{ai,brain}`; still chosen by the profile (`--with ai`) |

The Niri login entry is pacman-owned (`/usr/share/wayland-sessions`) and keeps
including `custom.kdl` (0066); no user config is edited.

## State
`~/.local/state/nacre/modules.json` (versioned state `modules`, v1):
`{"enabled": [extras], "files": {path: {"module", "original", "sha256"}}}`.
`configuration.json` keeps its format; `modules.json` adds the module per path,
seeds Nacre created (with their digest) and whether a user's file was replaced.
Replaced user files are kept byte-exact (with mode) under
`~/.local/state/nacre/originals/<path>`.

Resolution (`resolve`): `core`, `session`, profile modules, then the extras from
the explicit request, else `modules.json`, else (upgrade from a release without
it) every extra that already has a path in `configuration.json`. A new install
gets no extras. Selecting Hyprland adds `hypr-config`.

## Behaviour
- `configure.plan/apply(..., modules=None, adopt=())` deploy only enabled
  modules. `adopt` (module enable only) may replace a user's existing file of
  that module after saving the original; everything else still refuses local
  edits. Removing a managed file restores its saved original.
- `nacre setup`: the wizard asks for extras (default: current choice; `none` on
  a new install); `--module NAME` adds one. `--plan` prints `Modules: ...` and
  each path with `[module]`.
- `nacre module list|enable|disable NAME [--plan] [--yes]`. Enable re-runs the
  package deployment with the recorded compositors and profile (source installs:
  `configure.apply`), with a snapshot as usual. Disable removes the module's
  managed files, restores originals, keeps files edited since (their original
  goes to `<file>.pre-nacre`) and snapshots removed bytes under `backups/<ts>`.
- `nacre uninstall [--plan] [--keep-data] [--yes]` removes every manifest path
  (restoring originals), package runtime links whose target still matches, then
  `~/.config/nacre` and `~/.local/state/nacre`. It keeps `~/.local/share/nacre`
  (chats, AI, Brain data), Brain notes and personal wallpapers. `--keep-data`
  also keeps `~/.config/nacre` and `~/.local/state/nacre`. Empty directories
  Nacre created are pruned; shared base directories are not. It does not call
  systemctl or pacman; it prints both next steps.
- `nacre status --files` prints `module, path, state` (managed, edited, missing,
  replaced, link) for every owned path.

## Known limits
- Files written at runtime (palette exports, shell installer copies, font setup)
  are outside the manifest and are not removed by uninstall.
- `hypr-config` cannot really be off for a Hyprland session; making Hyprland use
  a sourced include like Niri is a separate unit.
- `nacre/shell-tools/install.py` (owner-only live deploy) is unchanged; its
  configure step records modules through `configure.apply`.
- `tools/prepare.py` (empty-HOME staging for the smoke test) still stages every
  file.

## Tests
`tools/tests/test_install_modules.py` (temp HOMEs): fresh install writes only
core/session paths; enable then disable of every extra restores a byte-identical
HOME (including replaced user files and modes); edited files survive disable;
uninstall returns the HOME to its pre-install state with user data intact;
`--keep-data`; upgrade without `modules.json` keeps every module; status rows.
`tools/tests/test_setup.py` covers the new wizard question and plan output.

## No temporary duplicates
None added.
