"""Files routes use styled Thunar and preserve explicit application choices."""

import importlib.util
import json
from importlib.machinery import SourceFileLoader
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


class FilesRouteTests(unittest.TestCase):
    def test_terminal_and_browser_routes_preserve_literal_arguments(self):
        loader = SourceFileLoader("app_route", str(ROOT / "bin/nacre-app"))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        app = importlib.util.module_from_spec(spec)
        loader.exec_module(app)
        for role, argv in (
            ("terminal", ["alacritty", "--title", "My terminal"]),
            ("browser", ["firefox", "--new-window"]),
        ):
            with self.subTest(role=role), tempfile.TemporaryDirectory() as directory:
                home = Path(directory)
                path = home / ".config/nacre/apps.json"
                path.parent.mkdir(parents=True)
                path.write_text(json.dumps({role: argv}))
                with (
                    patch.object(app.Path, "home", return_value=home),
                    patch.object(app.sys, "argv", ["route", role]),
                    patch.object(
                        app.shutil, "which", return_value="/usr/bin/installed"
                    ),
                    patch.object(app.os, "execvp") as execute,
                    patch.object(app, "restricted_niri_session", return_value=False),
                ):
                    app.main()
                    execute.assert_called_once_with(argv[0], argv)

    def test_default_uses_styled_dolphin_with_literal_path(self):
        loader = SourceFileLoader("app_route", str(ROOT / "bin/nacre-app"))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        app = importlib.util.module_from_spec(spec)
        loader.exec_module(app)
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(app.Path, "home", return_value=Path(directory)),
                patch.object(app.sys, "argv", ["route", "files", "/tmp/with spaces"]),
                patch.object(app.shutil, "which", return_value="/usr/bin/installed"),
                patch.object(app.os, "execvp") as execute,
            ):
                app.main()
                execute.assert_called_once_with(
                    str(Path(directory) / ".local/bin/nacre-shell"),
                    [
                        str(Path(directory) / ".local/bin/nacre-shell"),
                        "dolphin",
                        "/tmp/with spaces",
                    ],
                )

    def test_explicit_file_manager_is_preserved(self):
        loader = SourceFileLoader("app_route", str(ROOT / "bin/nacre-app"))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        app = importlib.util.module_from_spec(spec)
        loader.exec_module(app)
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            config = home / ".config/nacre/apps.json"
            config.parent.mkdir(parents=True)
            config.write_text('{"files":["thunar"]}')
            with (
                patch.object(app.Path, "home", return_value=home),
                patch.object(app.sys, "argv", ["route", "files", "/tmp/with spaces"]),
                patch.object(app.shutil, "which", return_value="/usr/bin/thunar"),
                patch.object(app.os, "execvp") as execute,
            ):
                app.main()
                execute.assert_called_once_with(
                    "thunar", ["thunar", "/tmp/with spaces"]
                )
