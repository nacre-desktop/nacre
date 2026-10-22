"""Fresh installation uses public names without requiring private host aliases."""

import importlib.machinery
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


class PublicNamesTests(unittest.TestCase):
    def test_active_source_and_paths_have_no_previous_product_identifiers(self):
        # Split patterns so this scanner's own source is included in its scan.
        forbidden = ("siver" + "teh", "cael" + "estia")
        paths = (
            subprocess.check_output(
                ["git", "-c", f"safe.directory={ROOT}", "ls-files", "-z"], cwd=ROOT
            )
            .decode()
            .split("\0")
        )
        for name in paths:
            if (
                not name
                or name.startswith("docs/")
                or name == "README.md"
                or "NOTICE" in name
                or "LICENSE" in name
            ):
                continue
            path = ROOT / name
            self.assertFalse(any(word in name.lower() for word in forbidden), name)
            if not path.is_file():
                continue
            try:
                text = path.read_text().lower()
            except UnicodeError:
                continue
            if name == "nacre/login/metadata.desktop":
                text = "\n".join(
                    line for line in text.splitlines() if not line.startswith("author=")
                )
            if name == "nacre/shell-tools/launcher-menu.json":
                # Historical GitHub repository address is the real source URL,
                # not an active product/runtime identifier.
                text = text.replace(
                    "https://github.com/" + "siver" + "teh/siver" + "teh-os", ""
                )
            if name in (
                "CONTRIBUTING.md",
                "packaging/arch/PKGBUILD",
                "packaging/arch/recipe.py",
                "packaging/arch/niri/PKGBUILD",
                "packaging/arch/niri/recipe.py",
            ):
                # Owner-approved public commit/maintainer attribution is metadata,
                # not a private runtime alias. Keep this exception exact and scoped.
                text = text.replace(
                    "siver" + "teh <92440237+siver" + "teh@users.noreply.github.com>",
                    "",
                )
                if name == "CONTRIBUTING.md":
                    text = text.replace("`user.name` to `siver" + "teh`", "")
            self.assertFalse(any(word in text for word in forbidden), name)

    def test_fresh_home_controller_roots_and_provider_profiles_are_canonical(self):
        loader = importlib.machinery.SourceFileLoader(
            "public_controller", str(ROOT / "bin/nacre-ai.py.rollback")
        )
        spec = importlib.util.spec_from_loader(loader.name, loader)
        controller = importlib.util.module_from_spec(spec)
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.dict(os.environ, {"HOME": directory}, clear=True),
        ):
            loader.exec_module(controller)
            home = Path(directory)
            self.assertEqual(
                controller.settings_path(), home / ".config/nacre/ai/projects.json"
            )
            self.assertEqual(
                controller.preferences_path(), home / ".config/nacre/ai/settings.json"
            )
            self.assertEqual(
                controller.general_chat()["path"],
                str(home / ".local/share/nacre/ai/chats"),
            )
