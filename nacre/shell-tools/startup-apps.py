#!/usr/bin/env python3
"""Idempotent selected-profile startup; ordinary terminals stay unrestricted."""

import fcntl
import json
import os
import subprocess
import time
import shutil
import importlib.util
from pathlib import Path

HOME = Path.home()


PROFILE_SPEC = importlib.util.spec_from_file_location(
    "startup_profile_policy", Path(__file__).with_name("profile-policy.py")
)
POLICY = importlib.util.module_from_spec(PROFILE_SPEC)
PROFILE_SPEC.loader.exec_module(POLICY)


def plan(clients, profile=None):
    profile = POLICY.load(HOME) if profile is None else POLICY.validate(profile)
    classes = {c["class"].casefold() for c in clients}
    return [
        (
            item["label"],
            item["workspace"],
            item["argv"],
            bool(classes & {name.casefold() for name in item["classes"]})
            or (
                item["label"] == "Brain"
                and any(c.get("title") == "Nacre Brain" for c in clients)
            ),
        )
        for item in profile["startup"]
    ]


def main():
    profile = POLICY.load(HOME)
    if not profile["startup"]:
        return
    lock = (
        Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
        / "nacre-startup-apps.lock"
    )
    with lock.open("w") as guard:
        try:
            fcntl.flock(guard, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        spec = importlib.util.spec_from_file_location(
            "session", Path(__file__).with_name("session-watch.py")
        )
        session = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(session)
        session.readiness()
        clients = json.loads(subprocess.check_output(["hyprctl", "clients", "-j"]))
        for label, workspace, argv, exists in plan(clients, profile):
            if exists or not shutil.which(argv[0]):
                continue
            subprocess.run(
                [
                    "hyprctl",
                    "eval",
                    f"hl.dispatch(hl.dsp.focus({{workspace={workspace},on_current_monitor=true}}))",
                ],
                stdout=subprocess.DEVNULL,
                check=True,
            )
            subprocess.Popen(
                ["systemd-run", "--user", "--scope", "--collect", "--quiet", *argv],
                start_new_session=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            # Wait for a real window/service acknowledgement; never relaunch a slow app.
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                current = json.loads(
                    subprocess.check_output(["hyprctl", "clients", "-j"])
                )
                if (
                    next(
                        (
                            entry[3]
                            for entry in plan(current, profile)
                            if entry[0] == label
                        ),
                        False,
                    )
                    or label == "Brain"
                ):
                    break
                time.sleep(0.2)
        subprocess.run(
            [
                "hyprctl",
                "eval",
                "hl.dispatch(hl.dsp.focus({workspace=1,on_current_monitor=true}))",
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )


if __name__ == "__main__":
    main()
