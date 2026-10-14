"""A candidate helper replaces its pathname without truncating a running binary."""

import importlib.util
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    "native_helper_installer", ROOT / "nacre/shell-tools/install.py"
)
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class NativeHelperInstallTests(unittest.TestCase):
    def test_upgrade_removes_retired_helper_but_preserves_unported_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            tools = Path(directory)
            for name in (
                "pearl-state.py",
                "pearl-window.py",
                "nacre-helper",
                "private.json",
            ):
                (tools / name).write_text("existing")
            installer.retire_native_helpers(tools)
            self.assertFalse((tools / "pearl-state.py").exists())
            for name in ("pearl-window.py", "nacre-helper", "private.json"):
                self.assertEqual((tools / name).read_text(), "existing")
            installer.retire_native_helpers(tools)

    def test_atomic_candidate_keeps_existing_service_alive(self):
        binary = Path(
            os.environ.get("NACRE_HELPER_BINARY") or ROOT / "target/debug/nacre-helper"
        )
        self.assertTrue(
            binary.is_file(), "Build the native helper before repository checks"
        )
        with tempfile.TemporaryDirectory(prefix="native-install-") as directory:
            home = Path(directory)
            old = home / "nacre-helper"
            shutil.copy2(binary, old)
            env = {
                "HOME": str(home),
                "PATH": "",
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(home / "absent-bus"),
            }
            process = subprocess.Popen(
                [str(old), "serve"],
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                self.assertTrue(
                    select.select([process.stdout], [], [], 3)[0],
                    "Native service startup timed out",
                )
                self.assertEqual(
                    json.loads(process.stdout.readline()), {"ready": True, "version": 1}
                )
                inode = old.stat().st_ino
                for ident in [1, 2]:
                    process.stdin.write(
                        json.dumps(
                            {
                                "id": ident,
                                "tool": "notification-history",
                                "arguments": ["load"],
                                "input": None,
                            }
                        )
                        + "\n"
                    )
                    process.stdin.flush()
                    self.assertTrue(
                        select.select([process.stdout], [], [], 3)[0],
                        "Native service response timed out",
                    )
                    result = json.loads(process.stdout.readline())
                    self.assertEqual(result["id"], ident)
                    self.assertEqual(result["output"], "[]\n")
                    self.assertEqual(result["code"], 0)
                    if ident == 1:
                        installer.install_native_helper(binary, old)
                        self.assertNotEqual(old.stat().st_ino, inode)
                        self.assertEqual(
                            Path(f"/proc/{process.pid}/exe").stat().st_ino, inode
                        )
                        self.assertEqual(old.read_bytes(), binary.read_bytes())
                        self.assertEqual(
                            old.stat().st_mode & 0o777, binary.stat().st_mode & 0o777
                        )
                        self.assertEqual(list(home.glob(".nacre-helper.*")), [])
                process.stdin.close()
                self.assertEqual(process.wait(timeout=3), 0)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=3)
                for stream in (process.stdin, process.stdout, process.stderr):
                    if stream is not None:
                        stream.close()

    def test_installing_the_same_owned_binary_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "helper"
            path.write_bytes(b"existing executable")
            inode = path.stat().st_ino
            installer.install_native_helper(path, path)
            self.assertEqual(path.stat().st_ino, inode)
            self.assertEqual(path.read_bytes(), b"existing executable")
