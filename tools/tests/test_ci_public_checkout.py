import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.ci_public_checkout import restore


class PublicCheckoutTests(unittest.TestCase):
    def test_restores_tracked_modes_without_touching_private_or_symlink_data(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "data").write_text("public")
            (root / "run").write_text("#!/bin/sh\n")
            os.chmod(root / "data", 0o644)
            os.chmod(root / "run", 0o755)
            (root / "link").symlink_to("data")
            subprocess.run(
                ["git", "-C", str(root), "add", "data", "run", "link"], check=True
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(root),
                    "-c",
                    "user.name=test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                ],
                check=True,
            )
            secret = root / ".git" / "fake-secret"
            secret.write_text("secret")
            os.chmod(secret, 0o600)
            private = root / "private-untracked"
            private.write_text("private")
            os.chmod(private, 0o600)
            os.chmod(root / "data", 0o600)
            os.chmod(root / "run", 0o700)

            self.assertEqual(restore(root), 2)

            self.assertEqual((root / "data").stat().st_mode & 0o777, 0o644)
            self.assertEqual((root / "run").stat().st_mode & 0o777, 0o755)
            self.assertTrue((root / "link").is_symlink())
            self.assertEqual(private.stat().st_mode & 0o777, 0o600)
            self.assertEqual(secret.stat().st_mode & 0o777, 0o600)


if __name__ == "__main__":
    unittest.main()
