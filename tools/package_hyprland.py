"""Prepare the packaged Hyprland session without deploying Niri experiment files."""

from pathlib import Path
import os
import subprocess


def supported(root):
    return (root / "hypr/hyprland.lua").is_file()


def files(root, home, profile, keyboard, destination, common, *, validate_config=True):
    for name in ("hypr", "uwsm", "fish", "kitty", "fastfetch"):
        directory = home / ".config" / name
        if directory.is_symlink():
            raise RuntimeError(
                "Local configuration symlink preserved: " + str(directory)
            )
    result = {
        path: source
        for path, source in common.items()
        if not str(path).startswith(".config/uwsm/")
        or str(path) in (".config/uwsm/env", ".config/uwsm/env-hyprland")
    }

    def add(relative, text):
        path = Path(relative)
        source = destination / path
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(text.replace("@NACRE_HOME@", str(home)))
        source.chmod(0o600)
        result[path] = source

    add(
        ".config/systemd/user/nacre-hyprland.target",
        "[Unit]\nDescription=Nacre Hyprland session services\n"
        "PartOf=graphical-session.target\nAfter=graphical-session.target\n"
        "Wants=nacre-shell.service nacre-session-watch.service nacre-power-key.service hypridle.service\n",
    )
    autostart = Path(".config/hypr/conf/autostart.lua")
    if autostart in result:
        add(
            autostart,
            result[autostart].read_text()
            + '\nhl.on("hyprland.start", function()\n    hl.exec_cmd("systemctl --user start nacre-hyprland.target")\nend)\n',
        )
    idle = home / ".config/nacre/hypridle.local.conf"
    if not idle.exists():
        add(
            ".config/nacre/hypridle.local.conf",
            (root / "tools/defaults/hypridle.conf").read_text(),
        )
    return result


def session(home=None):
    home = Path.home() if home is None else home
    if not (home / ".config/hypr/hyprland.lua").is_file():
        raise RuntimeError(
            "Run nacre setup --compositor hyprland before starting its session"
        )
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    environment = dict(os.environ, NACRE_COMPOSITOR="hyprland")
    for name in ("NIRI_SOCKET", "NIRI_CONFIG"):
        environment.pop(name, None)
    os.execvpe(
        "uwsm",
        ["uwsm", "start", "-e", "-D", "Hyprland", "hyprland.desktop"],
        environment,
    )
