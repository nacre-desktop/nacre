"""Prepared code remains visible through an already running mount namespace."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / "nacre/shell-tools"
NAMESPACE = """
import subprocess, sys
subprocess.run(['mount', '--bind', sys.argv[1], sys.argv[2]], check=True)
subprocess.run(['mount', '-o', 'remount,bind,ro', sys.argv[2]], check=True)
print('ready', flush=True)
for line in sys.stdin:
    try:
        result = subprocess.run([sys.argv[2] + '/tools/nacre-helper'], capture_output=True, text=True)
        print(str(result.returncode) + ':' + result.stdout.strip(), flush=True)
    except FileNotFoundError:
        print('missing', flush=True)
"""


class SessionDeployTests(unittest.TestCase):
    def test_prepare_refresh_and_rollback_preserve_running_bind(self):
        probe = subprocess.run(
            ["unshare", "--mount", "--propagation", "private", "true"],
            capture_output=True,
            text=True,
        )
        if probe.returncode:
            if os.environ.get("NACRE_REQUIRE_SESSION_MOUNT_TEST") == "1":
                self.fail(probe.stderr)
            self.skipTest("mount namespaces unavailable: " + probe.stderr.strip())
        binary = Path(
            os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/release/nacre-helper")
        ).resolve()
        with tempfile.TemporaryDirectory(prefix="nacre-session-deploy-") as folder:
            base = Path(folder)
            home, backend = base / "home", base / "backend"
            for relative in ("nacre/shell", "nacre/shell-tools", "niri"):
                shutil.copytree(
                    ROOT / relative,
                    backend / relative,
                    ignore=shutil.ignore_patterns(
                        "tests", "__pycache__", "nacre-helper"
                    ),
                )
            shutil.copy2(ROOT / "VERSION", backend / "VERSION")
            subprocess.run(["git", "init", "-q", str(backend)], check=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(backend),
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                    "--allow-empty",
                ],
                check=True,
            )
            config = home / ".config/nacre"
            state = home / ".local/state/nacre"
            config.mkdir(parents=True)
            state.mkdir(parents=True)
            shutil.copy2(ROOT / "profiles/default.json", config / "profile.json")
            (state / "scheme.json").write_text(
                json.dumps(
                    {
                        "colours": {
                            "primary": "#abcdef",
                            "secondary": "#123456",
                            "outlineVariant": "#abcdef",
                            "error": "#123abc",
                        }
                    }
                )
            )
            installed = home / ".local/share/nacre/shell"
            (installed / "bin").mkdir(parents=True)
            (installed / "bin/qs").touch()
            environment = dict(os.environ, HOME=str(home))
            for key in ("XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME"):
                environment.pop(key, None)
            command = [
                str(binary),
                "niri-session-runtime",
                "--backend",
                str(backend),
                "--stock-niri",
                "prepare",
            ]
            helper = backend / "nacre/shell-tools/nacre-helper"

            def version(value):
                helper.write_text("#!/bin/sh\nprintf '%s\\n' " + value + "\n")
                helper.chmod(0o755)

            def prepare():
                subprocess.run(
                    command, env=environment, check=True, capture_output=True
                )

            version("one")
            prepare()
            session = home / ".local/share/nacre/niri-on-pc/session"
            shell = session / "home-view/.local/share/nacre/shell"
            inode = shell.stat().st_ino
            legacy = session.with_name("session.previous")
            shutil.copytree(session, legacy)
            legacy_shell = legacy / "home-view/.local/share/nacre/shell"

            def namespace(source):
                return subprocess.Popen(
                    [
                        "unshare",
                        "--mount",
                        "--propagation",
                        "private",
                        sys.executable,
                        "-c",
                        NAMESPACE,
                        str(source),
                        str(installed),
                    ],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )

            process = namespace(shell)
            old_process = namespace(legacy_shell)
            try:
                self.assertEqual(process.stdout.readline().strip(), "ready")
                self.assertEqual(old_process.stdout.readline().strip(), "ready")

                def resolves(expected):
                    process.stdin.write("run\n")
                    process.stdin.flush()
                    self.assertEqual(process.stdout.readline().strip(), "0:" + expected)
                    self.assertEqual(shell.stat().st_ino, inode)
                    old_process.stdin.write("run\n")
                    old_process.stdin.flush()
                    self.assertEqual(old_process.stdout.readline().strip(), "0:one")

                resolves("one")
                version("two")
                prepare()  # Direct prepare must also be repeatable.
                resolves("two")
                sys.path.insert(0, str(TOOLS))
                try:
                    spec = importlib.util.spec_from_file_location(
                        "session_installer", TOOLS / "install.py"
                    )
                    installer = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(installer)
                finally:
                    sys.path.pop(0)
                with (
                    patch.object(installer, "HOME", home),
                    patch.dict(os.environ, environment, clear=True),
                ):
                    version("three")
                    installer.refresh_niri_session(("two -> three", command, True))
                    resolves("three")
                    version("four")
                    installer.refresh_niri_session(("three -> four", command, True))
                    resolves("four")  # Replacing session.rollback must be harmless.
                    version("failed")
                    run = subprocess.run

                    def fail_after_prepare(argv, **kwargs):
                        run(argv, **kwargs)
                        raise subprocess.CalledProcessError(1, argv)

                    with patch.object(
                        installer.subprocess, "run", side_effect=fail_after_prepare
                    ):
                        with self.assertRaises(subprocess.CalledProcessError):
                            installer.refresh_niri_session(
                                ("four -> failed", command, True)
                            )
                    resolves("four")
                # Reproduce an older deploy's deleted mount and check the
                # documented administrative repair, exclusively in this fake.
                orphan = shell.with_name("orphan")
                shell.rename(orphan)
                shutil.copytree(orphan, shell)
                shutil.rmtree(orphan)
                process.stdin.write("run\n")
                process.stdin.flush()
                self.assertEqual(process.stdout.readline().strip(), "missing")
                for arguments in (
                    ["umount", "--lazy", str(installed)],
                    ["mount", "--bind", str(shell), str(installed)],
                    ["mount", "-o", "remount,bind,ro", str(installed)],
                ):
                    subprocess.run(
                        [
                            "nsenter",
                            "--target",
                            str(process.pid),
                            "--mount",
                            "--",
                            *arguments,
                        ],
                        check=True,
                    )
                inode = shell.stat().st_ino
                resolves("four")
            finally:
                for child in (process, old_process):
                    child.stdin.close()
                    try:
                        child.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=10)
                    child.stdout.close()
                    child.stderr.close()
