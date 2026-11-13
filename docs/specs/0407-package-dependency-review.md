# N0407 — Package dependency review after the QML shell's retirement

Status: done (release lead, 2026-11-13). N0406 landed before this review. Owner's
2026-11-12 decision is authoritative: Kitty stays required; Fish, Fastfetch and Dolphin
become optional apps. Follow-up review fixes verified the optional AI package case and
made `nacre update` discoverable; the rebuilt niri VM passed every stage.

The original `nacre` core package declared 58 hard dependencies. The table records
each dependency's installed use on main (`a0aaf12a`) and its verdict. “Optdepend”
means absence is already handled by a hidden capability, fallback, or clear error;
“move” puts a session integration with that session package.

| Dependency | Installed use and absence behavior (source:line on main) | Verdict |
|---|---|---|
| `python` | Setup and installed helpers run Python (`packaging/arch/stage.py:197`). | keep |
| `python-pillow` | The core ships no Pillow runtime imports; Brain retains its own Pillow dependency, while core tests use Pillow (`packaging/packages.json:72-78`, `nacre/shell-tools/tests/test_colors_backend.py:13`, `crates/nacre-shell/src/wallpaper_decode.rs:130`). | drop |
| `orient` | Palette CLI and engine version use the locked engine (`packaging/arch/recipe.py:40-43`, `packaging/arch/test-installed.py:96`). | keep |
| `xdg-desktop-portal` | Session portal infrastructure, paired with each compositor portal backend (`packaging/packages.json:122-135`). | move |
| `uwsm` | Starts the managed Niri session and wraps session apps (`config/uwsm/sessions/nacre-niri.desktop:4`, `config/niri/bindings.kdl:8`). | keep |
| `fish` | Optional shell config and greeting (`config/fish/conf.d/90-nacre.fish:1`, `config/kitty/greeting.fish:2`). Kitty now uses its normal login shell when Fish is absent. | optdepend |
| `kitty` | Default terminal route and private terminal profile (`crates/nacre-helper/src/launchers.rs:43,106-111`). | keep |
| `fastfetch` | Fish greeting invokes it (`config/kitty/greeting.fish:4`); no terminal startup requires that greeting. | optdepend |
| `dolphin` | File route invokes its wrapper (`nacre/shell-tools/control.sh:11`, `nacre/shell-tools/dolphin-files.py:133`); missing app now returns a clear install/choose message before seeding. `TryExec` hides the desktop entry (`nacre/desktop/org.kde.dolphin.desktop:6-7`). | optdepend |
| `kio-extras` | Dolphin archive/network handlers; no core route calls it (`nacre/shell-tools/install-dolphin.py:16-19`). | optdepend |
| `qt6ct` | Optional Qt appearance for desktop apps (`config/uwsm/env:3`); the session leaves Qt's platform theme unset when qt6ct is absent. | optdepend |
| `breeze` | Optional cursor theme (`config/uwsm/env:11`); an absent theme uses the desktop default. | optdepend |
| `kconfig` | Dolphin style plugin uses KDE settings libraries (`nacre/shell-tools/install-dolphin.py:64-69`). | optdepend |
| `kcolorscheme` | Dolphin style plugin uses KDE color libraries (`nacre/shell-tools/install-dolphin.py:65-69`). | optdepend |
| `gtk3` | Palette publishes GTK settings as files; GTK is needed only by GTK apps (`crates/nacre-helper/src/classic_state.rs:733-768`). | optdepend |
| `hypridle` | Idle service is installed by each compositor session (`tools/package_niri.py:389-398`, `tools/package_hyprland.py:38`). | move |
| `hyprlock` | Idle lock command and Niri lock route (`config/hypr/hypridle.conf:3`, `config/uwsm/niri-session-bin/loginctl:5-8`). | move |
| `grim` | Screenshot actions and capture control (`config/niri/bindings.kdl:15-16`, `crates/nacre-helper/src/daemon/control_center.rs:200-203`). | keep |
| `slurp` | Screenshot selection (`config/niri/bindings.kdl:15-16`, `crates/nacre-helper/src/daemon/control_center.rs:200-203`). | keep |
| `wl-clipboard` | Screenshot copy and Niri clipboard watcher (`config/niri/bindings.kdl:15-16`, `tools/package_niri.py:329`). | keep |
| `libnotify` | Screenshot and shell actions send user notices (`config/niri/bindings.kdl:15-16`, `config/hypr/scripts/window-close.sh:14`). | keep |
| `brightnessctl` | Hardware key fallback when the daemon reports unavailable (`config/niri/bindings.kdl:9-12`). | keep |
| `ffmpeg` | Media generation is check/smoke tooling; runtime video playback uses libmpv (`tools/wallpaper_video_smoke.py:7`, `crates/nacre-shell/src/bin/bead-wallpaper-video/mpv.rs:2-4`). | drop |
| `fontconfig` | Native font discovery and fallback (`crates/nacre-shell/src/font_sources.rs:166-186`). | keep |
| `freetype2` | Font rasterization and glyph loading (`crates/nacre-helper/src/image_text.rs:35,55,115`). | keep |
| `libraqm` | Text shaping FFI used for rendered text (`crates/nacre-helper/src/image_text.rs:17,42-68`). | keep |
| `libjpeg-turbo` | Native JPEG decode/compress (`crates/nacre-helper/Cargo.toml:58`, `crates/nacre-shell/Cargo.toml:13`). | keep |
| `lcms2` | Color-managed image stack and system image plugins (`crates/nacre-shell/src/wallpaper_decode.rs:130-137`). | keep |
| `librsvg` | Logo rasterization reports a clear missing-tool message (`crates/nacre-helper/src/branding.rs:231-236`). | optdepend |
| `qt6-imageformats` | Image plugins for the retained SDDM QML greeter (`nacre/login/Main.qml:60`, `nacre/login/Logo.qml:30`). | keep |
| `ddcutil` | External monitor brightness is capability-gated (`crates/nacre-helper/src/light_devices.rs:192,366`, `crates/nacre-helper/src/daemon/bar_services.rs:65`). | optdepend |
| `wtype` | Optional Niri keyboard typing maintenance route (`nacre/shell-tools/maintenance.py:242`); reports when unavailable. | optdepend |
| `papirus-icon-theme` | Optional icon base with Adwaita/hicolor fallback (`crates/nacre-helper/src/file_icons.rs:44,71-89`). | optdepend |
| `networkmanager` | Network topic reports service unavailable and retains stale state when absent (`crates/nacre-helper/src/daemon/network.rs:147-171`). | optdepend |
| `bluez` | Bluetooth state reads BlueZ and degrades when the service is absent (`crates/nacre-helper/src/daemon/bluetooth.rs:292,575`). | optdepend |
| `pipewire` | Audio controls are hidden when the PipeWire socket is missing (`crates/nacre-helper/src/daemon/control_center.rs:164-194`). | optdepend |
| `wireplumber` | Software controls check its unit before offering audio actions (`crates/nacre-helper/src/software_actions.rs:102`). | optdepend |
| `xdg-utils` | File, URL and library actions call `xdg-open` (`crates/nacre-helper/src/launcher_workflows.rs:473,526,544`). | keep |
| `rofi` | Optional palette target; no launcher route requires Rofi (`tools/nacre_modules.py:25`, `nacre/shell-tools/theme-targets.json:70-76`). | optdepend |
| `kwallet-pam` | PAM wallet startup is generated for Niri sessions (`tools/package_niri.py:337`). | move |
| `polkit-gnome` | Authentication agent starts with Niri and Hyprland (`tools/package_niri.py:328`, `config/hypr/conf/autostart.lua:10`). | move |
| `cliphist` | Niri clipboard watcher and clipboard history actions (`tools/package_niri.py:329`, `crates/nacre-helper/src/desktop_extras.rs:186-209`). | keep |
| `qt6-base` | Qt widgets for Dolphin style and the retained SDDM greeter (`nacre/shell-tools/install-dolphin.py:47-68`, `nacre/login/Main.qml:22`). | keep |
| `glibc` | C runtime for packaged native executables (`packaging/arch/stage.py:161-176`). | keep |
| `python-gobject` | No shipped core runtime imports GI after QML retirement; it remains in AI/Brain lists (`packaging/packages.json:48,65-78`). | drop |
| `libgweather-4` | No installed core route uses it after QML retirement; the old entry is only in the package map (`packaging/packages.json:49`). | drop |
| `geoclue` | Optional timezone location lookup (`nacre/shell-tools/timezone.py:23,310`); timezone setup works without location. | optdepend |
| `bash` | Installed helpers and compositor scripts use Bash (`nacre/shell-tools/control.sh:1`, `config/hypr/scripts/window-trash.sh:1`). | keep |
| `jq` | Compositor scripts parse JSON with jq (`config/hypr/scripts/window-trash.sh:50-58`). | keep |
| `qt6-declarative` | Qt Quick modules for the retained SDDM QML greeter (`nacre/login/Main.qml:22`). | keep |
| `libgcc` | Runtime library for packaged native code (`crates/nacre-helper/Cargo.toml:58`, `packaging/arch/test-installed.py:96-103`). | keep |
| `libstdc++` | C++ runtime for the Dolphin style plugin (`nacre/shell-tools/install-dolphin.py:52-68`). | keep |
| `glib2` | GLib types used by the GDK Pixbuf image decoder (`crates/nacre-shell/Cargo.toml:15`, `crates/nacre-shell/src/wallpaper_decode.rs:130-137`). | keep |
| `gcc-libs` | GCC/C++ runtime for packaged native executables and the style plugin (`nacre/shell-tools/install-dolphin.py:52-68`). | keep |
| `libxkbcommon` | Wayland keyboard handling in the Bead shell (`crates/nacre-shell/Cargo.toml:27-32`). | keep |
| `gdk-pixbuf2` | Runtime wallpaper image decoding (`crates/nacre-shell/Cargo.toml:15`, `crates/nacre-shell/src/wallpaper_decode.rs:130-137`). | keep |
| `libpipewire` | No direct runtime link remains; installing optional PipeWire brings its client library (`crates/nacre-helper/src/diagnostics.rs:121-130`, `crates/nacre-helper/src/software_actions.rs:102`). | drop |
| `mpv` | Video helper dynamically loads libmpv and reports when it is absent (`crates/nacre-shell/src/bin/bead-wallpaper-video/mpv.rs:2-4,94-101`). | optdepend |

Qt note: `qt6-base`, `qt6-declarative`, and `qt6-imageformats` remain required for the
retained SDDM greeter; the greeter itself is unchanged. Dolphin-specific Qt/KDE packages
(`kconfig`, `kcolorscheme`) are optional with Dolphin. `qt6ct` and Breeze are optional
desktop appearance packages.

## Applied changes

- Core hard dependencies: 58 → 28. Of the original set, 20 are optdepends, 5 move
  to session packages, and 5 are dropped as unused, indirect, or check-only.
- `kitty` remains required; `fish`, `fastfetch`, and `dolphin` are optional with plain
  descriptions. Niri and Hyprland session packages now own their portal, idle, lock and
  authentication integrations.
- Kitty's generated profile no longer forces Fish. Dolphin setup skips with a clear
  message if Dolphin or its optional style packages are absent; its app route already
  gives an actionable missing-app message and its desktop entry has `TryExec`.
- The session exports the `qt6ct` theme only when its plugin package is present.
- The bundled shell-launcher health check no longer requires optional `mpv`; the VM
  screenshot assertion now reads PNG structure with the standard library instead of
  relying on Pillow from the core dependency closure.
- Focused package/setup suites passed in one fresh Arch container:
  `nice -n 10 python -m unittest test_setup test_package_repository test_package_transaction test_package_vm test_shell_session test_dolphin_files test_ui_shell_switch test_releases`
  — 118 tests, `OK`. The LEAN package build used
  `CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, 2 Cargo
  jobs, and `RUST_MIN_STACK=33554432`; `makepkg` produced the `r3068` core and split
  packages from source `970f20cd3bdb5af2992debcf3ad90c7f1fead274`.
- Installed-package proof passed four fresh users (`nacre-niri`, `niri`, `hyprland`,
  `both`) in a clean Arch container without compiler, Node/npm, Ruff, Git or pip. It
  confirms Fish and Fastfetch are absent, Kitty does not force Fish, and the missing
  Dolphin route prints a clear message. The controller check is conditional on the
  optional package. The failure was case (a): the core-only proof did not install
  `nacre-ai`. Bootstrap installs `nacre` only (`bootstrap.sh:51-53`); the `nacre-ai`
  split package stages the private wrapper (`stage.py:309-325`). A fresh core-only Arch
  proof confirmed `pacman -Q nacre-ai` reports it absent; setup leaves AI unselected and
  creates no `~/.local/bin/nacre-ai`, then the installed proof passes. If the package is
  present, the test checks `default_assistant()`; if a wrapper exists without the
  package, it requires the loader's explicit missing-tools message
  (`nacre_ai_controller.py:13-27`). This is not a setup bug.
- `pacman -Qi` core dependency closure: 199 packages, 1,270,993,289 bytes (1,212.11
  MiB). Direct core dependencies: 58 → 28; verdicts: 28 keep, 20 optdepend, 5 move,
  5 drop.
- Final `packaging/arch/test-vm.py` results from the saved branch packages:

  | Tier | Prepare | Inventory | Repository | Bootstrap plan | Bootstrap install | Setup | Session | Update | Uninstall | Package removal | Leftovers |
  |---|---|---|---|---|---|---|---|---|---|---|---|
  | `nacre-niri` | PASS | PASS | PASS | PASS | PASS | PASS | PASS | SKIP | PASS | PASS | PASS |
  | `niri` | PASS | PASS | PASS | PASS | PASS | PASS | PASS | SKIP | PASS | PASS | PASS |
  | `hyprland` | PASS | PASS | PASS | PASS | PASS | PASS | PASS | SKIP | PASS | PASS | PASS |

- `ruff 0.16.10 format --check` and `ruff 0.16.10 check` passed on changed Python
  files; `python3 tools/compositor_boundary.py` passed. Full command output remains in
  the task logs on dev-1, not committed.
- `tools/releases.py` lists `update` in the existing action choices (`:929-941`);
  the early `update` dispatch remains before argparse (`:865-870`). Its help test and
  existing dispatch tests passed in the focused suite. A rebuilt niri VM using the
  `r3068` packages passed every stage, including `update` and leftovers:

  | Tier | Prepare | Inventory | Repository | Bootstrap plan | Bootstrap install | Setup | Session | Update | Uninstall | Package removal | Leftovers |
  |---|---|---|---|---|---|---|---|---|---|---|---|
  | `niri` follow-up | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS |

Acceptance: dependency review, conditional AI-controller proof, focused tests, installed
package proof, static checks, and the requested niri VM rerun passed. The earlier
nacre-niri and hyprland runs recorded `update: SKIP` before help exposed the dispatcher;
the follow-up rerun confirms the corrected niri stage.

Not covered: AI and Brain dependencies, build/check dependency lists, or SDDM greeter
packaging. Temporary duplicate paths: none introduced by N0407.
