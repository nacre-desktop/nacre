"""A separate, package-backed Niri login; never replace the user's plain Niri config."""

import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys

# Shared state is independent of the palette Python environment.
for _state_directory in (
    Path(__file__).resolve().parents[1] / "nacre/shell-tools",
    Path.home() / ".local/share/nacre/shell/tools",
):
    if (_state_directory / "nacre_state.py").is_file():
        sys.path.insert(0, str(_state_directory))
        break
_state = importlib.import_module("nacre_state")

DEPENDENCIES = ("niri", "xwayland-satellite", "xdg-desktop-portal-gnome")
TOOLKIT_ENVIRONMENT = {
    "QT_QPA_PLATFORM": "wayland;xcb",
    "QT_QPA_PLATFORMTHEME": "qt6ct",
    "QT_WAYLAND_DISABLE_WINDOWDECORATION": "1",
    "XCURSOR_THEME": "breeze_cursors",
    "XCURSOR_SIZE": "24",
}
COLLECTION_BINDINGS = (
    "binds {\n"
    '    Super+Ctrl+Tab repeat=false hotkey-overlay-title="Nacre:collection-next" { focus-collection-next; }\n'
    '    Super+Ctrl+Shift+Tab repeat=false hotkey-overlay-title="Nacre:collection-previous" { focus-collection-previous; }\n'
    '    Super+Ctrl+N repeat=false hotkey-overlay-title="Nacre:collection-create" { create-collection; }\n'
    '    Super+Ctrl+Shift+Page_Down repeat=false hotkey-overlay-title="Nacre:collection-move-next" { move-window-to-collection-next; }\n'
    '    Super+Ctrl+Shift+Page_Up repeat=false hotkey-overlay-title="Nacre:collection-move-previous" { move-window-to-collection-previous; }\n'
    "}\n"
)
def edge_trigger_config(delay_ms):
    if (
        isinstance(delay_ms, bool)
        or not isinstance(delay_ms, int)
        or not 0 <= delay_ms <= 800
    ):
        delay_ms = 350
    return (
        "edge-triggers {\n"
        f"    left {{ action {{ event; }}; dwell-time-ms {delay_ms}; }}\n"
        f"    right {{ action {{ event; }}; dwell-time-ms {delay_ms}; }}\n"
        f"    top {{ action {{ event; }}; dwell-time-ms {delay_ms}; }}\n"
        f"    bottom {{ action {{ event; }}; dwell-time-ms {delay_ms}; }}\n"
        "}\n"
    )


def edge_trigger_delay(home):
    settings = home / ".config/nacre/desktop.json"
    if settings.is_file():
        try:
            value = _state.read_json_file(settings).get("lipHoverDelay", 350)
            if isinstance(value, int) and not isinstance(value, bool) and 0 <= value <= 800:
                return value
        except (OSError, ValueError):
            pass
    return 350


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def supported(root):
    return all(
        (root / name).is_file()
        for name in (
            "tools/package_niri.py",
            "nacre/shell-tools/nacre_native_config.py",
            "nacre/shell-tools/nacre-helper",
        )
    )


def files(root, home, profile, keyboard, destination, common, *, validate_config=True):
    """Render private deployment sources in a temporary directory, before any writes."""
    for directory in (
        home / ".config",
        home / ".config/nacre",
        home / ".config/nacre/niri",
    ):
        if directory.is_symlink():
            raise RuntimeError(
                "Local Niri configuration symlink preserved: " + str(directory)
            )
    # A new session must not take over CachyOS/Noctalia's config, shell or startup.
    preserved = (
        ".config/hypr/",
        ".config/uwsm/",
        ".config/fish/",
        ".config/kitty/",
        ".config/fastfetch/",
        ".local/share/dbus-1/services/",
    )
    result = {
        path: source
        for path, source in common.items()
        if not str(path).startswith(preserved)
    }
    lock = Path(".config/hypr/hyprlock.conf")
    if lock in common and not (home / lock).exists():
        result[lock] = common[lock]

    def add(relative, text, executable=False):
        path = Path(relative)
        source = destination / path
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(text.replace("@NACRE_HOME@", str(home)))
        source.chmod(0o755 if executable else 0o600)
        result[path] = source
        return source

    compiler = module(
        root / "nacre/shell-tools/nacre_native_config.py", "package_niri_compiler"
    )
    config = Path(".config/nacre/niri")
    includes = []
    for name, text in compiler.niri.render(compiler.load()).items():
        add(config / Path(name).name, text)
        if name.endswith(".kdl"):
            includes.append(Path(name).name)
    keyboard_path = home / config / "keyboard.kdl"
    add(
        config / "keyboard.kdl",
        keyboard_path.read_text()
        if keyboard_path.is_file()
        else compiler.render_keyboard(keyboard, backend="niri"),
    )
    names = {
        i: profile["workspaceNames"][i - 1]
        if i <= len(profile["workspaceNames"]) and profile["workspaceNames"][i - 1]
        else "nacre-" + str(i)
        for i in range(1, profile["workspaceCount"] + 1)
    }
    add(
        config / "profile.kdl",
        compiler.render_profile(profile, backend="niri", workspace_names=names),
    )
    includes += ["keyboard.kdl", "profile.kdl"]
    desktop_path = home / config / "desktop.kdl"
    if desktop_path.is_file() and desktop_path.read_text().strip():
        desktop = desktop_path.read_text()
    else:
        # The preview installer emitted an empty file on a fresh account. Seed
        # real Nacre geometry there too; never import another host's monitors.
        values = _state.read_json_file(root / "nacre/shell-tools/desktop-defaults.json")
        preferences = home / ".config/nacre/desktop.json"
        backend = module(
            root / "nacre/shell-tools/nacre_native_compositor.py",
            "package_niri_preferences",
        )
        if preferences.is_file():
            values.update(_state.read_json_file(preferences))
        desktop = "prefer-no-csd\n" + backend.render_for_backend(
            "niri", "render_desktop_options", values
        )
        monitors = values.get("compositorDisplays", {}).get("niri")
        if monitors is not None:
            desktop += backend.render_for_backend("niri", "render_monitors", monitors)
    add(config / "desktop.kdl", desktop)
    includes.append("desktop.kdl")
    # Orient owns later palette updates. Seed this only if the user has none yet.
    palette_path = config / "palette.kdl"
    palette = home / palette_path
    if palette.is_file():
        add(palette_path, palette.read_text())
    else:
        scheme = home / ".local/state/nacre/scheme.json"
        if not scheme.is_file():
            scheme = common.get(Path(".local/state/nacre/scheme.json"))
        if scheme is not None and scheme.is_file():
            colors = _state.read_json_file(scheme)["colours"]
            text = (
                root / "nacre/shell-tools/compositor_backends/niri/palette.kdl.in"
            ).read_text()
            for role in ("primary", "secondary", "outlineVariant", "error"):
                color = colors[role].lstrip("#")
                if len(color) not in (6, 8) or any(
                    c not in "0123456789abcdefABCDEF" for c in color
                ):
                    raise ValueError("Invalid Niri palette color")
                text = text.replace("{{" + role + "}}", color)
            add(palette_path, text)
    if palette_path in result:
        includes.append("palette.kdl")
    private = home / config / "custom.kdl"
    add(config / "custom.kdl", private.read_text() if private.is_file() else "")
    # Existing custom settings stay user-owned; use their bytes for validation only.
    if private.exists():
        result.pop(config / "custom.kdl")
    entry = "\n".join("include " + json.dumps(name) for name in includes)
    entry += (
        '\nlayer-rule { match namespace="^(nacre-background|bead-wallpaper|'
        'bead-wallpaper-video)$"; place-within-backdrop true; }\n'
    )
    entry += 'layout { background-color "transparent"; }\n'
    entry += "hotkey-overlay { skip-at-startup; }\n"
    entry += (
        "environment {\n"
        '    NACRE_COMPOSITOR "niri"\n'
        "    NIRI_CONFIG "
        + json.dumps(str(home / config / "config.kdl"))
        + "\n"
        + "".join(
            "    " + name + " " + json.dumps(value) + "\n"
            for name, value in TOOLKIT_ENVIRONMENT.items()
        )
        + "}\n"
        'spawn-at-startup "systemctl" "--user" "start" "nacre-niri-startup.service"\n'
        'spawn-at-startup "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1"\n'
        'spawn-at-startup "wl-paste" "--watch" "nacre-desktop-extras" "store"\n'
    )
    # Same session helpers as hypr/conf/autostart.lua (tools/tests/test_session_startup.py
    # keeps the two sets equal). startup-apps skips apps that already have a window
    # and nacre-welcome --login honours its own show-once state.
    entry += "".join(
        "spawn-at-startup " + " ".join(json.dumps(word) for word in words) + "\n"
        for words in (
            ("uwsm", "app", "-t", "scope", "--", "/usr/lib/pam_kwallet_init"),
            (
                "uwsm",
                "app",
                "--",
                str(home / ".local/share/nacre/shell/tools/nacre-helper"),
                "startup-apps",
            ),
            ("uwsm", "app", "--", str(home / ".local/bin/nacre-welcome"), "--login"),
        )
    )
    entry += "".join("workspace " + json.dumps(name) + "\n" for name in names.values())
    entry += '\ninclude "custom.kdl"\n'
    source = add(config / "config.kdl", entry)
    add(
        config / "nacre-niri.kdl",
        'include "config.kdl"\n\n'
        + COLLECTION_BINDINGS
        + "\n"
        + edge_trigger_config(edge_trigger_delay(home)),
    )
    # Validation is offline; never query, reload or restart the running compositor.
    if validate_config:
        validate(source)
    add(
        ".config/systemd/user/nacre-niri.target",
        "[Unit]\nDescription=Nacre Niri session services\n"
        "PartOf=graphical-session.target\nAfter=graphical-session.target\n"
        "Wants=nacre-shell.service nacre-niri-widths.service nacre-niri-lock.service\n",
    )
    # Niri discards startup children's stdout/stderr. A oneshot records failures
    # in the journal and waits for the existing graphical-session ordering.
    add(
        ".config/systemd/user/nacre-niri-startup.service",
        "[Unit]\nDescription=Start Nacre services in the Niri session\n"
        "PartOf=graphical-session.target\nAfter=graphical-session.target\n"
        "[Service]\nType=oneshot\nRemainAfterExit=yes\n"
        "ExecStart=/usr/bin/python3 "
        + str(root / "tools/package_niri.py")
        + " finalize\n",
    )
    # Handle explicit loginctl lock requests. Idle/suspend policy is not enabled
    # before the owner validates manual password unlock on the real hardware.
    add(
        config / "hypridle.conf",
        "general {\n    lock_cmd = pidof hyprlock || hyprlock -c "
        + shlex.quote(str(home / lock))
        + "\n}\n",
    )
    add(
        ".config/systemd/user/nacre-niri-lock.service",
        "[Unit]\nDescription=Nacre Niri manual screen locking\n"
        "PartOf=graphical-session.target\nAfter=graphical-session.target\n"
        "[Service]\nExecStart=/usr/bin/hypridle -c %h/.config/nacre/niri/hypridle.conf\n"
        "Restart=on-failure\nRestartSec=3\n",
    )
    return result


def validate(config, compositor="niri"):
    binary = "/usr/bin/nacre-niri" if compositor == "nacre-niri" else "niri"
    try:
        subprocess.run(
            [binary, "validate", "-c", str(config)],
            check=True,
            capture_output=True,
            text=True,
            timeout=15,
        )
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            "Niri configuration validation failed:\n" + error.stderr
        ) from error
    except subprocess.TimeoutExpired as error:
        raise RuntimeError("Niri configuration validation timed out") from error


def command(config, compositor="niri"):
    if compositor not in ("niri", "nacre-niri"):
        raise ValueError("Unknown Niri session compositor: " + compositor)
    return [
        "uwsm",
        "start",
        "-e",
        "-D",
        "niri",
        "--",
        "/usr/bin/nacre-niri" if compositor == "nacre-niri" else "niri",
        "--session",
        "--config",
        str(config),
    ]


def session(home=None, compositor="niri"):
    home = Path.home() if home is None else home
    config = home / ".config/nacre/niri/config.kdl"
    prepared = home / ".local/share/nacre/niri-on-pc/session"
    if compositor == "nacre-niri" and (prepared / "source-revision").is_file():
        config = prepared / "config/config.kdl"
    if compositor == "nacre-niri":
        overlay = config.with_name("nacre-niri.kdl")
        if overlay.is_file():
            config = overlay
    if not config.is_file():
        raise RuntimeError(
            "Run nacre setup --compositor niri before choosing Nacre (niri)"
        )
    validate(config, compositor)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    # Let UWSM own export and logout cleanup of the static session variables.
    environment = {**os.environ, **TOOLKIT_ENVIRONMENT}
    environment["NACRE_COMPOSITOR"] = "niri"
    environment["XDG_CURRENT_DESKTOP"] = "niri"
    # UWSM continues to expose the standard desktop identity and finalizes the
    # standard Niri session variables even though the selected executable differs.
    environment["NACRE_NIRI_FORK"] = "0"
    finalize_names = set(environment.get("UWSM_FINALIZE_VARNAMES", "").split())
    environment["UWSM_FINALIZE_VARNAMES"] = " ".join(
        sorted(finalize_names | {"NACRE_NIRI_FORK"})
    )
    if compositor == "nacre-niri":
        environment["NACRE_NIRI_FORK"] = "1"
        wait_names = set(environment.get("UWSM_WAIT_VARNAMES", "").split())
        environment["UWSM_WAIT_VARNAMES"] = " ".join(
            sorted(wait_names | {"NIRI_SOCKET"})
        )
        environment["UWSM_FINALIZE_VARNAMES"] = " ".join(
            sorted(
                set(environment["UWSM_FINALIZE_VARNAMES"].split())
                | {
                    "WAYLAND_DISPLAY",
                    "DISPLAY",
                    "NIRI_SOCKET",
                    "NACRE_COMPOSITOR",
                    "NIRI_CONFIG",
                    "NACRE_NIRI_FORK",
                    "XCURSOR_SIZE",
                    "XCURSOR_THEME",
                }
            )
        )
    environment["NIRI_CONFIG"] = str(config)
    for name in (
        "HYPRLAND_INSTANCE_SIGNATURE",
        "HYPRCURSOR_THEME",
        "HYPRCURSOR_SIZE",
        "GDK_BACKEND",
    ):
        environment.pop(name, None)
    os.execvpe(
        "uwsm",
        command(config, compositor),
        environment,
    )


def finalize():
    # Niri already imports its display environment and sends READY=1. Its child
    # spawner deliberately removes NOTIFY_SOCKET, so repeating `uwsm finalize`
    # can race autoready's NotifyAccess update and fail before starting services.
    # Publish the child context explicitly without a second readiness handshake.
    variables = [
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "NIRI_SOCKET",
        "NACRE_COMPOSITOR",
        "NIRI_CONFIG",
        *TOOLKIT_ENVIRONMENT,
    ]
    subprocess.run(
        ["systemctl", "--user", "import-environment", *variables],
        check=True,
    )
    subprocess.run(
        ["dbus-update-activation-environment", *variables],
        check=True,
    )
    subprocess.run(["systemctl", "--user", "start", "nacre-niri.target"], check=True)


if __name__ == "__main__":
    if sys.argv[1:] == ["session"]:
        session()
    elif sys.argv[1:] == ["finalize"]:
        finalize()
    else:
        raise SystemExit("Usage: package_niri.py session|finalize")
