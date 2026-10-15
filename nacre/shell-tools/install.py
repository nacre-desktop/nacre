#!/usr/bin/env python3
"""Install or reload Nacre native shell source with a reversible cutover."""

import argparse
from nacre_i18n import tr
import datetime as dt
import hashlib
import json
import nacre_state as _state
import os
import shutil
import signal
import subprocess
import tempfile
import time
from pathlib import Path

import nacre_native_compositor as compositor

HOME = Path.home()
ROOT = Path(__file__).resolve().parent
SHELL = ROOT.parent / "shell"
DEST = HOME / ".local/share/nacre/shell"
STATE = HOME / ".local/state/nacre/shell"


def refresh_orient(helper):
    state = HOME / ".local/state/nacre"
    scheme = _state.read_json_file(state / "scheme.json")
    # Fixed presets are resolved by the existing publisher, not by this command.
    subprocess.run(
        [
            str(helper),
            "nacre_shell",
            "scheme",
            "set",
            "-m",
            scheme.get("mode", "dark"),
        ],
        check=True,
    )


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None


def retire_unused_config():
    path = HOME / ".config/nacre/shell.json"
    if not path.is_file() or path.is_symlink():
        return
    if (
        digest(path)
        != "05a4a1ffca4933456163dc394e2fe645457787304d2ea715a970b4da84cfe0f1"
    ):
        print("Unused shell.json has local edits; leaving it private and inactive")
        return
    saved = (
        STATE
        / "backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        / "shell.json"
    )
    saved.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    shutil.copy2(path, saved)
    path.unlink()


def stop_other_shells():
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            args = (proc / "cmdline").read_bytes().split(b"\0")
        except OSError:
            continue
        if not args or Path(args[0].decode()).name not in ("qs", "quickshell"):
            continue
        if any(
            b"/observatory/shell/" in x or b"/os-shell/shell.qml" in x for x in args
        ):
            try:
                os.kill(int(proc.name), signal.SIGTERM)
            except ProcessLookupError:
                pass


def source_digest(directory):
    digest = hashlib.sha256()
    for file in sorted(directory.rglob("*")):
        if (
            not file.is_file()
            or any(
                part in ("build", "__pycache__")
                for part in file.relative_to(directory).parts
            )
            or file.name == ".qmlls.ini"
        ):
            continue
        digest.update(str(file.relative_to(directory)).encode())
        digest.update(file.read_bytes())
    return digest.hexdigest()


def install_native_helper(helper, target):
    if helper.resolve() == target.resolve():
        return
    fd, name = tempfile.mkstemp(prefix=".nacre-helper.", dir=target.parent)
    os.close(fd)
    temporary = Path(name)
    try:
        shutil.copy2(helper, temporary)
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)


def retire_native_helpers(directory):
    for retired in (
        "kde-palette",
        "terminal-branding",
        "login-appearance",
        "colors",
        "classic-state",
        "branding",
        "file-icons",
        "lock-config",
        "lock-dashboard",
        "provision",
        "wallpaper-watch",
        "wallpaper-media",
        "demo-wallpapers",
        "helper-service",
        "diagnostics",
        "menu-backend",
        "dashboard-data",
        "dashboard-media",
        "network-state",
        "launcher-preferences",
        "notification-history",
        "dashboard-system",
        "resource-state",
        "window-chat-title",
        "light-devices",
        "menu-cli",
        "pearl-state",
        "pearl-window",
        "compositor",
        "compositor_backends/hyprland",
        "compositor_backends/niri",
        "compositor_backends/niri_widths",
        "compositor_config",
        "compositor_backends/hyprland_config",
        "compositor_backends/niri_config",
    ):
        (directory / (retired + ".py")).unlink(missing_ok=True)
    # The retired prepared-session controller is code, never user state.
    for name in ("niri-session-runtime", "niri-acceptance-monitor", "niri-preview"):
        (directory.parent.parent / "niri-on-pc" / (name + ".py")).unlink(
            missing_ok=True
        )


def deploy(code_only=False):
    # Resolve/build the native candidate before any source/runtime activation.
    helper = ROOT / "nacre-helper"
    if not helper.is_file():
        repo = ROOT.parent.parent
        helper = Path(
            subprocess.check_output(
                ["bash", str(repo / "tools/build-helper.sh")], text=True
            ).strip()
        )
    if not helper.is_file() or not os.access(helper, os.X_OK):
        raise RuntimeError("The native nacre-helper candidate is unavailable")
    # Parse every QML file before replacing live source. A failed candidate stays local.
    formatter = (
        "/usr/lib/qt6/bin/qmlformat"
        if Path("/usr/lib/qt6/bin/qmlformat").exists()
        else shutil.which("qmlformat")
    )
    if formatter:
        for file in SHELL.rglob("*.qml"):
            result = subprocess.run(
                [formatter, str(file)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                text=True,
            )
            if result.returncode:
                raise RuntimeError(
                    tr(
                        "helper.install.qml-validation-failed-value0-value1",
                        "QML validation failed: {value0} {value1}",
                        {
                            "value0": str(file.relative_to(SHELL)),
                            "value1": result.stderr.strip(),
                        },
                    )
                )
    if not Path("/usr/bin/quickshell").is_file() or not helper.is_file():
        raise RuntimeError(
            tr(
                "helper.install.install-system-desktop-packages-and-provision-the-palette-engine",
                "Install system desktop packages and provision the palette engine first",
            )
        )
    # The release transaction snapshots the old runtime before this activation.

    # Fonts have their own hash/notice/rollback owner; loaded applications stay alive.
    subprocess.run(["python3", str(ROOT / "font-setup.py"), "--apply"], check=True)
    pending = DEST / "source.next"
    if pending.exists():
        shutil.rmtree(pending)
    shutil.copytree(
        SHELL, pending, ignore=shutil.ignore_patterns("build", "__pycache__")
    )
    if (DEST / "source").exists():
        stale = DEST / "source.previous"
        if stale.exists():
            shutil.rmtree(stale)
        (DEST / "source").rename(stale)
    pending.rename(DEST / "source")
    shutil.copytree(
        ROOT,
        DEST / "tools",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "tests", "nacre-helper"),
    )
    target = DEST / "tools/nacre-helper"
    install_native_helper(helper, target)
    # The release transaction already backed up this owned runtime tree.
    retire_native_helpers(DEST / "tools")
    # Retire the superseded procedural-image dedication from owned helper code.
    # The release transaction snapshots this tree before activation.
    (DEST / "tools/demo-wallpapers.LICENSE").unlink(missing_ok=True)
    # Licensed demo artwork is ready offline on fresh installs. Validate/cache the
    # bundled bytes; never import into or overwrite the private library.
    subprocess.run(
        [str(target), "demo-wallpapers"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    # Keep ordinary terminal titles aligned with the desktop labels.
    title = HOME / ".config/fish/functions/fish_title.fish"
    title.parent.mkdir(parents=True, exist_ok=True)
    if title.exists() and title.read_bytes() != (ROOT / "fish_title.fish").read_bytes():
        saved = (
            STATE
            / "backups"
            / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
            / "fish_title.fish"
        )
        saved.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(title, saved)
    shutil.copyfile(ROOT / "fish_title.fish", title)
    shell_service = HOME / ".config/systemd/user/nacre-shell.service"
    shell_service.parent.mkdir(parents=True, exist_ok=True)
    shell_service.unlink(missing_ok=True) if shell_service.is_symlink() else None
    shutil.copyfile(ROOT / "nacre-shell.service", shell_service)
    chat_service = HOME / ".config/systemd/user/nacre-sidebar-ai.service"
    chat_service.parent.mkdir(parents=True, exist_ok=True)
    import importlib.util

    profile_spec = importlib.util.spec_from_file_location(
        "shell_profile", ROOT / "profile-policy.py"
    )
    profile_policy = importlib.util.module_from_spec(profile_spec)
    profile_spec.loader.exec_module(profile_policy)
    if "ai" in profile_policy.load(HOME)["components"]:
        shutil.copyfile(ROOT / "nacre-sidebar-ai.service", chat_service)
    observer = HOME / ".config/systemd/user/nacre-session-watch.service"
    shutil.copyfile(ROOT / "nacre-session-watch.service", observer)
    shutil.copyfile(
        ROOT / "nacre-niri-widths.service",
        HOME / ".config/systemd/user/nacre-niri-widths.service",
    )
    subprocess.run(["python3", str(ROOT / "idle-policy.py")], check=True)
    idle_dropin = HOME / ".config/systemd/user/hypridle.service.d/nacre.conf"
    idle_dropin.parent.mkdir(parents=True, exist_ok=True)
    idle_dropin.write_text(
        "[Service]\nExecStartPre=/usr/bin/python3 %h/.local/share/nacre/shell/tools/idle-policy.py\n"
    )
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(["python3", str(ROOT / "power-button-policy.py")], check=True)
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-power-key.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "hypridle.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-session-watch.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-niri-widths.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["python3", str(ROOT / "desktop-settings.py"), "init"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    old_dbus = HOME / ".local/share/dbus-1/services/org.erikreider.swaync.service"
    if old_dbus.is_file() and "start nacre-shell.service" in old_dbus.read_text():
        old_dbus.unlink()
    dbus = HOME / ".local/share/dbus-1/services/org.freedesktop.Notifications.service"
    dbus.parent.mkdir(parents=True, exist_ok=True)
    dbus.write_text(
        "[D-BUS Service]\nName=org.freedesktop.Notifications\nExec=/usr/bin/systemctl --user start nacre-shell.service\nSystemdService=nacre-shell.service\n"
    )
    subprocess.run(
        ["systemctl", "--user", "mask", "--now", "swaync.service", "waybar.service"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    subprocess.run(["python3", str(ROOT / "install-dolphin.py")], check=True)
    subprocess.run(
        ["systemctl", "--user", "enable", "nacre-shell.service"],
        check=True,
        capture_output=True,
    )
    retire_unused_config()
    menu_command = HOME / ".local/bin/nacre-menu"
    menu_command.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "menu.sh", menu_command)
    menu_command.chmod(0o755)
    if code_only:
        for src, dest in [
            ("control.sh", HOME / ".local/bin/nacre-shell"),
            ("menu.sh", HOME / ".local/bin/nacre-menu"),
            ("cli-bridge.sh", DEST / "bin/nacre_shell"),
            ("launch.sh", DEST / "bin/qs"),
        ]:
            shutil.copyfile(ROOT / src, dest)
            dest.chmod(0o755)
        current = HOME / ".config/quickshell/nacre"
        current.unlink(missing_ok=True)
        current.symlink_to(DEST / "source")
        subprocess.run(["python3", str(ROOT / "install-extras.py")], check=True)
        if (HOME / ".local/state/nacre/scheme.json").exists():
            refresh_orient(target)
            subprocess.run(
                [str(DEST / "tools/nacre-helper"), "classic-state"], check=True
            )
        subprocess.run(["python3", str(ROOT / "isolate-apps.py")], check=True)
        subprocess.run(["systemctl", "--user", "restart", "nacre-shell"], check=True)
        validate_live(source_digest(SHELL))
        return
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    backup = (
        STATE
        / "backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    )
    backup.mkdir(parents=True, mode=0o700)
    entries = []

    def write(path, body=None, link=None, mode=None):
        entry = {"target": str(path), "type": "absent"}
        if path.is_symlink():
            entry.update(type="symlink", link=os.readlink(path))
        elif path.exists():
            saved = backup / path.relative_to(HOME)
            saved.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, saved)
            entry.update(type="file", backup=str(saved))
        entries.append(entry)
        path.parent.mkdir(parents=True, exist_ok=True)
        if link:
            path.unlink(missing_ok=True)
            path.symlink_to(link)
        else:
            path.write_text(body)
            if mode:
                path.chmod(mode)
        entry["after_link"] = os.readlink(path) if path.is_symlink() else None
        entry["after_hash"] = digest(path)

    for name in ["cli.json"]:
        write(HOME / ".config/nacre" / name, (ROOT / name).read_text())
    write(HOME / ".config/quickshell/nacre", link=DEST / "source")
    write(
        HOME / ".local/bin/nacre-shell",
        (ROOT / "control.sh").read_text(),
        mode=0o755,
    )
    write(DEST / "bin/qs", (ROOT / "launch.sh").read_text(), mode=0o755)
    write(DEST / "bin/nacre_shell", (ROOT / "cli-bridge.sh").read_text(), mode=0o755)
    write(HOME / ".config/nacre/rofi.rasi", (ROOT / "rofi.rasi").read_text())
    write(
        HOME / ".config/rofi/config.rasi",
        '@import "' + str(HOME / ".config/nacre/rofi.rasi") + '"\n',
    )
    write(
        HOME / ".config/systemd/user/nacre-shell.service",
        (ROOT / "nacre-shell.service").read_text(),
    )
    write(
        HOME / ".config/systemd/user/nacre-niri-widths.service",
        (ROOT / "nacre-niri-widths.service").read_text(),
    )
    # Apply the selected palette last, after any older profile overrides.
    path = HOME / ".config/hypr/hyprland.lua"
    marker = "-- Nacre committed wallpaper palette"
    if (
        marker not in path.read_text()
        or 'load_private("palette")' not in path.read_text()
    ):
        raise RuntimeError(
            tr(
                "helper.install.managed-palette-loader-missing-run-install-sh-apply-from-the-repo",
                "Managed palette loader missing; run ./install.sh --apply from the repository",
            )
        )
    # The reference style is code; wallpaper files remain private user assets.
    style = _state.read_json_file(ROOT / "reference-style.json")
    scheme = HOME / ".local/state/nacre/scheme"
    if not (scheme / "current.txt").exists():
        write(
            scheme / "current.txt",
            "\n".join(k + " " + v.lstrip("#") for k, v in style["colours"].items())
            + "\n",
        )
    if not (scheme / "current-mode.txt").exists():
        write(scheme / "current-mode.txt", style["mode"])
    wallpaper = HOME / "Pictures/Wallpapers/Default/default.jpg"
    if (
        wallpaper.exists()
        and not (HOME / ".local/state/nacre/wallpaper/last.txt").exists()
    ):
        write(HOME / ".local/state/nacre/wallpaper/last.txt", str(wallpaper))
    if (HOME / ".local/state/nacre/scheme.json").exists():
        # Palette generation also owns these files; include them in cutover rollback.
        tracked = {e["target"] for e in entries}
        outputs = [
            ".config/nacre/palette.lua",
            ".config/nacre/qt.conf",
            ".config/nacre/kitty-colors.conf",
            ".config/hypr/hyprlock.conf",
            ".config/nacre/colors/primary",
            ".config/nacre/colors/secondary",
            ".config/nacre/colors/onsurface",
            ".config/nacre/colors/onprimary",
            ".config/nacre/colors/surface",
            ".config/nacre/colors/surfacecontainer",
        ]
        outputs += [
            f".config/gtk-{v}/{name}"
            for v in ["3.0", "4.0"]
            for name in ["gtk.css", "settings.ini"]
        ]
        outputs += [f".config/qt{v}ct/qt{v}ct.conf" for v in [5, 6]]
        for rel in outputs:
            path = HOME / rel
            if str(path) in tracked:
                continue
            entry = {"target": str(path), "type": "absent", "after_link": None}
            if path.is_symlink():
                entry.update(type="symlink", link=os.readlink(path))
            elif path.is_file():
                saved = backup / rel
                saved.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, saved)
                entry.update(type="file", backup=str(saved))
            entries.append(entry)
        subprocess.run([str(DEST / "tools/nacre-helper"), "classic-state"], check=True)
        for entry in entries:
            entry["after_hash"] = digest(Path(entry["target"]))
    _state.write(backup / "manifest.json", entries)
    (STATE / "latest-backup").write_text(str(backup))
    stop_other_shells()
    subprocess.run(
        [
            "systemctl",
            "--user",
            "disable",
            "--now",
            "nacre-brain-ui-wallpaper.timer",
        ],
        capture_output=True,
    )
    subprocess.run(["python3", str(ROOT / "install-extras.py")], check=True)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-shell.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-niri-widths.service"],
        check=True,
        capture_output=True,
    )
    compositor.reload(timeout=None)
    subprocess.run(["python3", str(ROOT / "isolate-apps.py")], check=True)
    subprocess.run(
        ["systemctl", "--user", "try-restart", "nacre-niri-widths.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "restart", "nacre-shell.service"], check=True
    )
    validate_live(source_digest(SHELL))
    print("Native shell installed. Backup:", backup)


def validate_live(expected=None):
    for _ in range(80):
        result = subprocess.run(
            [
                str(DEST / "bin/qs"),
                "-c",
                "nacre",
                "ipc",
                "call",
                "nacre",
                "state",
            ],
            capture_output=True,
            text=True,
            timeout=3,
        )
        try:
            ready = result.returncode == 0 and "reveal" in json.loads(result.stdout)
        except ValueError:
            ready = False
        if ready:
            if expected is not None and source_digest(DEST / "source") != expected:
                raise RuntimeError(
                    tr(
                        "helper.install.the-candidate-failed-to-load-the-previous-validated-desktop-was-r",
                        "The candidate failed to load; the previous validated desktop was restored.",
                    )
                )
            if os.environ.get("NACRE_RELEASE_TRANSACTION"):
                return
            good = DEST / "source.good"
            pending = DEST / "source.good.next"
            if pending.exists():
                shutil.rmtree(pending)
            shutil.copytree(DEST / "source", pending)
            if good.exists():
                shutil.rmtree(good)
            pending.rename(good)
            return
        time.sleep(0.25)
    raise RuntimeError(
        tr(
            "helper.install.desktop-did-not-load-the-supervisor-will-retain-or-recover-the-la",
            "Desktop did not load; the supervisor will retain or recover the last validated source.",
        )
    )


def restore():
    backup = Path((STATE / "latest-backup").read_text())
    entries = _state.read_json_file(backup / "manifest.json")
    for e in entries:
        path = Path(e["target"])
        if (
            digest(path) != e["after_hash"]
            or (os.readlink(path) if path.is_symlink() else None) != e["after_link"]
        ):
            raise RuntimeError(
                tr(
                    "helper.install.later-edit-preserved-value0",
                    "Later edit preserved: {value0}",
                    {"value0": str(path)},
                )
            )
    subprocess.run(["systemctl", "--user", "stop", "nacre-shell"], check=True)
    for e in reversed(entries):
        path = Path(e["target"])
        path.unlink(missing_ok=True)
        if e["type"] == "symlink":
            path.symlink_to(e["link"])
        elif e["type"] == "file":
            shutil.copy2(e["backup"], path)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    compositor.reload(timeout=None)
    print("Previous configuration restored; restart its shell service or launcher.")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--code-only", action="store_true")
    p.add_argument("--restore", action="store_true")
    a = p.parse_args()
    restore() if a.restore else deploy(a.code_only)
