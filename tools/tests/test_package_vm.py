"""Focused VM-driver checks; no QEMU, user session or installer is run."""

import contextlib
import importlib.util
import io
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

ARCH = Path(__file__).resolve().parents[2] / "packaging/arch"
with patch.object(sys, "path", [str(ARCH), *sys.path]):
    spec = importlib.util.spec_from_file_location("package_vm", ARCH / "test-vm.py")
    vm = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(vm)


class VMGateTests(unittest.TestCase):
    def test_unrecorded_shell_state_is_a_product_leftover(self):
        paths = {
            "/home/tester/.local/state/bead/ui-health-release.json": [0, 0, 0],
            "/home/tester/.local/state/bead/ui-health-release.lock": [0, 0, 0],
            "/home/tester/.local/state/bead/ui/surface.json": [0, 0, 0],
        }
        self.assertEqual(set(vm.leftovers({}, paths)[1]), set(paths))
        self.assertEqual(vm.leftovers(paths, paths), ({}, []))

    def test_inventory_compares_restoration_and_managed_paths(self):
        path = "/home/tester/.config/gtk-3.0/settings.ini"
        original = [33188, 1000, 1000, "original-hash"]
        edited = [33188, 1000, 1000, "nacre-hash"]
        self.assertEqual(
            vm.leftovers({path: original}, {path: original}, [path]), ({}, [])
        )
        self.assertEqual(
            vm.leftovers({path: original}, {path: edited}, [path])[1], [path]
        )

    def test_product_links_units_and_runtime_files_fail(self):
        paths = {
            "/home/tester/.config/systemd/user/nacre-shell.service": [0, 0, 0],
            "/home/tester/.local/share/nacre/session.json": [0, 0, 0],
            "/home/tester/.config/quickshell/shell.qml": [
                0,
                0,
                0,
                "/usr/share/nacre/gone",
            ],
            "/home/tester/.local/bin/helper": [0, 0, 0, "/usr/lib/nacre/gone"],
            "/home/tester/.local/bin/relative": [
                0,
                0,
                0,
                "../../../../../usr/lib/nacre/gone",
            ],
            "/usr/lib/nacre/orphan": [0, 0, 0],
            "/etc/nacre/orphan": [0, 0, 0],
        }
        self.assertEqual(set(vm.leftovers({}, paths)[1]), set(paths))

    def test_repository_and_dependency_changes_are_recorded(self):
        paths = {
            "/etc/pacman.d/nacre-dev.conf": [0, 0, 0],
            "/etc/pacman.d/gnupg/pubring.gpg": [0, 0, 0],
            "/home/tester/.cache/fontconfig/cache": [0, 0, 0],
        }
        self.assertEqual(vm.leftovers({}, paths), (paths, []))

    def test_command_failure_retains_output(self):
        with patch.object(
            vm.subprocess,
            "run",
            side_effect=subprocess.CalledProcessError(
                7, ["command"], output="diagnostic"
            ),
        ):
            with self.assertRaises(subprocess.CalledProcessError) as error:
                vm.run("command")
        self.assertEqual(error.exception.stdout, "diagnostic")

    def test_stage_failure_stops_and_cleans_exact_vm(self):
        process = MagicMock()
        process.poll.return_value = None
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "proof"
            calls = []

            def fake_run(command, *args, **kwargs):
                argv = [*shlex.split(command), *map(str, args)]
                calls.append(argv)
                if argv[0] == "ssh-keygen":
                    Path(argv[-1] + ".pub").write_text("ssh-ed25519 fixture")
                if argv[0] == "ssh" and "sudo python" in argv[-1]:
                    raise subprocess.CalledProcessError(
                        9, argv, output="inventory failed"
                    )
                return ""

            argv = [
                "test-vm.py",
                "--tier",
                "niri",
                "--packages",
                tmp,
                "--base",
                tmp + "/base",
                "--out",
                str(out),
                "--lock",
                tmp + "/lock",
            ]
            with (
                patch.object(sys, "argv", argv),
                patch.object(vm, "run", fake_run),
                patch.object(vm.subprocess, "Popen", return_value=process),
                patch.object(vm.signal, "signal"),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                self.assertEqual(vm.main(), 1)
            process.terminate.assert_called_once_with()
            process.wait.assert_called_once_with(timeout=15)
            process.kill.assert_not_called()
            table = (out / "stages.txt").read_text()
            self.assertIn("FAIL", table)
            self.assertNotIn("bootstrap-install", table)
            self.assertIn("inventory failed", (out / "failure.txt").read_text())
            self.assertFalse(any("uninstall" in str(call) for call in calls))

    def test_renderer_embeds_frontend_and_keeps_production_trust(self):
        bootstrap = vm.render_bootstrap(vm.ROOT)
        self.assertNotIn("@NACRE_PACMAN_FRONTEND@", bootstrap)
        self.assertIn("--proto '=https'", bootstrap)
        self.assertIn("fingerprint=886CBE272BA30B44DBB55F66714CFAAC876C349E", bootstrap)


if __name__ == "__main__":
    unittest.main()
