"""Real dispatcher/profile-policy boundaries with private tools; never installs."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper"))


@unittest.skipUnless(BINARY.is_file(), "Candidate native helper required")
class AurReviewTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="nacre-aur-dispatch-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = dict(os.environ)
        for key, suffix in [
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_STATE_HOME", "state"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_RUNTIME_DIR", "runtime"),
        ]:
            path = self.root / suffix
            path.mkdir(mode=0o700)
            self.env[key] = str(path)
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "NIRI_SOCKET",
            "HYPRLAND_INSTANCE_SIGNATURE",
        ]:
            self.env.pop(key, None)
        self.env.update(
            DBUS_SESSION_BUS_ADDRESS="unix:path=" + self.temp.name + "/absent",
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + self.temp.name + "/absent",
        )
        self.tools = self.root / "bin"
        self.tools.mkdir()
        self.env["PATH"] = str(self.tools)
        self.log = self.root / "calls.jsonl"
        for name in ["paru", "pacman", "pkexec", "makepkg", "less", "git"]:
            path = self.tools / name
            path.write_text(
                "#!"
                + sys.executable
                + "\nimport json,sys\nfrom pathlib import Path\nwith Path("
                + repr(str(self.log))
                + ").open('a') as log:log.write(json.dumps(sys.argv)+'\\n')\n"
            )
            path.chmod(0o755)
        self.profile = self.root / "config/nacre/profile.json"
        self.profile.parent.mkdir()

    def run_helper(self, *args):
        return subprocess.run(
            [str(BINARY), *args],
            env=self.env,
            input="",
            text=True,
            capture_output=True,
            timeout=10,
        )

    def test_general_review_refuses_background_install_before_any_external_tool(self):
        self.profile.write_text(json.dumps({"version": 1, "id": "default"}))
        result = self.run_helper("software-package", "install", "sample-editor")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("visible terminal", result.stderr)
        self.assertFalse(self.log.exists())

    def test_declared_native_personal_flow_retains_exact_existing_manager_arguments(
        self,
    ):
        for profile in [
            {"version": 1, "id": "studio-profile"},
            {"version": 1, "id": "default", "aurReview": "native"},
        ]:
            self.profile.write_text(json.dumps(profile))
            result = self.run_helper("software-package", "install", "sample-editor")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                json.loads(self.log.read_text().splitlines()[-1])[1:],
                ["-S", "--needed", "--skipreview", "--", "sample-editor"],
            )

    def test_policy_provenance_and_future_version_refusal(self):
        self.profile.write_text(
            json.dumps({"version": 1, "id": "default", "aurReview": "required"})
        )
        result = self.run_helper("aur-review", "policy")
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data["mode"], "required")
        self.assertEqual(
            data["provenance"],
            {
                "source": "registered-profile-setting",
                "store": "profile",
                "key": "aurReview",
                "profile": "default",
            },
        )
        before = '{"version":999,"aurReview":"native"}'
        self.profile.write_text(before)
        self.assertNotEqual(self.run_helper("aur-review", "policy").returncode, 0)
        self.assertEqual(self.profile.read_text(), before)
        self.assertFalse(self.log.exists())
