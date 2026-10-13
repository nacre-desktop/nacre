#!/usr/bin/env python3
"""Select an isolated Nacre code view for niri while preserving Hyprland startup."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess

HOVER_SOURCE = "8ed0da44d974c32c6877d2f4630c314da0717ecb"
PACKAGED_HOVER = Path("/usr/lib/nacre/niri")


def verified_hover_binary(root, owner, *, packaged=False):
    """Validate the companion receipt; system packages have a separate trust root."""
    try:
        receipt = root / "manifest.json"
        manifest = json.loads(receipt.read_text())
        binary = root / "niri"
        if (
            manifest.get("schema") != 1
            or manifest.get("enabled") is not True
            or (not packaged and manifest.get("nestedVerified") is not True)
            or manifest.get("sourceRevision") != HOVER_SOURCE
            or manifest.get("protocol") != "NacreOverviewHoverStream"
            or not binary.is_file()
            or binary.is_symlink()
            or binary.stat().st_uid != owner
            or receipt.is_symlink()
            or receipt.stat().st_uid != owner
            or (packaged and (binary.stat().st_mode | receipt.stat().st_mode) & 0o022)
            or not os.access(binary, os.X_OK)
            or (
                not packaged
                and hashlib.sha256(binary.read_bytes()).hexdigest()
                != manifest.get("binarySha256")
            )
        ):
            return None
        # Avoid retaining the old experiment after a packaged compositor upgrade.
        if not packaged:
            installed = subprocess.check_output(
                ["/usr/bin/niri", "--version"], text=True, timeout=2
            )
            if not installed.startswith("niri 26.04 "):
                return None
        candidate = subprocess.check_output(
            [str(binary), "--version"], text=True, timeout=2
        )
        return binary if candidate.startswith("niri 26.04 ") else None
    except (OSError, ValueError, AttributeError, subprocess.SubprocessError):
        return None


def overview_hover_binary(home):
    """Prefer the pacman-owned companion, then verified user builds and old receipts."""
    packaged = verified_hover_binary(PACKAGED_HOVER, 0, packaged=True)
    if packaged:
        return packaged
    for relative in (
        ".local/share/nacre/niri/overview-hover",
        ".local/share/nacre/niri-on-pc/overview-hover",
    ):
        candidate = verified_hover_binary(Path(home) / relative, os.getuid())
        if candidate:
            return candidate
    return None


def ensure_overview_hover(backend, home):
    """Niri preparation installs the companion automatically if none is available."""
    if overview_hover_binary(home):
        return
    subprocess.run(
        [
            "/usr/bin/python3",
            str(Path(backend) / "tools/build-niri-overview-hover.py"),
            "--install",
        ],
        check=True,
    )
    if not overview_hover_binary(home):
        raise RuntimeError("Niri hover installation did not produce a usable companion")


def host_preferences(desktop):
    """Seed host scale/input preferences without modifying the shared profile."""
    natural = desktop.get("naturalScroll", True)
    if type(natural) is not bool:
        raise ValueError("Invalid natural scrolling preference")
    gap = desktop.get("gapsOut", 12)
    border = desktop.get("borderSize", 1)
    if any(
        type(value) not in (int, float)
        or not math.isfinite(value)
        or not 0 <= value <= 100
        for value in (gap, border)
    ):
        raise ValueError("Invalid saved window margins")
    text = (
        "prefer-no-csd\n"
        "layout {\n    gaps " + str(gap) + "\n"
        "    default-column-width { proportion 0.5; }\n"
        "    preset-column-widths {\n"
        "        proportion 0.33333\n        proportion 0.5\n"
        "        proportion 0.66667\n        proportion 1.0\n    }\n"
        "    focus-ring { width " + str(border) + "; }\n}\n"
    )
    text += "input {\n    touchpad {\n"
    if natural:
        text += "        natural-scroll\n"
    # Match Nacre's existing Hyprland touchpad defaults.
    text += "        scroll-factor 0.25\n    }\n}\n"
    for display in desktop.get("displays", []):
        if display.get("disabled"):
            continue
        name, scale = display.get("name"), display.get("scale")
        if (
            not isinstance(name, str)
            or not name
            or type(scale) not in (int, float)
            or not math.isfinite(scale)
            or not 0 < scale <= 10
        ):
            raise ValueError("Invalid saved display scale")
        text += "output " + json.dumps(name) + " {\n    scale " + str(scale) + "\n}\n"
    text += (
        "window-rule {\n"
        '    match app-id="^nacre-ai-(dashboard|task)$"\n'
        "    default-column-width { proportion 0.5; }\n}\n"
        "binds {\n"
        "    Super+Ctrl+Tab { toggle-overview; }\n"
        "    Super+M { maximize-column; }\n"
        "    Super+R { switch-preset-column-width; }\n"
        "    Super+Shift+WheelScrollUp cooldown-ms=150 { focus-column-left; }\n"
        "    Super+Shift+WheelScrollDown cooldown-ms=150 { focus-column-right; }\n"
        "}\n"
    )
    return text


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def palette_include(home, config):
    # The existing palette publisher writes this backend-owned path at runtime.
    published = Path(home) / ".config/nacre/niri/palette.kdl"
    return published if published.is_file() else Path(config) / "colors.kdl"


def command(action, environment=None, home=None):
    env = dict(os.environ if environment is None else environment)
    home = Path.home() if home is None else Path(home)
    gate = module(Path(__file__).with_name("niri-session-gate.py"), "runtime_gate")
    original = [str(home / ".local/bin/nacre-shell"), "start"]
    if action == "brain":
        original = [
            "/usr/bin/python3",
            str(home / ".local/share/nacre/brain/control.py"),
            "watch-brain",
        ]
    if action in ("shell", "brain") and gate.compositor(env) == "hyprland":
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
    hover_binary = (
        None
        if action in ("widths", "widths-check") or (root / "stock-niri").is_file()
        else overview_hover_binary(home)
    )
    env["NACRE_NIRI_HOVER_STREAM"] = "1" if hover_binary else "0"
    if action == "compositor":
        args += [str(hover_binary) if hover_binary else "/usr/bin/niri", "--session"]
    elif action == "brain":
        args += original
    elif action in ("widths", "widths-check"):
        args += [
            "/usr/bin/python3",
            str(
                home
                / ".local/share/nacre/shell/tools/compositor_backends/niri_widths.py"
            ),
        ]
        if action == "widths-check":
            args += ["--check-session"]
    else:
        args += [
            "/usr/bin/python3",
            str(home / ".local/share/nacre/shell/tools/shell-supervisor.py"),
        ]
    return args, env


def prepare(backend, destination, home, *, stock_niri=False):
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
    for role in ("primary", "secondary", "outlineVariant", "error"):
        color = scheme["colours"][role].lstrip("#")
        if len(color) not in (6, 8) or any(
            c not in "0123456789abcdefABCDEF" for c in color
        ):
            raise ValueError("Invalid palette role")
        palette = palette.replace("{{" + role + "}}", color)
    (config / "colors.kdl").write_text(palette)
    desktop_path = home / ".config/nacre/desktop.json"
    desktop = json.loads(desktop_path.read_text()) if desktop_path.exists() else {}
    (config / "host.kdl").write_text(host_preferences(desktop))
    text = (
        "\n".join(
            "include "
            + json.dumps(
                str(
                    palette_include(home, config)
                    if name == "colors.kdl"
                    else config / name
                )
            )
            for name in (
                "bindings.kdl",
                "window-rules.kdl",
                "keyboard.kdl",
                "profile.kdl",
                "colors.kdl",
                "host.kdl",
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
    if stock_niri:
        marker = destination / "stock-niri"
        marker.write_text("Prepared with --stock-niri\n")
        marker.chmod(0o600)
    (destination / "source-revision").write_text(
        subprocess.check_output(
            ["git", "-C", str(backend), "rev-parse", "HEAD"], text=True
        )
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=("prepare", "compositor", "shell", "brain", "widths", "widths-check"),
    )
    parser.add_argument("--backend", type=Path)
    parser.add_argument(
        "--stock-niri",
        action="store_true",
        help="prepare a session using distribution Niri without hover support",
    )
    args = parser.parse_args()
    if args.action == "prepare":
        if args.backend is None:
            parser.error("prepare requires --backend")
        if not args.stock_niri:
            ensure_overview_hover(args.backend, Path.home())
        prepare(
            args.backend,
            Path.home() / ".local/share/nacre/niri-on-pc/session",
            Path.home(),
            stock_niri=args.stock_niri,
        )
    else:
        argv, env = command(args.action)
        os.execvpe(argv[0], argv, env)


if __name__ == "__main__":
    main()
