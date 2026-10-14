"""Installed Python consumers must never import a shell command wrapper."""

import importlib.util
import tempfile
import unittest
from pathlib import Path
import nacre_ai_controller


class AIControllerTests(unittest.TestCase):
    def test_packaged_controller_loads_code_instead_of_private_shell_launcher(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root / "nacre/shell-tools"
            tools.mkdir(parents=True)
            controller = root / "bin/nacre-ai"
            controller.parent.mkdir()
            controller.write_text(
                '#!/usr/bin/env python3\ndef default_assistant(): return "codex"\n'
            )
            local = root / "home/.local/bin/nacre-ai"
            local.parent.mkdir(parents=True)
            local.write_text('#!/bin/sh\nexec /usr/share/nacre/bin/nacre-ai "$@"\n')
            script = tools / "nacre_ai_controller.py"
            script.write_bytes(Path(nacre_ai_controller.__file__).read_bytes())
            spec = importlib.util.spec_from_file_location(
                "fixture_ai_controller", script
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.assertEqual(
                module.load(home=root / "home")["default_assistant"](), "codex"
            )
            with self.assertRaises(ValueError):
                module.load("../../private")
