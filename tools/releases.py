#!/usr/bin/env python3
"""Versioned desktop releases: snapshot, stage, exercise, promote, or restore."""

import argparse
from contextlib import contextmanager, nullcontext
import datetime as dt
import fcntl
import hashlib
import importlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# Shared state is independent of the palette Python environment.
for _state_directory in (
    Path(__file__).resolve().parents[1] / "nacre/shell-tools",
    Path.home() / ".local/share/nacre/shell/tools",
):
    if (_state_directory / "nacre_state.py").is_file():
        sys.path.insert(0, str(_state_directory))
        break
_state = importlib.import_module("nacre_state")
_writer = importlib.import_module("writer_lease")

HOME = Path.home()
STATE = HOME / ".local/state/nacre/releases"
CONTROL = HOME / ".local/share/nacre/control"


def compositor():
    """Load the common facade from a source checkout or the installed shell."""
    for directory in (
        Path(__file__).resolve().parents[1] / "nacre/shell-tools",
        Path.home() / ".local/share/nacre/shell/tools",
    ):
        if (directory / "nacre_native_compositor.py").is_file():
            if str(directory) not in sys.path:
                sys.path.insert(0, str(directory))
            return importlib.import_module("nacre_native_compositor")
    raise RuntimeError("The Nacre compositor service is not installed")


def atomic(path, data):
    if _state.schema(path):
        _state.write(path, data)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name("." + path.name + ".next")
    temp.write_text(json.dumps(data, indent=2))
    temp.chmod(0o600)
    os.replace(temp, path)


def fingerprint(path):
    if path.is_symlink():
        return (
            "link:"
            + os.readlink(path)
            + (":" + fingerprint(path.resolve()) if path.resolve().is_dir() else "")
        )
    if not path.exists():
        return None
    h = hashlib.sha256()
    files = sorted(path.rglob("*")) if path.is_dir() else [path]
    for p in files:
        if "__pycache__" in p.parts:
            continue
        if p.is_symlink():
            h.update(str(p.relative_to(path) if path.is_dir() else p.name).encode())
            h.update(os.readlink(p).encode())
        elif p.is_file():
            h.update(str(p.relative_to(path) if path.is_dir() else p.name).encode())
            with p.open("rb") as stream:
                for block in iter(lambda: stream.read(1048576), b""):
                    h.update(block)
    return h.hexdigest()


def paths(repo):
    # Explicit code/config allowlist. Never snapshot accounts, browser profiles, chats or wallets.
    result = [
        HOME / p
        for p in (
            ".local/share/nacre/shell",
            ".local/share/nacre/palette-runtime",
            ".local/share/nacre/thunar-style",
            ".local/share/nacre/dolphin-style",
            ".local/share/nacre/control",
            ".local/share/nacre/brain",
            ".local/share/nacre/ai/application",
        )
    ]
    result += [
        HOME / ".local/bin" / p
        for p in (
            "nacre",
            "nacre-shell",
            "nacre-menu",
            "nacre-app",
            "xdg-open",
            "nacre-brain-ui",
            "nacre-brain",
            "nacre-brain-maintain",
            "nacre-brain-sync",
        )
    ]
    result += [
        HOME / ".config/systemd/user" / p
        for p in (
            "nacre-shell.service",
            "nacre-session-watch.service",
            "nacre-niri-widths.service",
            "nacre-sidebar-ai.service",
            "nacre-brain.service",
            "nacre-brain-sync.service",
            "nacre-brain-sync.timer",
            "nacre-brain-check.service",
            "nacre-brain-check.timer",
            "nacre-power-key.service",
        )
    ]
    result += [
        HOME / ".local/state/nacre/configuration.json",
        HOME / ".config/nacre/shell.json",
        HOME / ".config/nacre/profile.json",
        HOME / ".config/nacre/profile.lua",
        HOME / ".local/share/dbus-1/services/org.freedesktop.Notifications.service",
        HOME / ".local/share/dbus-1/services/org.freedesktop.FileManager1.service",
        HOME / ".local/share/applications/nacre-thunar.desktop",
    ]
    managed = HOME / ".local/state/nacre/configuration.json"
    for rel in _state.read_json_file(managed) if managed.exists() else []:
        p = Path(rel)
        if p.is_absolute() or ".." in p.parts:
            raise RuntimeError("Invalid managed path")
        result.append(HOME / p)
    for name in ("hypr", "kitty", "fastfetch", "fish", "uwsm"):
        path = HOME / ".config" / name
        if path.is_symlink():
            result.append(path)
    configuration_source = repo / "tools/configure.py"
    if not configuration_source.is_file():
        configuration_source = Path(__file__).with_name("configure.py")
    spec = importlib.util.spec_from_file_location(
        "release_config_owner", configuration_source
    )
    configuration = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(configuration)
    result.extend(HOME / relative for relative in configuration.files(repo))
    return sorted(set(result), key=str)


def capture(repo, revision, extra=(), package=False, *, lease=None):
    if lease is not None:
        lease.ensure_valid()
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    release = STATE / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    release.mkdir(mode=0o700)
    current = (
        _state.read_json_file(STATE / "current.json")
        if (STATE / "current.json").exists()
        else {}
    )
    record = dict(
        revision=revision,
        previous=current.get("revision", "baseline"),
        status="staging",
        created=dt.datetime.now(dt.timezone.utc).isoformat(),
        entries=[],
    )
    captured = set(extra) if package else set(paths(repo)) | set(extra)
    captured = {
        path
        for path in captured
        if path.name not in {".writer.lock", "release.lock"}
    }
    for i, path in enumerate(sorted(captured, key=str)):
        if lease is not None:
            lease.ensure_valid()
        entry = dict(
            path=str(path),
            before=fingerprint(path),
            kind="absent",
            backup=str(release / "snapshot" / str(i)),
        )
        backup = Path(entry["backup"])
        if lease is not None:
            lease.ensure_valid()
        backup.parent.mkdir(parents=True, exist_ok=True)
        if (
            not package
            and path.is_symlink()
            and path.name in ("shell-runtime", "palette-runtime")
            and path.resolve().is_dir()
        ):
            entry.update(kind="runtime", link=os.readlink(path))
            shutil.copytree(
                path.resolve(),
                backup,
                symlinks=True,
                ignore=shutil.ignore_patterns("__pycache__"),
            )
        elif path.is_symlink():
            entry.update(kind="link", link=os.readlink(path))
        elif path.is_dir():
            entry["kind"] = "directory"
            shutil.copytree(
                path,
                backup,
                symlinks=True,
                ignore=shutil.ignore_patterns("__pycache__"),
            )
        elif path.is_file():
            entry["kind"] = "file"
            shutil.copy2(path, backup)
        record["entries"].append(entry)
    if lease is not None:
        lease.ensure_valid()
    atomic(release / "release.json", record)
    return release, record


def _restore_locked(release, record=None, force=False, *, lease):
    record = record or _state.read_json_file(release / "release.json")
    if not force:
        for e in record["entries"]:
            if "after" in e and fingerprint(Path(e["path"])) != e["after"]:
                raise RuntimeError("Later edit preserved: " + e["path"])
    live = record.get("deployment") != "package"
    desktop = compositor() if live else None
    if live:
        for service in (
            "nacre-shell.service",
            "nacre-brain.service",
            "nacre-session-watch.service",
            "nacre-niri-widths.service",
        ):
            subprocess.run(
                ["systemctl", "--user", "stop", service], capture_output=True
            )
    for e in record["entries"]:
        # Confirm the pinned destination and lock are still reachable before
        # each destructive restore step.
        # _restore_locked is only called through restore with this lease held.
        lease.ensure_valid()
        path = Path(e["path"])
        if path.is_symlink() or path.is_file():
            lease.ensure_valid()
            path.unlink()
        elif path.is_dir():
            lease.ensure_valid()
            shutil.rmtree(path)
        lease.ensure_valid()
        path.parent.mkdir(parents=True, exist_ok=True)
        if e["kind"] == "runtime":
            restored = STATE / "restored-runtimes" / release.name
            lease.ensure_valid()
            restored.parent.mkdir(parents=True, exist_ok=True)
            if restored.exists():
                lease.ensure_valid()
                shutil.rmtree(restored)
            lease.ensure_valid()
            shutil.copytree(e["backup"], restored, symlinks=True)
            lease.ensure_valid()
            path.symlink_to(restored)
        elif e["kind"] == "link":
            lease.ensure_valid()
            path.symlink_to(e["link"])
        elif e["kind"] == "directory":
            lease.ensure_valid()
            shutil.copytree(e["backup"], path, symlinks=True)
        elif e["kind"] == "file":
            lease.ensure_valid()
            shutil.copy2(e["backup"], path)
    if live:
        subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
        desktop.reload()
        subprocess.run(
            [
                "systemctl",
                "--user",
                "start",
                "nacre-shell.service",
                "nacre-brain.service",
            ],
            check=True,
        )
        subprocess.run(
            ["systemctl", "--user", "start", "nacre-session-watch.service"],
            capture_output=True,
        )
        subprocess.run(
            ["systemctl", "--user", "start", "nacre-niri-widths.service"],
            capture_output=True,
        )
    record["status"] = "rolled-back"
    lease.ensure_valid()
    atomic(release / "release.json", record)
    lease.ensure_valid()
    atomic(
        STATE / "current.json",
        dict(revision=record["previous"], restoredFrom=str(release), status="restored"),
    )
    return record["previous"]


def restore(release, record=None, force=False, *, lease=None):
    """Restore a snapshot while holding the destination writer lease."""
    manager = (
        nullcontext(lease)
        if lease is not None
        else _writer.writer_lease(HOME / ".config/nacre")
    )
    with manager as held:
        held.ensure_valid()
        return _restore_locked(release, record, force, lease=held)


def install_controller(repo):
    CONTROL.mkdir(parents=True, exist_ok=True)
    for name in ("releases.py", "check-overlays.py", "configure.py"):
        shutil.copy2(repo / "tools" / name, CONTROL / name)
    wrapper = HOME / ".local/bin/nacre"
    wrapper.parent.mkdir(parents=True, exist_ok=True)
    wrapper.write_text(
        '#!/bin/sh\nexec python3 "$HOME/.local/share/nacre/control/releases.py" "$@"\n'
    )
    wrapper.chmod(0o755)


def prune_releases(keep=10):
    current = (
        _state.read_json_file(STATE / "current.json")
        if (STATE / "current.json").exists()
        else {}
    )
    rows = []
    for path in sorted(STATE.iterdir(), reverse=True):
        if (
            not path.is_dir()
            or path.is_symlink()
            or not (path / "release.json").is_file()
        ):
            continue
        try:
            record = _state.read_json_file(path / "release.json")
        except (OSError, ValueError):
            continue
        rows.append((path, record))
    current_path = Path(current.get("release", "/nonexistent"))
    current_record = next((record for path, record in rows if path == current_path), {})
    good = [(path, record) for path, record in rows if record.get("status") == "good"]
    preserved = {path for path, _ in good[:keep]} | {current_path}
    previous = current_record.get("previous") or current.get("previous")
    previous_path = next(
        (
            path
            for path, record in rows
            if record.get("revision") == previous and path != current_path
        ),
        None,
    )
    if previous_path:
        preserved.add(previous_path)
    removed = []
    for path, _ in good:
        if path in preserved or (path / "retired-native").exists():
            continue
        shutil.rmtree(path)
        removed.append(str(path))
    return removed


def retire_native_runtimes(release):
    """Move recognized obsolete package trees out of active paths after live gates."""
    candidates = ((HOME / ".local/share/nacre/palette-runtime/usr", "bin/quickshell"),)
    retired = []
    for path, executable in candidates:
        if not (path / executable).is_file():
            continue
        # Do not move a runtime still used by an independently launched app.
        for proc in Path("/proc").iterdir():
            if not proc.name.isdigit():
                continue
            try:
                arguments = (proc / "cmdline").read_bytes().split(b"\0")
            except OSError:
                continue
            if arguments and arguments[0].startswith(str(path).encode() + b"/"):
                raise RuntimeError("Retired runtime is still in use: " + str(path))
        saved = release / "retired-native" / path.parent.name / path.name
        saved.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        path.rename(saved)
        retired.append({"path": str(path), "backup": str(saved)})
    if retired:
        atomic(release / "retired-native.json", retired)
    return retired


DESKTOP_SERVICES = (
    "nacre-shell.service",
    "nacre-sidebar-ai.service",
    "nacre-brain.service",
)


def required_live_services(components, initially_active):
    required = set(initially_active)
    required.add("nacre-shell.service")
    if "brain" in components:
        required.add("nacre-brain.service")
    return sorted(required)


def _deploy_locked(repo, components, keyboard, migrate=False, *, lease):
    repo = repo.resolve()
    revision = subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True
    ).strip()
    if subprocess.check_output(
        ["git", "-C", str(repo), "status", "--porcelain"], text=True
    ).strip():
        raise RuntimeError("Commit the reviewed candidate before deploying a release")
    subprocess.run([sys.executable, str(repo / "tools/check.py")], check=True)
    # Configuration preflight runs before any snapshot/cutover.
    if "configs" in components or "apps" in components:
        subprocess.run(
            [
                sys.executable,
                str(repo / "tools/configure.py"),
                *(["--app-routes-only"] if "configs" not in components else []),
                *(["--migrate-owned"] if migrate else []),
            ],
            check=True,
        )
    keyboard = keyboard or shutil.which("wtype") or "/usr/bin/wtype"
    if not Path(keyboard).is_file():
        raise RuntimeError("Install wtype for the live release gate")
    initially_active = {
        service
        for service in DESKTOP_SERVICES
        if subprocess.run(
            ["systemctl", "--user", "is-active", "--quiet", service],
            capture_output=True,
        ).returncode
        == 0
    }
    lease.ensure_valid()
    release, record = capture(repo, revision, lease=lease)
    environment = dict(os.environ)
    environment["NACRE_WRITER_LEASE_FD"] = str(lease.fd)
    try:
        lease.ensure_valid()
        subprocess.run(
            [
                sys.executable,
                str(repo / "nacre/shell-tools/profile-policy.py"),
                "install",
            ],
            env=environment,
            pass_fds=(lease.fd,),
            check=True,
        )
        for component in components:
            lease.ensure_valid()
            commands = {
                "configs": [
                    sys.executable,
                    str(repo / "tools/configure.py"),
                    "--apply",
                    *(["--migrate-owned"] if migrate else []),
                ],
                "apps": [
                    sys.executable,
                    str(repo / "tools/configure.py"),
                    "--app-routes-only",
                    "--apply",
                ],
                "shell": [
                    sys.executable,
                    str(repo / "nacre/shell-tools/install.py"),
                    "--code-only",
                ],
                "brain": [sys.executable, str(repo / "brain/install.py")],
                "ai": [sys.executable, str(repo / "ai/install-component.py")],
            }
            subprocess.run(commands[component], env=environment, pass_fds=(lease.fd,), check=True)
            if component == "configs" and any(
                Path(entry["path"]).is_relative_to(HOME / ".config/hypr")
                and entry["before"] != fingerprint(Path(entry["path"]))
                for entry in record["entries"]
            ):
                compositor().reload()
        subprocess.run(
            [
                sys.executable,
                str(repo / "tools/check-overlays.py"),
                "--keyboard",
                keyboard,
                "--dismiss-hover",
            ],
            check=True,
        )
        for service in required_live_services(components, initially_active):
            subprocess.run(
                ["systemctl", "--user", "is-active", "--quiet", service], check=True
            )
        errors = compositor().config_errors().strip()
        if errors:
            raise RuntimeError(errors)
        installed = HOME / ".local/share/nacre/shell"
        good = installed / "source.good"
        pending = installed / "source.good.next"
        if pending.exists():
            shutil.rmtree(pending)
        shutil.copytree(installed / "source", pending)
        if good.exists():
            shutil.rmtree(good)
        pending.rename(good)
        retire_native_runtimes(release)
        install_controller(repo)
        for entry in record["entries"]:
            entry["after"] = fingerprint(Path(entry["path"]))
        record["status"] = "good"
        atomic(release / "release.json", record)
        atomic(
            STATE / "current.json",
            dict(
                revision=revision,
                release=str(release),
                status="good",
                checked=record["created"],
            ),
        )
        prune_releases()
        return record
    except BaseException as error:
        record["status"] = "failed"
        record["error"] = str(error)
        atomic(release / "release.json", record)
        restore(release, record, force=True, lease=lease)
        raise


def deploy(repo, components, keyboard, migrate=False):
    """Run a source deployment under the config destination lease."""
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (STATE / "release.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        with _writer.writer_lease(HOME / ".config/nacre") as lease:
            return _deploy_locked(repo, components, keyboard, migrate, lease=lease)


@contextmanager
def package_configuration(repo, profile, keyboard, compositor, *, validate_config=True):
    import configure
    import package_sessions

    original, original_files = configure.selected_profile, configure.files
    configure.selected_profile = lambda root=repo: profile
    try:
        with tempfile.TemporaryDirectory(prefix="nacre-package-config-") as temporary:
            scope = ()
            desired = {}
            common = original_files(repo)
            import package_runtime

            common = package_runtime.presentation_seed(HOME, common, Path(temporary))
            common = package_runtime.launcher_seed(HOME, common, Path(temporary))
            for name in package_sessions.backends(compositor):
                backend = package_sessions.integration(repo, name)
                if backend is None:
                    desired.update(common)
                else:
                    desired.update(
                        backend.files(
                            repo,
                            HOME,
                            profile,
                            keyboard,
                            Path(temporary) / name,
                            common,
                            validate_config=validate_config,
                        )
                    )
            if any(
                package_sessions.integration(repo, name) is not None
                for name in package_sessions.backends(compositor)
            ):
                configure.files = lambda root=repo: desired
                scope = tuple(str(path) for path in desired)
            yield scope
    finally:
        configure.selected_profile, configure.files = original, original_files


def package_plan(
    repo,
    profile,
    compositor="hyprland",
    keyboard=None,
    *,
    validate_config=True,
    lease=None,
):
    if lease is not None:
        lease.ensure_valid()
    import configure
    import package_runtime
    from keyboard import system_keyboard

    keyboard = system_keyboard() if keyboard is None else keyboard
    with package_configuration(
        repo, profile, keyboard, compositor, validate_config=validate_config
    ) as scope:
        collisions = set(configure.files(repo)) & set(
            package_runtime.links(repo, profile, compositor)
        )
        if collisions:
            raise RuntimeError(
                "Package declares both file and runtime link: "
                + str(sorted(collisions)[0])
            )
        changed, links, _ = configure.plan(HOME, repo, only=scope)
        if links:
            raise RuntimeError(
                "Source-linked configuration requires explicit source migration first"
            )
        runtime = package_runtime.plan(HOME, repo, profile, compositor)
        result = [HOME / relative for relative, _ in changed + runtime]
        result += [
            HOME / ".config/nacre" / name
            for name in ("profile.json", "profile.lua", "keyboard.lua")
        ]
        if lease is not None:
            lease.ensure_valid()
        return sorted(set(result), key=str)


def deploy_package(repo, profile, keyboard, compositor):
    """Apply prebuilt files without a running desktop; use the same rollback owner."""
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (STATE / "release.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        with _writer.writer_lease(HOME / ".config/nacre") as lease:
            lease.ensure_valid()
            return _deploy_package_locked(repo, profile, keyboard, compositor, lease)


def _deploy_package_locked(repo, profile, keyboard, compositor, lease):
    import configure
    import package_runtime
    from keyboard import seed
    import setup
    import package_sessions

    selected = package_sessions.backends(compositor)
    if any(name not in setup.supported_compositors(repo) for name in selected):
        raise RuntimeError(
            "This package does not yet include a complete compositor setup contract"
        )
    if not (repo / "package.json.installed").is_file():
        raise RuntimeError("Package metadata missing")
    desired = package_plan(repo, profile, compositor, keyboard, lease=lease)
    lease.ensure_valid()
    extra = desired + [
        HOME / ".local/state/nacre/package-links.json",
        HOME / ".local/state/nacre/configuration.json",
    ]
    revision = package_runtime.metadata(repo)["revision"]
    release, record = capture(repo, revision, extra, package=True, lease=lease)
    record.update(
        deployment="package",
        compositor=selected[0],
        compositors=list(selected),
        release=str(release),
    )
    try:
        with package_configuration(repo, profile, keyboard, compositor) as scope:
            lease.ensure_valid()
            seed(HOME, keyboard)
            lease.ensure_valid()
            configure.apply(HOME, repo, only=scope)
            profiles = setup.policy(repo)
            profiles.selected = lambda: profile
            profiles.install(HOME, lease=lease)
            lease.ensure_valid()
            package_runtime.apply(HOME, repo, profile, compositor)
        for entry in record["entries"]:
            lease.ensure_valid()
            entry["after"] = fingerprint(Path(entry["path"]))
        record["status"] = "prepared"
        lease.ensure_valid()
        atomic(release / "release.json", record)
        lease.ensure_valid()
        atomic(
            STATE / "current.json",
            dict(
                revision=revision,
                release=str(release),
                status="prepared",
                checked=record["created"],
            ),
        )
        return record
    except BaseException as error:
        record.update(status="failed", error=str(error))
        lease.ensure_valid()
        atomic(release / "release.json", record)
        restore(release, record, force=True, lease=lease)
        raise

def main():
    if len(sys.argv) > 1 and sys.argv[1] == "session":
        import package_sessions

        installed = Path("/usr/share/nacre")
        if len(sys.argv) != 3 or not (installed / "package.json.installed").is_file():
            raise SystemExit(
                "Usage after installing packages: nacre session COMPOSITOR"
            )
        package_sessions.start(installed, sys.argv[2])
        return
    if len(sys.argv) > 1 and sys.argv[1] == "setup":
        installed = Path("/usr/share/nacre/tools/setup.py")
        candidate = (
            installed if installed.exists() else Path(__file__).with_name("setup.py")
        )
        raise SystemExit(
            subprocess.call([sys.executable, str(candidate), *sys.argv[2:]])
        )
    p = argparse.ArgumentParser(
        epilog="Package users: nacre setup [OPTIONS] prepares a login; nacre session COMPOSITOR starts it from a TTY after logout."
    )
    p.add_argument(
        "action",
        choices=[
            "deploy",
            "status",
            "rollback",
            "doctor",
            "profile",
            "check",
            "session",
            "restart",
        ],
    )
    p.add_argument("repo", nargs="?", type=Path)
    p.add_argument(
        "--component",
        action="append",
        choices=["configs", "apps", "shell", "brain", "ai"],
    )
    p.add_argument("--keyboard")
    p.add_argument("--migrate-owned", action="store_true")
    output = p.add_mutually_exclusive_group()
    output.add_argument(
        "--report", action="store_true", help="Redacted Markdown support report"
    )
    output.add_argument(
        "--json", action="store_true", help="Redacted JSON support report"
    )
    a = p.parse_args()
    if (a.report or a.json) and a.action != "doctor":
        p.error("--report and --json are only valid for doctor")
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    if a.action in ("doctor", "profile", "check", "session", "restart"):
        subprocess.run(
            [
                sys.executable,
                str(HOME / ".local/share/nacre/shell/tools/maintenance.py"),
                a.action,
                *(["--report"] if a.report else ["--json"] if a.json else []),
            ],
            check=True,
        )
        return
    if a.action == "deploy":
        print(
            json.dumps(
                deploy(
                    a.repo or Path.cwd(),
                    a.component or ["configs", "shell", "brain"],
                    a.keyboard,
                    a.migrate_owned,
                )
            )
        )
        return
    with (STATE / "release.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if a.action == "status":
            print(
                (STATE / "current.json").read_text()
                if (STATE / "current.json").exists()
                else "{}"
            )
        else:
            current = _state.read_json_file(STATE / "current.json")
            print(json.dumps(dict(restored=restore(Path(current["release"])))))


if __name__ == "__main__":
    main()
