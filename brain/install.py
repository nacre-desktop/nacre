#!/usr/bin/env python3
"""Plan or deploy the current Brain without changing browser profiles."""

import argparse
import ast
import datetime as dt
import json
import os
import shlex
from pathlib import Path
import shutil
import signal
import subprocess
import time
import urllib.request


ROOT = Path(__file__).resolve().parent
FILES = ("control.py", "auth.py", "discovery.py", "semantic.py", "semantic-client.py")
STOP_UNITS = (
    "nacre-brain.service",
    "nacre-brain-sync.timer",
    "nacre-brain-check.timer",
    "nacre-brain-sync.service",
    "nacre-brain-check.service",
)


def _validate_sources(root):
    for name in FILES:
        ast.parse((root / name).read_text())
    node = shutil.which("node")
    node_cmd = (
        [node]
        if node
        else (["nacre-ai-tools", "node"] if shutil.which("nacre-ai-tools") else [])
    )
    if not node_cmd:
        raise RuntimeError(
            "Node.js is required to validate Brain JavaScript before deployment"
        )
    subprocess.run([*node_cmd, "--check", str(root / "web/app.js")], check=True)


def _helper_supports_web(home):
    helper = home / ".local/share/nacre/shell/tools/nacre-helper"
    if not helper.is_file():
        return False, f"Installed helper missing: {helper}"
    # brain-launch rejects an invalid action with its usage text before doing any
    # work. Require that exact response; an arbitrary exit code is not evidence.
    result = subprocess.run(
        [str(helper), "brain-launch", "__nacre_capability_probe__"],
        capture_output=True,
        text=True,
        timeout=5,
    )
    usage = result.stdout + result.stderr
    if (
        result.returncode == 2
        and "Usage: nacre-brain-ui [brain|capture|notes]" in usage
    ):
        return True, ""
    return (
        False,
        f"Installed nacre-helper does not support brain web commands ({usage.strip() or 'probe rejected'})",
    )


def _is_worktree(root, home):
    private = (home / ".local/share/nacre/ai/worktrees").resolve()
    if root.resolve().is_relative_to(private):
        return True
    try:
        git_dir = Path(
            subprocess.run(
                ["git", "-C", str(root), "rev-parse", "--absolute-git-dir"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        ).resolve()
        common = Path(
            subprocess.run(
                ["git", "-C", str(root), "rev-parse", "--git-common-dir"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        )
        if not common.is_absolute():
            common = (root / common).resolve()
        return git_dir != common.resolve()
    except (subprocess.CalledProcessError, OSError):
        return False


def _backup(path, home, backup):
    destination = backup / path.relative_to(home)
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink():
        destination.symlink_to(os.readlink(path))
    elif path.is_dir():
        shutil.copytree(path, destination, symlinks=True)
    else:
        shutil.copy2(path, destination)


def _restore(backup, home, dest, wrapper, service):
    for target in (dest, wrapper, service):
        saved = backup / target.relative_to(home)
        if target.exists() or target.is_symlink():
            if target.is_dir() and not target.is_symlink():
                shutil.rmtree(target)
            else:
                target.unlink()
        if saved.exists() or saved.is_symlink():
            target.parent.mkdir(parents=True, exist_ok=True)
            if saved.is_dir() and not saved.is_symlink():
                shutil.copytree(saved, target, symlinks=True)
            elif saved.is_symlink():
                target.symlink_to(os.readlink(saved))
            else:
                shutil.copy2(saved, target)


def _write_restore_script(backup, home, targets):
    script = backup / "restore.sh"
    lines = ["#!/usr/bin/env bash", "set -euo pipefail"]
    for target in targets:
        saved = backup / target.relative_to(home)
        target_q = shlex.quote(str(target))
        saved_q = shlex.quote(str(saved))
        lines.extend(
            (
                f"rm -rf -- {target_q}",
                f"if [[ -e {saved_q} || -L {saved_q} ]]; then mkdir -p -- {shlex.quote(str(target.parent))}; cp -a -- {saved_q} {target_q}; fi",
            )
        )
    script.write_text("\n".join(lines) + "\n")
    script.chmod(0o700)
    return script


def deploy(
    root=ROOT, home=None, systemctl=None, readiness=None, helper_probe=None, wait=True
):
    root = Path(root)
    home = Path.home() if home is None else Path(home)
    systemctl = systemctl or (
        lambda *args, **kwargs: subprocess.run(["systemctl", "--user", *args], **kwargs)
    )
    readiness = readiness or _ready
    helper_probe = helper_probe or _helper_supports_web
    supported, detail = helper_probe(home)
    if not supported:
        raise RuntimeError(detail)
    dest = home / ".local/share/nacre/brain"
    wrapper = home / ".local/bin/nacre-brain-ui"
    service = home / ".config/systemd/user/nacre-brain.service"
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    backup = home / ".local/state/nacre/brain/backups" / ("brain-" + stamp)
    backup.mkdir(parents=True, mode=0o700)
    for target in (dest, wrapper, service):
        if target.exists() or target.is_symlink():
            _backup(target, home, backup)
    restore_script = _write_restore_script(backup, home, (dest, wrapper, service))
    try:
        systemctl("stop", *STOP_UNITS, capture_output=True)
        for process in Path("/proc").iterdir():
            if not process.name.isdigit():
                continue
            try:
                if process.stat().st_uid == os.getuid():
                    args = (process / "cmdline").read_bytes().split(b"\0")
                    if (
                        any(str(dest / "control.py").encode() in arg for arg in args)
                        and b"serve" in args
                    ):
                        os.kill(int(process.name), signal.SIGTERM)
            except (OSError, ProcessLookupError):
                pass
        if dest.is_symlink():
            dest.unlink()
        elif dest.exists():
            shutil.rmtree(dest)
        dest.mkdir(parents=True, exist_ok=True)
        for name in FILES:
            shutil.copy2(root / name, dest / name)
        shutil.copytree(root / "web", dest / "web", dirs_exist_ok=True)
        shutil.copytree(
            root.parent / "ai/brain", dest / "templates", dirs_exist_ok=True
        )
        wrapper.parent.mkdir(parents=True, exist_ok=True)
        if wrapper.is_symlink():
            wrapper.unlink()
        wrapper.write_text((root / "launch.sh").read_text())
        wrapper.chmod(0o755)
        service.parent.mkdir(parents=True, exist_ok=True)
        if service.is_symlink():
            service.unlink()
        service.write_text((root / "brain.service").read_text())
        systemctl("daemon-reload", check=True)
        systemctl(
            "enable",
            "--now",
            "nacre-brain-sync.timer",
            "nacre-brain-check.timer",
            check=True,
        )
        systemctl("enable", "--now", service.name, check=True)
        if wait:
            for _ in range(50):
                if readiness():
                    print(
                        f"Brain ready. Restore: bash {shlex.quote(str(restore_script))}"
                    )
                    return backup
                time.sleep(0.2)
            raise RuntimeError("Brain did not become ready")
        return backup
    except Exception:
        _restore(backup, home, dest, wrapper, service)
        systemctl("daemon-reload", check=False)
        systemctl("restart", service.name, check=False)
        print(
            f"Readiness/deployment failed; restored backup {backup} and restarted {service.name}."
        )
        raise


def _ready():
    try:
        with urllib.request.urlopen(
            "http://127.0.0.1:17843/api/health", timeout=1
        ) as response:
            return json.load(response).get("ready", False)
    except OSError:
        return False


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="Apply the plan")
    parser.add_argument(
        "--allow-worktree",
        action="store_true",
        help="Allow deployment from a Git worktree",
    )
    args = parser.parse_args(argv)
    home = Path.home()
    if _is_worktree(ROOT, home) and not args.allow_worktree:
        parser.error(
            f"Refusing install from a private or non-main Git worktree: {ROOT}; pass --allow-worktree"
        )
    _validate_sources(ROOT)
    if not args.apply:
        dest = home / ".local/share/nacre/brain"
        wrapper = home / ".local/bin/nacre-brain-ui"
        service = home / ".config/systemd/user/nacre-brain.service"
        print(f"COPY/REPLACE {dest}: existing tree → source Brain tree")
        print(f"COPY/REPLACE {wrapper}: existing wrapper → {ROOT / 'launch.sh'}")
        print(f"COPY/REPLACE {service}: existing unit → {ROOT / 'brain.service'}")
        print("STOP current state → stopped: " + ", ".join(STOP_UNITS))
        print("KILL matching per-user legacy Brain serve PIDs → none (if any)")
        print(
            "ENABLE/START current timer and service states → enabled and active: nacre-brain-sync.timer nacre-brain-check.timer nacre-brain.service"
        )
        print(
            "Plan only; pass --apply to deploy. Every replaced path is backed up first."
        )
        return 0
    deploy(ROOT, home)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
