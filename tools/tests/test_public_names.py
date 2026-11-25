"""Fresh installation uses public names without requiring private host aliases."""

from pathlib import Path
import subprocess
import unittest

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
                "packaging/arch/PKGBUILD",
                "packaging/arch/recipe.py",
                "packaging/arch/niri/recipe.py",
            ):
                # Owner-approved public commit/maintainer attribution is metadata,
                # not a private runtime alias. Keep this exception exact and scoped.
                text = text.replace(
                    "siver" + "teh <92440237+siver" + "teh@users.noreply.github.com>",
                    "",
                )
            self.assertFalse(any(word in text for word in forbidden), name)
