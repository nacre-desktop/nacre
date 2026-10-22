"""Prepare a reviewable installation tree without running a desktop or touching accounts."""

import json
import importlib.util
import shutil
import subprocess
import sys
import os
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "nacre/shell-tools"))
from writer_lease import writer_lease


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def prepare(repo, destination, profile):
    destination = Path(os.path.abspath(destination.expanduser()))
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("Preparation destination must be empty")
    with writer_lease(destination / ".config/nacre") as lease:
        lease.ensure_valid()
        return _prepare_locked(repo, destination, profile, lease)


def _prepare_locked(repo, destination, profile, lease):
    destination.mkdir(parents=True, exist_ok=True)
    policy = module(repo / "nacre/shell-tools/profile-policy.py", "prepare_profile")
    profile = policy.validate(profile)
    config = module(repo / "tools/configure.py", "prepare_configuration")
    # The file map uses the selected profile; its source is not installed as private state.
    original = config.selected_profile
    config.selected_profile = lambda root=repo: profile
    try:
        for relative, source in config.files(repo).items():
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    finally:
        config.selected_profile = original
    data = destination / ".config/nacre"
    data.mkdir(parents=True, exist_ok=True)
    lease.publish("profile.json", (json.dumps(profile, indent=2) + "\n").encode())
    lease.publish("profile.lua", policy.lua(profile).encode())
    shell = destination / ".local/share/nacre/shell"
    shutil.copytree(
        repo / "nacre/shell",
        shell / "source",
        ignore=shutil.ignore_patterns("__pycache__"),
    )
    shutil.copytree(
        repo / "nacre/shell-tools",
        shell / "tools",
        ignore=shutil.ignore_patterns("tests", "__pycache__", "nacre-helper"),
    )
    helper = repo / "nacre/shell-tools/nacre-helper"
    if not helper.is_file():
        helper = Path(
            subprocess.check_output(
                ["bash", str(repo / "tools/build-helper.sh")], text=True
            ).strip()
        )
    shutil.copy2(helper, shell / "tools/nacre-helper")
    # Preserve the historical command paths as argv[0]-selected native aliases.
    for name in ("nacre-app", "nacre-settings", "nacre-welcome", "nacre-desktop-extras", "nacre-maintenance", "nacre-welcome-state"):
        alias = destination / ".local/bin" / name
        alias.parent.mkdir(parents=True, exist_ok=True)
        alias.symlink_to("../share/nacre/shell/tools/nacre-helper")
    units = destination / ".config/systemd/user"
    units.mkdir(parents=True, exist_ok=True)
    shutil.copy2(repo / "nacre/shell-tools/nacre-shell.service", units)
    shutil.copy2(repo / "nacre/shell-tools/nacre-niri-widths.service", units)
    if "ai" in profile["components"]:
        shutil.copy2(repo / "nacre/shell-tools/nacre-sidebar-ai.service", units)
    if "brain" in profile["components"]:
        shutil.copytree(
            repo / "brain",
            destination / ".local/share/nacre/brain",
            ignore=shutil.ignore_patterns("tests", "__pycache__"),
        )
        shutil.copy2(repo / "brain/brain.service", units / "nacre-brain.service")
    (destination / "installation.json").write_text(
        json.dumps(
            {
                "version": (repo / "VERSION").read_text().strip(),
                "profile": profile["id"],
                "components": profile["components"],
                "prepared": True,
                "activated": False,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        "Prepared "
        + profile["id"]
        + " installation at "
        + str(destination)
        + "; no services, packages, devices or accounts changed."
    )
