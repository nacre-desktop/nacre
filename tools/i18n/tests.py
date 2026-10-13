"""Native message engine and source guard contracts; never modify live preferences."""

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "nacre_i18n_runtime", ROOT / "nacre/shell-tools/nacre_i18n.py"
)
nacre_i18n = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nacre_i18n)


class TranslationTests(unittest.TestCase):
    def test_actual_catalog_preserves_compositor_error_english_and_external_detail(
        self,
    ):
        messages = nacre_i18n.english()
        with patch.object(nacre_i18n, "language", return_value="en"):
            self.assertEqual(
                nacre_i18n.tr(
                    "ui.compositor.error.minimization-unavailable", "fallback"
                ),
                "Window minimization is unavailable on this compositor.",
            )
            self.assertEqual(
                nacre_i18n.tr("ui.niri.error.ipc-disconnected", "fallback"),
                "Niri IPC disconnected; reconnecting.",
            )
            self.assertEqual(
                nacre_i18n.tr(
                    "ui.niri.error.action-failed",
                    "fallback",
                    {"detail": "IPC_ERROR /tmp/NIRI_SOCKET"},
                ),
                "Niri action failed: IPC_ERROR /tmp/NIRI_SOCKET",
            )
        self.assertIn("ui.niri.error.action-failed", messages)
        with patch.object(nacre_i18n, "language", return_value="pseudo"):
            translated = nacre_i18n.tr(
                "ui.niri.error.action-failed",
                "fallback",
                {"detail": "IPC_ERROR /tmp/NIRI_SOCKET"},
            )
        self.assertIn("IPC_ERROR /tmp/NIRI_SOCKET", translated)
        self.assertNotIn("Niri action failed", translated)

    def test_python_fallback_placeholders_plural_and_pseudo_protection(self):
        with (
            patch.object(
                nacre_i18n,
                "english",
                return_value={
                    "count.windows": {
                        "one": "{count} window",
                        "other": "{count} windows",
                    }
                },
            ),
            patch.object(nacre_i18n, "language", return_value="en"),
        ):
            self.assertEqual(nacre_i18n.tr("missing.key", "Open"), "Open")
            self.assertEqual(
                nacre_i18n.tr("count.windows", "", {"count": 1}), "1 window"
            )
            self.assertEqual(
                nacre_i18n.tr("count.windows", "", {"count": 2}), "2 windows"
            )
            self.assertEqual(nacre_i18n.tr("missing.key", "{unknown}"), "{unknown}")
        with patch.object(nacre_i18n, "language", return_value="pseudo"):
            value = nacre_i18n.tr(
                "missing.key",
                '<a href="{url}">Open</a>',
                {"url": "https://example.test/ASSET"},
            )
            self.assertIn('<a href="https://example.test/ASSET">', value)
            self.assertIn("</a>", value)
            self.assertTrue(value.startswith("[") and value.endswith("]"))
            self.assertNotIn("Open", value)

    def test_helper_language_cache_refreshes_only_when_preferences_change(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            preference = home / ".config/nacre/desktop.json"
            preference.parent.mkdir(parents=True)
            preference.write_text('{"language":"en"}')
            nacre_i18n._preference_language.cache_clear()
            with (
                patch.object(nacre_i18n.Path, "home", return_value=home),
                patch.dict(os.environ, {"NACRE_LANGUAGE": ""}),
            ):
                self.assertEqual(nacre_i18n.language(), "en")
                self.assertEqual(nacre_i18n.language(), "en")
                self.assertEqual(nacre_i18n._preference_language.cache_info().misses, 1)
                preference.write_text('{"language":"pseudo"}')
                self.assertEqual(nacre_i18n.language(), "pseudo")
                self.assertEqual(nacre_i18n._preference_language.cache_info().misses, 2)

    def test_notification_bridge_fallback_preserves_count_without_installed_assets(
        self,
    ):
        script = (ROOT / "hypr/scripts/window-trash.sh").read_text()
        function = script[
            script.index("nacre_message() {") : script.index("\nnotify() {")
        ]
        with tempfile.TemporaryDirectory() as home:
            result = subprocess.run(
                ["bash"],
                input=function
                + '\nnacre_message example.count "Restored {count} window(s) from trash" \'{"count":"2"}\'\n',
                capture_output=True,
                text=True,
                env={"HOME": home, "PATH": os.environ["PATH"]},
                check=True,
            )
            self.assertEqual(result.stdout, "Restored 2 window(s) from trash")

    def test_native_service_locale_fallback_and_expansion(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            (target / "i18n").mkdir()
            shutil.copy2(
                ROOT / "nacre/shell/i18n/translation.js", target / "i18n/translation.js"
            )
            source = (
                (ROOT / "nacre/shell/services/NacreI18n.qml")
                .read_text()
                .replace("pragma Singleton\n", "")
                .replace("import Quickshell\n", 'import "fixtures"\n')
                .replace("import Quickshell.Io\n", "")
                .replace(
                    'import "../i18n/translation.js"', 'import "i18n/translation.js"'
                )
                .replace("Singleton {", "Item {")
                .replace("Quickshell.env(", "Environment.env(")
            )
            (target / "TranslationService.qml").write_text(source)
            (fixtures / "Environment.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var values:({});function env(key){return values[key]||""}}'
            )
            (fixtures / "DesktopSettings.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var data:({language:"en"})}'
            )
            (fixtures / "FileView.qml").write_text(
                'import QtQuick\nQtObject {property string path;property bool printErrors:true;property string data:"{}";signal loaded();signal loadFailed(int error);function text(){return data}}'
            )
            (fixtures / "qmldir").write_text(
                "singleton Environment 1.0 Environment.qml\nsingleton DesktopSettings 1.0 DesktopSettings.qml\nFileView 1.0 FileView.qml\n"
            )
            (target / "tst_translations.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 name:"Translations";visible:true;when:windowShown
 Component {id:service;TranslationService {}}
 function init(){Environment.values={};DesktopSettings.data={language:"en"}}
 function test_english_and_missing_language(){
  const view=createTemporaryObject(service,this);
  view.english={"button.open":"Open","count.windows":{one:"{count} window",other:"{count} windows"}};
  compare(view.tr("button.open","Fallback"),"Open");
  compare(view.tr("unknown.key","Fallback"),"Fallback");
  compare(view.tr("count.windows","",{count:1}),"1 window");
  compare(view.tr("count.windows","",{count:2}),"2 windows");
  Environment.values={NACRE_LANGUAGE:"zz"};compare(view.language,"zz");compare(view.tr("button.open","Fallback"),"Open");
 }
 function test_pseudo_preserves_html_and_args(){
  Environment.values={NACRE_LANGUAGE:"pseudo"};const view=createTemporaryObject(service,this);
  const text="A longer sentence to measure translation expansion without changing any controls";
  const expanded=view.tr("test.expansion",text);
  verify(expanded.startsWith("[")&&expanded.endsWith("]"));verify(expanded.length>text.length*1.3);verify(expanded.length<text.length*1.6);
  const link=view.tr("test.link","<a href='{url}'>Open</a>",{url:"https://example.test/ASSET"});
  verify(link.includes("<a href='https://example.test/ASSET'>"));verify(link.includes("</a>"));verify(!link.includes("Open"));
  const variable=view.tr("test.argument","Open {name}",{name:"MY_USER_CONTENT"});verify(variable.includes("MY_USER_CONTENT"));
 }
 function test_locale_number_and_date(){
  const view=createTemporaryObject(service,this);Environment.values={NACRE_LANGUAGE:"de_DE"};
  compare(view.number(1234.5,1),Number(1234.5).toLocaleString(Qt.locale("de_DE"),"f",1));
  const date=new Date(2026,9,12,13,45);compare(view.date(date,"dd MMMM yyyy"),date.toLocaleDateString(Qt.locale("de_DE"),"dd MMMM yyyy"));
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
