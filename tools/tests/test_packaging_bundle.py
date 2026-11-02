"""Private staging proof for the imported native UI shell bundle."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


STAGE_PATH = Path(__file__).resolve().parents[2] / "packaging/arch/stage.py"
SPEC = importlib.util.spec_from_file_location("nacre_stage", STAGE_PATH)
stage_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage_module)


class UiShellBundleTests(unittest.TestCase):
    def test_private_bundle_and_notices_keep_the_installer_layout(self):
        with tempfile.TemporaryDirectory(prefix="nacre-package-bundle-") as temp:
            root = Path(temp)
            bundle = root / "artifacts/ui-shell"
            binaries = bundle / "bin"
            binaries.mkdir(parents=True)
            for name in (
                "nacre-shell",
                "bead-wallpaper-video",
                "nacre-wallpaper-decode",
            ):
                executable = binaries / name
                executable.write_text("fixture: " + name + "\n")
                executable.chmod(0o755)
            (bundle / "shell/bar").mkdir(parents=True)
            (bundle / "shell/palette.json").write_text("{}\n")
            (bundle / "shell/bar/main.luau").write_text("return {}\n")
            product = root / "source/bead-shell"
            product.mkdir(parents=True)
            for name in ("LICENSE.md", "LICENSE-MIT", "LICENSE-GPL-3.0"):
                (product / name).write_text("notice: " + name + "\n")
            dest = root / "pkg"

            stage_module.stage_ui_shell(
                bundle, dest / "usr/lib/nacre/bead-shell", product
            )

            private = dest / "usr/lib/nacre/bead-shell"
            for name in (
                "nacre-shell",
                "bead-wallpaper-video",
                "nacre-wallpaper-decode",
            ):
                path = private / "bin" / name
                self.assertEqual(path.read_text(), "fixture: " + name + "\n")
                self.assertTrue(path.stat().st_mode & 0o111)
            self.assertEqual(
                (private / "shell/bar/main.luau").read_text(), "return {}\n"
            )
            self.assertEqual((private / "shell/palette.json").read_text(), "{}\n")
            self.assertFalse((dest / "usr/bin/nacre-shell").exists())
            for name in ("LICENSE.md", "LICENSE-MIT", "LICENSE-GPL-3.0"):
                self.assertEqual(
                    (dest / "usr/share/licenses/nacre/bead-shell" / name).read_text(),
                    "notice: " + name + "\n",
                )


if __name__ == "__main__":
    unittest.main()
