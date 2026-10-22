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
        "paging_window": {
            "start": {"pswpin": 100, "pswpout": 200},
            "end": {"pswpin": 100, "pswpout": 200},
            "page_size_bytes": 4096,
            "elapsed_s": 60.0,
            "counter_bracket_elapsed_s": 60.0,
            "delta_in_pages": 0,
            "delta_in_bytes": 0,
            "delta_out_pages": 0,
            "delta_out_bytes": 0,
        },
        "cpu_percent_one_core": {"shell": {"mean": 0.1}},
    }
    timing = {"frame_ms": {"p95": 400}, "synchronous_ms": {"p95": 80}, "samples": [{}]}
    return {
        "full_idle_windows": False,
        "paging_start": {"pswpin": 100, "pswpout": 200},
        "paging_end": {"pswpin": 100, "pswpout": 200},
        "page_size_bytes": 4096,
        "scenarios": {name: copy.deepcopy(sample) for name in BUDGETS["ci_pss_mib"]},
        "latency": {
            label: {
                action: copy.deepcopy(timing) for action in ("launcher", "dashboard")
            }
            for label in ("cold", "reopen")
        },
    }


class BudgetTests(unittest.TestCase):
    def test_frame_interval_summary_keeps_raw_client_intervals_and_percentiles(self):
        self.assertEqual(
            session.frame_interval_summary([16.2, 18.1, 12.0, 30.0]),
            {
                "count": 4,
                "p50_ms": 16.2,
                "p95_ms": 30.0,
                "max_ms": 30.0,
                "raw_ms": [16.2, 18.1, 12.0, 30.0],
            },
        )
        self.assertEqual(
            session.frame_interval_summary([]),
            {"count": 0, "p50_ms": None, "p95_ms": None, "max_ms": None, "raw_ms": []},
        )

    def test_motion_trace_rejects_growing_body_even_if_its_top_matches_search(self):
        before = {
            "bodyTopY": 100,
            "bodyBottomY": 400,
            "bodyHeight": 300,
            "searchY": 350,
            "searchVisiblePx": 49,
        }
        rigid = {
            "bodyTopY": 90,
            "bodyBottomY": 390,
            "bodyHeight": 300,
            "searchY": 340,
            "searchVisiblePx": 49,
        }
        growing = {
            "bodyTopY": 90,
            "bodyBottomY": 400,
            "bodyHeight": 310,
            "searchY": 340,
            "searchVisiblePx": 49,
        }
        good = session.motion_summary([{"openPositions": [before, rigid]}])
        self.assertEqual(good["max_motion_error_px"], 0)
        self.assertEqual(good["max_body_height_change_px"], 0)
        bad = session.motion_summary([{"openPositions": [before, growing]}])
        self.assertEqual(bad["max_motion_error_px"], 10)
        self.assertEqual(bad["max_body_height_change_px"], 10)
        self.assertEqual(session.motion_summary([{}])["missing_traces"], 1)
        self.assertIsNone(session.motion_summary([{}])["max_motion_error_px"])

    def test_private_session_options_are_opt_in_and_use_private_runtime_paths(self):
        with tempfile.TemporaryDirectory() as folder:
            runtime = Path(folder)
            original = {"HOME": "/private/home"}
            default, default_socket = session.private_session_environment(
                original, runtime
            )
            self.assertEqual(default, original)
            self.assertIsNone(default_socket)
            configured, daemon_socket = session.private_session_environment(
                original, runtime, daemon_client=True, software_rendering=True
            )
            self.assertEqual(configured["NACRE_DAEMON_CLIENT"], "1")
            self.assertEqual(
                configured["NACRE_DAEMON_SOCKET"], str(runtime / "nacre.sock")
            )
            self.assertEqual(daemon_socket, runtime / "nacre.sock")
            self.assertEqual(configured["LIBGL_ALWAYS_SOFTWARE"], "1")
            self.assertEqual(configured["QT_QUICK_BACKEND"], "software")
            self.assertEqual(original, {"HOME": "/private/home"})

    def test_retained_fixture_root_is_private_for_daemon_writer_lease(self):
        with tempfile.TemporaryDirectory() as directory:
            retained = Path(directory) / "fixture"
            with patch.object(
                sys,
                "argv",
                [
                    "performance-session.py",
                    "--daemon-client",
                    "--keep-fixtures",
                    str(retained),
                    "--output",
                    str(Path(directory) / "report.json"),
                ],
            ):
                with self.assertRaisesRegex(ValueError, "requires --helper-binary"):
                    session.main()
            self.assertEqual(retained.stat().st_mode & 0o777, 0o700)

    def test_typing_probe_runs_before_text_edit_and_preserves_key_handler(self):
        with tempfile.TemporaryDirectory() as folder:
            shell = Path(folder)
            browser = shell / "modules/launcher/NacreAppBrowser.qml"
            browser.parent.mkdir(parents=True)
            browser.write_text(
                'Item { TextInput { id: search; objectName: "launcherSearch"\nKeys.onPressed: event => { originalHandler(event); } } }'
            )
            session.instrument_launcher_keys(shell)
            source = browser.read_text()
            self.assertIn("signal perfKeyReceived()", source)
            self.assertLess(
                source.index("search.perfKeyReceived();"),
                source.index("originalHandler(event)"),
            )
            self.assertIn("function onPerfKeyReceived()", session.PROBE)
            self.assertNotIn("function onTextEdited()", session.PROBE)

    def test_probe_finds_query_resources_and_reports_query_current(self):
        self.assertIn("item.resources || []", session.PROBE)
        self.assertIn("queryCurrent: root.perfQueryCurrent()", session.PROBE)

    def test_probe_waits_for_current_query_and_ready_images_after_first_frame(self):
        probe = session.PROBE
        self.assertIn("!NacreDaemon.enabled || root.perfQueryCurrent()", probe)
        self.assertIn(
            "root.perfRows?.visible ? root.perfRows : root.perfApps, true", probe
        )
        self.assertIn("perfKeyResultDeadline", probe)
        self.assertIn("keyFrameMs: root.perfKeyFrameMs", probe)
        self.assertIn("resultFrameMs: root.perfResultFrameMs", probe)
        self.assertNotIn("onEntriesChanged()", probe)

    def test_launcher_stall_hook_is_test_only_and_reported(self):
        self.assertIn("function stallOpen(milliseconds: int)", session.PROBE)
        self.assertIn("while (Date.now() < until) {}", session.PROBE)
        self.assertIn("stallInjected: root.perfStallInjected", session.PROBE)
        self.assertIn("panel.motionItem !== undefined", session.PROBE)
        self.assertIn(
            'console.log("PERF_WINDOW", root.contentItem.Window.window)', session.PROBE
        )
        self.assertIn('"--stall-open-ms"', Path(session.__file__).read_text())
        self.assertIn('"--launcher-only"', Path(session.__file__).read_text())
        self.assertIn('QSG_INFO="1"', Path(session.__file__).read_text())
        self.assertNotIn("perfEntriesSignature", session.PROBE)
        source = Path(session.__file__).read_text()
        self.assertIn("launcher result frame or timeout", source)
        self.assertIn("timeout=5", source)

    def test_niri_launcher_latency_refocuses_the_private_input_window(self):
        source = Path(session.__file__).read_text()
        self.assertIn('window["title"] == "Nacre Performance Input"', source)
        self.assertIn(
            '"focus-window",\n                                "--id",', source
        )
        self.assertIn(
            '"niri",\n                                "msg",\n                                "action",',
            source,
        )
        self.assertIn('window.get("is_focused")', source)
        self.assertIn('"niri performance input focus"', source)
        self.assertIn('"niri performance input window"', source)
        self.assertIn(
            "function focusInput(): void { perfKeys.forceActiveFocus(); }", source
        )
        self.assertIn("inputFocused: root.perfInputFocused()", source)
        self.assertIn('"performance input item focus"', source)

    def test_open_first_frame_latency_is_separate_from_animation_frame_gaps(self):
        source = Path(session.__file__).read_text()
        self.assertIn("open_to_first_frame_ms", source)
        self.assertIn("open_animation_frame_intervals", source)
        self.assertIn(
            "else if (root.perfFirstFrameMs > 0) root.perfOpenFrameMs.push",
            session.PROBE,
        )

    def test_all_three_idle_phases_observe_full_sixty_seconds(self):
        self.assertEqual(
            [
                session.idle_sample_duration(name, 1, 1, full_idle_windows=True)
                for name in session.FULL_IDLE_PHASES
            ],
            [60.0, 60.0, 60.0],
        )
        self.assertEqual(
            [
                session.idle_sample_duration(name, 1, 1)
                for name in session.FULL_IDLE_PHASES
            ],
            [1, 1, 1],
        )
        self.assertEqual(session.idle_sample_duration("video", 1, 1), 1)

    def test_legacy_idle_durations_and_reports_remain_default(self):
        self.assertFalse(report().get("full_idle_windows", False))
        for name in session.FULL_IDLE_PHASES:
            called = []
            data = session.observe_idle_phase(
                name,
                1,
                lambda duration, target=called: target.append(duration) or {},
                lambda: self.fail("legacy mode must not read per-phase counters"),
                lambda: self.fail("legacy mode must not read per-phase clock"),
                4096,
            )
            self.assertEqual(called, [1])
            self.assertNotIn("paging_window", data)

    def test_paging_windows_keep_warmup_and_idle_separate(self):
        calls = []
        counters = iter(
            (
                {"pswpin": 120, "pswpout": 4},
                {"pswpin": 123, "pswpout": 4},
            )
        )
        clock = iter((600.0, 660.0, 660.01))
        result = report()
        whole_arm_start = copy.deepcopy(result["paging_start"])
        result["paging_end"]["pswpin"] += 30
        whole_arm_end = copy.deepcopy(result["paging_end"])
        data = session.observe_idle_phase(
            "fresh_static",
            60,
            lambda duration: calls.append(duration) or {"samples": []},
            lambda: next(counters),
            lambda: next(clock),
            4096,
            full_idle_windows=True,
        )
        result["scenarios"]["fresh_static"].update(data)
        self.assertEqual(calls, [60])
        self.assertEqual(data["paging_window"]["start"]["pswpin"], 120)
        self.assertEqual(data["paging_window"]["end"]["pswpin"], 123)
        self.assertEqual(data["paging_window"]["elapsed_s"], 60)
        self.assertEqual(result["paging_start"], whole_arm_start)
        self.assertEqual(result["paging_end"], whole_arm_end)
        self.assertAlmostEqual(
            data["paging_window"]["counter_bracket_elapsed_s"], 60.01
        )
        self.assertEqual(data["paging_window"]["delta_in_pages"], 3)
        self.assertEqual(data["paging_window"]["delta_out_pages"], 0)

    def test_paging_delta_uses_recorded_page_size(self):
        counters = iter(
            (
                {"pswpin": 10, "pswpout": 2},
                {"pswpin": 13, "pswpout": 2},
            )
        )
        clock = iter((5.0, 65.0, 65.0))
        data = session.observe_idle_phase(
            "all_panels_closed",
            60,
            lambda _duration: {"samples": []},
            lambda: next(counters),
            lambda: next(clock),
            8192,
            full_idle_windows=True,
        )
        self.assertEqual(data["paging_window"]["delta_in_pages"], 3)
        self.assertEqual(data["paging_window"]["delta_in_bytes"], 24576)
        self.assertEqual(data["paging_window"]["delta_out_pages"], 0)
        self.assertEqual(data["paging_window"]["delta_out_bytes"], 0)
        self.assertEqual(data["paging_window"]["page_size_bytes"], 8192)

    def test_idle_window_requires_full_sixty_seconds_excluding_counter_read(self):
        def check(elapsed):
            data = report()
            data["full_idle_windows"] = True
            for name in session.FULL_IDLE_PHASES:
                data["scenarios"][name]["paging_window"]["elapsed_s"] = elapsed
                data["scenarios"][name]["paging_window"][
                    "counter_bracket_elapsed_s"
                ] = 600
            return session.idle_phase_failures(data)

        self.assertTrue(any("shorter than 60" in e for e in check(59.999)))
        self.assertEqual(check(60), [])

        counters = iter(({"pswpin": 1, "pswpout": 2}, {"pswpin": 1, "pswpout": 2}))
        clock = iter((0.0, 59.0, 120.0))
        data = session.observe_idle_phase(
            "fresh_static",
            59,
            lambda _duration: {},
            lambda: next(counters),
            lambda: next(clock),
            4096,
            full_idle_windows=True,
        )
        self.assertEqual(data["paging_window"]["elapsed_s"], 59.0)
        self.assertEqual(data["paging_window"]["counter_bracket_elapsed_s"], 120.0)
        validation = report()
        validation["full_idle_windows"] = True
        validation["scenarios"]["fresh_static"].update(data)
        self.assertTrue(
            any("shorter than 60" in e for e in session.idle_phase_failures(validation))
        )

    def test_whole_arm_paging_failure_remains_invalid(self):
        for full_idle_windows in (False, True):
            data = report()
            data["full_idle_windows"] = full_idle_windows
            data["page_size_bytes"] = 4096
            data["paging_end"]["pswpin"] += 256
            self.assertTrue(
                any("1 MiB" in error for error in session.quiet_window_failures(data)),
                full_idle_windows,
            )

    def test_incomplete_idle_or_counter_reset_is_invalid(self):
        data = report()
        data["full_idle_windows"] = True
        del data["scenarios"]["fresh_static"]["paging_window"]
        self.assertTrue(
            any(
                "missing full idle" in error
                for error in session.quiet_window_failures(data)
            )
        )
        data = report()
        data["full_idle_windows"] = True
        data["scenarios"]["all_panels_closed"]["paging_window"]["end"]["pswpin"] = 99
        self.assertTrue(
            any(
                "counter reset" in error
                for error in session.quiet_window_failures(data)
            )
        )

    def test_idle_counter_rejects_boolean_negative_and_outbound_reset(self):
        for field, value in (("pswpin", True), ("pswpout", -1)):
            data = report()
            data["full_idle_windows"] = True
            data["scenarios"]["fresh_static"]["paging_window"]["start"][field] = value
            self.assertTrue(session.idle_phase_failures(data), (field, value))
        data = report()
        data["full_idle_windows"] = True
        data["scenarios"]["fresh_static"]["paging_window"]["end"]["pswpout"] = 199
        self.assertTrue(
            any("counter reset" in error for error in session.idle_phase_failures(data))
        )

    def test_first_nested_process_uses_private_bus_and_cannot_start_user_manager(self):
        class StopBeforeLaunch(Exception):
            pass

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            home.mkdir()
            args = SimpleNamespace(
                backend="niri",
                daemon_client=False,
                software_rendering=False,
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
                        "GVFS_DISABLE_FUSE": "0",
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
                self.assertEqual(environment["GVFS_DISABLE_FUSE"], "1")
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
