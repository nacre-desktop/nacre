#!/usr/bin/env python3
"""Stage system packages and private deployment templates under DESTDIR."""

import argparse
import json
import shutil
from pathlib import Path


def stage(root, destination, component, build):
    sessions = json.loads((root / "packaging/sessions.json").read_text())
    if component not in {
        "core",
        "ai",
        "brain",
        *("session-" + name for name in sessions),
    }:
        raise ValueError("Unknown package component: " + component)
    shared = destination / "usr/share/nacre"
    deploy = shared / "deployment" / component

    def copy(source, target):
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(
                source,
                target,
                dirs_exist_ok=True,
                ignore=shutil.ignore_patterns(
                    "tests", "__pycache__", "*.pyc", ".qmlls.ini", "nacre-helper"
                ),
            )
        else:
            shutil.copy2(source, target)
            target.chmod(0o755 if source.stat().st_mode & 0o111 else 0o644)

    def text(relative, body, executable=False):
        target = deploy / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body)
        target.chmod(0o755 if executable else 0o644)

    tools = root / "nacre/shell-tools"
    if component == "core":
        for name in (
            "hypr",
            "kitty",
            "fish",
            "fastfetch",
            "uwsm",
            "tools",
            "profiles",
            "packaging/packages.json",
            "packaging/sessions.json",
            "VERSION",
            "nacre/shell",
            "nacre/shell-tools",
        ):
            copy(root / name, shared / name)
        (shared / "package.json.installed").write_text(
            json.dumps(
                {
                    "version": (root / "VERSION").read_text().strip(),
                    "revision": (build / "revision").read_text().strip(),
                    "compositors": list(
                        json.loads((root / "packaging/sessions.json").read_text())
                    ),
                }
            )
            + "\n"
        )
        copy(build / "native/nacre-helper", destination / "usr/lib/nacre/nacre-helper")
        helper_reference = shared / "nacre/shell-tools/nacre-helper"
        helper_reference.symlink_to("../../../../lib/nacre/nacre-helper")
        for name in (
            "nacre-app",
            "nacre-settings",
            "nacre-colors",
            "nacre-welcome",
            "nacre-terminal-logo",
            "xdg-open",
        ):
            copy(root / "bin" / name, shared / "bin" / name)
            public = destination / "usr/bin" / name
            public.parent.mkdir(parents=True, exist_ok=True)
            if name != "xdg-open":
                public.symlink_to("../share/nacre/bin/" + name)
        for source in (root / "nacre/desktop").glob("*.desktop"):
            if "brain" not in source.name and "ai" not in source.name:
                copy(source, shared / "nacre/desktop" / source.name)
        for name, source in (("nacre-shell", "control.sh"), ("nacre-menu", "menu.sh")):
            copy(tools / source, deploy / ".local/bin" / name)
            public = destination / "usr/bin" / name
            public.parent.mkdir(parents=True, exist_ok=True)
            public.symlink_to("../share/nacre/nacre/shell-tools/" + source)
        text(
            ".local/bin/nacre",
            '#!/bin/sh\nexec python3 /usr/share/nacre/tools/releases.py "$@"\n',
            True,
        )
        copy(deploy / ".local/bin/nacre", destination / "usr/bin/nacre")
        for name in (
            "nacre-shell",
            "nacre-session-watch",
            "nacre-power-key",
            "nacre-niri-widths",
        ):
            copy(
                tools / (name + ".service"),
                deploy / ".config/systemd/user" / (name + ".service"),
            )
        for name, source in (("qs", "launch.sh"), ("nacre_shell", "cli-bridge.sh")):
            copy(tools / source, deploy / ".local/share/nacre/shell/bin" / name)
        for name in ("shortcuts.lua", "cli.json"):
            copy(tools / name, deploy / ".config/nacre" / name)
        text(
            ".config/rofi/config.rasi",
            '@import "@NACRE_HOME@/.config/nacre/rofi.rasi"\n',
        )
        for service, command in (
            (
                "Notifications",
                "/usr/bin/systemctl --user start nacre-shell.service\nSystemdService=nacre-shell.service",
            ),
            ("FileManager1", "@NACRE_HOME@/.local/bin/nacre-shell dolphin --daemon"),
        ):
            text(
                f".local/share/dbus-1/services/org.freedesktop.{service}.service",
                f"[D-BUS Service]\nName=org.freedesktop.{service}\nExec={command}\n",
            )
        text(
            ".config/systemd/user/hypridle.service.d/nacre.conf",
            "[Service]\nExecStartPre=/usr/bin/python3 %h/.local/share/nacre/shell/tools/idle-policy.py\n",
        )
        for source in (build / "defaults").rglob("*"):
            if not source.is_file() or (
                ".cache" in source.relative_to(build / "defaults").parts
                and "lock-ready" not in source.parts
            ):
                continue
            target = deploy / source.relative_to(build / "defaults")
            copy(source, target)
            if source.suffix not in (".png", ".jpg", ".webp"):
                try:
                    target.write_text(
                        source.read_text().replace(
                            str(build / "defaults"), "@NACRE_HOME@"
                        )
                    )
                except UnicodeDecodeError:
                    pass
        (shared / "seed-paths.json").write_text(
            json.dumps(
                [
                    str(p.relative_to(build / "defaults"))
                    for p in (build / "defaults").rglob("*")
                    if p.is_file()
                    and (".cache" not in p.parts or "lock-ready" in p.parts)
                ]
            )
        )
        palette = shared / "runtime/palette/venv"
        (palette / "bin").mkdir(parents=True)
        for name in ("python", "nacre_shell", "orient"):
            (palette / "bin" / name).symlink_to("/usr/bin/" + name)
        (palette / "lib").symlink_to("/usr/lib")
        for name in ("dolphin-style", "thunar-style"):
            copy(build / "native" / name, destination / "usr/lib/nacre" / name)
        for source in (build / "fonts").glob("*.ttf"):
            copy(source, destination / "usr/share/fonts/nacre" / source.name)
        for source in (build / "fonts").glob("*.txt"):
            copy(source, destination / "usr/share/licenses/nacre" / source.name)
        if (tools / "emoji-LICENSE.txt").is_file():
            copy(
                tools / "emoji-LICENSE.txt",
                destination / "usr/share/licenses/nacre/emoji-LICENSE.txt",
            )
    elif component.startswith("session-"):
        name = component.removeprefix("session-")
        copy(
            root / sessions[name]["desktop"],
            destination / "usr/share/wayland-sessions" / ("nacre-" + name + ".desktop"),
        )
    elif component == "ai":
        # Brain templates belong to nacre-brain, avoiding split-package conflicts.
        copy(root / "ai", shared / "ai")
        shutil.rmtree(shared / "ai/brain")
        application = shared / "runtime/ai"
        application.mkdir(parents=True)
        (application / "ai").symlink_to("../../ai")
        (application / "bin").symlink_to("../../bin")
        for source in (root / "bin").glob("nacre-ai*"):
            copy(source, shared / "bin" / source.name)
            public = destination / "usr/bin" / source.name
            public.parent.mkdir(parents=True, exist_ok=True)
            public.symlink_to("../share/nacre/bin/" + source.name)
            text(
                ".local/bin/" + source.name,
                f'#!/bin/sh\nexec /usr/share/nacre/bin/{source.name} "$@"\n',
                True,
            )
        copy(
            tools / "nacre-sidebar-ai.service",
            deploy / ".config/systemd/user/nacre-sidebar-ai.service",
        )
        copy(
            root / "nacre/desktop/nacre-ai.desktop",
            shared / "nacre/desktop/nacre-ai.desktop",
        )
    else:
        copy(root / "brain", shared / "brain")
        copy(root / "ai/brain", shared / "brain/templates")
        copy(root / "ai/brain", shared / "ai/brain")
        for source in (root / "bin").glob("nacre-brain*"):
            copy(source, shared / "bin" / source.name)
            public = destination / "usr/bin" / source.name
            public.parent.mkdir(parents=True, exist_ok=True)
            public.symlink_to("../share/nacre/bin/" + source.name)
            text(
                ".local/bin/" + source.name,
                f'#!/bin/sh\nexec /usr/share/nacre/bin/{source.name} "$@"\n',
                True,
            )
        copy(root / "brain/launch.sh", deploy / ".local/bin/nacre-brain-ui")
        (destination / "usr/bin/nacre-brain-ui").symlink_to(
            "../share/nacre/brain/launch.sh"
        )
        copy(
            root / "brain/brain.service",
            deploy / ".config/systemd/user/nacre-brain.service",
        )
        for source in (root / "ai/systemd").glob("nacre-brain-*"):
            copy(source, deploy / ".config/systemd/user" / source.name)
        copy(
            root / "nacre/desktop/nacre-brain.desktop",
            shared / "nacre/desktop/nacre-brain.desktop",
        )
    copy(
        root / "LICENSE",
        destination
        / "usr/share/licenses"
        / ("nacre" if component == "core" else "nacre-" + component)
        / "LICENSE",
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("component")
    parser.add_argument("build", type=Path)
    args = parser.parse_args()
    stage(
        args.root.resolve(),
        args.destination.resolve(),
        args.component,
        args.build.resolve(),
    )
