"""Public bundle format/provenance and the real CLI payload boundary."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "nacre/shell-tools/theme-bundles"
BINARY = Path(os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper"))


class ThemeBundleTests(unittest.TestCase):
    @unittest.skipUnless(
        importlib.util.find_spec("jsonschema"), "Developer jsonschema required"
    )
    def test_catalog_schema_and_per_asset_provenance(self):
        import jsonschema

        schema = json.loads((ASSETS / "schema.json").read_text())
        document = json.loads((ASSETS / "catalog.json").read_text())
        jsonschema.Draft202012Validator(schema).validate(document)
        for bundle in document["bundles"]:
            self.assertTrue((ASSETS / bundle["upstream"]["licenseFile"]).is_file())
            self.assertEqual(
                {row["mode"] for row in bundle["wallpapers"]}, {"dark", "light"}
            )
            bad = json.loads(json.dumps(document))
            bad["bundles"][0]["wallpapers"][0]["file"] = "../unreviewed.png"
            with self.assertRaises(jsonschema.ValidationError):
                jsonschema.Draft202012Validator(schema).validate(bad)

    @unittest.skipUnless(BINARY.is_file(), "Candidate helper required")
    def test_cli_reads_apply_scope_instead_of_defaulting_to_both(self):
        with tempfile.TemporaryDirectory(prefix="nacre-bundle-payload-") as folder:
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
            env.update(
                NACRE_COMPOSITOR="hyprland",
                NACRE_HELPER_TOOLS=str(ASSETS.parent),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent",
            )
            for key in [
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "NIRI_SOCKET",
            ]:
                env.pop(key, None)
            result = subprocess.run(
                [str(BINARY), "software-library", "bundle-apply", "bundle:catppuccin"],
                input=json.dumps({"scope": "invalid-scope", "mode": "light"}),
                text=True,
                capture_output=True,
                env=env,
                timeout=15,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Choose a mode", json.loads(result.stdout)["error"])
            self.assertFalse((root / "config/nacre/wallpaper-picker.json").exists())
