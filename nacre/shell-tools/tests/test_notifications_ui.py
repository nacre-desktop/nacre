from qml_source import install_i18n

"""Production notification cards and stack with service-owned fixture entries."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from qml_source import install_foundation_interaction, remove_objects

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


def install_history_owner(target, fixtures):
    source = (SHELL / "services/NacreNotifs.qml").read_text()
    source = source.replace("pragma Singleton", "").replace("Singleton {", "Item {")
    for module in (
        "Quickshell.Services.Notifications",
        "Quickshell.Io",
        "Quickshell",
        "qs.widgets",
        "qs.config",
    ):
        source = source.replace("import " + module, 'import "fixtures"')
    source = source.replace('Quickshell.env("HOME")', '"/fixture"')
    source = remove_objects(source, r"\b(?:IpcHandler|NacreShortcut)\s*\{")
    (target / "NotificationHistory.qml").write_text(source)
    (target / "NacreNotificationEntry.qml").write_text(
        'import "fixtures"\n'
        + (SHELL / "services/NacreNotificationEntry.qml").read_text()
    )
    definitions = {
        "NacreHelperTask": "property var command:[];property bool running:false;property bool stdinEnabled:false;signal started();signal output(string text);signal exited(int code,int status);function write(text){}",
        "FileView": 'property string path:"";property bool printErrors:false;signal loaded();function text(){return ""}',
        "NotificationServer": "property bool keepOnReload:false;property bool actionsSupported:false;property bool imageSupported:false;property bool persistenceSupported:false;property bool bodyMarkupSupported:false;signal notification(var notification)",
        "NacrePanelState": 'property var screens:({});property var panels:({});property var providerViews:[];property bool settingsVisible:false;property string settingsPage:""',
    }
    with (fixtures / "qmldir").open("a") as manifest:
        for name, body in definitions.items():
            singleton = name == "NacrePanelState"
            (fixtures / (name + ".qml")).write_text(
                ("pragma Singleton\n" if singleton else "")
                + "import QtQuick\nQtObject {"
                + body
                + "}"
            )
            manifest.write(
                "\n"
                + ("singleton " if singleton else "")
                + name
                + " 1.0 "
                + name
                + ".qml\n"
            )


class NotificationUITests(unittest.TestCase):
    def test_cards_and_popup_stack(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            for name in ("NacreNotice", "NacreNotificationStack"):
                source = (SHELL / "modules/notifications" / (name + ".qml")).read_text()
                source = source.replace("import Quickshell", "")
                source = source.replace("import qs.widgets", 'import "fixtures"')
                source = source.replace("import qs.services", "")
                source = source.replace("import qs.config", "")
                source = source.replace(
                    "Quickshell.iconPath(root.modelData.appIcon, true)", '""'
                )
                (target / (name + ".qml")).write_text(source)
            for name in ("NacreSurface", "NacreText", "ActionButton", "FastScroll"):
                source = (SHELL / "widgets" / (name + ".qml")).read_text()
                (fixtures / (name + ".qml")).write_text(
                    source.replace("import qs.services", 'import "."')
                )
            (fixtures / "BrandLogo.qml").write_text(
                'import QtQuick\nItem {property bool notifications:false;property color background:"black"}'
            )
            (fixtures / "NacreIcon.qml").write_text("import QtQuick\nText {}")
            (fixtures / "NacreNotifications.qml").write_text(
                (SHELL / "config/NacreNotifications.qml").read_text()
            )
            (fixtures / "NacreNotifs.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var popups:[];property string dismissed:"";function dismiss(entry){dismissed=entry.key}}'
            )
            with (fixtures / "qmldir").open("a") as output:
                output.write(
                    "\nBrandLogo 1.0 BrandLogo.qml\nNacreIcon 1.0 NacreIcon.qml\nsingleton NacreNotifications 1.0 NacreNotifications.qml\nsingleton NacreNotifs 1.0 NacreNotifs.qml\n"
                )
            shutil.copy2(
                ROOT / "tests/notification-qml/tst_notifications.qml",
                target / "tst_notifications.qml",
            )
            install_i18n(fixtures)
            install_history_owner(target, fixtures)
            shutil.copy2(
                ROOT / "tests/notification-qml/tst_history_clear.qml",
                target / "tst_history_clear.qml",
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
