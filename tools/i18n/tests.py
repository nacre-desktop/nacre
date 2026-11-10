"""Native message engine and source guard contracts; never modify live preferences."""

import importlib.util
import os
from pathlib import Path
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
    def test_actual_catalog_preserves_live_plural_english_and_pseudo(self):
        messages = nacre_i18n.english()
        with patch.object(nacre_i18n, "language", return_value="en"):
            self.assertEqual(
                nacre_i18n.tr("brain.ui.count.facts", "fallback", {"count": 1}),
                "1 fact",
            )
            self.assertEqual(
                nacre_i18n.tr("brain.ui.count.facts", "fallback", {"count": 2}),
                "2 facts",
            )
        self.assertIn("brain.ui.count.facts", messages)
        with patch.object(nacre_i18n, "language", return_value="pseudo"):
            translated = nacre_i18n.tr("brain.ui.count.facts", "fallback", {"count": 2})
        self.assertIn("2", translated)
        self.assertNotIn("2 facts", translated)

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
                patch.dict(
                    os.environ,
                    {"NACRE_LANGUAGE": "", "XDG_CONFIG_HOME": str(home / ".config")},
                ),
            ):
                self.assertEqual(nacre_i18n.language(), "en")
                self.assertEqual(nacre_i18n.language(), "en")
                self.assertEqual(nacre_i18n._preference_language.cache_info().misses, 1)
                previous = preference.stat()
                preference.write_text('{"language":"pseudo"}')
                # Rapid writes can share a timestamp on a container overlay.
                # Give this changed fixture an explicit distinct cache version.
                os.utime(
                    preference,
                    ns=(previous.st_atime_ns, previous.st_mtime_ns + 2_000_000_000),
                )
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


if __name__ == "__main__":
    unittest.main()
