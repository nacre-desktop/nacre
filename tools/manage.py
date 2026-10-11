#!/usr/bin/env python3
"""Explicit component deployment for an existing supported Nacre host."""

import argparse
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="apply the reviewed plan")
    parser.add_argument(
        "--migrate-owned",
        action="store_true",
        help="convert old repository-linked config into private installed copies",
    )
    parser.add_argument(
        "--component",
        action="append",
        choices=["configs", "apps", "shell", "brain", "ai"],
        help="repeat to select components; default configs, shell, brain",
    )
    parser.add_argument(
        "--profile",
        help="default or a profile JSON path; existing installed profile is retained when omitted",
    )
    parser.add_argument(
        "--with", dest="optional", action="append", choices=["ai", "brain"], default=[]
    )
    parser.add_argument(
        "--prepare",
        type=Path,
        help="stage an empty-home installation without starting services or changing the live desktop",
    )
    args = parser.parse_args()
    import importlib.util
    import os
    import json

    spec = importlib.util.spec_from_file_location(
        "profile_policy", ROOT / "nacre/shell-tools/profile-policy.py"
    )
    policy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(policy)
    profile_path = (
        ROOT / "profiles/default.json"
        if args.profile == "default"
        else (Path(args.profile).resolve() if args.profile else None)
    )
    profile = (
        policy.validate(json.loads(profile_path.read_text()))
        if profile_path
        else policy.load()
    )
    profile["components"] = list(
        dict.fromkeys([*profile["components"], *args.optional])
    )
    if profile_path or args.optional:
        import tempfile

        descriptor, name = tempfile.mkstemp(prefix="nacre-profile-", suffix=".json")
        os.close(descriptor)
        Path(name).write_text(json.dumps(profile))
        os.environ["NACRE_PROFILE_FILE"] = name
    if args.prepare:
        from prepare import prepare

        prepare(ROOT, args.prepare, profile)
        return
    components = args.component or profile["components"]
    commands = {
        "configs": [
            "python3",
            str(ROOT / "tools/configure.py"),
            *(["--migrate-owned"] if args.migrate_owned else []),
            "--apply",
        ],
        "apps": [
            "python3",
            str(ROOT / "tools/configure.py"),
            "--app-routes-only",
            "--apply",
        ],
        "shell": [
            "python3",
            str(ROOT / "nacre/shell-tools/install.py"),
            *(
                ["--code-only"]
                if (Path.home() / ".local/share/nacre/shell/bin/qs").exists()
                else []
            ),
        ],
        "brain": ["python3", str(ROOT / "brain/install.py")],
        "ai": ["python3", str(ROOT / "ai/install-component.py")],
    }
    for name in components:
        print(name + ": " + " ".join(commands[name]), flush=True)
    if not args.apply:
        print(
            "Plan only. Read docs/maintenance.md and tools/check.py output before applying."
        )
        return
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "tools/releases.py"),
            "deploy",
            str(ROOT),
            *(["--migrate-owned"] if args.migrate_owned else []),
            *[part for name in components for part in ("--component", name)],
        ],
        check=True,
    )


if __name__ == "__main__":
    main()
