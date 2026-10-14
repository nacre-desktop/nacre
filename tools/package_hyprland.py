"""Prepare the packaged Hyprland session without deploying Niri experiment files."""

from pathlib import Path


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
