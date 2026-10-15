"""Exercise installed path defaults and isolated-helper override precedence in Qt."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SHELL = Path(__file__).resolve().parents[2] / "shell"


class PathTests(unittest.TestCase):
    def test_native_paths_and_overrides(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            (target / "fixtures").mkdir()
            (target / "fixtures/Environment.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var values:({HOME:"/test home"});function env(key){return values[key]||""}}'
            )
            (target / "fixtures/qmldir").write_text(
                "singleton Environment 1.0 Environment.qml\n"
            )
            (target / "Paths.qml").write_text(
                (SHELL / "utils/NacrePaths.qml")
                .read_text()
                .replace("pragma Singleton\n", "")
                .replace("import Quickshell", 'import "fixtures"')
                .replace("Quickshell.env(", "Environment.env(")
            )
            (target / "tst_paths.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
Item {
    Paths {id: paths}
    TestCase {
        name: "InstalledPaths"; when: windowShown
        function test_defaults() {
            Environment.values = {HOME: "/test home"};
            compare(paths.share, "/test home/.local/share/nacre");
            compare(paths.tools, paths.share + "/shell/tools");
            compare(paths.shellBin, paths.share + "/shell/bin");
            compare(paths.helperBinary, paths.tools + "/nacre-helper");
            compare(paths.compositorTools, paths.tools);
            compare(paths.pearlTools, paths.tools);
            compare(paths.dashboardTools, paths.tools);
            compare(paths.config, "file:///test home/.config/nacre");
            compare(paths.configRoot, "file:///test home/.config");
            compare(paths.cacheRoot, "file:///test home/.cache");
            compare(paths.cache, "file:///test home/.cache/nacre");
            compare(paths.state, "file:///test home/.local/state/nacre");
            compare(paths.userBin, "/test home/.local/bin");
        }
        function test_overrides() {
            Environment.values = {HOME: "/test home", NACRE_PEARL_TOOLS: "/pearl", NACRE_DASHBOARD_TOOLS: "/dashboard", XDG_CONFIG_HOME: "/config", XDG_STATE_HOME: "/state", XDG_CACHE_HOME: "/cache", XDG_DATA_HOME: "/data"};
            compare(paths.compositorTools, "/pearl");
            compare(paths.pearlTools, "/pearl");
            compare(paths.dashboardTools, "/dashboard");
            compare(paths.tools, "/test home/.local/share/nacre/shell/tools");
            compare(paths.config, "file:///config/nacre");
            compare(paths.configRoot, "file:///config");
            compare(paths.cacheRoot, "file:///cache");
            compare(paths.cache, "file:///cache/nacre");
            compare(paths.state, "file:///state/nacre");
            Environment.values = Object.assign({}, Environment.values, {NACRE_COMPOSITOR_TOOLS: "/compositor"});
            compare(paths.compositorTools, "/compositor");
            compare(paths.pearlTools, "/pearl");
        }
    }
}""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_installed_share_locations_have_one_owner(self):
        for path in SHELL.rglob("*.qml"):
            if path.name != "NacrePaths.qml":
                self.assertNotIn("/.local/share/nacre", path.read_text(), str(path))

    def test_no_silent_javascript_catches(self):
        for path in SHELL.rglob("*"):
            if path.suffix in (".qml", ".js"):
                self.assertNotRegex(
                    path.read_text(), r"catch\s*\([^)]*\)\s*\{\s*\}", str(path)
                )
