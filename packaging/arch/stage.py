#!/usr/bin/env python3
"""Stage system packages and private deployment templates under DESTDIR."""

import argparse
import json
import importlib.util
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

    state_spec = importlib.util.spec_from_file_location(
        "stage_state", root / "nacre/shell-tools/nacre_state.py"
    )
    state = importlib.util.module_from_spec(state_spec)
    state_spec.loader.exec_module(state)

    def text(relative, body, executable=False):
        target = deploy / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        entry = state.schema(target)
        if entry and not target.exists():
            target.write_bytes(state._encode(json.loads(body), entry))
        elif not state.write_text_if_managed(
            target, body, mode=0o755 if executable else 0o644
        ):
            target.write_text(body)
        target.chmod(0o755 if executable else 0o644)

    tools = root / "nacre/shell-tools"
    if component == "core":
        copy(root / "bin/nacre_state.py", shared / "bin/nacre_state.py")
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
                    "orient": json.loads((root / "dependencies.lock.json").read_text())[
                        "orient"
                    ],
                    "vendor_user_units": True,
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
        for name in ("nacre-daemon", "nacrectl"):
            public = destination / "usr/bin" / name
            public.parent.mkdir(parents=True, exist_ok=True)
            public.symlink_to("../lib/nacre/nacre-helper")
        for name in (
            "nacre-app",
            "nacre-settings",
            "nacre-colors",
            "nacre-welcome",
            "nacre-terminal-logo",
            "nacre-terminal",
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
        palette_cli = shared / "bin/nacre_shell"
        palette_cli.write_text(
            "#!/bin/sh\n"
            "export NACRE_HELPER_BINARY=/usr/lib/nacre/nacre-helper\n"
            "export NACRE_HELPER_TOOLS=/usr/share/nacre/nacre/shell-tools\n"
            'exec /usr/share/nacre/nacre/shell-tools/cli-bridge.sh "$@"\n'
        )
        palette_cli.chmod(0o755)
        (destination / "usr/bin/nacre_shell").symlink_to(
            "../share/nacre/bin/nacre_shell"
        )
        for name in (
            "nacre-shell",
            "nacre-session-watch",
            "nacre-power-key",
            "nacre-niri-widths",
        ):
            copy(
                tools / (name + ".service"),
                destination / "usr/lib/systemd/user" / (name + ".service"),
            )
        # Ship manual-only vendor units. Enabling/starting the socket
        # is a separate owner-approved action, never a package/setup side effect.
        for name in ("nacre-daemon.socket", "nacre-daemon.service"):
            copy(tools / name, destination / "usr/lib/systemd/user" / name)
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
                        source.read_text()
                        .replace(str(build / "defaults"), "@NACRE_HOME@")
                        .replace(str(root), "/usr/share/nacre")
                    )
                except UnicodeDecodeError:
                    pass
        # First-login defaults are private seeds, never re-applied over choices.
        text(
            ".local/state/nacre/wallpaper/last.txt",
            "/usr/share/nacre/nacre/shell-tools/demo-wallpapers/forest.jpg\n",
        )
        text(
            ".config/nacre/launcher.json",
            json.dumps(
                {
                    "favorites": [
                        "nacre-welcome",
                        "nacre-settings",
                        "nacre-colors",
                        "nacre-ai",
                        "nacre-brain",
                        "nacre-terminal",
                        "org.kde.dolphin",
                    ],
                    "hidden": [],
                }
            )
            + "\n",
        )
        text(".local/state/nacre/defaults.json", '{"launcher":1}\n')
        terminal = ".config/nacre/terminal/"
        text(
            terminal + "kitty.conf",
            (root / "kitty/kitty.conf").read_text()
            + "\nshell fish --init-command 'source \"$HOME/.config/nacre/terminal/greeting.fish\"'\n",
        )
        copy(root / "kitty/greeting.fish", deploy / terminal / "greeting.fish")
        copy(root / "fastfetch/system-age.sh", deploy / terminal / "system-age.sh")
        text(
            terminal + "fastfetch.jsonc",
            (root / "fastfetch/config.jsonc")
            .read_text()
            .replace(
                "$HOME/.config/fastfetch/system-age.sh",
                "$HOME/.config/nacre/terminal/system-age.sh",
            ),
        )
        (shared / "seed-paths.json").write_text(
            json.dumps(
                [
                    str(p.relative_to(build / "defaults"))
                    for p in (build / "defaults").rglob("*")
                    if p.is_file()
                    and (".cache" not in p.parts or "lock-ready" in p.parts)
                ]
                + [
                    ".local/state/nacre/wallpaper/last.txt",
                    ".config/nacre/launcher.json",
                    ".local/state/nacre/defaults.json",
                ]
            )
        )
        for name in ("LICENSE-Orient", "LICENSE-Pillow"):
            copy(
                root / "crates/nacre-helper" / name,
                destination / "usr/share/licenses/nacre" / name,
            )
        for name, source in (
            ("NOTICE-Orient", "NOTICE"),
            ("libjpeg-turbo-LICENSE.md", "third-party/libjpeg-turbo-LICENSE.md"),
        ):
            copy(
                root / "deps/orient" / source,
                destination / "usr/share/licenses/nacre" / name,
            )
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
            if source.name.endswith(".py.rollback"):
                continue
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
