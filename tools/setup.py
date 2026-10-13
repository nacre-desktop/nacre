#!/usr/bin/env python3
"""Review and deploy private Nacre configuration from prebuilt system packages."""

import argparse
import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
# Also resolve the shared message runtime when invoked from installed packages.
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
tr = importlib.import_module("nacre_i18n").tr


def policy(root):
    spec = importlib.util.spec_from_file_location(
        "setup_profile", root / "nacre/shell-tools/profile-policy.py"
    )
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def distro(mapping, release=Path("/etc/os-release")):
    values = {}
    for line in release.read_text().splitlines():
        key, separator, value = line.partition("=")
        if separator:
            values[key] = value.strip('"')
    for name in (values.get("ID", ""), *values.get("ID_LIKE", "").split()):
        if name in mapping:
            return name
    raise RuntimeError(
        tr(
            "setup.error.unsupported-distribution",
            "Package setup does not yet support this distribution; see the contributor source path",
        )
    )


def supported_compositors(root):
    # Only advertise backends whose complete setup contract is installed.
    metadata = json.loads((root / "package.json.installed").read_text())
    choices = ["hyprland"]
    contract = root / "nacre/shell-tools/compositor_config.py"
    if contract.exists():
        spec = importlib.util.spec_from_file_location("setup_compositors", contract)
        backend = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(backend)
        if hasattr(backend, "supported_backends") and hasattr(
            backend, "backend_capabilities"
        ):
            choices = [
                name
                for name in backend.supported_backends()
                if backend.backend_capabilities(name).get("setup_ready", False)
            ]
    return [name for name in choices if name in metadata["compositors"]]


def ask(label, default):
    return input(f"{label} [{default}]: ").strip() or default


def selection(args, root, home, interactive):
    from keyboard import system_keyboard

    keyboard = system_keyboard()
    if interactive:
        print(
            tr(
                "setup.keyboard.detected",
                "System keyboard: {keyboard}",
                {"keyboard": json.dumps(keyboard)},
            )
        )
    for name in ("layout", "variant", "model", "options"):
        value = getattr(args, name)
        if value is None and interactive:
            value = ask(
                tr("setup.prompt.keyboard", "Keyboard {option}", {"option": name}),
                keyboard[name],
            )
        if value is not None:
            if value and not re.fullmatch(r"[a-zA-Z0-9_+:,.-]{1,200}", value):
                raise ValueError(
                    tr(
                        "setup.error.invalid-xkb",
                        "Invalid XKB {option}",
                        {"option": name},
                    )
                )
            keyboard[name] = value
    if not keyboard["layout"]:
        raise ValueError(
            tr("setup.error.empty-layout", "Keyboard layout cannot be empty")
        )
    choices = supported_compositors(root)
    compositor = args.compositor or (
        ask(
            tr(
                "setup.prompt.compositor",
                "Compositor ({choices})",
                {"choices": ", ".join(choices)},
            ),
            "hyprland",
        )
        if interactive
        else "hyprland"
    )
    if compositor not in choices:
        raise ValueError(
            tr(
                "setup.error.unavailable-backend",
                "Compositor setup backend unavailable: {compositor}",
                {"compositor": compositor},
            )
        )
    profiles = policy(root)
    chosen = args.profile
    if chosen is None and interactive:
        chosen = ask(
            tr(
                "setup.prompt.profile",
                "Profile ({default} or JSON path; {installed} keeps current)",
                {"default": "default", "installed": "installed"},
            ),
            "installed"
            if (home / ".config/nacre/profile.json").exists()
            else "default",
        )
    if chosen in (None, "installed"):
        profile = profiles.load(home)
    else:
        path = (
            root / "profiles/default.json"
            if chosen == "default"
            else Path(chosen).expanduser()
        )
        profile = profiles.validate(json.loads(path.read_text()))
    profile = copy.deepcopy(profile)
    optional = list(args.optional)
    if interactive:
        for name in ("ai", "brain"):
            default = (
                "yes" if name in profile["components"] or name in optional else "no"
            )
            enabled = ask(
                tr(
                    "setup.prompt.optional",
                    "Enable optional {component}",
                    {"component": name},
                ),
                default,
            ).lower()
            if enabled not in ("yes", "no", "y", "n"):
                raise ValueError(
                    tr(
                        "setup.error.answer-yes-no",
                        "Answer {yes} or {no}",
                        {"yes": "yes", "no": "no"},
                    )
                )
            profile["components"] = [c for c in profile["components"] if c != name]
            optional = [value for value in optional if value != name]
            if enabled in ("yes", "y"):
                optional.append(name)
    profile["components"] = list(
        dict.fromkeys(["configs", "shell", *profile["components"], *optional])
    )
    for component in ("ai", "brain"):
        if (
            component in profile["components"]
            and not (root / "deployment" / component).is_dir()
        ):
            raise RuntimeError(
                tr(
                    "setup.error.install-optional",
                    "Install {package} before enabling it",
                    {"package": "nacre-" + component},
                )
            )
    return profiles.validate(profile), keyboard, compositor


def missing_packages(root, profile):
    mapping = json.loads((root / "packaging/packages.json").read_text())
    names = mapping[distro(mapping)]
    packages = list(
        dict.fromkeys(
            [
                *names["core"],
                *[
                    p
                    for c in ("ai", "brain")
                    if c in profile["components"]
                    for p in names[c]
                ],
            ]
        )
    )
    return [
        p
        for p in packages
        if subprocess.run(
            ["pacman", "-Q", p.split("=")[0]],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        ).returncode
    ]


def main(argv=None):
    parser = argparse.ArgumentParser(
        description=tr(
            "setup.description",
            "Review and deploy private Nacre configuration from prebuilt system packages.",
        )
    )
    parser.add_argument("--non-interactive", action="store_true")
    parser.add_argument(
        "--yes",
        action="store_true",
        help=tr("setup.help.yes", "confirm applying the printed plan"),
    )
    parser.add_argument(
        "--plan",
        action="store_true",
        help=tr("setup.help.plan", "print the plan without changing any user files"),
    )
    parser.add_argument("--profile")
    parser.add_argument("--compositor")
    parser.add_argument(
        "--with", dest="optional", action="append", choices=("ai", "brain"), default=[]
    )
    for name in ("layout", "variant", "model", "options"):
        parser.add_argument("--" + name)
    args = parser.parse_args(argv)
    if os.geteuid() == 0:
        raise RuntimeError(
            tr(
                "setup.error.run-as-desktop-user",
                "Run {command} as your desktop user, without {sudo}",
                {"command": "nacre setup", "sudo": "sudo"},
            )
        )
    if not (ROOT / "package.json.installed").is_file():
        raise RuntimeError(
            tr(
                "setup.error.packages-required",
                "Install the Nacre packages first; source contributors use {installer}",
                {"installer": "install.sh"},
            )
        )
    interactive = sys.stdin.isatty() and not args.non_interactive and not args.yes
    if not interactive and not (args.yes or args.plan):
        parser.error(
            tr(
                "setup.error.non-interactive",
                "non-interactive setup requires {plan} or {yes}",
                {"plan": "--plan", "yes": "--yes"},
            )
        )
    profile, keyboard, compositor = selection(args, ROOT, Path.home(), interactive)
    with tempfile.TemporaryDirectory(prefix="nacre-setup-") as temporary:
        selected = Path(temporary) / "profile.json"
        selected.write_text(json.dumps(profile))
        previous = os.environ.get("NACRE_PROFILE_FILE")
        os.environ["NACRE_PROFILE_FILE"] = str(selected)
        try:
            from releases import package_plan, deploy_package

            plan = package_plan(ROOT, profile)
            missing = missing_packages(ROOT, profile)
            print(
                tr(
                    "setup.plan.summary",
                    "Nacre setup: profile={profile}; compositor={compositor}; components={components}",
                    {
                        "profile": profile["id"],
                        "compositor": compositor,
                        "components": ", ".join(profile["components"]),
                    },
                )
            )
            override = Path.home() / ".config/nacre/keyboard.lua"
            print(
                tr(
                    "setup.keyboard.preserved",
                    "Keyboard: existing private override preserved",
                )
                if override.exists() or override.is_symlink()
                else tr(
                    "setup.keyboard.selected",
                    "Keyboard: {keyboard}",
                    {"keyboard": json.dumps(keyboard)},
                )
            )
            print(tr("setup.plan.files", "Private file plan:"))
            for path in plan:
                print("  " + str(path))
            if missing:
                print(
                    tr(
                        "setup.plan.missing-packages",
                        "Missing packages: {packages}",
                        {"packages": ", ".join(missing)},
                    )
                )
            if args.plan:
                return 0
            if missing:
                raise RuntimeError(
                    tr(
                        "setup.error.missing-packages",
                        "Install the missing packages before applying setup",
                    )
                )
            if not args.yes and ask(
                tr(
                    "setup.prompt.apply",
                    "Apply this plan? ({yes}/{no})",
                    {"yes": "yes", "no": "no"},
                ),
                "no",
            ).lower() not in (
                "yes",
                "y",
            ):
                print(
                    tr("setup.cancelled", "Setup cancelled; no configuration changed.")
                )
                return 0
            result = deploy_package(ROOT, profile, keyboard, compositor)
            print(
                tr(
                    "setup.applied",
                    "Setup applied. Rollback snapshot: {release}",
                    {"release": result["release"]},
                )
            )
            print(
                tr(
                    "setup.next-session",
                    "Log out and back in to the selected UWSM session, then open Welcome with {welcome}. Run {status} or {rollback} to inspect or restore this setup.",
                    {
                        "welcome": "nacre-welcome",
                        "status": "nacre status",
                        "rollback": "nacre rollback",
                    },
                )
            )
        finally:
            if previous is None:
                os.environ.pop("NACRE_PROFILE_FILE", None)
            else:
                os.environ["NACRE_PROFILE_FILE"] = previous
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        raise SystemExit(
            tr(
                "setup.error.prefix",
                "{command}: {error}",
                {"command": "nacre setup", "error": str(error)},
            )
        ) from error
