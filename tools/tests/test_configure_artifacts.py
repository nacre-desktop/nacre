"""Generated Python/QML metadata is not a deployable user configuration."""

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "configure_artifacts", Path(__file__).parents[1] / "configure.py"
)
configure = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(configure)


class ConfigurationArtifactTests(unittest.TestCase):
    def test_source_enumeration_skips_machine_generated_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "hypr/scripts"
            scripts.mkdir(parents=True)
            (scripts / "example.py").write_text("source")
            (scripts / "example.pyc").write_bytes(b"bytecode")
            (scripts / "example.pyo").write_bytes(b"bytecode")
            (scripts / ".qmlls.ini").write_text("machine path")
            cache = scripts / "__pycache__"
            cache.mkdir()
            (cache / "example.cpython-314.pyc").write_bytes(b"bytecode")
            self.assertEqual(
                list(configure.files(root)), [Path(".config/hypr/scripts/example.py")]
            )

    def test_only_owned_generated_copy_is_retired_with_backup(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            home, root = base / "home", base / "source"
            root.mkdir()
            owned = home / ".config/hypr/scripts/__pycache__/owned.pyc"
            owned.parent.mkdir(parents=True)
            owned.write_bytes(b"known bytecode")
            unowned = owned.with_name("user.pyc")
            unowned.write_bytes(b"unowned bytecode")
            manifest = home / ".local/state/nacre/configuration.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_text(
                json.dumps(
                    {
                        str(owned.relative_to(home)): hashlib.sha256(
                            owned.read_bytes()
                        ).hexdigest()
                    }
                )
            )
            backup = configure.apply(home, root)
            self.assertFalse(owned.exists())
            self.assertEqual(unowned.read_bytes(), b"unowned bytecode")
            self.assertEqual(
                (backup / owned.relative_to(home)).read_bytes(), b"known bytecode"
            )

    def test_changed_owned_generated_copy_keeps_drift_refusal(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            home, root = base / "home", base / "source"
            root.mkdir()
            target = home / ".config/hypr/scripts/old.pyc"
            target.parent.mkdir(parents=True)
            target.write_bytes(b"local edit")
            manifest = home / ".local/state/nacre/configuration.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_text(
                json.dumps(
                    {
                        str(target.relative_to(home)): hashlib.sha256(
                            b"previous"
                        ).hexdigest()
                    }
                )
            )
            with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                configure.plan(home, root)
            self.assertEqual(target.read_bytes(), b"local edit")
