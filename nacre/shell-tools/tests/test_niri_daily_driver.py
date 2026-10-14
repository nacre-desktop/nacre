"""Niri daily controls must be explicit, owned, and reversible."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


niri = load("daily_niri", "compositor_backends/niri.py")
controls = load("daily_controls", "control-tools.py")
desktop = load("daily_desktop", "desktop-settings.py")


class NiriDailyDriverTests(unittest.TestCase):
    def test_owned_controller_can_be_stopped_after_gamma_control_disappears(self):
        backend = niri.Backend()
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(controls.compositor, "backend_name", "niri"),
        ):
            home = Path(folder)
            controls.save({"owner": "owned", "enabled": True}, home)
            with (
                patch.object(
                    controls.compositor, "night_light_available", return_value=False
                ),
                patch.object(
                    controls.compositor,
                    "set_night_light_temperature",
                    backend.set_night_light_temperature,
                ),
                patch.object(
                    controls, "service", side_effect=[(True, "owned"), (False, "")]
                ),
                patch.object(controls, "configured_elsewhere", return_value=False),
                patch.object(controls, "unmanaged_socket", return_value=False),
                patch.object(controls.subprocess, "run") as run,
            ):
                self.assertFalse(controls.night_light(False, home)["nightLightEnabled"])
            self.assertEqual(
                run.call_args.args[0],
                ["systemctl", "--user", "stop", "nacre-night-light.service"],
            )

    def test_controller_watch_record_honors_xdg_state_without_crossing_compositors(
        self,
    ):
        with (
            patch.dict(os.environ, {"XDG_STATE_HOME": "/tmp/test-state"}),
            patch.object(controls.compositor, "backend_name", "niri"),
        ):
            self.assertEqual(
                controls.record_path(),
                Path("/tmp/test-state/nacre/control-tools-niri.json"),
            )
            self.assertEqual(
                controls.record_path(Path("/tmp/test-home")),
                Path("/tmp/test-home/.local/state/nacre/control-tools-niri.json"),
            )

    def test_display_transactions_reject_mirroring_before_mutation_and_restore_after_hotplug(
        self,
    ):
        backend = niri.Backend()
        original = dict(
            name="test",
            width=1920,
            height=1080,
            refreshRate=60.002,
            scale=1,
            transform=0,
            x=0,
            y=0,
        )
        changed = {**original, "scale": 1.25}
        with (
            patch.object(backend, "query", return_value=[original]),
            patch.object(
                backend, "_request", return_value={"OutputConfigChanged": "Applied"}
            ) as request,
        ):
            backend.configure_monitors([changed])
            self.assertEqual(
                request.call_args_list[0].args[0]["Output"]["action"]["Mode"]["mode"][
                    "Specific"
                ]["refresh"],
                60.002,
            )
            self.assertEqual(
                request.call_args_list[1].args[0]["Output"]["action"],
                {"Scale": {"scale": {"Specific": 1.25}}},
            )
            request.reset_mock()
            with self.assertRaisesRegex(ValueError, "mirroring"):
                backend.configure_monitors([{**changed, "mirrorOf": "other"}])
            request.assert_not_called()
            request.side_effect = [
                {"OutputConfigChanged": "Applied"},
                {"OutputConfigChanged": "OutputWasMissing"},
                *[{"OutputConfigChanged": "OutputWasMissing"}] * 4,
            ]
            with self.assertRaisesRegex(RuntimeError, "not applied"):
                backend.configure_monitors([changed])
            rollback = request.call_args_list[3].args[0]["Output"]["action"]
            self.assertEqual(rollback, {"Scale": {"scale": {"Specific": 1}}})

    def test_niri_display_preferences_do_not_overwrite_hyprland_records(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            state = home / "desktop.json"
            original = [{"name": "legacy", "mirrorOf": "other"}]
            state.write_text(json.dumps({"displays": original}))
            current = [
                dict(
                    name="test",
                    width=1920,
                    height=1080,
                    refreshRate=60.002,
                    scale=1.25,
                    transform=0,
                    x=0,
                    y=0,
                )
            ]
            with (
                patch.object(desktop, "STATE", state),
                patch.object(desktop, "LUA", home / "desktop.kdl"),
                patch.object(desktop.compositor, "backend_name", "niri"),
                patch.dict(
                    desktop.compositor.capabilities,
                    {"perOutputWorkspaces": True, "monitorConfiguration": True},
                ),
                patch.object(desktop, "monitor_state", return_value=current),
                patch.object(
                    desktop.compositor, "render_desktop_options", return_value=""
                ),
                patch.object(
                    desktop.compositor,
                    "render_monitors",
                    niri.Backend().render_monitors,
                ),
            ):
                desktop.persist({"dnd": True})
                self.assertEqual(desktop.load()["displays"], current)
            saved = json.loads(state.read_text())
            self.assertEqual(saved["displays"], original)
            self.assertEqual(saved["compositorDisplays"]["niri"], current)
            self.assertIn("scale 1.25", (home / "desktop.kdl").read_text())

    def test_cycle_wraps_within_active_workspace_using_stable_window_ids(self):
        backend = niri.Backend()
        state = {
            "activeWorkspace": {"id": "niri-workspace:1"},
            "activeWindow": {"windowId": "niri-window:1"},
            "windows": [
                {
                    "windowId": "niri-window:" + str(number),
                    "workspaceId": "niri-workspace:" + str(1 if number < 4 else 2),
                }
                for number in (3, 1, 4, 2)
            ],
        }
        with (
            patch.object(backend, "_state", return_value=state),
            patch.object(backend, "focus_window") as focus,
        ):
            backend.cycle_window("next")
            self.assertEqual(focus.call_args.args, ("niri-window:2",))
            backend.cycle_window("previous")
            self.assertEqual(focus.call_args.args, ("niri-window:3",))
            with self.assertRaises(ValueError):
                backend.cycle_window("left")

    def test_generated_shortcuts_are_inspectable_without_native_ipc(self):
        backend = niri.Backend()
        path = ROOT.parents[1] / "niri/bindings.kdl"
        with patch.object(backend, "binding_source_files", return_value=[path]):
            rows = backend.query("bindings")
        self.assertGreater(len(rows), 60)
        self.assertTrue(any(row["key"] == "G" and row["modmask"] == 64 for row in rows))
        self.assertFalse(any(row["description"] == "Nacre:restore" for row in rows))
        self.assertFalse(backend.capabilities["bindingIntrospection"])
        with tempfile.TemporaryDirectory() as folder:
            private = Path(folder) / "bindings.kdl"
            private.write_text("binds { Super+Q { close-window; } }")
            with self.assertRaisesRegex(RuntimeError, "Private KDL"):
                backend.capture_bindings(private)

    def test_toggle_values_and_dimensions_are_validated_before_rendering(self):
        backend = niri.Backend()
        for preferences in (
            {"blur": "true"},
            {"rounding": -1},
            {"gapsIn": float("nan")},
            {"borderSize": "1; off"},
        ):
            with self.assertRaises(ValueError):
                backend.render_desktop_options(preferences)
        self.assertEqual(backend.render_monitors([]), "")
        with self.assertRaisesRegex(ValueError, "mirroring"):
            backend.render_monitors([{"name": "test", "mirrorOf": "other"}])

    def test_supported_desktop_settings_parse_for_both_toggle_states(self):
        if not shutil.which("niri"):
            self.skipTest("Native KDL validator unavailable")
        for enabled in (True, False):
            with tempfile.TemporaryDirectory() as folder:
                path = Path(folder) / "desktop.kdl"
                path.write_text(
                    niri.Backend().render_desktop_options(
                        dict(
                            animations=enabled,
                            blur=enabled,
                            shadow=enabled,
                            followMouse=enabled,
                            naturalScroll=enabled,
                            gapsIn=6,
                            borderSize=2,
                            rounding=12,
                        )
                    )
                )
                result = subprocess.run(
                    ["niri", "validate", "-c", str(path)],
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_failed_desktop_reload_restores_previous_include(self):
        backend = niri.Backend()
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            config = home / "config.kdl"
            config.write_text('include "desktop.kdl"\n')
            target = home / "desktop.kdl"
            target.write_text("// previous\n")
            with (
                patch.dict(
                    os.environ,
                    {
                        "NACRE_NIRI_CONFIG": str(config),
                        "NACRE_NIRI_DESKTOP_CONFIG": str(target),
                    },
                ),
                patch.object(niri.Path, "home", return_value=home),
                patch.object(backend, "query", return_value=[]),
                patch.object(
                    niri.subprocess, "run", return_value=SimpleNamespace(returncode=0)
                ),
                patch.object(
                    backend, "reload", side_effect=RuntimeError("reload failed")
                ),
            ):
                with self.assertRaisesRegex(RuntimeError, "reload failed"):
                    backend.set_desktop_options({"gapsIn": 8})
            self.assertEqual(target.read_text(), "// previous\n")
            self.assertEqual(
                sorted(p.name for p in home.iterdir()), ["config.kdl", "desktop.kdl"]
            )

    def test_shell_preferences_preserve_legacy_display_records_without_emitting_them(
        self,
    ):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            data = {"dnd": True, "displays": [{"name": "legacy-output"}]}
            with (
                patch.object(desktop, "STATE", home / "desktop.json"),
                patch.object(desktop, "LUA", home / "desktop.kdl"),
                patch.dict(
                    desktop.compositor.capabilities, {"monitorConfiguration": False}
                ),
                patch.object(
                    desktop.compositor, "render_desktop_options", return_value=""
                ),
                patch.object(
                    desktop.compositor, "render_monitors", return_value=""
                ) as monitors,
            ):
                desktop.persist(data)
            monitors.assert_called_once_with([])
            self.assertEqual(json.loads((home / "desktop.json").read_text()), data)

    def test_native_window_screenshot_keeps_opaque_identity_and_absolute_path(self):
        backend = niri.Backend()
        with patch.object(backend, "_action") as action:
            backend.screenshot_window("/tmp/test.png", "niri-window:9007199254740995")
        self.assertEqual(action.call_args.args[0], "ScreenshotWindow")
        self.assertEqual(action.call_args.args[1]["id"], 9007199254740995)
        self.assertFalse(action.call_args.args[1]["show_pointer"])
        with self.assertRaises(ValueError):
            backend.screenshot_window("relative.png", "niri-window:1")

    def test_gamma_protocol_is_required_and_discovery_does_not_launch_controller(self):
        backend = niri.Backend()
        with (
            patch.object(niri.shutil, "which", return_value="/fixture/tool"),
            patch.object(
                niri.subprocess,
                "run",
                return_value=SimpleNamespace(
                    returncode=0, stdout="ext_background_effect_manager_v1"
                ),
            ) as run,
        ):
            self.assertFalse(backend.night_light_available())
            run.assert_called_once()
            self.assertEqual(run.call_args.args[0], ["wayland-info"])
            run.return_value.stdout = "zwlr_gamma_control_manager_v1"
            self.assertTrue(backend.night_light_available())

    def test_niri_controller_off_stops_only_owned_unit_and_clears_record(self):
        backend = niri.Backend()
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(controls.compositor, "backend_name", "niri"),
        ):
            home = Path(folder)
            controls.save({"owner": "owned", "enabled": True}, home)
            with (
                patch.object(controls.compositor, "backend_name", "niri"),
                patch.object(
                    controls.compositor,
                    "night_light_service",
                    backend.night_light_service,
                ),
                patch.object(
                    controls.compositor, "night_light_available", return_value=True
                ),
                patch.object(
                    controls.compositor,
                    "set_night_light_temperature",
                    backend.set_night_light_temperature,
                ),
                patch.object(
                    controls, "service", side_effect=[(True, "owned"), (False, "")]
                ),
                patch.object(controls, "configured_elsewhere", return_value=False),
                patch.object(controls, "unmanaged_socket", return_value=False),
                patch.object(controls.subprocess, "run") as run,
            ):
                result = controls.night_light(False, home)
            self.assertFalse(result["nightLightEnabled"])
            self.assertEqual(
                run.call_args.args[0],
                ["systemctl", "--user", "stop", "nacre-night-light.service"],
            )
            self.assertEqual(controls.load(home)["owner"], "")
            controls.save({"owner": "other", "enabled": True}, home)
            with (
                patch.object(
                    controls.compositor, "night_light_available", return_value=True
                ),
                patch.object(controls, "service", return_value=(True, "unowned")),
                patch.object(controls, "configured_elsewhere", return_value=False),
                patch.object(controls, "unmanaged_socket", return_value=False),
                patch.object(controls.subprocess, "run") as run,
            ):
                with self.assertRaisesRegex(ValueError, "externally"):
                    controls.night_light(False, home)
                run.assert_not_called()
