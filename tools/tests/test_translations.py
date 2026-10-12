"""Message API and coverage regressions for the owned desktop UI."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/i18n"))
from source import uncovered

spec = importlib.util.spec_from_file_location(
    "translation_runtime_tests", ROOT / "tools/i18n/tests.py"
)
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)
TranslationTests = runtime.TranslationTests
spec = importlib.util.spec_from_file_location(
    "translation_guard", ROOT / "tools/i18n/check.py"
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class CoverageTests(unittest.TestCase):
    def test_visible_literals_are_required_but_icons_and_identity_are_not_messages(
        self,
    ):
        self.assertEqual(
            list(uncovered('NacreText { text: "New label" }'))[0][2], "New label"
        )
        self.assertEqual(
            list(uncovered('NacreTextField { placeholder: "Search" }'))[0][2], "Search"
        )
        self.assertEqual(
            list(uncovered('Item { Accessible.name: "Open" }'))[0][2], "Open"
        )
        self.assertEqual(list(uncovered('NacreIcon { text: "arrow_back" }')), [])
        self.assertEqual(
            list(
                uncovered(
                    'Item {property var client: rows.find(c=>c.title === "Nacre Colors")}'
                )
            ),
            [],
        )
        self.assertEqual(
            list(
                uncovered(
                    'NacreText { text: NacreI18n.tr("example.label", "New label") }'
                )
            ),
            [],
        )

    def fixture(self, root, english, qml):
        shell = root / "nacre/shell"
        (shell / "i18n").mkdir(parents=True)
        tools = root / "nacre/shell-tools"
        tools.mkdir(parents=True)
        (shell / "modules/settings").mkdir(parents=True)
        (shell / "modules/welcome").mkdir(parents=True)
        (shell / "modules/settings/settings-catalog.js").write_text("")
        (shell / "modules/welcome/welcome-catalog.js").write_text("")
        (tools / "launcher-menu.json").write_text('{"entries": []}')
        (shell / "i18n/options.json").write_text("{}")
        (shell / "i18n/en.json").write_text(json.dumps(english))
        (shell / "i18n/context.json").write_text(
            json.dumps({key: "Test context" for key in english})
        )
        (shell / "Test.qml").write_text(qml)

    def test_missing_unused_and_unwrapped_messages_fail(self):
        for english, qml, error in (
            (
                {},
                'Item {text: NacreI18n.tr("test.missing", "Hello")}',
                "Missing English ID",
            ),
            ({"test.unused": "Hello"}, "Item {}", "Unused English ID"),
            ({}, 'Item {text: "Hello"}', "unwrapped text"),
        ):
            with self.subTest(error=error), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                self.fixture(root, english, qml)
                with self.assertRaisesRegex(ValueError, error):
                    guard.check(root)

    def test_conditional_plural_ids_are_tracked(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.fixture(
                root,
                {
                    "test.active": {
                        "one": "{count} window",
                        "other": "{count} windows",
                    },
                    "test.inactive": "No windows",
                },
                'Item {text: NacreI18n.tr(active ? "test.active" : "test.inactive", "", {count: count})}',
            )
            self.assertEqual(guard.references(root), {"test.active", "test.inactive"})

    def test_language_preference_and_pseudo_gate(self):
        from unittest.mock import patch

        spec = importlib.util.spec_from_file_location(
            "translation_settings", ROOT / "nacre/shell-tools/desktop-settings.py"
        )
        settings = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(settings)
        self.assertEqual(settings.validate("language", "system"), "system")
        self.assertEqual(settings.validate("language", "en"), "en")
        with patch.dict(settings.os.environ, {"NACRE_DEVELOPER_MODE": "0"}):
            with self.assertRaises(ValueError):
                settings.validate("language", "pseudo")
        with patch.dict(settings.os.environ, {"NACRE_DEVELOPER_MODE": "1"}):
            self.assertEqual(settings.validate("language", "pseudo"), "pseudo")
        self.assertNotIn("language", settings.PRESET_KEYS)

    def test_indirect_messages_and_rendering_factories_are_covered(self):
        for source in (
            'Item {readonly property string estimate: "Battery unavailable"}',
            'result.reason = "Optional tool is not installed";',
            'valueRow(route, "choice", "New choice", "operation", "value", "icon", "Choice detail")',
            'el("span", "New caption", "class-name")',
            'button("New action", callback)',
            'element.textContent = "New content";',
            'form.button = "Create app";',
            'Item {section: {help:"Getting help"}[page]}',
        ):
            with self.subTest(source=source):
                self.assertTrue(list(uncovered(source)))
        self.assertEqual(
            list(
                uncovered('el("span", tr("test.message", "Translated"), "class-name")')
            ),
            [],
        )

    def test_shortcut_protocol_names_are_not_messages(self):
        self.assertEqual(list(uncovered('NacreShortcut {name:"mediaToggle"}')), [])

    def test_python_catalog_companion_ids_are_references(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.fixture(root, {"test.title": "Open"}, "Item {}")
            (root / "nacre/shell-tools/catalog.py").write_text(
                'ROWS = {"action":{"titleId":"test.title"}}'
            )
            self.assertEqual(guard.references(root), {"test.title"})


if __name__ == "__main__":
    unittest.main()
