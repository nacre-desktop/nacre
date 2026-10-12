"""The CI boundary catches new native access and permits only declared backends."""

import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "boundary", Path(__file__).parents[1] / "compositor_boundary.py"
)
boundary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(boundary)


class CompositorBoundaryTests(unittest.TestCase):
    def test_python_qml_shell_and_expressions_cannot_bypass_service(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            samples = {
                "helper.py": 'import subprocess\nsubprocess.run(["hyprctl","clients","-j"])\n',
                "absolute.py": 'command=["/usr/bin/hyprctl","reload"]\n',
                "expression.py": 'expression="hl.dispatch(hl.dsp.exit())"\n',
                "Window.qml": "import Quickshell.Hyprland\nItem {}\n",
                "picker.sh": "#!/bin/sh\nhyprctl clients -j\n",
                "version.py": 'command=["Hyprland","--version"]\n',
            }
            for name, text in samples.items():
                path = root / name
                path.write_text(text)
                self.assertTrue(boundary.violations(path, root), name)

    def test_backend_generated_configuration_tests_and_neutral_calls_are_allowed(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name, text in {
                "nacre/shell/services/compositor/HyprlandBackend.qml": "import Quickshell.Hyprland\n",
                "nacre/shell-tools/compositor_backends/hyprland.py": 'command=["hyprctl"]\n',
                "hypr/conf/keybinding.lua": "hl.bind('Super+A',hl.dsp.exec_cmd('app'))\n",
                "tests/test_transport.py": 'command=["hyprctl"]\n',
                "helper.py": 'import compositor\ncompositor.query("windows")\n',
                "wrapper.sh": "#!/bin/sh\nexec python3 ~/.config/hypr/scripts/compositor-compat.py\n",
            }.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
                self.assertEqual(boundary.violations(path, root), [], name)
