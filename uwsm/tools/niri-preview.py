#!/usr/bin/env python3
"""Prepare and run an isolated, windowed Nacre/niri preview without session takeover."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SAFE_CONFIG = (
    "profile.json",
    "profile.lua",
    "desktop.json",
    "wallpaper-picker.json",
    "apps.json",
    "workflows.json",
    "lock.json",
    "launcher.json",
    "shortcuts.lua",
    "dashboard.json",
    "colors.json",
    "kitty-colors.conf",
    "welcome.json",
    "cli.json",
)
SAFE_STATE = ("scheme.json", "presentation.json", "wallpaper")


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def private_copy(source, target):
    if not source.exists():
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.is_dir():
        shutil.copytree(source, target, symlinks=False, dirs_exist_ok=True)
    else:
        shutil.copyfile(source, target)
        target.chmod(0o600)


def copy_cached_presentation(home, state, cache):
    """Copy only cache assets referenced by the saved presentation, not all caches."""
    root = (home / ".cache/nacre").resolve()

    def visit(value):
        if isinstance(value, dict):
            for child in value.values():
                visit(child)
        elif isinstance(value, list):
            for child in value:
                visit(child)
        elif isinstance(value, str) and value.startswith(str(root) + "/"):
            source = Path(value).resolve()
            if source.is_relative_to(root) and source.is_file():
                private_copy(source, cache / source.relative_to(root))

    for name in ("presentation.json", "wallpaper/media.json"):
        path = state / name
        if path.is_file():
            visit(json.loads(path.read_text()))


def preview_bindings(home, names):
    """Accessible aliases when the parent compositor consumes Super shortcuts."""
    controller = json.dumps(str(home / ".local/bin/nacre-shell"))
    rows = [
        "    Alt+Space { spawn " + controller + ' "launcher"; }',
        "    Alt+O { spawn " + controller + ' "toggle"; }',
        '    Alt+Return { spawn "kitty"; }',
        "    Alt+Shift+Escape { quit skip-confirmation=true; }",
    ]
    rows.extend(
        "    Alt+" + str(number) + " { focus-workspace " + json.dumps(name) + "; }"
        for number, name in names.items()
        if number <= 9
    )
    return "binds {\n" + "\n".join(rows) + "\n}\n"


def prepare(backend, destination, home):
    backend, destination, home = map(
        lambda path: Path(path).resolve(), (backend, destination, home)
    )
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("Preview destination must be empty")
    destination.mkdir(parents=True, exist_ok=True, mode=0o700)
    profile_path = home / ".config/nacre/profile.json"
    profile = json.loads(profile_path.read_text())
    prep = module(backend / "tools/prepare.py", "niri_preview_prepare")
    tree = destination / "home-view"
    prep.prepare(backend, tree, profile)
    for name in SAFE_CONFIG:
        private_copy(home / ".config/nacre" / name, tree / ".config/nacre" / name)
    # Keep account/authentication/config directories outside the copied scope.
    # Their original paths remain read-only in the preview's filesystem view.
    state = destination / "state"
    cache = destination / "cache"
    state.mkdir(mode=0o700)
    cache.mkdir(mode=0o700)
    for name in SAFE_STATE:
        private_copy(home / ".local/state/nacre" / name, state / name)
    copy_cached_presentation(home, state, cache)
    private_copy(home / ".cache/nacre/weather.json", cache / "weather.json")
    private_copy(home / ".config/kitty", tree / ".config/kitty")
    (destination / "kitty-cache").mkdir(mode=0o700)
    shell = tree / ".local/share/nacre/shell"
    private_copy(home / ".local/share/nacre/shell/bin", shell / "bin")
    controllers = tree / ".local/bin"
    controllers.mkdir(parents=True, exist_ok=True)
    for name in (
        "nacre-shell",
        "nacre-settings",
        "nacre-menu",
        "nacre-app",
        "nacre-brain",
    ):
        original = home / ".local/bin" / name
        if original.is_file():
            shutil.copyfile(original, controllers / name)
            (controllers / name).chmod(0o700)
    config = module(
        backend / "nacre/shell-tools/compositor_config.py", "niri_preview_config"
    )
    policy = module(
        backend / "nacre/shell-tools/profile-policy.py", "niri_preview_policy"
    )
    profile = policy.validate(profile)
    generated = config.render(config.load(), backend="niri")
    generated_dir = destination / "config"
    generated_dir.mkdir(mode=0o700)
    for path, text in generated.items():
        target = generated_dir / Path(path).name
        target.write_text(text)
        target.chmod(0o600)
    keyboard = module(backend / "tools/keyboard.py", "niri_preview_keyboard")
    (generated_dir / "keyboard.kdl").write_text(
        config.render_keyboard(keyboard.system_keyboard(), backend="niri")
    )
    names = {
        number: profile["workspaceNames"][number - 1]
        if number <= len(profile["workspaceNames"])
        and profile["workspaceNames"][number - 1]
        else "nacre-" + str(number)
        for number in range(1, profile["workspaceCount"] + 1)
    }
    (generated_dir / "profile.kdl").write_text(
        config.render_profile(profile, backend="niri", workspace_names=names)
    )
    scheme = json.loads((state / "scheme.json").read_text())
    palette = (
        backend / "nacre/shell-tools/compositor_backends/niri/palette.kdl.in"
    ).read_text()
    for role in ("primary", "secondary", "outlineVariant", "error"):
        color = scheme["colours"][role].lstrip("#")
        if len(color) not in (6, 8) or any(
            char not in "0123456789abcdefABCDEF" for char in color
        ):
            raise ValueError("Invalid palette role")
        palette = palette.replace("{{" + role + "}}", color)
    (generated_dir / "colors.kdl").write_text(palette)
    includes = [
        "bindings.kdl",
        "window-rules.kdl",
        "keyboard.kdl",
        "profile.kdl",
        "colors.kdl",
    ]
    # Mounted path is stable inside the preview while host files remain unchanged.
    virtual_config = home / ".config/nacre/niri-on-pc/preview"
    text = (
        "\n".join(
            "include " + json.dumps(str(virtual_config / name)) for name in includes
        )
        + "\n"
    )
    text += "\n".join("workspace " + json.dumps(name) for name in names.values()) + "\n"
    text += (
        'spawn-at-startup "quickshell" "-n" "-p" '
        + json.dumps(str(home / ".config/quickshell/nacre/shell.qml"))
        + "\n"
    )
    text += "hotkey-overlay {\n    skip-at-startup\n}\n"
    text += preview_bindings(home, names)
    (generated_dir / "config.kdl").write_text(text)
    (tree / ".config/nacre/niri-on-pc/preview").mkdir(parents=True, exist_ok=True)
    for name in ("palette-runtime", "branding", "ai", "control"):
        if (home / ".local/share/nacre" / name).is_dir():
            (tree / ".local/share/nacre" / name).mkdir(exist_ok=True)
    quickshell_config = tree / ".config/quickshell"
    quickshell_config.mkdir(parents=True, exist_ok=True)
    (quickshell_config / "nacre").symlink_to(
        home / ".local/share/nacre/shell/source", target_is_directory=True
    )
    # Block session/hardware takeover paths and real user-manager app routing.
    binary = destination / "bin"
    binary.mkdir(mode=0o700)
    (binary / "uwsm").write_text(
        '#!/bin/sh\nif [ "$1" = app ]; then shift; [ "${1:-}" != -- ] || shift; exec "$@"; fi\necho "Session management is unavailable in the windowed preview" >&2\nexit 1\n'
    )
    for name in (
        "loginctl",
        "systemctl",
        "powerprofilesctl",
        "brightnessctl",
        "ddcutil",
        "pkexec",
        "sudo",
        "hyprlock",
        "hypridle",
    ):
        (binary / name).write_text(
            '#!/bin/sh\necho "System/session actions are disabled in the isolated preview" >&2\nexit 1\n'
        )
    for path in binary.iterdir():
        path.chmod(0o700)
    manifest = dict(
        backendRevision=subprocess.check_output(
            ["git", "-C", str(backend), "rev-parse", "HEAD"], text=True
        ).strip(),
        profileDigest=hashlib.sha256(profile_path.read_bytes()).hexdigest(),
        workspaceCount=profile["workspaceCount"],
        customShortcutsCopied=True,
        customLuaShortcutsTranslated=False,
        mode="windowed-preview",
        activated=False,
    )
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2))
    print(
        "Prepared private Nacre/niri preview; original profile and runtime untouched."
    )
    return destination


def command(destination, home, environment, runtime):
    destination, home, runtime = map(Path, (destination, home, runtime))
    display = environment.get("WAYLAND_DISPLAY", "")
    if not display:
        raise ValueError("Windowed preview needs the running desktop's Wayland display")
    outer_runtime = Path(
        environment.get("XDG_RUNTIME_DIR", "/run/user/" + str(os.getuid()))
    )
    parent = Path(display) if Path(display).is_absolute() else outer_runtime / display
    tree = destination / "home-view"
    args = [
        "bwrap",
        "--ro-bind",
        "/",
        "/",
        "--proc",
        "/proc",
        "--dev-bind",
        "/dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/run/dbus",
        "--unshare-pid",
        "--die-with-parent",
        "--bind",
        str(runtime),
        str(runtime),
    ]
    for source, target in (
        (tree / ".config/nacre", home / ".config/nacre"),
        (tree / ".config/hypr", home / ".config/hypr"),
        (tree / ".config/quickshell", home / ".config/quickshell"),
        (tree / ".config/kitty", home / ".config/kitty"),
        (destination / "state", home / ".local/state/nacre"),
        (destination / "cache", home / ".cache/nacre"),
        (destination / "kitty-cache", home / ".cache/kitty"),
    ):
        args += ["--bind", str(source), str(target)]
    args += [
        "--ro-bind",
        str(destination / "config"),
        str(home / ".config/nacre/niri-on-pc/preview"),
        "--ro-bind",
        str(destination / "bin"),
        str(destination / "bin"),
    ]
    env = dict(environment)
    for key in (
        "NIRI_SOCKET",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "NOTIFY_SOCKET",
        "GDK_BACKEND",
    ):
        env.pop(key, None)
    env.update(
        NACRE_COMPOSITOR="niri",
        XDG_CURRENT_DESKTOP="niri",
        XDG_RUNTIME_DIR=str(runtime),
        WAYLAND_DISPLAY=str(parent),
        NACRE_COMPOSITOR_TOOLS=str(home / ".local/share/nacre/shell/tools"),
        NACRE_PEARL_TOOLS=str(home / ".local/share/nacre/shell/tools"),
        NACRE_QS_CONFIG=str(home / ".config/quickshell/nacre/shell.qml"),
        NACRE_QS_BIN=shutil.which("quickshell") or "/usr/bin/quickshell",
        PATH=str(destination / "bin") + ":" + environment.get("PATH", os.defpath),
    )
    # No --session/UWSM startup, no global activation-environment import.
    # Installed runtimes can be symlinks into managed release state. Bind them
    # into normal directories in this preview namespace rather than following
    # a destination symlink into the writable state snapshot.
    nacre_share = tree / ".local/share/nacre"
    args += ["--ro-bind", str(nacre_share), str(home / ".local/share/nacre")]
    for name in ("palette-runtime", "branding", "ai", "control"):
        original = home / ".local/share/nacre" / name
        if original.is_dir():
            args += [
                "--ro-bind",
                str(original.resolve()),
                str(home / ".local/share/nacre" / name),
            ]
    args += ["--ro-bind", str(tree / ".local/bin"), str(home / ".local/bin")]
    args += [
        "dbus-run-session",
        "--",
        "niri",
        "-c",
        str(home / ".config/nacre/niri-on-pc/preview/config.kdl"),
    ]
    return args, env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    stage = sub.add_parser("prepare")
    stage.add_argument("--backend", type=Path, required=True)
    stage.add_argument("--destination", type=Path, required=True)
    run = sub.add_parser("run")
    run.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    if args.action == "prepare":
        prepare(args.backend, args.destination, Path.home())
        return
    destination = args.destination.resolve()
    if not (destination / "manifest.json").is_file():
        raise ValueError("Prepare the preview first")
    outer = Path(os.environ.get("XDG_RUNTIME_DIR", "/run/user/" + str(os.getuid())))
    with tempfile.TemporaryDirectory(prefix="nacre-niri-preview-", dir=outer) as folder:
        runtime = Path(folder)
        runtime.chmod(0o700)
        arguments, environment = command(destination, Path.home(), os.environ, runtime)
        raise SystemExit(subprocess.call(arguments, env=environment))


if __name__ == "__main__":
    main()
