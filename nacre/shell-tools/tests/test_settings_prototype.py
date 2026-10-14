"""Exercise the new renderer unchanged with isolated providers in native Qt."""

import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

from qml_fixtures import run_qml, stage_qml, install_foundation_interaction

ROOT = Path(__file__).resolve().parents[3]
SHELL = ROOT / "nacre/shell"
sys.path.insert(0, str(ROOT / "tools"))
from settings_preview_fixtures import stage  # noqa: E402


class SettingsPrototypeTests(unittest.TestCase):
    def test_native_rows_search_routes_and_editors(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test is unavailable")
        with tempfile.TemporaryDirectory(prefix="nacre-settings-test-") as directory:
            target = Path(directory)
            demo = stage(target / "demo")
            fixtures = target / "fixtures"
            shutil.copytree(demo / "services", fixtures)
            shutil.copy2(demo / "wallpaper-demo.svg", target / "wallpaper-demo.svg")
            shutil.rmtree(demo)
            for path in (SHELL / "modules/settings/prototype").glob("*.qml"):
                stage_qml(path, target / (path.stem + ".qml"))
            for folder, names in {
                "widgets": [
                    "NacreSurface",
                    "NacreText",
                    "NacreTextField",
                    "NacreRangeSlider",
                    "NacreToggleSwitch",
                    "NacreSelect",
                    "NacreFolderEntry",
                    "NacreIcon",
                    "ActionButton",
                    "NacreScrollBar",
                    "FastScroll",
                ],
                "modules/dashboard": [
                    "NacreDashboardCustomize",
                    "NacreDashboardCustomizeSection",
                    "NacreDashboardToggleRow",
                    "NacreDashboardExtrasOptions",
                    "NacreDashboardNavigation",
                ],
                "modules/controlcenter": [
                    "NacreControlEdit",
                    "NacreControlAddOption",
                    "NacreControlCatalog",
                    "NacreControlTile",
                    "NacreControlButton",
                ],
            }.items():
                for name in names:
                    stage_qml(
                        SHELL / folder / (name + ".qml"), fixtures / (name + ".qml")
                    )
            install_foundation_interaction(fixtures, SHELL / "widgets")
            schema_root = SHELL / "modules/settings/registry"
            manifest = json.loads((schema_root / "manifest.json").read_text())
            documents = [
                json.loads((schema_root / name).read_text())
                for name in manifest["fragments"]
            ]
            messages = json.loads((SHELL / "i18n/en.json").read_text())
            (fixtures / "RegistryData.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var documents:"
                + json.dumps(documents)
                + ";property var messages:"
                + json.dumps(messages)
                + "}"
            )
            (fixtures / "NacreI18n.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {function tr(id,fallback,args){let text=RegistryData.messages[id] || fallback || id;for(const key of Object.keys(args || {}))text=text.replace("{"+key+"}",args[key]);return text}function option(domain,value){return value}}'
            )
            shutil.copy2(
                ROOT
                / "nacre/shell-tools/tests/settings-prototype-qml/tst_prototype.qml",
                target / "tst_prototype.qml",
            )
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=40,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
