#!/usr/bin/env python3
"""Deploy reviewed configuration copies, detect drift and retain private rollback."""

import argparse
import datetime as dt
import hashlib
import json
import sys
import os
import importlib
from pathlib import Path
import shutil
import tempfile

ROOT = Path(__file__).resolve().parent.parent
# Standalone helpers share the translation runtime during source-tree tooling.
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
_state = importlib.import_module("nacre_state")
sys.path.insert(0, str(Path(__file__).resolve().parent))
import nacre_modules  # noqa: E402

ACTIVE_DIRS = (
    "uwsm",
    "hypr",
    "kitty",
    "fastfetch",
    "fish",
    "gtk-3.0",
    "gtk-4.0",
    "rofi",
)
RETIRED_DIRS = (
    "waybar",
    "swaync",
    "waypaper",
    "wlogout",
    "matugen",
    "nwg-dock-hyprland",
    "kanshi",
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def files(root=ROOT):
    result = {}
    for folder in ("hypr", "kitty", "fastfetch", "fish", "uwsm"):
        for source in (root / folder).rglob("*"):
            if (
                source.is_file()
                and "__pycache__" not in source.relative_to(root).parts
                and source.suffix not in (".pyc", ".pyo")
                and source.name != ".qmlls.ini"
            ):
                result[Path(".config") / source.relative_to(root)] = source
    for name in (
        "nacre-ai",
        "nacre-colors",
        "orient",
        "nacre-terminal-logo",
        "nacre-terminal",
        "nacre-desktop-extras",
        "nacre-maintenance",
        "nacre-welcome-state",
        "nacre-brain",
        "nacre-brain-maintain",
        "nacre-brain-sync",
        "nacre-brain.py.rollback",
        "nacre-brain-maintain.py.rollback",
        "nacre-brain-sync.py.rollback",
        "xdg-open",
    ):
        result[Path(".local/bin") / name] = root / "bin" / name
    result[Path(".local/share/applications/org.kde.dolphin.desktop")] = (
        root / "nacre/desktop/org.kde.dolphin.desktop"
    )
    result[Path(".local/share/applications/nacre-ai.desktop")] = (
        root / "nacre/desktop/nacre-ai.desktop"
    )
    result[Path(".local/share/applications/nacre-welcome.desktop")] = (
        root / "nacre/desktop/nacre-welcome.desktop"
    )
    result[Path(".local/share/applications/nacre-colors.desktop")] = (
        root / "nacre/desktop/nacre-colors.desktop"
    )
    result[Path(".local/share/applications/nacre-settings.desktop")] = (
        root / "nacre/desktop/nacre-settings.desktop"
    )
    result[Path(".local/share/applications/nacre-widgets.desktop")] = (
        root / "nacre/desktop/nacre-widgets.desktop"
    )
    result[Path(".local/share/icons/hicolor/scalable/apps/nacre-widgets.svg")] = (
        root / "nacre/shell/branding/nacre-widgets.svg"
    )
    result[Path(".local/share/applications/nacre-terminal.desktop")] = (
        root / "nacre/desktop/nacre-terminal.desktop"
    )
    result[Path(".local/share/applications/nacre-brain.desktop")] = (
        root / "nacre/desktop/nacre-brain.desktop"
    )
    for kind in ("sync", "check"):
        for suffix in ("service", "timer"):
            name = f"nacre-brain-{kind}.{suffix}"
            result[Path(".config/systemd/user") / name] = root / "ai/systemd" / name
    result[Path(".config/systemd/user/nacre-brain-sync.service")] = (
        root / "ai/systemd/nacre-brain-sync.service"
    )
    result[Path(".config/systemd/user/nacre-power-key.service")] = (
        root / "nacre/shell-tools/nacre-power-key.service"
    )
    result[Path(".config/systemd/user/nacre-niri-widths.service")] = (
        root / "nacre/shell-tools/nacre-niri-widths.service"
    )
    # The general profile starts with actual licensed, offline wallpaper choices.
    # Flat unique names avoid empty category scaffolding and personal-file collisions.
    if selected_profile(root).get("id") == "default":
        collection = root / "nacre/shell-tools/demo-wallpapers"
        for source in sorted(collection.glob("*.jpg")):
            result[Path("Pictures/Wallpapers") / ("nacre-" + source.name)] = source
    features = selected_profile(root)["components"]

    def included(path):
        name = path.name
        if name.startswith("nacre-ai") and "ai" not in features:
            return False
        if name.startswith("nacre-brain") and "brain" not in features:
            return False
        return True

    result = {
        path: source
        for path, source in result.items()
        if source.exists() and included(path)
    }
    if (root / "package.json.installed").exists():
        from package_runtime import files as package_files, vendor_units

        result.pop(Path(".local/bin/orient"), None)
        result.update(package_files(root, selected_profile(root)))
        for relative in vendor_units(root):
            result.pop(relative, None)
    return result


def content(source, home):
    data = source.read_bytes()
    if "deployment" in source.parts:
        data = data.replace(b"@NACRE_HOME@", str(home).encode())
    return data


def wanted_digest(source, home):
    return hashlib.sha256(content(source, home)).hexdigest()


def selected_profile(root=ROOT):
    import importlib.util

    path = root / "nacre/shell-tools/profile-policy.py"
    if not path.exists():
        return {"components": ["configs", "shell"]}
    spec = importlib.util.spec_from_file_location("install_profile", path)
    policy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(policy)
    return policy.selected()


def atomic(path, data, mode=0o644):
    if _state.write_text_if_managed(path, data, mode=mode):
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(dir=path.parent, prefix="." + path.name)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
        os.chmod(temp, mode)
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


IDLE_WRAPPER = "# Host locking policy is private; installation seeds it only once.\nsource = ~/.config/nacre/hypridle.local.conf\n"


POWER_TAIL = '\n-- Local manual power-button and display-wake preference.\nrequire("conf.manual-power")\n'


def power_policy(home, root, migrate_idle):
    if not migrate_idle:
        return None
    current = home / ".config/hypr/hyprland.lua"
    source = root / "hypr/hyprland.lua"
    if not current.exists() or not source.exists():
        return None
    # Accept only this recognized historical addition, never arbitrary Lua drift.
    old_base = source.read_text().split("\n-- Optional private host behavior.")[0]
    if current.read_text() != old_base + POWER_TAIL:
        return None
    content = (home / ".config/hypr/conf/manual-power.lua").read_bytes()
    private = home / ".config/nacre/host.lua"
    if private.exists() and private.read_bytes() != content:
        raise RuntimeError("Private host policy differs; reconcile it before migration")
    return content


def idle_policy(home, root, migrate_idle):
    source = root / "hypr/hypridle.conf"
    if not source.exists() or source.read_text() != IDLE_WRAPPER:
        return None
    private = home / ".config/nacre/hypridle.local.conf"
    current = home / ".config/hypr/hypridle.conf"
    if migrate_idle and current.exists() and current.read_text() != IDLE_WRAPPER:
        content = current.read_bytes()
        if private.exists() and private.read_bytes() != content:
            raise RuntimeError(
                "Private idle policy differs; reconcile it before migration"
            )
        return content
    if not private.exists():
        return (root / "tools/defaults/hypridle.conf").read_bytes()
    return None


LEGACY_SYNC = {
    ".config/systemd/user/nacre-power-key.service": "e4ea2240157898be3c3f4355a04985681df0d9cd1a72163f6f6f2316a8b3b200",
    ".local/bin/nacre-brain-sync": "b1f7eb81b42ab6c205fb3e9bf19df46371ef31bfe854c6d3e1d984b70ea0c799",
    ".config/systemd/user/nacre-brain-sync.service": "8ea45eb1bf5536819ba8fdbda31fa8cae115910db406b950abc3aa5c30ecbc83",
}


def configuration_scope(app_routes_only=False, only=()):
    if app_routes_only and only:
        raise ValueError("Choose app routes or explicit configuration files")
    return (
        {Path(".local/bin/xdg-open")}
        if app_routes_only
        else {Path(path) for path in only}
    )


def enabled_files(root, enabled):
    return {
        path: source
        for path, source in files(root).items()
        if nacre_modules.module_of(path) in enabled
    }


def plan(
    home,
    root=ROOT,
    migrate=False,
    app_routes_only=False,
    migrate_idle=False,
    only=(),
    modules=None,
    adopt=(),
):
    scope = configuration_scope(app_routes_only, only)
    if not scope:
        idle_policy(home, root, migrate_idle)
        power_policy(home, root, migrate_idle)
    manifest = home / ".local/state/nacre/configuration.json"
    known = _state.read_json_file(manifest) if manifest.exists() else {}
    enabled = nacre_modules.resolve(home, receipt=known) if modules is None else modules
    if only and scope - files(root).keys():
        raise ValueError("Requested file is not managed by this configuration")
    desired = enabled_files(root, enabled)
    if scope:
        desired = {path: source for path, source in desired.items() if path in scope}
    links = {}
    for name in (*ACTIVE_DIRS, *RETIRED_DIRS):
        if scope or (
            name in ACTIVE_DIRS
            and nacre_modules.module_of(f".config/{name}/") not in enabled
        ):
            continue
        path = home / ".config" / name
        if path.is_symlink():
            target = path.resolve()
            managed = {
                relative: value
                for relative, value in known.items()
                if Path(relative).is_relative_to(Path(".config") / name)
            }
            managed_fish = (
                name == "fish"
                and bool(managed)
                and all(
                    digest(home / relative) == value
                    for relative, value in managed.items()
                )
            )
            source_tree = (target.parent / ".git").exists()
            if (
                not migrate
                or target.name != name
                or not target.is_dir()
                or not (source_tree or managed_fish)
            ):
                raise RuntimeError(
                    f"Refusing to replace configuration symlink: {path}; review --migrate-owned"
                )
            links[path] = target
    changed = []
    seeds = (
        set(_state.read_json_file(root / "seed-paths.json"))
        if (root / "seed-paths.json").exists()
        else set()
    )
    known = {key: value for key, value in known.items() if key not in seeds}
    for relative, source in desired.items():
        if (
            str(relative) in seeds
            and (home / relative).exists()
            and not legacy_empty_launcher(relative, home / relative)
        ):
            continue  # Defaults seed first login; later palette/private state stays local.
        dest = home / relative
        current, wanted = digest(dest), wanted_digest(source, home)
        if current == wanted and not any(dest.is_relative_to(p) for p in links):
            continue
        owned_link = any(dest.is_relative_to(p) for p in links)
        if dest.is_symlink():
            owned_link = owned_link or (
                migrate
                and dest.resolve().name == source.name
                and (dest.resolve().parent.parent / ".git").exists()
            )
        if (
            current is not None
            and not owned_link
            and current != known.get(str(relative))
            and not (
                str(relative) not in known and current == LEGACY_SYNC.get(str(relative))
            )
            and not (str(relative) in seeds and legacy_empty_launcher(relative, dest))
            and not (
                relative == Path(".config/hypr/hyprland.lua")
                and power_policy(home, root, migrate_idle) is not None
            )
            and not (
                migrate_idle
                and relative == Path(".config/hypr/hypridle.conf")
                and source.read_text() == IDLE_WRAPPER
            )
            and not (
                nacre_modules.module_of(relative) in adopt
                and str(relative) not in known
                and not dest.is_symlink()
            )
        ):
            raise RuntimeError(f"Local edit preserved: {dest}")
        changed.append((relative, source))
    from package_runtime import vendor_units

    vendor = vendor_units(root)
    for relative, previous in known.items():
        if Path(relative) in vendor:
            continue  # Package migration preserves edited HOME shadows with a warning.
        if relative in {
            ".config/systemd/user/nacre-shell.service",
            ".config/systemd/user/nacre-session-watch.service",
        }:
            continue  # The shell installer owns these services independently.
        if scope and Path(relative) not in scope:
            continue
        if Path(relative) in desired:
            continue
        dest = home / relative
        if not dest.exists():
            continue
        if digest(dest) != previous:
            raise RuntimeError(f"Local edit preserved: {dest}")
        changed.append((Path(relative), None))
    return changed, links, known


def legacy_empty_launcher(relative, path):
    # Migrate an empty first-preview launcher once, preserving hidden/usage
    # metadata. After the defaults receipt exists, a cleared list stays empty.
    if str(relative) != ".config/nacre/launcher.json" or path.is_symlink():
        return False
    home = path.parents[2]
    if (home / ".local/state/nacre/defaults.json").exists():
        return False
    try:
        data = _state.read_json_file(path)
        return (
            isinstance(data, dict)
            and data.get("favorites") == []
            and isinstance(data.get("hidden"), list)
            and set(data) <= {"favorites", "hidden", "recent", "usage"}
        )
    except (OSError, ValueError):
        return False


def record_modules(home, enabled, known, written=(), removed=()):
    """Persist enabled extras and the module of every path Nacre wrote."""
    state = nacre_modules.read(home)
    state["enabled"] = sorted(enabled & set(nacre_modules.EXTRAS))
    entries = state.setdefault("files", {})
    for relative, value in known.items():
        entries.setdefault(
            relative,
            dict(module=nacre_modules.module_of(relative), original=False),
        )["sha256"] = value
    for relative, original in written:
        entry = entries.setdefault(
            str(relative),
            dict(module=nacre_modules.module_of(relative), original=False),
        )
        entry["original"] = entry["original"] or original
        entry["sha256"] = digest(home / relative)
    for relative in removed:
        entries.pop(str(relative), None)
    nacre_modules.write(home, state)


def apply(
    home,
    root=ROOT,
    migrate=False,
    app_routes_only=False,
    migrate_idle=False,
    only=(),
    modules=None,
    adopt=(),
):
    scope = configuration_scope(app_routes_only, only)
    seeds = (
        set(_state.read_json_file(root / "seed-paths.json"))
        if (root / "seed-paths.json").exists()
        else set()
    )
    manifest = home / ".local/state/nacre/configuration.json"
    enabled = (
        nacre_modules.resolve(
            home, receipt=_state.read_json_file(manifest) if manifest.exists() else {}
        )
        if modules is None
        else modules
    )
    changed, links, known = plan(
        home, root, migrate, app_routes_only, migrate_idle, only, enabled, adopt
    )
    if not scope:
        from keyboard import seed

        seed(home)
    host_policy = None if scope else power_policy(home, root, migrate_idle)
    if host_policy is not None:
        atomic(home / ".config/nacre/host.lua", host_policy, 0o600)
    policy = None if scope else idle_policy(home, root, migrate_idle)
    if policy is not None:
        atomic(home / ".config/nacre/hypridle.local.conf", policy, 0o600)
    if not changed and not links:
        current = enabled_files(root, enabled)
        receipt = (
            {
                **known,
                **{
                    str(p): digest(home / p)
                    for p in current
                    if p in scope and str(p) not in seeds
                },
            }
            if scope
            else {str(p): digest(home / p) for p in current if str(p) not in seeds}
        )
        atomic(manifest, json.dumps(receipt, indent=2).encode(), 0o600)
        record_modules(home, enabled, receipt)
        return None
    backup = (
        home
        / ".local/state/nacre/backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    )
    backup.mkdir(parents=True, mode=0o700)
    entries = []
    # Capture every directory before unlinking it. Generated files remain private.
    for dest, target in links.items():
        saved = backup / dest.relative_to(home)
        shutil.copytree(target, saved, symlinks=True)
        entries.append(dict(path=str(dest), backup=str(saved), link=str(target)))
    monitor = home / ".config/hypr/conf/monitor.lua"
    private_monitor = home / ".config/nacre/monitor.lua"
    if (
        home / ".config/hypr" in links
        and monitor.exists()
        and not private_monitor.exists()
    ):
        atomic(private_monitor, monitor.read_bytes())
    for dest, _target in links.items():
        saved = backup / dest.relative_to(home)
        dest.unlink()
        if dest.name in RETIRED_DIRS:
            continue
        if dest.name == "fish":
            shutil.copytree(saved, dest, symlinks=True)
            continue
        dest.mkdir()
        # Preserve explicit user overrides and the current lock appearance.
        for rel in ("custom.conf", "hyprlock.conf", "settings.ini", "config.rasi"):
            if (saved / rel).is_file():
                shutil.copy2(saved / rel, dest / rel)
    tracked = nacre_modules.read(home).get("files", {})
    written, removed = [], []
    for relative, source in changed:
        dest = home / relative
        adopted = (
            source is not None
            and nacre_modules.module_of(relative) in adopt
            and str(relative) not in known
            and str(relative) not in tracked
            and dest.is_file()
            and not dest.is_symlink()
        )
        if adopted:
            # Keep the user's own file for module disable and uninstall.
            original = nacre_modules.originals(home) / relative
            original.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            if not original.exists():
                shutil.copy2(dest, original)
        if dest.exists() or dest.is_symlink():
            saved = backup / relative
            saved.parent.mkdir(parents=True, exist_ok=True)
            if not saved.exists():
                shutil.copy2(dest, saved)
            entries.append(dict(path=str(dest), backup=str(saved)))
        else:
            entries.append(dict(path=str(dest), backup=None))
        if source is None:
            dest.unlink(missing_ok=True)
            known.pop(str(relative), None)
            original = nacre_modules.originals(home) / relative
            if tracked.get(str(relative), {}).get("original") and original.is_file():
                shutil.copy2(original, dest)
                original.unlink()
            removed.append(relative)
        else:
            atomic(
                dest,
                content(source, home),
                0o755 if os.access(source, os.X_OK) else 0o644,
            )
            if str(relative) not in seeds:
                known[str(relative)] = digest(dest)
            written.append((relative, adopted))
    atomic(manifest, json.dumps(known, indent=2).encode(), 0o600)
    record_modules(home, enabled, known, written, removed)
    atomic(backup / "manifest.json", json.dumps(entries, indent=2).encode(), 0o600)
    return backup


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--apply", action="store_true")
    p.add_argument("--migrate-owned", action="store_true")
    p.add_argument(
        "--app-routes-only",
        action="store_true",
        help="deploy native app launch routes without touching compositor config",
    )
    p.add_argument(
        "--only",
        action="append",
        default=[],
        metavar="PATH",
        help="update only this managed path relative to home; repeat for multiple files",
    )
    p.add_argument(
        "--migrate-idle-override",
        action="store_true",
        help="preserve the current full idle policy as a private override",
    )
    args = p.parse_args()
    changed, links, _ = plan(
        Path.home(),
        migrate=args.migrate_owned,
        app_routes_only=args.app_routes_only,
        migrate_idle=args.migrate_idle_override,
        only=args.only,
    )
    print(
        f"{len(changed)} configuration files to deploy; {len(links)} source symlinks to migrate"
    )
    if args.apply:
        print(
            "Private configuration backup:",
            apply(
                Path.home(),
                migrate=args.migrate_owned,
                app_routes_only=args.app_routes_only,
                migrate_idle=args.migrate_idle_override,
                only=args.only,
            )
            or "no changes",
        )


if __name__ == "__main__":
    main()
