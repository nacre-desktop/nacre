"""Only real managed sessions count; observations never claim operator unlock."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
import native_niri_acceptance_monitor as monitor


def sample(compositor, shell=False, idle=False, socket=False):
    return dict(
        compositor=compositor,
        desktopIsNiri=compositor == "niri",
        desktopIsHyprland=compositor == "hyprland",
        niriSocketPresent=socket,
        units={"nacre-shell.service": shell, "hypridle.service": idle},
    )


class NiriAcceptanceMonitorTests(unittest.TestCase):
    def test_nested_socket_does_not_count_as_a_managed_niri_login(self):
        cycle = monitor.Cycle()
        result = cycle.observe(sample("hyprland", shell=True, socket=True))
        self.assertFalse(result["niriSessionObserved"])
        self.assertFalse(result["returnedHyprland"])

    def test_plain_cycle_requires_isolation_then_clean_hyprland_environment(self):
        cycle = monitor.Cycle()
        cycle.observe(sample("hyprland", shell=True))
        result = cycle.observe(sample("niri", socket=True))
        self.assertTrue(result["niriSessionObserved"])
        self.assertTrue(result["plainNiriIsolated"])
        self.assertFalse(result["returnedHyprland"])
        self.assertFalse(
            cycle.observe(sample("hyprland", shell=True, socket=True))[
                "returnedHyprland"
            ]
        )
        self.assertTrue(
            cycle.observe(sample("hyprland", shell=True))["returnedHyprland"]
        )
        self.assertNotIn("unlockVerified", result)

    def test_plain_niri_with_idle_running_is_not_accepted(self):
        cycle = monitor.Cycle()
        result = cycle.observe(sample("niri", socket=True, idle=True))
        self.assertFalse(result["plainNiriIsolated"])
        self.assertFalse(
            cycle.observe(sample("niri", socket=True))["plainNiriIsolated"]
        )

    def test_configuration_fingerprints_detect_changes_and_reject_escape(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            backup = home / "backup"
            backup.mkdir()
            target = home / "protected"
            target.write_text("original")
            manifest = backup / "baseline-hashes.json"
            manifest.write_text(
                json.dumps(
                    {"protected": hashlib.sha256(target.read_bytes()).hexdigest()}
                )
            )
            self.assertTrue(monitor.protected_files_match(backup, home))
            target.write_text("changed")
            self.assertFalse(monitor.protected_files_match(backup, home))
            manifest.write_text("[]")
            self.assertFalse(monitor.protected_files_match(backup, home))
            manifest.write_text('{"../escape":"bad"}')
            self.assertFalse(monitor.protected_files_match(backup, home))
