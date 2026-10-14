"""The shipped catalog is licensed, unique and searchable with familiar keywords."""

import json
from pathlib import Path
import unittest

TOOLS = Path(__file__).resolve().parents[1]


class EmojiCatalogTests(unittest.TestCase):
    def test_full_catalog_provenance_keywords_and_qualification(self):
        data = json.loads((TOOLS / "emoji-catalog.json").read_text())
        entries = data["entries"]
        self.assertEqual(data["version"], "17.0")
        self.assertEqual(data["annotations"]["version"], "48.0.0")
        self.assertEqual(len(entries), 3944)
        self.assertEqual(len({row["value"] for row in entries}), len(entries))
        self.assertTrue(all(row["name"] and row["category"] for row in entries))
        self.assertIn(
            "happy", next(row for row in entries if row["value"] == "😀")["aliases"]
        )
        self.assertIn(
            "UK", next(row for row in entries if row["value"] == "🇬🇧")["aliases"]
        )
        self.assertIn(
            "US", next(row for row in entries if row["value"] == "🇺🇸")["aliases"]
        )
        self.assertIn("UNICODE LICENSE V3", (TOOLS / "emoji-LICENSE.txt").read_text())
