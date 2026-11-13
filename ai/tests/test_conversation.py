import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from test_workflow import ROOT


def native_ai_command(name):
    binary = os.environ.get(
        "NACRE_HELPER_BINARY", str(ROOT / "target/release/nacre-helper")
    )
    return [binary, name]


class Conversation(unittest.TestCase):
    def test_memory_creates_topics_worlds_and_deduplicates_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ, HOME=tmp, NACRE_BRAIN=tmp + "/brain")
            command = [
                *native_ai_command("nacre-ai-memory"),
                "--title",
                "A useful result",
                "--source",
                "user statement",
                "--confidence",
                "reported",
                "--world",
                "Climbing",
                "--world",
                "Personal",
                "--topic",
                "Training",
            ]
            first = subprocess.run(
                command,
                input="Prefers bouldering.",
                text=True,
                capture_output=True,
                env=env,
                check=True,
            )
            second = subprocess.run(
                command,
                input="Prefers bouldering.",
                text=True,
                capture_output=True,
                env=env,
                check=True,
            )
            self.assertEqual(
                json.loads(first.stdout)["path"], json.loads(second.stdout)["path"]
            )
            root = Path(tmp) / "brain"
            self.assertEqual(list((root / "wiki").glob("*.md")), [])
            body = Path(json.loads(first.stdout)["path"]).read_text()
            taxonomy = json.loads((root / "registry.json").read_text())
            personal = next(
                world["name"]
                for world in taxonomy["worlds"]
                if "Personal" in world["aliases"]
            )
            self.assertIn(f"Worlds: Climbing, {personal}", body)
            self.assertIn("Topics: training", body)
            refused = subprocess.run(
                command,
                input="password: hunter2",
                text=True,
                capture_output=True,
                env=env,
            )
            self.assertNotEqual(refused.returncode, 0)

    def test_memory_taxonomy_growth_is_independent_of_coding_registry(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "brain"
            registry = Path(tmp) / "projects.json"
            registry.write_text(
                json.dumps(
                    {
                        "projects": [
                            {
                                "id": "gardensense",
                                "label": "GardenSense",
                                "path": "/repo",
                            }
                        ]
                    }
                )
            )
            subprocess.run(
                [
                    *native_ai_command("nacre-ai-memory"),
                    "--title",
                    "A useful observation",
                    "--source",
                    "user",
                    "--world",
                    "GardenSense",
                ],
                input="Soil moisture readings are useful.",
                text=True,
                capture_output=True,
                env=dict(
                    os.environ,
                    HOME=tmp,
                    NACRE_BRAIN=str(root),
                    NACRE_AI_PROJECTS=str(registry),
                ),
                check=True,
            )
            self.assertFalse((root / "wiki/entity-gardensense.md").exists())
            taxonomy = json.loads((root / "registry.json").read_text())
            self.assertTrue(
                any(
                    world["name"] == "Gardensense" and world["status"] == "auto"
                    for world in taxonomy["worlds"]
                )
            )

    def test_memory_existing_alias_preserves_curated_page(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "brain"
            (root / "wiki").mkdir(parents=True)
            page = root / "wiki/existing.md"
            original = "# My subject\nEntity: world\nName: Climbing\nAliases: Bouldering\n\nCurated evidence\n"
            page.write_text(original)
            subprocess.run(
                [
                    *native_ai_command("nacre-ai-memory"),
                    "--title",
                    "Training",
                    "--source",
                    "user",
                    "--world",
                    "Bouldering",
                ],
                input="A preference.",
                text=True,
                capture_output=True,
                env=dict(os.environ, HOME=tmp, NACRE_BRAIN=str(root)),
                check=True,
            )
            self.assertEqual(page.read_text(), original)
            self.assertFalse((root / "wiki/entity-bouldering.md").exists())


if __name__ == "__main__":
    unittest.main()
