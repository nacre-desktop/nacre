"""Session aliases and native startup gates preserve the working Hyprland path."""

import configparser
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
import native_niri_session_gate as gate


class NiriSessionGateTests(unittest.TestCase):
    def test_hyprland_context_retains_existing_startup_despite_stale_niri_socket(self):
        with tempfile.TemporaryDirectory() as folder:
            for action in ("shell", "observer", "idle"):
                self.assertTrue(
                    gate.allowed(
                        action,
                        {"XDG_CURRENT_DESKTOP": "Hyprland", "NIRI_SOCKET": "/stale"},
                        folder,
                    )
                )
            self.assertTrue(
                gate.allowed(
                    "shell", {"HYPRLAND_INSTANCE_SIGNATURE": "fixture"}, folder
                )
            )

    def test_plain_niri_never_loads_shared_shell_or_automatic_lock(self):
        with tempfile.TemporaryDirectory() as folder:
            for action in ("shell", "observer", "idle"):
                self.assertFalse(
                    gate.allowed(
                        action,
                        {
                            "XDG_CURRENT_DESKTOP": "niri",
                            "HYPRLAND_INSTANCE_SIGNATURE": "stale",
                        },
                        folder,
                    )
                )
            self.assertFalse(gate.allowed("shell", {}, folder))

    def test_readiness_and_unlock_are_separate_explicit_gates(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            record = home / ".config/nacre/niri-on-pc/acceptance.json"
            record.parent.mkdir(parents=True)
            environment = {"XDG_CURRENT_DESKTOP": "niri"}
            for contents in (
                "bad",
                "[]",
                '{"phase":"plain","backendReady":true,"unlockVerified":true}',
            ):
                record.write_text(contents)
                self.assertFalse(gate.allowed("shell", environment, home))
                self.assertFalse(gate.allowed("idle", environment, home))
            record.write_text(
                json.dumps(dict(phase="nacre", backendReady=True, unlockVerified=False))
            )
            self.assertTrue(gate.allowed("shell", environment, home))
            self.assertFalse(gate.allowed("observer", environment, home))
            self.assertFalse(gate.allowed("idle", environment, home))
            record.write_text(
                json.dumps(dict(phase="nacre", backendReady=True, unlockVerified=True))
            )
            self.assertTrue(gate.allowed("idle", environment, home))
            record.write_text(
                json.dumps(dict(phase="nacre", backendReady=True, observerReady=True))
            )
            self.assertTrue(gate.allowed("observer", environment, home))
            record.write_text(
                json.dumps(dict(phase="nacre", backendReady=False, unlockVerified=True))
            )
            self.assertFalse(gate.allowed("idle", environment, home))

    def test_both_aliases_use_original_compositor_entry_under_uwsm(self):
        for name, desktop, entry in (
            ("hyprland", "Hyprland", "hyprland.desktop"),
            ("niri", "niri", "niri.desktop"),
        ):
            config = configparser.ConfigParser(interpolation=None)
            config.read(ROOT / "uwsm/sessions" / ("nacre-" + name + ".desktop"))
            session = config["Desktop Entry"]
            self.assertEqual(
                session["Exec"], "uwsm start -e -D " + desktop + " " + entry
            )
            self.assertEqual(session["Name"], "Nacre (" + desktop + ")")
            self.assertEqual(session["DesktopNames"], desktop)

    def test_prepared_niri_session_path_uses_deployed_uwsm_wrappers(self):
        with tempfile.TemporaryDirectory() as folder:
            marker = Path(folder) / ".local/share/nacre/niri-on-pc/session/source-revision"
            marker.parent.mkdir(parents=True)
            marker.write_text("fixture\n")
            result = subprocess.check_output(
                ["sh", "-c", '. "$1"; env', "probe", str(ROOT / "uwsm/env-niri")],
                env={"PATH": "/usr/bin", "HOME": folder},
                text=True,
            )
        exported = dict(
            line.split("=", 1) for line in result.splitlines() if "=" in line
        )
        self.assertTrue(
            exported["PATH"].startswith(folder + "/.config/uwsm/niri-session-bin:")
        )
        self.assertNotIn("niri-on-pc/session-bin", exported["PATH"])

    def test_niri_environment_does_not_leak_hyprland_into_activation_context(self):
        result = subprocess.check_output(
            ["sh", "-c", '. "$1"; env', "probe", str(ROOT / "uwsm/env-niri")],
            env={
                "PATH": "/usr/bin",
                "HOME": "/fixture",
                "HYPRLAND_INSTANCE_SIGNATURE": "stale",
                "GDK_BACKEND": "wayland,x11,*",
                "HYPRCURSOR_SIZE": "24",
            },
            text=True,
        )
        exported = dict(
            line.split("=", 1) for line in result.splitlines() if "=" in line
        )
        self.assertEqual(
            exported["NIRI_CONFIG"], "/fixture/.config/nacre/niri/config.kdl"
        )
        self.assertNotIn("niri-on-pc/session-bin", exported["PATH"])
        self.assertEqual(exported["NACRE_COMPOSITOR"], "niri")
        for variable in (
            "HYPRLAND_INSTANCE_SIGNATURE",
            "GDK_BACKEND",
            "HYPRCURSOR_SIZE",
        ):
            self.assertNotIn(variable, exported)
