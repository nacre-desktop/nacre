"""Fresh core/optional preparation and personal profile validation."""

import importlib.util
import json
import tempfile
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "profile_policy", ROOT / "nacre/shell-tools/profile-policy.py"
)
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)
spec = importlib.util.spec_from_file_location(
    "prepare_install", ROOT / "tools/prepare.py"
)
staging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(staging)


class ProfileTests(unittest.TestCase):
    def test_generic_profile_has_no_personal_routes_apps_or_agent_permissions(self):
        profile = policy.validate(
            json.loads((ROOT / "profiles/default.json").read_text())
        )
        self.assertEqual(profile["components"], ["configs", "shell"])
        self.assertEqual(profile["workspaceIcons"], [])
        self.assertEqual(profile["startup"], [])
        self.assertEqual(profile["routes"], [])
        self.assertFalse(profile["aiFullAccess"])
        self.assertFalse(profile["windowTrashBindings"])
        self.assertIsNone(profile["brainWorkspace"])
        self.assertIsNone(profile["aiWorkspace"])
        self.assertNotIn("hl.window_rule", policy.lua(profile))

    def test_invalid_profiles_are_rejected_before_install(self):
        for value in (
            {"workspaceCount": 0},
            {"components": ["unknown"]},
            {"brainWorkspace": 20},
            {"workspaceNames": [5]},
            {"startup": [{"argv": "shell string"}]},
            {"unknown": True},
            {"windowTrashBindings": "true"},
        ):
            with self.assertRaises(ValueError):
                policy.validate(value)

    def test_trash_bindings_are_explicit_and_preserve_plain_close(self):
        generic = policy.lua(policy.validate({}))
        self.assertNotIn("window-trash.sh", generic)
        personal = policy.lua(policy.validate({"windowTrashBindings": True}))
        self.assertIn('"SUPER + CTRL + Q"', personal)
        self.assertNotIn('"SUPER + Q"', personal)
        for action in ("put-away", "restore", "restore-all", "force-close"):
            self.assertIn("Nacre:" + action, personal)

    def test_core_prepare_installs_no_optional_services_or_personal_data(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / "fresh"
            staging.prepare(ROOT, home, policy.validate({}))
            prepared = json.loads((home / ".config/nacre/profile.json").read_text())
            self.assertEqual(prepared["startup"], [])
            self.assertEqual(prepared["routes"], [])
            self.assertNotIn(
                "hl.window_rule", (home / ".config/nacre/profile.lua").read_text()
            )
            self.assertTrue((home / ".local/bin/nacre-settings").exists())
            self.assertTrue(
                (home / ".config/systemd/user/nacre-niri-widths.service").is_file()
            )
            self.assertTrue(
                (
                    home
                    / ".local/share/nacre/shell/tools/compositor_backends/niri_widths.py"
                ).is_file()
            )
            self.assertFalse((home / ".local/bin/nacre-ai").exists())
            self.assertFalse((home / ".local/bin/nacre-brain").exists())
            self.assertFalse(
                (home / ".config/systemd/user/nacre-sidebar-ai.service").exists()
            )
            self.assertFalse(
                (home / ".config/systemd/user/nacre-brain.service").exists()
            )
            self.assertFalse((home / "Documents").exists())
            self.assertFalse((home / ".local/share/nacre/ai/accounts").exists())
            self.assertFalse(
                json.loads((home / "installation.json").read_text())["activated"]
            )
            with self.assertRaises(ValueError):
                staging.prepare(ROOT, home, policy.validate({}))

    def test_optional_prepare_only_adds_requested_code_and_services(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / "optional"
            profile = policy.validate(
                {
                    "components": ["configs", "shell", "ai", "brain"],
                    "id": "example",
                    "brainWorkspace": 3,
                }
            )
            staging.prepare(ROOT, home, profile)
            self.assertTrue(
                (home / ".config/systemd/user/nacre-sidebar-ai.service").exists()
            )
            self.assertTrue((home / ".local/share/nacre/brain/control.py").exists())
            self.assertFalse((home / "Documents").exists())
            self.assertIn(
                'workspace="3 silent"', (home / ".config/nacre/profile.lua").read_text()
            )
