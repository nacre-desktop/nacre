"""Install modules: every Nacre-managed path belongs to one named module.

Tier 0 (core) and tier 1 (session) are always on. Tier 2 extras touch other
apps' files and are opt-in. AI and Brain follow the profile components.
The module manifest lives beside configure.py's hash receipt.
"""

import argparse
import datetime as dt
from functools import partial
import json
import os
from pathlib import Path
import shutil
import sys

MODULES = {
    "core": "Nacre's own settings, state and data, nacre-* commands, desktop entries and user units",
    "session": "Login session wiring: Niri config in ~/.config/nacre/niri, session targets, first lock screen",
    "hypr-config": "Manage ~/.config/hypr and ~/.config/uwsm (required by the Hyprland session)",
    "theming-kitty": "Nacre kitty terminal config in ~/.config/kitty",
    "theming-fish": "Nacre fish shell config in ~/.config/fish",
    "theming-gtk": "GTK 3/4 settings in ~/.config/gtk-3.0 and gtk-4.0",
    "theming-rofi": "rofi config in ~/.config/rofi",
    "theming-fastfetch": "fastfetch config in ~/.config/fastfetch",
    "xdg-open": "~/.local/bin/xdg-open shim that routes links and files through Nacre",
    "dolphin-override": "Dolphin desktop entry and FileManager1 D-Bus override",
    "wallpapers": "Demo wallpapers in ~/Pictures/Wallpapers",
    "ai": "Nacre AI (follows the profile: nacre setup --with ai)",
    "brain": "Brain (follows the profile: nacre setup --with brain)",
}
BASE = ("core", "session")
PROFILE = ("ai", "brain")
EXTRAS = tuple(name for name in MODULES if name not in BASE + PROFILE)
# Nacre's own directories; --keep-data leaves them untouched.
OWN_DIRS = (".config/nacre", ".local/state/nacre", ".local/share/nacre")
# Shared base directories are never pruned, even when they become empty.
KEEP_DIRS = {
    ".config",
    ".config/systemd",
    ".config/systemd/user",
    ".local",
    ".local/bin",
    ".local/share",
    ".local/share/applications",
    ".local/state",
    "Pictures",
}
# (module, path prefix) in priority order.
_RULES = (
    ("session", ".config/hypr/hyprlock.conf"),
    ("session", ".config/nacre/niri/"),
    ("session", ".config/systemd/user/nacre-hyprland.target"),
    ("session", ".config/systemd/user/nacre-niri.target"),
    ("hypr-config", ".config/hypr/"),
    ("hypr-config", ".config/uwsm/"),
    ("hypr-config", ".config/systemd/user/hypridle.service.d/"),
    ("theming-kitty", ".config/kitty/"),
    ("theming-fish", ".config/fish/"),
    ("theming-gtk", ".config/gtk-3.0/"),
    ("theming-gtk", ".config/gtk-4.0/"),
    ("theming-rofi", ".config/rofi/"),
    ("theming-fastfetch", ".config/fastfetch/"),
    ("xdg-open", ".local/bin/xdg-open"),
    ("dolphin-override", ".local/share/applications/org.kde.dolphin.desktop"),
    (
        "dolphin-override",
        ".local/share/dbus-1/services/org.freedesktop.FileManager1.service",
    ),
    ("wallpapers", "Pictures/"),
    ("ai", ".local/share/nacre/ai/"),
    ("brain", ".local/share/nacre/brain"),
)


def module_of(relative):
    path = Path(relative).as_posix()
    for name, prefix in _RULES:
        if path == prefix.rstrip("/") or path.startswith(prefix):
            return name
    leaf = path.rsplit("/", 1)[-1]
    if leaf.startswith(("nacre-ai", "nacre-sidebar-ai")):
        return "ai"
    if leaf.startswith("nacre-brain"):
        return "brain"
    return "core"


def manifest_path(home):
    return Path(home) / ".local/state/nacre/modules.json"


def originals(home):
    return Path(home) / ".local/state/nacre/originals"


def _state():
    import configure

    return configure._state


def read(home):
    path = manifest_path(home)
    if not path.is_file():
        return {}
    data = _state().read_json_file(path)
    return data if isinstance(data, dict) else {}


def write(home, data):
    from configure import atomic

    payload = {
        "enabled": sorted(set(data.get("enabled", ())) & set(EXTRAS)),
        "files": dict(sorted(data.get("files", {}).items())),
    }
    atomic(manifest_path(home), json.dumps(payload, indent=2).encode(), 0o600)


def known(home):
    path = Path(home) / ".local/state/nacre/configuration.json"
    return _state().read_json_file(path) if path.exists() else {}


def resolve(home, requested=None, compositors=(), receipt=None):
    """Enabled modules: base + profile + chosen extras (existing installs keep theirs)."""
    unknown = set(requested or ()) - set(EXTRAS)
    if unknown:
        raise ValueError("Unknown optional module: " + ", ".join(sorted(unknown)))
    state = read(home)
    if requested is not None:
        extras = set(requested)
    elif "enabled" in state:
        extras = set(state["enabled"])
    else:
        # Upgrade: keep every extra the existing receipt already manages.
        receipt = known(home) if receipt is None else receipt
        extras = {module_of(path) for path in receipt} & set(EXTRAS)
    if "hyprland" in compositors:
        extras.add("hypr-config")
    return frozenset({*BASE, *PROFILE, *extras})


def owned(home):
    """Every path the manifests record as written by Nacre, with its module."""
    receipt, files = known(home), read(home).get("files", {})
    result = {path: module_of(path) for path in receipt}
    result.update({path: entry["module"] for path, entry in files.items()})
    return dict(sorted(result.items()))


def _expected(path, receipt, files):
    return receipt.get(path) or files.get(path, {}).get("sha256")


def removal_plan(home, paths):
    """Classify removal of manifest paths; edited files are kept, never lost."""
    from configure import digest

    home = Path(home)
    receipt, files = known(home), read(home).get("files", {})
    actions = []
    for path in sorted(paths):
        dest = home / path
        original = (
            files.get(path, {}).get("original") and (originals(home) / path).is_file()
        )
        if dest.is_symlink():
            actions.append((path, "keep-edited"))  # A user link is never followed.
        elif not dest.exists():
            actions.append((path, "restore" if original else "absent"))
        elif digest(dest) == _expected(path, receipt, files):
            actions.append((path, "restore" if original else "remove"))
        else:
            actions.append((path, "keep-edited"))
    return actions


def _prune(home, path):
    parent = (home / path).parent
    while parent != home and parent.relative_to(home).as_posix() not in KEEP_DIRS:
        try:
            parent.rmdir()
        except OSError:
            return
        parent = parent.parent


def apply_removal(home, actions):
    """Remove/restore planned paths; snapshot removed bytes like configure.apply."""
    from configure import atomic

    home = Path(home)
    receipt, state = known(home), read(home)
    files = state.setdefault("files", {})
    backup = (
        home
        / ".local/state/nacre/backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    )
    entries = []
    for path, action in actions:
        dest, original = home / path, originals(home) / path
        if action in ("remove", "restore") and dest.is_file():
            saved = backup / path
            saved.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            shutil.copy2(dest, saved)
            entries.append(dict(path=str(dest), backup=str(saved)))
            dest.unlink()
        if action == "restore":
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(original, dest)
            original.unlink()
        if action == "keep-edited" and original.is_file():
            # The user's own pre-Nacre file stays reachable beside their edit.
            shutil.copy2(original, dest.with_name(dest.name + ".pre-nacre"))
            original.unlink()
        receipt.pop(path, None)
        files.pop(path, None)
        if action in ("remove", "absent"):
            _prune(home, Path(path))
    if entries:
        atomic(backup / "manifest.json", json.dumps(entries, indent=2).encode(), 0o600)
    atomic(
        home / ".local/state/nacre/configuration.json",
        json.dumps(receipt, indent=2).encode(),
        0o600,
    )
    write(home, state)
    return backup if entries else None


def _extra(name):
    if name not in EXTRAS:
        raise ValueError(
            f"{name} is not an optional module"
            + (
                "; it follows the profile (nacre setup --with)"
                if name in PROFILE
                else ""
            )
        )


def disable(home, name, *, apply=False):
    _extra(name)
    actions = removal_plan(
        home, [path for path, module in owned(home).items() if module == name]
    )
    if apply:
        state = read(home)
        state["enabled"] = sorted(
            (resolve(home) - {name}) & set(EXTRAS)
            if "enabled" not in state
            else set(state["enabled"]) - {name}
        )
        write(home, state)
        apply_removal(home, actions)
    return actions


def _under(path, roots):
    return any(path == root or path.startswith(root + "/") for root in roots)


def uninstall(home, *, keep_data=False, apply=False):
    """Plan or apply removal of exactly what the manifests say Nacre wrote."""
    home = Path(home)
    paths = [
        path
        for path in owned(home)
        if not (keep_data and _under(path, OWN_DIRS))
        # Nacre's own data dirs go as a whole below; restore nothing inside.
        and (keep_data or not _under(path, OWN_DIRS[:2]))
    ]
    actions = removal_plan(home, paths)
    link_file = home / ".local/state/nacre/package-links.json"
    links = _state().read_json_file(link_file) if link_file.is_file() else {}
    link_actions = [
        (path, "remove-link")
        for path, target in sorted(links.items())
        if (home / path).is_symlink() and os.readlink(home / path) == target
    ]
    dirs = [] if keep_data else [d for d in OWN_DIRS[:2] if (home / d).exists()]
    if apply:
        apply_removal(home, actions)
        for path, _ in link_actions:
            (home / path).unlink()
            _prune(home, Path(path))
        for directory in dirs:
            shutil.rmtree(home / directory)
    return actions, link_actions, dirs


def status_files(home):
    from configure import digest

    home = Path(home)
    receipt, files = known(home), read(home).get("files", {})
    rows = []
    for path, module in owned(home).items():
        dest = home / path
        state = (
            "missing"
            if not dest.exists() and not dest.is_symlink()
            else "managed"
            if digest(dest) == _expected(path, receipt, files)
            else "edited"
        )
        if files.get(path, {}).get("original"):
            state += ",replaced"
        rows.append((module, path, state))
    link_file = home / ".local/state/nacre/package-links.json"
    if link_file.is_file():
        for path, _target in sorted(_state().read_json_file(link_file).items()):
            rows.append((module_of(path), path, "link"))
    return rows


LABELS = {
    "remove": "remove",
    "restore": "restore original",
    "absent": "forget (already gone)",
    "keep-edited": "keep (edited after install)",
    "remove-link": "remove link",
}


def _print_actions(home, actions):
    for path, action in actions:
        print(f"  {LABELS[action]}: ~/{path} [{module_of(path)}]")


def _confirm(args, question):
    if args.yes:
        return True
    if not sys.stdin.isatty():
        raise SystemExit("Non-interactive use requires --plan or --yes")
    return input(question + " (yes/no) [no]: ").strip().lower() in ("yes", "y")


def _enable(home, root, name, plan_only, yes):
    import configure

    _extra(name)
    wanted = resolve(home) | {name}
    if (root / "package.json.installed").is_file():
        import releases
        import setup

        current = releases.STATE / "current.json"
        record = (
            _state().read_json_file(
                Path(_state().read_json_file(current)["release"]) / "release.json"
            )
            if current.is_file()
            else {}
        )
        compositors = record.get("compositors") or [record.get("compositor")]
        if not compositors[0]:
            raise RuntimeError("Run nacre setup before enabling modules")
        profile = setup.policy(root).load(home)
        from keyboard import system_keyboard

        keyboard = system_keyboard()
        planned = releases.package_plan(
            root, profile, compositors, keyboard, modules=wanted, adopt={name}
        )
        paths = [
            path for path in planned if module_of(Path(path).relative_to(home)) == name
        ]
        apply = partial(
            releases.deploy_package,
            root,
            profile,
            keyboard,
            compositors,
            modules=wanted,
            adopt={name},
        )
    else:
        changed, _links, _ = configure.plan(home, root, modules=wanted, adopt={name})
        paths = [home / path for path, _ in changed if module_of(path) == name]
        apply = partial(configure.apply, home, root, modules=wanted, adopt={name})
    print(f"Enable {name}: {MODULES[name]}")
    for path in paths:
        note = (
            " (replaces your file; the original is kept for disable/uninstall)"
            if path.exists() and str(path.relative_to(home)) not in owned(home)
            else ""
        )
        print(f"  write: {path}{note}")
    if plan_only or not _confirm(argparse.Namespace(yes=yes), "Apply?"):
        return 0
    apply()
    print(f"Enabled {name}.")
    return 0


def main(argv, home, root):
    parser = argparse.ArgumentParser(prog="nacre")
    commands = parser.add_subparsers(dest="command", required=True)
    module = commands.add_parser("module", help="list, enable or disable modules")
    module.add_argument("action", choices=("list", "enable", "disable"))
    module.add_argument("name", nargs="?")
    remove = commands.add_parser("uninstall", help="remove what Nacre wrote")
    remove.add_argument("--keep-data", action="store_true")
    for sub in (module, remove):
        sub.add_argument("--plan", action="store_true", help="print, change nothing")
        sub.add_argument("--yes", action="store_true", help="apply without asking")
    status = commands.add_parser("status", help="list every path Nacre owns")
    status.add_argument("--files", action="store_true", required=True)
    args = parser.parse_args(argv)
    home, root = Path(home), Path(root)
    if args.command == "status":
        for module_name, path, state in status_files(home):
            print(f"{module_name}\t~/{path}\t{state}")
        return 0
    if args.command == "uninstall":
        actions, links, dirs = uninstall(home, keep_data=args.keep_data)
        print("Nacre uninstall plan:")
        _print_actions(home, actions + links)
        for directory in dirs:
            print(f"  remove directory: ~/{directory} (Nacre settings and state)")
        print(
            "  keep: ~/.local/share/nacre (chats, AI and Brain data), Brain notes,"
            " your own wallpapers"
            + (", ~/.config/nacre and ~/.local/state/nacre" if args.keep_data else "")
        )
        if args.plan or not _confirm(args, "Uninstall?"):
            return 0
        uninstall(home, keep_data=args.keep_data, apply=True)
        print(
            "Removed. Run systemctl --user daemon-reload, then remove the packages"
            " with your package manager (for example: sudo pacman -R nacre)."
        )
        return 0
    if args.action == "list":
        enabled = resolve(home)
        for name, description in MODULES.items():
            mark = "on " if name in enabled else "off"
            if name in PROFILE:
                mark = "on " if any(m == name for m in owned(home).values()) else "off"
            print(f"{mark} {name:18} {description}")
        return 0
    if not args.name:
        parser.error("module name required")
    if args.action == "enable":
        return _enable(home, root, args.name, args.plan, args.yes)
    actions = disable(home, args.name)
    print(f"Disable {args.name}:")
    _print_actions(home, actions)
    if args.plan or not _confirm(args, "Apply?"):
        return 0
    disable(home, args.name, apply=True)
    print(f"Disabled {args.name}.")
    return 0
