"""Real floating window bounds tour preloading to the visible color-demo page."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class WelcomePreloadTests(unittest.TestCase):
    def test_native_window_prefetch_page_and_visibility_lifetime(self):
        binary = shutil.which("quickshell")
        if not binary:
            self.skipTest("Quickshell unavailable")
        from PIL import Image

        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            widgets = target / "widgets"
            services = target / "services"
            widgets.mkdir()
            services.mkdir()
            for name in ("NacreWelcomeWindow", "NacreWelcomeView"):
                if name == "NacreWelcomeView":
                    (target / (name + ".qml")).write_text("import QtQuick\nItem {}")
                else:
                    shutil.copy2(SHELL / "modules/welcome" / (name + ".qml"), target)
            shutil.copy2(SHELL / "widgets/NacreIdleLoader.qml", widgets)
            (widgets / "NacreAppHeader.qml").write_text(
                'import QtQuick\nItem {property string title:"";property string section:"";signal moveRequested}'
            )
            (widgets / "NacreTokens.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property color body:"#123456"}'
            )
            (widgets / "qmldir").write_text(
                "singleton NacreTokens 1.0 NacreTokens.qml\nNacreIdleLoader 1.0 NacreIdleLoader.qml\nNacreAppHeader 1.0 NacreAppHeader.qml\n"
            )
            Image.new("RGB", (64, 48), "red").save(target / "poster.png")
            demos = [{"poster": str(target / "poster.png")} for _ in range(4)]
            (services / "NacreWelcomeApp.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property bool visible:true;property bool active:true;property var window:null;property string page:"welcome";property var demoEntries:'
                + json.dumps(demos)
                + ";function close(){visible=false;active=false}}"
            )
            (services / "NacreHyprland.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var focusedMonitor:null}"
            )
            (services / "qmldir").write_text(
                "singleton NacreWelcomeApp 1.0 NacreWelcomeApp.qml\nsingleton NacreHyprland 1.0 NacreHyprland.qml\n"
            )
            (target / "shell.qml").write_text("""import QtQuick
import Quickshell
import qs.services
ShellRoot {
    NacreWelcomeWindow { id: window }
    Timer {
        property int step: 0
        interval: 100
        running: true
        repeat: true
        onTriggered: {
            const posters = window.contentItem.children.find(item => item.objectName === "welcomePosterPreloads");
            if (!posters) throw new Error("Production preload repeater missing");
            const expected = [0,4,0,0,4][step];
            if (posters.count !== expected) throw new Error("Incorrect preload count at step " + step + ": " + posters.count);
            if (step === 0) NacreWelcomeApp.page = "colors";
            else if (step === 1) NacreWelcomeApp.page = "help";
            else if (step === 2) { NacreWelcomeApp.page = "colors"; NacreWelcomeApp.active = false; NacreWelcomeApp.visible = false; }
            else if (step === 3) { NacreWelcomeApp.active = true; NacreWelcomeApp.visible = true; }
            else { console.log("WELCOME_PRELOAD_OK"); Qt.quit(); }
            step++;
        }
    }
}
""")
            runtime = target / "runtime"
            runtime.mkdir(mode=0o700)
            result = subprocess.run(
                [binary, "-p", str(target), "-n"],
                env={
                    **os.environ,
                    "HOME": str(target / "home"),
                    "XDG_RUNTIME_DIR": str(runtime),
                    "XDG_CONFIG_HOME": str(target / "config"),
                    "XDG_CACHE_HOME": str(target / "cache"),
                    "XDG_DATA_HOME": str(target / "data"),
                    "QT_QPA_PLATFORM": "offscreen",
                },
                capture_output=True,
                text=True,
                timeout=10,
            )
            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertIn("WELCOME_PRELOAD_OK", output)
            self.assertNotIn("ERROR", output)
            self.assertNotIn("TypeError", output)
