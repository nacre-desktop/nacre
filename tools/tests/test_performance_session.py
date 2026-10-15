"""Performance guards reject unavailable measurements and large regressions."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import sys
import unittest
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location(
    "session", TOOLS / "performance-session.py"
)
session = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(session)
BUDGETS = json.loads((TOOLS / "performance-budgets.json").read_text())


def report():
    sample = {
        "pss_mib": {"shell": {"max": 200}},
        "delivered_video_fps": 24,
        "memory_valid_samples": {"pss": {"shell": 2}, "swap_pss": {"shell": 2}},
        "swap_pss_mib": {"shell": {"max": 0}},
        "samples": [
            {"processes": [{"swap_pss_mib": 0}]},
            {"processes": [{"swap_pss_mib": 0}]},
        ],
        "cpu_percent_one_core": {"shell": {"mean": 0.1}},
    }
    timing = {"frame_ms": {"p95": 400}, "synchronous_ms": {"p95": 80}, "samples": [{}]}
    return {
        "paging_start": {"pswpin": 100, "pswpout": 200},
        "paging_end": {"pswpin": 100, "pswpout": 200},
        "scenarios": {name: copy.deepcopy(sample) for name in BUDGETS["ci_pss_mib"]},
        "latency": {
            label: {
                action: copy.deepcopy(timing) for action in ("launcher", "dashboard")
            }
            for label in ("cold", "reopen")
        },
    }


class BudgetTests(unittest.TestCase):
    def test_first_nested_process_uses_private_bus_and_cannot_start_user_manager(self):
        class StopBeforeLaunch(Exception):
            pass

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            home.mkdir()
            args = SimpleNamespace(
                backend="niri",
                revision=None,
                control=False,
                helper_binary=None,
                orient_binary=None,
                parent_renderer="pixman",
                labwc_libraries=None,
                labwc="unused-labwc",
            )
            with (
                patch.dict(
                    os.environ,
                    {
                        "HOME": "/unused-owner-home",
                        "XDG_RUNTIME_DIR": "/unused-owner-runtime",
                        "DBUS_SESSION_BUS_ADDRESS": "unix:path=/unused-owner-bus",
                        "HYPRLAND_NO_SYSTEMD": "0",
                    },
                ),
                patch.object(
                    session, "prepare", return_value=(home, root / "shell", None)
                ),
                patch.object(
                    session.subprocess, "Popen", side_effect=StopBeforeLaunch
                ) as launch,
            ):
                with self.assertRaises(StopBeforeLaunch):
                    session.run(args, root)
                environment = launch.call_args.kwargs["env"]
                runtime = Path(environment["XDG_RUNTIME_DIR"])
                self.assertEqual(environment["HOME"], str(home))
                self.assertEqual(runtime.parent, Path("/tmp"))
                self.assertTrue(runtime.name.startswith("npr-"))
                self.assertEqual(
                    environment["DBUS_SESSION_BUS_ADDRESS"],
                    "unix:path=" + str(runtime / "absent-session-bus"),
                )
                self.assertEqual(environment["HYPRLAND_NO_SYSTEMD"], "1")
                self.assertFalse(runtime.exists())

    def test_palette_gate_covers_witnesses_during_selection(self):
        data = report()
        data["page_size_bytes"] = 4096
        data["wallpaper_frame_smoke"] = {}
        self.assertIn("during selection", session.quiet_window_failures(data)[0])
        data["palette_process_swap_samples"] = [{"processes": [{"swap_pss_bytes": 0}]}]
        self.assertEqual(session.quiet_window_failures(data), [])
        data["palette_process_swap_samples"][0]["processes"][0]["swap_pss_bytes"] = None
        self.assertIn("unknown SwapPss", session.quiet_window_failures(data)[0])

    def test_desktop_palette_result_requires_every_consumer_pixels(self):
        expected = {name: "112233" for name in ("shell", "gtk", "qt", "kitty", "lock")}
        frame = {
            "boundary": "compositor-pixel-capture",
            "color": "112233",
            "region": [0, 0, 10, 10],
            "capture_sha256": "a" * 64,
            "observed_at_ns": 3000000,
        }
        frames = {name: dict(frame) for name in expected}
        result = session.desktop_palette_result(1000000, expected, frames)
        self.assertTrue(result["complete"])
        self.assertEqual(result["visible_upper_bound_ms"], 2)
        frames["kitty"]["boundary"] = "OSC11 live color acknowledgement"
        result = session.desktop_palette_result(1000000, expected, frames)
        self.assertFalse(result["complete"])
        self.assertIsNone(result["visible_upper_bound_ms"])

    def test_desktop_palette_result_rejects_stale_color_and_unknown_capture(self):
        expected = {name: "112233" for name in ("shell", "gtk", "qt", "kitty", "lock")}
        frames = {"gtk": {"boundary": "compositor-pixel-capture", "color": "aabbcc"}}
        result = session.desktop_palette_result(1000000, expected, frames)
        self.assertEqual(result["missing"]["gtk"], "rendered color has not converged")
        self.assertFalse(result["complete"])

    def test_machine_receipt_records_cpu_ram_kernel_distro_and_governor(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            proc = root / "proc"
            sysroot = root / "sys"
            etc = root / "etc"
            (proc / "sys/kernel").mkdir(parents=True)
            etc.mkdir()
            (proc / "cpuinfo").write_text(
                "model name : Fixture CPU\nmodel name : Fixture CPU\n"
            )
            (proc / "meminfo").write_text("MemTotal: 1048576 kB\n")
            (proc / "sys/kernel/osrelease").write_text("fixture-kernel\n")
            (etc / "os-release").write_text('PRETTY_NAME="Fixture Linux"\n')
            governor = sysroot / "devices/system/cpu/cpufreq/policy0/scaling_governor"
            governor.parent.mkdir(parents=True)
            governor.write_text("powersave\n")
            value = session.machine_metadata(proc, sysroot, etc)
            self.assertEqual(
                value,
                {
                    "cpu": "Fixture CPU",
                    "logical_cpus": 2,
                    "ram_bytes": 1073741824,
                    "kernel": "fixture-kernel",
                    "distro": "Fixture Linux",
                    "cpu_governors": ["powersave"],
                    "errors": [],
                },
            )

    def test_native_fixture_stages_selected_helper_and_source_assets(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            helper = root / "candidate-helper"
            helper.write_bytes(b"explicit immutable candidate")
            helper.chmod(0o700)
            capture = root / "capture"
            with patch.object(session.subprocess, "run"):
                home, shell, _ = session.prepare(capture, helper_binary=helper)
            tools = home / ".local/share/nacre/shell/tools"
            self.assertEqual((tools / "nacre-helper").read_bytes(), helper.read_bytes())
            self.assertEqual((tools.parent / "source").resolve(), shell.resolve())
            self.assertEqual(
                (tools.parent / "shell-cli").resolve(),
                (home / ".local/share/nacre/shell-cli").resolve(),
            )
            self.assertEqual(
                (home / ".local/share/nacre/shell-tools").resolve(), tools.resolve()
            )
            self.assertTrue((tools.parent / "bin/nacre_shell").is_file())
            self.assertEqual(
                (
                    home
                    / ".local/share/nacre/shell-cli/src/nacre_shell/engine-pin.json"
                ).read_bytes(),
                (
                    session.ROOT / "nacre/shell-cli/src/nacre_shell/engine-pin.json"
                ).read_bytes(),
            )
            self.assertIn(
                "perfChooseWallpaper",
                (shell / "modules/drawers/NacreScreen.qml").read_text(),
            )

    def test_owner_quiet_gate_uses_bytes_and_remains_below_one_mib(self):
        data = report()
        data["page_size_bytes"] = 4096
        data["paging_end"]["pswpin"] += 126
        self.assertEqual(session.quiet_window_failures(data), [])
        data["paging_end"]["pswpin"] += 130
        self.assertIn("1 MiB", session.quiet_window_failures(data)[0])

    def test_owner_quiet_gate_rejects_swapped_or_unknown_processes(self):
        data = report()
        data["page_size_bytes"] = 4096
        data["scenarios"]["video"]["swap_pss_mib"]["shell"]["max"] = 0.01
        self.assertTrue(
            any(
                "swapped pages" in error
                for error in session.quiet_window_failures(data)
            )
        )
        data["scenarios"]["video"]["samples"][0]["processes"].append(
            {"swap_pss_mib": 0.01}
        )
        self.assertTrue(
            any(
                "swapped pages" in error
                for error in session.quiet_window_failures(data)
            )
        )
        data["scenarios"]["video"]["memory_valid_samples"]["swap_pss"]["shell"] = 1
        self.assertTrue(
            any("incomplete" in error for error in session.quiet_window_failures(data))
        )

    def test_complete_controlled_capture_passes(self):
        self.assertEqual(session.check_report(report(), BUDGETS), [])

    def test_unknown_memory_is_never_treated_as_zero(self):
        for change in (
            {"pss_mib": {"shell": None}},
            {"memory_valid_samples": {"pss": {"shell": 1}, "swap_pss": {"shell": 2}}},
            {"samples": []},
        ):
            with self.subTest(change=change):
                data = report()
                data["scenarios"]["video"].update(change)
                self.assertIn(
                    "incomplete shell PSS coverage",
                    session.check_report(data, BUDGETS)[0],
                )

    def test_memory_peak_and_latency_tail_fail_even_with_low_medians(self):
        data = report()
        data["scenarios"]["all_panels_closed"]["pss_mib"]["shell"].update(
            max=1000, median=200
        )
        data["latency"]["reopen"]["launcher"]["frame_ms"].update(p95=1500, median=400)
        failures = session.check_report(data, BUDGETS)
        self.assertEqual(len(failures), 2)
        self.assertIn("shell PSS", failures[0])
        self.assertIn("frame p95", failures[1])

    def test_software_tier_is_explicit_and_does_not_relax_hardware_budget(self):
        data = report()
        data["scenarios"]["fresh_static"]["pss_mib"]["shell"]["max"] = 800
        self.assertEqual(len(session.check_report(data, BUDGETS)), 1)
        data["budget_tier"] = "software"
        self.assertEqual(session.check_report(data, BUDGETS), [])

    def test_paging_cannot_masquerade_as_memory_savings(self):
        data = report()
        data["scenarios"]["video"]["swap_pss_mib"]["shell"]["max"] = 1
        self.assertIn("swapped shell memory", session.check_report(data, BUDGETS)[0])
        data = report()
        data["paging_end"]["pswpin"] += 1
        self.assertIn("host paging", session.check_report(data, BUDGETS)[0])

    def test_stalled_video_cannot_pass_with_low_memory(self):
        data = report()
        data["scenarios"]["video"]["delivered_video_fps"] = 0
        failures = session.check_report(data, BUDGETS)
        self.assertEqual(len(failures), 1)
        self.assertIn("stalled video frame delivery", failures[0])

    def test_missing_keyboard_capture_and_busy_idle_fail(self):
        data = report()
        data["latency"]["cold"].pop("dashboard")
        data["scenarios"]["static_after_latency"]["cpu_percent_one_core"]["shell"][
            "mean"
        ] = 10
        failures = session.check_report(data, BUDGETS)
        self.assertEqual(len(failures), 2)
        self.assertIn("missing frame timing", failures[0])
        self.assertIn("static idle CPU", failures[1])


if __name__ == "__main__":
    unittest.main()
