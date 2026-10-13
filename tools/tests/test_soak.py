"""Negative controls for leak detection and disposable-session isolation."""

import importlib.util
import io
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location("soak", TOOLS / "soak.py")
soak = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(soak)


def series(values, *, phase="measured", pid=10, start=100):
    return [
        {
            "seconds": i * 60,
            "phase": phase,
            "role": "shell",
            "pid": pid,
            "start": start,
            "pss_mib": value,
            "uss_mib": value,
            "fds": 30,
            "threads": 4,
            "children": 2,
        }
        for i, value in enumerate(values)
    ]


class AnalysisTests(unittest.TestCase):
    def test_steady_leak_is_flagged_with_real_units(self):
        result = soak.analyze(series(range(100, 160)))
        flag = next(f for f in result["growth_flags"] if f["metric"] == "pss_mib")
        self.assertAlmostEqual(flag["per_hour"], 60)
        self.assertAlmostEqual(flag["r_squared"], 1)

    def test_warmup_growth_does_not_count_as_a_leak(self):
        rows = series(range(50, 110), phase="warmup")
        rows += series([110] * 60)
        self.assertEqual(soak.analyze(rows)["growth_flags"], [])

    def test_single_step_cache_warmup_is_not_steady_growth(self):
        rows = series([100] * 55 + [200] * 5)
        self.assertEqual(soak.analyze(rows)["growth_flags"], [])

    def test_missing_memory_is_not_replaced_with_zero(self):
        rows = series([None] * 60)
        result = soak.analyze(rows)
        self.assertIsNone(result["trends"][0]["metrics"]["pss_mib"])
        self.assertEqual(result["growth_flags"], [])

    def test_pid_reuse_never_joins_two_lifetimes(self):
        rows = series([100] * 30, start=100) + series([200] * 30, start=200)
        result = soak.analyze(rows)
        self.assertEqual(len(result["trends"]), 2)
        self.assertEqual(result["growth_flags"], [])

    def test_fd_growth_is_detected_even_when_memory_is_flat(self):
        rows = series([100] * 60)
        for i, row in enumerate(rows):
            row["fds"] += i
        self.assertEqual(
            [f["metric"] for f in soak.analyze(rows)["growth_flags"]], ["fds"]
        )


class IsolationTests(unittest.TestCase):
    def test_public_toolkit_policy_survives_environment_isolation(self):
        result = subprocess.check_output(
            [
                "bash",
                "--noprofile",
                "--norc",
                "-c",
                'source "$1"\nprintf "%s\\n" "$QT_QPA_PLATFORMTHEME" "$QT_WAYLAND_DISABLE_WINDOWDECORATION"',
                "policy",
                str(TOOLS.parent / "uwsm/env"),
            ],
            env={"PATH": "/usr/bin:/bin"},
            text=True,
        ).splitlines()
        env = soak.environment(Path("/tmp/test"), "niri")
        self.assertEqual(env["QT_QPA_PLATFORMTHEME"], result[0])
        self.assertEqual(env["QT_WAYLAND_DISABLE_WINDOWDECORATION"], result[1])

    def test_protocol_tail_is_bounded_and_keeps_latest_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tail.log"
            tail = soak.TraceTail(budget=100)
            for i in range(100):
                tail.append(f"[{i}.123] frame callback\n".encode())
            tail.append(b"fatal protocol error\n")
            tail.dump(path)
            self.assertLessEqual(path.stat().st_size, 100)
            self.assertTrue(path.read_bytes().endswith(b"fatal protocol error\n"))
            self.assertGreater(tail.dropped, 0)

    def test_protocol_trace_does_not_flood_or_hide_warning_log(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "shell.log"
            tail = soak.TraceTail()
            tail.read(
                io.BytesIO(
                    b"[12.345] -> wl_surface#3.commit()\nWARN: bad binding\nxdg_surface#3: error 3\n"
                ),
                path,
            )
            self.assertNotIn("wl_surface", path.read_text())
            self.assertIn("WARN: bad binding", path.read_text())
            self.assertIn("xdg_surface#3: error 3", path.read_text())
            tail.read(
                io.BytesIO(
                    b"[22:51:39.112219] {mesa egl surface queue} -> wl_display#1.sync()\nWARN: still visible\n"
                ),
                path,
            )
            self.assertNotIn("wl_display", path.read_text())
            self.assertIn("WARN: still visible", path.read_text())

    def test_environment_cannot_reuse_live_compositor_or_bus(self):
        with patch.dict(
            soak.os.environ,
            {
                "NIRI_SOCKET": "/live/niri",
                "HYPRLAND_INSTANCE_SIGNATURE": "live",
                "NACRE_QS_CONFIG": "/live/shell",
                "XDG_DATA_HOME": "/private/data",
                "QT_PLUGIN_PATH": "/private/plugins",
            },
            clear=True,
        ):
            env = soak.environment(Path("/tmp/test"), "niri")
        self.assertNotIn("NIRI_SOCKET", env)
        self.assertNotIn("HYPRLAND_INSTANCE_SIGNATURE", env)
        self.assertNotIn("DBUS_SESSION_BUS_ADDRESS", env)
        self.assertNotIn("QT_PLUGIN_PATH", env)
        self.assertEqual(env["XDG_DATA_HOME"], "/tmp/test/home/.local/share")
        self.assertEqual(env["NACRE_QS_CONFIG"], "/tmp/test/shell/shell.qml")

    def test_parent_access_exposes_only_display_endpoint(self):
        command = soak.sandbox(Path("/tmp/test"), Path("/run/user/1000/wayland-1"), [])
        self.assertIn("--unshare-pid", command)
        self.assertIn("--unshare-net", command)
        for value in ("/run/user", "/run/dbus", str(Path.home())):
            index = command.index(value)
            self.assertEqual(command[index - 1], "--tmpfs")
        self.assertIn("/tmp/soak-parent.sock", command)
        self.assertNotIn("--session", command)

    def test_warning_reader_counts_new_lines_only(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "shell.log"
            path.write_text("INFO: loaded\nWARN: broken binding\n")
            reader = soak.WarningReader(path)
            self.assertEqual(reader.read(), 1)
            self.assertEqual(reader.read(), 1)
            with path.open("a") as stream:
                stream.write("WARN: broken binding\n")
            self.assertEqual(reader.read(), 2)
            self.assertEqual(len(reader.counts), 1)

    def test_helper_exit_probes_do_not_inflate_warning_count(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "shell.log"
            path.write_text(
                "INFO qml: SOAK_HELPER_START helper-service\n"
                "INFO qml: SOAK_HELPER_EXIT helper-service -15 1\n"
                "INFO qml: SOAK_HELPER_START helper-service\n"
            )
            reader = soak.WarningReader(path)
            self.assertEqual(reader.read(), 0)
            self.assertEqual(reader.helper_events["START helper-service"], 2)
            self.assertEqual(reader.helper_events["EXIT helper-service -15 1"], 1)


if __name__ == "__main__":
    unittest.main()
