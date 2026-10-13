"""Only the shared path singleton may read HOME in desktop Qt sources."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "nacre_source_check", Path(__file__).resolve().parents[1] / "check.py"
)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class HomePathGuardTests(unittest.TestCase):
    def test_rejects_home_lookups_in_qml_and_javascript(self):
        for suffix, source in (
            (".qml", 'Item {property string path: Quickshell.env("HOME")}'),
            (".js", "const path = Quickshell . env ( 'HOME' );"),
        ):
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                target = root / ("nacre/shell/Other" + suffix)
                target.parent.mkdir(parents=True)
                target.write_text(source)
                with self.assertRaisesRegex(RuntimeError, "HOME lookup outside"):
                    check.check_home_paths(root)

    def test_only_the_actual_singleton_is_exempt(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            owner = root / "nacre/shell/utils/NacrePaths.qml"
            owner.parent.mkdir(parents=True)
            owner.write_text('QtObject {property string home: Quickshell.env("HOME")}')
            other = root / "nacre/shell/Other.qml"
            other.write_text("Item {property string path: NacrePaths.config}")
            check.check_home_paths(root)
            spoof = root / "nacre/shell/services/NacrePaths.qml"
            spoof.parent.mkdir()
            spoof.write_text(owner.read_text())
            with self.assertRaisesRegex(RuntimeError, "HOME lookup outside"):
                check.check_home_paths(root)
