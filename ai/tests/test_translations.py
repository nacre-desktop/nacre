"""Localized AI captions preserve canonical actions and standalone controller use."""

import importlib.machinery
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader(
    "translated_ai", str(ROOT / "bin/nacre-ai.py.rollback")
)
spec = importlib.util.spec_from_loader(loader.name, loader)
ai = importlib.util.module_from_spec(spec)
loader.exec_module(ai)


class AITranslationTests(unittest.TestCase):
    def test_canonical_choice_is_unchanged_when_caption_is_pseudo(self):
        label = ai.ui_label("ai.ui.close", "Close")
        with patch.object(ai, "tr", return_value="[Çľöšë ~~~]"):
            self.assertEqual(label, "Close")
            self.assertEqual(ai.display_label(label), "[Çľöšë ~~~]")
            self.assertEqual(str(label), "Close")
            self.assertEqual(
                ai.display_label("My private project"), "My private project"
            )

    def test_formatted_canonical_label_and_dynamic_args_stay_separate(self):
        value = ai.ui_label("ai.ui.example", "Project {name}", {"name": "User data"})
        self.assertEqual(value, "Project User data")
        with patch.object(
            ai, "tr", return_value="[Þŕöĵëçţ User data ~~~]"
        ) as translate:
            self.assertEqual(ai.display_label(value), "[Þŕöĵëçţ User data ~~~]")
            translate.assert_called_once_with(
                "ai.ui.example", "Project {name}", {"name": "User data"}
            )


if __name__ == "__main__":
    unittest.main()
