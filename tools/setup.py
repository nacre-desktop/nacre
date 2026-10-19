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
_state = importlib.import_module("nacre_state")
_writer = importlib.import_module("writer_lease")

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
    metadata = _state.read_json_file(root / "package.json.installed")
    choices = ["hyprland"]
    contract = root / "nacre/shell-tools/nacre_native_config.py"
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
    import package_sessions

    for name in metadata["compositors"]:
        try:
            backend = package_sessions.integration(root, name)
        except (KeyError, RuntimeError):
            continue
        if backend is not None and backend.supported(root):
            choices.append(name)
    return list(
        dict.fromkeys(name for name in choices if name in metadata["compositors"])
    )


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
                {"keyboard": keyboard["layout"]},
            )
        )
    for name in ("layout", "variant", "model", "options"):
        value = getattr(args, name)
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
    if not choices:
        raise RuntimeError("No supported compositor integration is installed")
    detected = next(
        (
            name
            for name in os.environ.get("XDG_CURRENT_DESKTOP", "").lower().split(":")
            if name in choices
        ),
        choices[0],
    )
    compositor = args.compositor or (
        ask(
            tr(
                "setup.prompt.compositor",
                "Compositor ({choices})",
                {"choices": ", ".join(choices)},
            ),
            detected,
        )
        if interactive
        else detected
    )
    selected = [name.strip().lower() for name in compositor.split(",")]
    compositor, *from_prompt = selected
    if any(name not in choices for name in selected):
        raise ValueError(
            tr(
                "setup.error.unavailable-backend",
                "Compositor setup backend unavailable: {compositor}",
                {"compositor": compositor},
            )
        )
    additional = [*from_prompt, *getattr(args, "additional_compositors", [])]
    if any(name not in choices for name in additional):
        raise ValueError("Unknown additional compositor")
    args.additional_compositors = [
        name for name in dict.fromkeys(additional) if name != compositor
    ]
    profiles = policy(root)
    chosen = args.profile
    if chosen in (None, "installed"):
        profile = profiles.load(home)
    else:
        path = (
            root / "profiles/default.json"
            if chosen == "default"
            else Path(chosen).expanduser()
        )
        profile = profiles.validate(_state.read_json_file(path))
    profile = copy.deepcopy(profile)
    optional = list(args.optional)
    if interactive:
        default = (
            "yes" if {"ai", "brain"} & set(profile["components"] + optional) else "no"
        )
        enabled = ask(
            tr(
                "setup.prompt.ai-tools",
                "Enable AI tools ({ai} and {brain})?",
                {"ai": "Nacre AI", "brain": "Brain"},
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
        profile["components"] = [
            c for c in profile["components"] if c not in ("ai", "brain")
        ]
        optional = [value for value in optional if value not in ("ai", "brain")]
        if enabled in ("yes", "y"):
            optional.extend(("ai", "brain"))
    profile["components"] = list(
        dict.fromkeys(["configs", "shell", *profile["components"], *optional])
    )
    return profiles.validate(profile), keyboard, compositor


def missing_packages(root, profile, compositor="hyprland"):
    import package_sessions
    import package_managers

    mapping = _state.read_json_file(root / "packaging/packages.json")
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
    for component in ("ai", "brain"):
        if (
            component in profile["components"]
            and not (root / "deployment" / component).is_dir()
        ):
            packages.append("nacre-" + component)
    registry = package_sessions.registry(root)
    for name in package_sessions.backends(compositor):
        definition = registry[name]
        if definition.get("package"):
            packages.append(definition["package"])
        packages.extend(names.get(definition.get("dependencies"), []))
        if name == "niri" and not definition.get("dependencies"):
            import package_niri

            packages.extend(package_niri.DEPENDENCIES)
    return package_managers.manager(distro(mapping)).missing(
        list(dict.fromkeys(packages))
    )


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
        "--also-compositor", dest="additional_compositors", action="append", default=[]
    )
    parser.add_argument(
        "--install-deps",
        action="store_true",
        help="install missing packages using the distribution package manager",
    )
    parser.add_argument(
        "--review-selected", action="store_true", help=argparse.SUPPRESS
    )
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
    if args.plan:
        profile, keyboard, compositor = selection(
            args, ROOT, Path.home(), interactive and not args.review_selected
        )
    else:
        with _writer.writer_lease(Path.home() / ".config/nacre") as lease:
            lease.ensure_valid()
            profile, keyboard, compositor = selection(
                args, ROOT, Path.home(), interactive and not args.review_selected
            )
    selected_backends = [compositor, *getattr(args, "additional_compositors", [])]
    with tempfile.TemporaryDirectory(prefix="nacre-setup-") as temporary:
        selected = Path(temporary) / "profile.json"
        selected.write_text(json.dumps(profile))
        previous = os.environ.get("NACRE_PROFILE_FILE")
        os.environ["NACRE_PROFILE_FILE"] = str(selected)
        try:
            from releases import package_plan, deploy_package

            missing = missing_packages(ROOT, profile, selected_backends)
            if args.plan:
                plan = package_plan(
                    ROOT,
                    profile,
                    selected_backends,
                    keyboard,
                    validate_config=not missing,
                )
            else:
                with _writer.writer_lease(Path.home() / ".config/nacre") as lease:
                    lease.ensure_valid()
                    plan = package_plan(
                        ROOT,
                        profile,
                        selected_backends,
                        keyboard,
                        validate_config=not missing,
                        lease=lease,
                    )
            print(
                tr(
                    "setup.plan.summary",
                    "Nacre setup: profile={profile}; compositor={compositor}; components={components}",
                    {
                        "profile": profile["id"],
                        "compositor": ", ".join(selected_backends),
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
            if args.plan:
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
                if args.install_deps or interactive:
                    import package_managers

                    mapping = _state.read_json_file(ROOT / "packaging/packages.json")
                    package_managers.manager(distro(mapping)).install(missing)
                    # Dependencies may upgrade Nacre itself. Review/apply through
                    # a fresh interpreter, never a mixture of old/new modules.
                    command = [
                        sys.executable,
                        str(ROOT / "tools/setup.py"),
                        "--review-selected",
                        "--profile",
                        str(selected),
                        "--compositor",
                        compositor,
                    ]
                    for backend in selected_backends[1:]:
                        command += ["--also-compositor", backend]
                    for name, value in keyboard.items():
                        command += ["--" + name, value]
                    if args.yes:
                        command.append("--yes")
                    return subprocess.call(command)
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
            result = deploy_package(ROOT, profile, keyboard, selected_backends)
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
