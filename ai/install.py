#!/usr/bin/env python3
"""Install the user-level AI workflow without changing existing credentials."""

import argparse
import datetime as dt
import os
import importlib
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import sys

for _state_directory in (
    Path(__file__).resolve().parents[1] / "nacre/shell-tools",
    Path.home() / ".local/share/nacre/shell/tools",
):
    if (_state_directory / "nacre_state.py").is_file():
        sys.path.insert(0, str(_state_directory))
        break
_state = importlib.import_module("nacre_state")


def install_launcher(repo, home, apply=False, name="nacre-ai"):
    """Update one recognized controller copy without reinstalling AI setup."""
    if name not in ("nacre-ai", "nacre-ai-chat", "nacre-ai-claude"):
        raise RuntimeError("Unrecognized AI controller; existing files left unchanged")
    target = (home / ".local/bin" / name).resolve()
    runtime = home / ".local/share/nacre/ai/conversation-runtime"
    if (
        not target.is_relative_to(runtime)
        or target.name != name
        or not target.is_file()
    ):
        raise RuntimeError(
            "Controller-only deployment requires a recognized private runtime copy; existing files were left unchanged"
        )
    source = repo / "bin" / name
    print("Controller-only plan:", source, "→", target)
    if not apply:
        print(
            "Plan only; add --apply to replace the controller and save its prior copy."
        )
        return
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    backup = home / ".local/state/nacre/ai/backups" / stamp
    backup.mkdir(parents=True, mode=0o700)
    shutil.copy2(target, backup / name)
    old = target.read_bytes()
    with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as out:
        out.write(source.read_bytes())
        pending = Path(out.name)
    pending.chmod(target.stat().st_mode & 0o777)
    if target.read_bytes() != old:
        pending.unlink()
        raise RuntimeError(
            "Installed controller changed during deployment; retry after review"
        )
    pending.replace(target)
    print("Controller updated; previous copy:", backup / name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--remote",
        action="store_true",
        help="Install host helpers and guidance without replacing its Codex binary",
    )
    parser.add_argument(
        "--repo", type=Path, default=Path(__file__).resolve().parent.parent
    )
    parser.add_argument(
        "--codex-home",
        type=Path,
        help="Also install guidance in an isolated Codex home",
    )
    parser.add_argument(
        "--launcher-only",
        action="store_true",
        help="Plan a code-only update of the recognized private launcher copy",
    )
    parser.add_argument(
        "--helper-only",
        choices=("nacre-ai", "nacre-ai-chat", "nacre-ai-claude"),
        help="Plan a code-only update of one recognized private controller",
    )
    parser.add_argument(
        "--apply", action="store_true", help="Apply the selected code-only update"
    )
    args = parser.parse_args()
    repo = args.repo.resolve()
    home = Path.home()
    if args.launcher_only and args.helper_only:
        parser.error("Choose either --launcher-only or --helper-only")
    if args.launcher_only or args.helper_only:
        install_launcher(repo, home, args.apply, args.helper_only or "nacre-ai")
        return
    if args.apply:
        parser.error("--apply requires --launcher-only or --helper-only")
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    backup = home / ".local/state/nacre/ai/backups" / stamp

    def save(path):
        backup.mkdir(parents=True, exist_ok=True, mode=0o700)
        dest = backup / path.relative_to(home)
        dest.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        if path.is_symlink():
            dest.symlink_to(os.readlink(path))
        elif path.is_dir():
            shutil.copytree(path, dest, symlinks=True)
        else:
            shutil.copy2(path, dest)

    def link(source, target):
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.is_symlink() and target.resolve() == source.resolve():
            return
        if target.exists() or target.is_symlink():
            save(target)
            if target.is_dir() and not target.is_symlink():
                raise RuntimeError(f"Refusing to replace existing directory: {target}")
            target.unlink()
        target.symlink_to(source)

    binaries = (
        ["nacre-ai", "nacre-brain", "nacre-ai-remote"]
        if args.remote
        else [
            "codex",
            "nacre-ai",
            "nacre-brain",
            "nacre-ai-remote",
        ]
    )
    binaries.extend(
        [
            "nacre-brain",
            "nacre-brain-maintain",
            "nacre-brain-sync",
            "nacre-ai-tools",
            "nacre-ai-skills",
            "nacre-ai-account",
            "nacre-ai-chat",
            "nacre-ai-claude",
            "nacre-ai-context",
            "nacre-ai-memory",
            "nacre-ai-usage",
        ]
    )
    if (
        not args.remote
        and (
            home / ".local/share/nacre/ai/obsidian/1.13.7/squashfs-root/AppRun"
        ).exists()
    ):
        binaries.append("obsidian")
    if not args.remote:
        for name in (
            "nacre-brain-sync.service",
            "nacre-brain-sync.timer",
            "nacre-brain-check.service",
            "nacre-brain-check.timer",
        ):
            link(repo / "ai/systemd" / name, home / ".config/systemd/user" / name)
        secret_service = (
            home / ".local/share/dbus-1/services/org.freedesktop.secrets.service"
        )
        if (
            Path("/usr/bin/ksecretd").exists()
            and not Path(
                "/usr/share/dbus-1/services/org.freedesktop.secrets.service"
            ).exists()
            and not secret_service.exists()
        ):
            link(repo / "ai/dbus/org.freedesktop.secrets.service", secret_service)
    for name in dict.fromkeys(binaries):
        link(repo / "bin" / name, home / ".local/bin" / name)
    for skill in (repo / "ai/skills").iterdir():
        if skill.is_dir():
            shared = home / ".agents/skills" / skill.name
            previous = shared.resolve() if shared.is_symlink() else None
            link(skill, shared)
            # Migrate only aliases proven to point at our previous shared skill.
            if previous and previous != shared.resolve():
                profiles = [home / ".codex", home / ".claude"]
                for group in ("accounts", "claude-accounts"):
                    profiles.extend((home / ".local/share/nacre/ai" / group).glob("*"))
                for profile in profiles:
                    alias = profile / "skills" / skill.name
                    if alias.is_symlink() and alias.resolve() == previous:
                        link(shared, alias)
    subprocess.run([str(home / ".local/bin/nacre-ai-skills")], check=True)
    guidance = home / ".codex/AGENTS.md"
    guidance.parent.mkdir(parents=True, exist_ok=True)
    if not guidance.exists():
        link(repo / "ai/AGENTS.md", guidance)
    elif (
        guidance.is_symlink()
        and guidance.resolve() == (repo / "ai/AGENTS.md").resolve()
    ):
        pass
    else:
        marker = "<!-- nacre-ai-guidance -->"
        current = guidance.read_text()
        block = (
            marker
            + "\nFor personal knowledge and cross-project context, read `"
            + str(repo / "ai/AGENTS.md")
            + "`.\n<!-- /nacre-ai-guidance -->"
        )
        if marker in current:
            updated, count = re.subn(
                re.escape(marker)
                + r"\nFor personal knowledge and cross-project context, read `[^\n]+`\.\n(?:<!-- /nacre-ai-guidance -->)?",
                lambda _: block,
                current,
                count=1,
            )
            if count != 1:
                raise RuntimeError(
                    "Existing personal guidance marker needs manual integration"
                )
        else:
            updated = current.rstrip() + "\n\n" + block + "\n"
        if updated != current:
            save(guidance)
            # A foreign symlink may point into another repository; never edit its target.
            if guidance.is_symlink():
                guidance.unlink()
            guidance.write_text(updated)
    if args.codex_home:
        isolated = args.codex_home.expanduser().resolve()
        isolated.mkdir(parents=True, exist_ok=True, mode=0o700)
        target = isolated / "AGENTS.md"
        if target.exists() and target.resolve() != (repo / "ai/AGENTS.md").resolve():
            raise RuntimeError(
                f"Existing isolated guidance needs manual integration: {target}"
            )
        link(repo / "ai/AGENTS.md", target)
    title_env = dict(os.environ, CODEX_HOME=str(home / ".codex"))
    if args.codex_home:
        title_env["CODEX_HOME"] = str(args.codex_home.expanduser().resolve())
    subprocess.run(
        [str(home / ".local/bin/nacre-ai-chat"), "configure"],
        env=title_env,
        check=True,
    )
    config = home / ".config/nacre/ai/projects.json"
    if not config.exists():
        config.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        _state.write(
            config, {"projects": [{"id": "os", "label": "Nacre", "path": str(repo)}]}
        )
        config.chmod(0o600)
    subprocess.run([str(home / ".local/bin/nacre-brain"), "init"], check=True)
    subprocess.run([str(home / ".local/bin/nacre-brain-maintain"), "setup"], check=True)
    print("Personal workflow installed. Existing account credentials were not changed.")
    print("Project registry:", config)
    if backup.exists():
        print("Replaced-file backup:", backup)


if __name__ == "__main__":
    main()
