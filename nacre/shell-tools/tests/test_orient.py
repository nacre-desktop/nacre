"""Independent engine invariants, image behavior and CLI publication boundaries."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

try:
    from PIL import Image
except ImportError:
    Image = None

SOURCE = Path(__file__).resolve().parents[2] / "shell-cli/src"
sys.path.insert(0, str(SOURCE))


@unittest.skipUnless(Image, "Pillow unavailable")
class OrientTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.environment = patch.dict(
            os.environ,
            {
                "HOME": str(self.home),
                "XDG_CONFIG_HOME": str(self.home / ".config"),
                "XDG_STATE_HOME": str(self.home / ".local/state"),
                "XDG_CACHE_HOME": str(self.home / ".cache"),
            },
        )
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def image(self, seed, name="image.png"):
        path = self.home / name
        Image.new("RGB", (128, 80), "#" + seed).save(path)
        return path

    def cli(self, *args):
        return subprocess.run(
            [sys.executable, "-m", "nacre_shell", *args],
            env={**os.environ, "PYTHONPATH": str(SOURCE)},
            capture_output=True,
            text=True,
        )

    def test_print_and_query_never_publish(self):
        path = self.image("d01818")
        result = self.cli("wallpaper", "-p", str(path))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["name"], "dynamic")
        self.assertFalse((self.home / ".local/state/nacre").exists())
        self.assertEqual(self.cli("scheme", "get", "--mode").stdout.strip(), "dark")
        self.assertFalse((self.home / ".local/state/nacre").exists())

    def test_invalid_selection_leaves_committed_state(self):
        result = self.cli("wallpaper", "-f", str(self.image("d01818")))
        self.assertEqual(result.returncode, 0, result.stderr)
        state = self.home / ".local/state/nacre/scheme.json"
        before = state.read_bytes()
        self.assertNotEqual(
            self.cli("wallpaper", "-f", str(self.home / "missing.png")).returncode, 0
        )
        self.assertEqual(state.read_bytes(), before)

    def test_saved_mode_and_persistent_accent(self):
        path = self.image("d01818")
        config = self.home / ".config/nacre"
        config.mkdir(parents=True)
        (config / "wallpaper-picker.json").write_text('{"paletteMode":"dark"}')
        self.assertEqual(self.cli("wallpaper", "-f", str(path)).returncode, 0)
        self.assertEqual(self.cli("scheme", "set", "--accent", "2255aa").returncode, 0)
        data = json.loads(self.cli("wallpaper", "-p", str(path)).stdout)
        self.assertEqual(data["source"]["selected"], "2255aa")
        self.assertEqual(self.cli("scheme", "set", "-m", "light").returncode, 0)
        self.assertEqual(self.cli("scheme", "get", "-m").stdout.strip(), "light")
        self.assertEqual(self.cli("scheme", "set", "--auto-accent").returncode, 0)
        self.assertEqual(
            json.loads(self.cli("wallpaper", "-p", str(path)).stdout)["source"][
                "selected"
            ],
            "d01818",
        )

    def test_harmony_cli_query_override_is_read_only_and_saved_setting_survives(self):
        path = self.image("bb2020")
        config = self.home / ".config/nacre"
        config.mkdir(parents=True)
        prefs = config / "wallpaper-picker.json"
        prefs.write_text('{"paletteHarmony":true,"paletteMode":"dark"}')
        for flag, expected in (("--no-harmony", False), ("--harmony", True)):
            query = self.cli("wallpaper", "-p", str(path), flag)
            self.assertEqual(query.returncode, 0, query.stderr)
            self.assertIs(json.loads(query.stdout)["input"]["harmony"], expected)
            self.assertTrue(json.loads(prefs.read_text())["paletteHarmony"])
            self.assertFalse((self.home / ".local/state/nacre").exists())
        self.assertEqual(self.cli("wallpaper", "-f", str(path)).returncode, 0)
        self.assertTrue(
            json.loads(self.cli("scheme", "get", "--json").stdout)["input"]["harmony"]
        )
        self.assertEqual(self.cli("scheme", "set", "--accent", "2255dd").returncode, 0)
        data = json.loads(self.cli("wallpaper", "-p", str(path)).stdout)
        self.assertTrue(data["input"]["harmony"])
        self.assertEqual(data["source"]["selected"], "2255dd")

    def test_smart_mode_keeps_the_saved_per_image_accent(self):
        path = self.image("bb2020")
        config = self.home / ".config/nacre"
        config.mkdir(parents=True)
        (config / "cli.json").write_text(
            json.dumps({"orient": {"accents": {str(path.resolve()): "2255dd"}}})
        )
        result = self.cli("wallpaper", "-f", str(path))
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(self.cli("scheme", "get", "--json").stdout)
        self.assertEqual(data["source"]["selected"], "2255dd")

    def test_named_default_and_legacy_variants(self):
        for variant in (
            "tonalspot",
            "vibrant",
            "expressive",
            "fidelity",
            "fruitsalad",
            "monochrome",
            "neutral",
            "rainbow",
            "content",
        ):
            result = self.cli(
                "scheme", "set", "-n", "default", "-v", variant, "-m", "dark"
            )
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.cli("scheme", "set", "-f", "unsupported").returncode, 2)

    def test_read_only_print_skips_custom_hook(self):
        config = self.home / ".config/nacre"
        config.mkdir(parents=True)
        (config / "cli.json").write_text(
            json.dumps({"wallpaper": {"postHook": 'touch "$HOME/hook-called"'}})
        )
        path = self.image("d01818")
        self.assertEqual(self.cli("wallpaper", "-p", str(path)).returncode, 0)
        self.assertFalse((self.home / "hook-called").exists())
        self.assertEqual(self.cli("wallpaper", "-f", str(path)).returncode, 0)
        self.assertTrue((self.home / "hook-called").exists())


class OrientBridgeTests(unittest.TestCase):
    def test_queries_skip_publisher_but_changes_publish_under_shared_lock(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            runtime = home / ".local/share/nacre/palette-runtime/venv/bin"
            runtime.mkdir(parents=True)
            cli = runtime / "nacre_shell"
            cli.write_text(
                '#!/bin/sh\nif [ "$2" = set ]; then printf "%s" "${NACRE_PALETTE_LOCKED:-}" > "$HOME/locked"; fi\nprintf "ok\\n"\n'
            )
            cli.chmod(0o755)
            tools = home / ".local/share/nacre/shell/tools"
            tools.mkdir(parents=True)
            (tools / "classic-state.py").write_text(
                'from pathlib import Path\n(Path.home()/"published").write_text("yes")\n'
            )
            bridge = Path(__file__).parents[1] / "cli-bridge.sh"
            for args in (
                ("scheme", "get", "-m"),
                ("wallpaper", "-p", "ignored"),
                ("wallpaper", "--print=ignored"),
            ):
                result = subprocess.run(
                    ["bash", str(bridge), *args],
                    env={**os.environ, "HOME": str(home)},
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse((home / "published").exists())
                self.assertFalse(
                    (home / ".local/state/nacre/palette-commit.lock").exists()
                )
            result = subprocess.run(
                ["bash", str(bridge), "scheme", "set", "-m", "dark"],
                env={**os.environ, "HOME": str(home)},
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((home / "published").exists())
            self.assertEqual((home / "locked").read_text(), "1")


class OrientPublicCommandTests(unittest.TestCase):
    def test_palette_commands_use_bridge_while_plain_wallpaper_opens_picker(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            binary = home / ".local/share/nacre/shell/bin"
            binary.mkdir(parents=True)
            for name, label in (("nacre_shell", "bridge"), ("qs", "picker")):
                path = binary / name
                path.write_text('#!/bin/sh\nprintf "%s\\n" "' + label + '" "$@"\n')
                path.chmod(0o755)
            control = Path(__file__).parents[1] / "control.sh"
            for args, label in (
                (("scheme", "set", "-m", "dark"), "bridge"),
                (("wallpaper", "-p", "image"), "bridge"),
                (("wallpaper",), "picker"),
            ):
                result = subprocess.run(
                    ["bash", str(control), *args],
                    env={**os.environ, "HOME": str(home)},
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.splitlines()[0], label)
