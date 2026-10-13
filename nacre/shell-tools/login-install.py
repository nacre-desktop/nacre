#!/usr/bin/env python3
"""Install a root-owned theme with user-writable appearance data, without restarting SDDM."""

import argparse
import datetime
import json
import os
import pwd
import shutil
from pathlib import Path

FILES = (
    "Main.qml",
    "Logo.qml",
    "LogoData.js",
    "translation.js",
    "messages.js",
    "theme.conf",
    "metadata.desktop",
)


def install(source, user, prefix=Path("/"), uid=None, gid=None):
    if uid is None:
        account = pwd.getpwnam(user)
        uid = account.pw_uid
        gid = account.pw_gid
    theme = prefix / "usr/share/sddm/themes/nacre"
    base = prefix / "var/lib/nacre/login"
    appearance = base / "appearance"
    config = prefix / "etc/sddm.conf.d/90-nacre-theme.conf"
    backup = (
        base
        / "backups"
        / datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    )
    backup.mkdir(parents=True, mode=0o700)
    entries = []
    for target in (theme, config):
        if target.exists() or target.is_symlink():
            saved = backup / target.relative_to(prefix)
            saved.parent.mkdir(parents=True, exist_ok=True)
            if target.is_dir():
                shutil.copytree(target, saved, symlinks=True)
            else:
                shutil.copy2(target, saved, follow_symlinks=False)
            entries.append(dict(target=str(target), backup=str(saved)))
        else:
            entries.append(dict(target=str(target), backup=None))
    (backup / "manifest.json").write_text(json.dumps(entries, indent=2))
    theme.mkdir(parents=True, exist_ok=True)
    base.chmod(0o755)
    appearance.mkdir(exist_ok=True)
    os.chown(appearance, uid, gid)
    appearance.chmod(0o755)
    for name in FILES:
        target = theme / name
        target.unlink(missing_ok=True)
        shutil.copyfile(source / name, target)
        target.chmod(0o644)
    link = theme / "theme.conf.user"
    link.unlink(missing_ok=True)
    link.symlink_to(appearance / "theme.conf")
    theme.chmod(0o755)
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text("[Theme]\nCurrent=nacre\n")
    config.chmod(0o644)
    # This installer never changes Autologin/PAM/session startup or restarts the display manager.
    return backup


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--user", required=True)
    parser.add_argument(
        "--theme", type=Path, default=Path(__file__).resolve().parents[1] / "login"
    )
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit(
            "Administrator authentication is required to install the system login theme."
        )
    for name in FILES:
        if not (args.theme / name).is_file():
            raise SystemExit("Missing theme file: " + name)
    backup = install(args.theme, args.user)
    print(
        "Nacre login theme installed. Rollback manifest: "
        + str(backup / "manifest.json")
    )
