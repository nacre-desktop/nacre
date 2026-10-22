import importlib.machinery
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader(
    "shared_skills", str(ROOT / "bin/nacre-ai-skills.py.rollback")
)
spec = importlib.util.spec_from_loader(loader.name, loader)
skills = importlib.util.module_from_spec(spec)
loader.exec_module(skills)


class SharedSkillsTests(unittest.TestCase):
    def test_accounts_follow_shared_library_relocation(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "shared"
            source.mkdir()
            for name in ("first", "second"):
                folder = root / name
                folder.mkdir()
                (folder / "SKILL.md").write_text(name)
            (source / "example").symlink_to(root / "first")
            target = root / "account/skills"
            skills.expose(source, target)
            (source / "example").unlink()
            (source / "example").symlink_to(root / "second")
            skills.expose(source, target)
            self.assertEqual((target / "example/SKILL.md").read_text(), "second")

    def test_foreign_skills_and_auth_are_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            source.mkdir()
            skill = source / "example"
            skill.mkdir()
            (skill / "SKILL.md").write_text("new")
            account = root / "account"
            target = account / "skills"
            target.mkdir(parents=True)
            (account / "auth.json").write_text("untouched")
            existing = target / "example"
            existing.mkdir()
            (existing / "SKILL.md").write_text("mine")
            with self.assertRaises(RuntimeError):
                skills.expose(source, target)
            self.assertEqual((existing / "SKILL.md").read_text(), "mine")
            self.assertEqual((account / "auth.json").read_text(), "untouched")


class UpgradeTests(unittest.TestCase):
    def test_owned_release_updates_but_foreign_link_does_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            library = root / "library"
            destination = root / "shared"
            for name in ("old", "new"):
                folder = library / name / "example"
                folder.mkdir(parents=True)
                (folder / "SKILL.md").write_text(name)
            skills.expose(library / "old", destination, owned_library=library)
            skills.expose(library / "new", destination, owned_library=library)
            self.assertEqual((destination / "example/SKILL.md").read_text(), "new")
