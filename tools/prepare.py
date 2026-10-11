"""Prepare a reviewable installation tree without running a desktop or touching accounts."""

import json
import importlib.util
import shutil


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def prepare(repo, destination, profile):
    destination = destination.expanduser().resolve()
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("Preparation destination must be empty")
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
    (data / "profile.json").write_text(json.dumps(profile, indent=2) + "\n")
    (data / "profile.lua").write_text(policy.lua(profile))
    shell = destination / ".local/share/nacre/shell"
    shutil.copytree(
        repo / "nacre/shell",
        shell / "source",
        ignore=shutil.ignore_patterns("__pycache__"),
    )
    shutil.copytree(
        repo / "nacre/shell-tools",
        shell / "tools",
        ignore=shutil.ignore_patterns("tests", "__pycache__"),
    )
    units = destination / ".config/systemd/user"
    units.mkdir(parents=True, exist_ok=True)
    shutil.copy2(repo / "nacre/shell-tools/nacre-shell.service", units)
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
