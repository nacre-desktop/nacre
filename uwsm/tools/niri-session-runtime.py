#!/usr/bin/env python3
"""Select an isolated Nacre code view for niri while preserving Hyprland startup."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def command(action, environment=None, home=None):
    env = dict(os.environ if environment is None else environment)
    home = Path.home() if home is None else Path(home)
    gate = module(Path(__file__).with_name("niri-session-gate.py"), "runtime_gate")
    original = [str(home / ".local/bin/nacre-shell"), "start"]
    if action == "shell" and gate.compositor(env) == "hyprland":
        return original, env
    if gate.compositor(env) != "niri":
        raise ValueError("Niri runtime requires an explicit niri session")
    root = home / ".local/share/nacre/niri-on-pc/session"
    shell = root / "home-view/.local/share/nacre/shell"
    for name in ("source/shell.qml", "tools/compositor_backends/niri.py", "bin/qs"):
        if not (shell / name).is_file():
            raise ValueError("Prepare the separate niri source before logging in")
    # The same virtual paths serve QML hard-coded helpers and generated bindings.
    # Only code is overlaid. Preferences, accounts and runtime sockets stay live.
    args = [
        "bwrap",
        "--bind",
        "/",
        "/",
        "--dev-bind",
        "/dev",
        "/dev",
        "--proc",
        "/proc",
        "--ro-bind",
        str(shell),
        str(home / ".local/share/nacre/shell"),
        "--die-with-parent",
    ]
    env.update(
        NACRE_COMPOSITOR="niri",
        NACRE_COMPOSITOR_TOOLS=str(home / ".local/share/nacre/shell/tools"),
        NACRE_PEARL_TOOLS=str(home / ".local/share/nacre/shell/tools"),
        NACRE_QS_CONFIG=str(home / ".config/quickshell/nacre/shell.qml"),
    )
    if action == "compositor":
        args += ["/usr/bin/niri", "--session"]
    else:
        args += [
            "/usr/bin/python3",
            str(home / ".local/share/nacre/shell/tools/shell-supervisor.py"),
        ]
    return args, env


def prepare(backend, destination, home):
    backend, destination, home = map(
        lambda p: Path(p).resolve(), (backend, destination, home)
    )
    profile = json.loads((home / ".config/nacre/profile.json").read_text())
    stage = module(backend / "tools/prepare.py", "session_prepare")
    stage.prepare(backend, destination / "home-view", profile)
    preview = module(Path(__file__).with_name("niri-preview.py"), "session_copy")
    preview.private_copy(
        home / ".local/share/nacre/shell/bin",
        destination / "home-view/.local/share/nacre/shell/bin",
    )
    render = module(
        backend / "nacre/shell-tools/compositor_config.py", "session_config"
    )
    keyboard = module(backend / "tools/keyboard.py", "session_keyboard")
    names = {
        i: (
            profile["workspaceNames"][i - 1]
            if i <= len(profile["workspaceNames"]) and profile["workspaceNames"][i - 1]
            else "nacre-" + str(i)
        )
        for i in range(1, profile["workspaceCount"] + 1)
    }
    config = destination / "config"
    config.mkdir(mode=0o700)
    for name, text in render.render(render.load(), backend="niri").items():
        (config / Path(name).name).write_text(text)
    (config / "keyboard.kdl").write_text(
        render.render_keyboard(keyboard.system_keyboard(), backend="niri")
    )
    (config / "profile.kdl").write_text(
        render.render_profile(profile, backend="niri", workspace_names=names)
    )
    scheme = json.loads((home / ".local/state/nacre/scheme.json").read_text())
    palette = (
        backend / "nacre/shell-tools/compositor_backends/niri/palette.kdl.in"
    ).read_text()
    for role in ("primary", "outlineVariant", "error"):
        color = scheme["colours"][role].lstrip("#")
        if len(color) not in (6, 8) or any(
            c not in "0123456789abcdefABCDEF" for c in color
        ):
            raise ValueError("Invalid palette role")
        palette = palette.replace("{{" + role + "}}", color)
    (config / "colors.kdl").write_text(palette)
    text = (
        "\n".join(
            "include " + json.dumps(str(config / name))
            for name in (
                "bindings.kdl",
                "window-rules.kdl",
                "keyboard.kdl",
                "profile.kdl",
                "colors.kdl",
            )
        )
        + "\n"
    )
    text += "\n".join("workspace " + json.dumps(name) for name in names.values()) + "\n"
    text += "hotkey-overlay {\n    skip-at-startup\n}\n"
    # UWSM activates the real shell service; no duplicate compositor autostart.
    (config / "config.kdl").write_text(text)
    for path in config.iterdir():
        path.chmod(0o600)
    subprocess.run(
        ["/usr/bin/niri", "validate", "-c", str(config / "config.kdl")], check=True
    )
    (destination / "source-revision").write_text(
        subprocess.check_output(
            ["git", "-C", str(backend), "rev-parse", "HEAD"], text=True
        )
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("prepare", "compositor", "shell"))
    parser.add_argument("--backend", type=Path)
    args = parser.parse_args()
    if args.action == "prepare":
        if args.backend is None:
            parser.error("prepare requires --backend")
        prepare(
            args.backend,
            Path.home() / ".local/share/nacre/niri-on-pc/session",
            Path.home(),
        )
    else:
        argv, env = command(args.action)
        os.execvpe(argv[0], argv, env)


if __name__ == "__main__":
    main()
