#!/usr/bin/env python3
"""Capture the real terminal pager against inert recipes on a private X server."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import signal
import shutil
import struct
import sys
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--xvfb", default=shutil.which("Xvfb"))
    parser.add_argument(
        "--helper",
        type=Path,
        default=Path(
            os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/release/nacre-helper")
        ),
    )
    args = parser.parse_args()
    if not args.xvfb:
        raise SystemExit(
            "A private Xvfb executable is required; no desktop capture fallback"
        )
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="nacre-aur-gallery-") as folder:
        root = Path(folder)
        env = dict(os.environ)
        for key, suffix in [
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_RUNTIME_DIR", "runtime"),
        ]:
            path = root / suffix
            path.mkdir(mode=0o700)
            env[key] = str(path)
        for key in [
            "WAYLAND_DISPLAY",
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "DISPLAY",
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
        ]:
            env.pop(key, None)
        env.update(
            LIBGL_ALWAYS_SOFTWARE="1",
            KITTY_DISABLE_WAYLAND="1",
            DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent",
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent",
            GIT_CONFIG_GLOBAL="/dev/null",
            GIT_CONFIG_NOSYSTEM="1",
            GIT_AUTHOR_DATE="2000-01-01T00:00:00Z",
            GIT_COMMITTER_DATE="2000-01-01T00:00:00Z",
        )
        display = next(
            number
            for number in range(240, 300)
            if not Path(f"/tmp/.X{number}-lock").exists()
            and not Path(f"/tmp/.X11-unix/X{number}").exists()
        )
        env["DISPLAY"] = ":" + str(display)
        authority = root / "Xauthority"
        # FamilyWild cookie applies only to this isolated, non-TCP display.
        cookie = secrets.token_bytes(16)
        fields = [b"", str(display).encode(), b"MIT-MAGIC-COOKIE-1", cookie]
        authority.write_bytes(
            struct.pack(">H", 65535)
            + b"".join(struct.pack(">H", len(field)) + field for field in fields)
        )
        authority.chmod(0o600)
        env["XAUTHORITY"] = str(authority)
        recipe = root / "sample-editor"
        recipe.mkdir()
        (recipe / ".SRCINFO").write_text(
            "pkgbase = sample-editor\n\tpkgname = sample-editor\n\tpkgver = 2\n\tpkgrel = 1\n\tarch = any\n"
        )
        (recipe / "PKGBUILD").write_text(
            "# Inert gallery fixture. The gallery never executes this recipe.\npkgname=sample-editor\npkgver=2\npkgrel=1\narch=('any')\nsource=('https://example.invalid/editor-2.tar.gz')\nsha256sums=('0000000000000000000000000000000000000000000000000000000000000000') # Fixture only\npackage() { printf 'fixture only\\n'; }\n"
        )
        (recipe / "fix.patch").write_text(
            "--- a/README\n+++ b/README\n@@ -1 +1 @@\n-old fixture\n+new fixture\n"
        )
        for command in [
            ["git", "init", "--quiet", str(recipe)],
            ["git", "-C", str(recipe), "add", "."],
            [
                "git",
                "-C",
                str(recipe),
                "-c",
                "user.name=Recipe Fixture",
                "-c",
                "user.email=fixture@example.org",
                "commit",
                "--quiet",
                "-m",
                "Inert review fixture",
            ],
        ]:
            subprocess.run(command, env=env, check=True, capture_output=True)
        # The production Inspector uses the same document and secure pager as installation.
        server = subprocess.Popen(
            [
                "nice",
                "-n",
                "10",
                args.xvfb,
                env["DISPLAY"],
                "-screen",
                "0",
                "1200x820x24",
                "-nolisten",
                "tcp",
                "-auth",
                str(authority),
            ],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        try:
            deadline = time.monotonic() + 10
            while not Path(f"/tmp/.X11-unix/X{display}").exists():
                if server.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError("Private X server did not start")
                time.sleep(0.05)
            wrapper = root / "inspect-wrapper.py"
            wrapper.write_text(
                "import subprocess,sys\nfrom pathlib import Path\nroot=Path(sys.argv[1])\nwith (root/'inspect-error.txt').open('wb') as error:\n child=subprocess.Popen(sys.argv[2:],stderr=error)\n (root/'inspect-started').write_text('started')\n status=child.wait()\n (root/'inspect-exited').write_text(str(status))\n"
            )
            for changed in [False, True]:
                if changed:
                    record = root / "state/nacre/aur-review/recipes/sample-editor.json"
                    record.parent.mkdir(parents=True)
                    record.write_text(
                        json.dumps(
                            {
                                "version": 1,
                                "digest": "previous-reviewed-recipe",
                                "document": "=== PKGBUILD ===\npkgver=1\nsource=('https://example.invalid/editor-1.tar.gz')\n",
                            }
                        )
                    )
                for marker in [
                    "inspect-started",
                    "inspect-exited",
                    "inspect-error.txt",
                ]:
                    (root / marker).unlink(missing_ok=True)
                terminal = subprocess.Popen(
                    [
                        "dbus-run-session",
                        "--",
                        "nice",
                        "-n",
                        "10",
                        "kitty",
                        "--config",
                        "NONE",
                        "--title",
                        "Nacre AUR review fixture",
                        "-o",
                        "font_family=DejaVu Sans Mono",
                        "-o",
                        "font_size=11",
                        "-o",
                        "initial_window_width=1200",
                        "-o",
                        "initial_window_height=820",
                        "-o",
                        "background=#12151b",
                        "-o",
                        "foreground=#e0e5ed",
                        "-o",
                        "window_padding_width=12",
                        "--",
                        sys.executable,
                        str(wrapper),
                        str(root),
                        str(args.helper.resolve()),
                        "aur-review",
                        "inspect",
                        str(recipe),
                    ],
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.PIPE,
                    start_new_session=True,
                )
                try:
                    deadline = time.monotonic() + 10
                    while (
                        not (root / "inspect-started").exists()
                        and terminal.poll() is None
                        and time.monotonic() < deadline
                    ):
                        time.sleep(0.05)
                    time.sleep(0.5)
                    if (root / "inspect-exited").exists():
                        raise RuntimeError(
                            "Read-only inspector exited before the pager capture: "
                            + (root / "inspect-error.txt").read_text()[-4000:]
                        )
                    if (
                        not (root / "inspect-started").exists()
                        and terminal.poll() is None
                    ):
                        raise RuntimeError(
                            "Private terminal did not start the inspector"
                        )
                    if terminal.poll() is not None:
                        raise RuntimeError(
                            "Private terminal exited: "
                            + terminal.stderr.read().decode(errors="replace")
                        )
                    name = "aur-review-diff.png" if changed else "aur-review-recipe.png"
                    subprocess.run(
                        [
                            sys.executable,
                            "-c",
                            "from PIL import ImageGrab; import sys; ImageGrab.grab(xdisplay=sys.argv[1]).save(sys.argv[2])",
                            env["DISPLAY"],
                            str(args.output / name),
                        ],
                        env=env,
                        check=True,
                        timeout=15,
                    )

                finally:
                    try:
                        os.killpg(terminal.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    terminal.wait(timeout=10)
            if (recipe / "should-never-exist").exists():
                raise RuntimeError("The fixture recipe was executed")
        finally:
            server.terminate()
            server.wait(timeout=10)
    receipt = {
        path.name: hashlib.sha256(path.read_bytes()).hexdigest()
        for path in args.output.glob("*.png")
    }
    (args.output / "receipt.json").write_text(
        json.dumps(
            {
                "kind": "real Kitty/less on private Xvfb",
                "inertRecipes": True,
                "buildsOrInstalls": False,
                "captures": receipt,
            },
            indent=2,
        )
        + "\n"
    )
    print("AUR pager gallery captured without builds, installs or live desktop access.")


if __name__ == "__main__":
    main()
