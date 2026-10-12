#!/usr/bin/env python3
"""Versioned desktop releases: snapshot, stage, exercise, promote, or restore."""

import argparse
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

HOME = Path.home()
STATE = HOME / ".local/state/nacre/releases"
CONTROL = HOME / ".local/share/nacre/control"


def atomic(path, data):
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
    for rel in json.loads(managed.read_text()) if managed.exists() else []:
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


def capture(repo, revision):
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    release = STATE / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    release.mkdir(mode=0o700)
    current = (
        json.loads((STATE / "current.json").read_text())
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
    for i, path in enumerate(paths(repo)):
        entry = dict(
            path=str(path),
            before=fingerprint(path),
            kind="absent",
            backup=str(release / "snapshot" / str(i)),
        )
        backup = Path(entry["backup"])
        backup.parent.mkdir(parents=True, exist_ok=True)
        if path.is_symlink() and path.name in ("shell-runtime", "palette-runtime"):
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
    atomic(release / "release.json", record)
    return release, record


def restore(release, record=None, force=False):
    record = record or json.loads((release / "release.json").read_text())
    if not force:
        for e in record["entries"]:
            if "after" in e and fingerprint(Path(e["path"])) != e["after"]:
                raise RuntimeError("Later edit preserved: " + e["path"])
    for service in (
        "nacre-shell.service",
        "nacre-brain.service",
        "nacre-session-watch.service",
    ):
        subprocess.run(["systemctl", "--user", "stop", service], capture_output=True)
    for e in record["entries"]:
        path = Path(e["path"])
        if path.is_symlink() or path.is_file():
            path.unlink()
        elif path.is_dir():
            shutil.rmtree(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        if e["kind"] == "runtime":
            restored = STATE / "restored-runtimes" / release.name
            restored.parent.mkdir(parents=True, exist_ok=True)
            if restored.exists():
                shutil.rmtree(restored)
            shutil.copytree(e["backup"], restored, symlinks=True)
            path.symlink_to(restored)
        elif e["kind"] == "link":
            path.symlink_to(e["link"])
        elif e["kind"] == "directory":
            shutil.copytree(e["backup"], path, symlinks=True)
        elif e["kind"] == "file":
            shutil.copy2(e["backup"], path)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(["hyprctl", "reload"], check=True, capture_output=True)
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
    record["status"] = "rolled-back"
    atomic(release / "release.json", record)
    atomic(
        STATE / "current.json",
        dict(revision=record["previous"], restoredFrom=str(release), status="restored"),
    )
    return record["previous"]


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
        json.loads((STATE / "current.json").read_text())
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
            record = json.loads((path / "release.json").read_text())
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


def release_disk_usage():
    result = subprocess.run(
        ["du", "-s", "-B1", str(STATE)], capture_output=True, text=True, check=True
    )
    return int(result.stdout.split()[0])


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


def deploy(repo, components, keyboard, migrate=False):
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
    release, record = capture(repo, revision)
    environment = dict(os.environ, NACRE_RELEASE_TRANSACTION="1")
    try:
        subprocess.run(
            [
                sys.executable,
                str(repo / "nacre/shell-tools/profile-policy.py"),
                "install",
            ],
            env=environment,
            check=True,
        )
        for component in components:
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
            subprocess.run(commands[component], env=environment, check=True)
            if component == "configs" and any(
                Path(entry["path"]).is_relative_to(HOME / ".config/hypr")
                and entry["before"] != fingerprint(Path(entry["path"]))
                for entry in record["entries"]
            ):
                subprocess.run(["hyprctl", "reload"], check=True, capture_output=True)
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
        errors = subprocess.check_output(["hyprctl", "configerrors"], text=True).strip()
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
        restore(release, record, force=True)
        raise


def main():
    p = argparse.ArgumentParser()
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
        "--component", action="append", choices=["configs", "apps", "shell", "brain"]
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
    with (STATE / "release.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
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
        elif a.action == "status":
            print(
                (STATE / "current.json").read_text()
                if (STATE / "current.json").exists()
                else "{}"
            )
        else:
            current = json.loads((STATE / "current.json").read_text())
            print(json.dumps(dict(restored=restore(Path(current["release"])))))


if __name__ == "__main__":
    main()
