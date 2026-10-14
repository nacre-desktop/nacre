"""Package-check cleanup must wait for private mounts without traversing them."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "package_check", ROOT / "packaging/arch/check.py"
)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class PackageCheckCleanupTests(unittest.TestCase):
    def test_mountinfo_paths_are_decoded_and_scoped_to_the_private_directory(self):
        with tempfile.TemporaryDirectory() as folder:
            info = Path(folder) / "mountinfo"
            info.write_text(
                "1 2 0:1 / /run/other rw - fuse.portal portal rw\n2 1 0:2 / /tmp/private\\040runtime/doc rw - fuse.portal portal rw\n3 1 0:3 / /tmp/private\\040runtime-unrelated rw - tmpfs tmpfs rw\n"
            )
            self.assertEqual(
                check.mounts_below(Path("/tmp/private runtime"), info),
                ["/tmp/private runtime/doc"],
            )

    def test_asynchronous_unmount_finishes_before_removal(self):
        directory = Path("/tmp/private-runtime")
        with (
            patch.object(
                check, "mounts_below", side_effect=[[str(directory / "doc")], []]
            ),
            patch.object(check.time, "sleep"),
            patch.object(
                check.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)
            ) as remove,
        ):
            check.cleanup(directory)
        self.assertEqual(remove.call_count, 1)
        self.assertIn("--one-file-system", remove.call_args.args[0])

    def test_stuck_mount_is_fatal_and_never_traversed(self):
        with (
            patch.object(
                check, "mounts_below", return_value=["/tmp/private-runtime/doc"]
            ),
            patch.object(check.subprocess, "run") as remove,
        ):
            with self.assertRaisesRegex(RuntimeError, "did not close cleanly"):
                check.cleanup(Path("/tmp/private-runtime"), timeout=0)
        remove.assert_not_called()
