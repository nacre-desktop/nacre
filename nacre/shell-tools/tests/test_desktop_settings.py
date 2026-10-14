import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "desktop", Path(__file__).resolve().parents[1] / "desktop-settings.py"
)
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)


def screen(name, w, h, scale=1, transform=0):
    return dict(
        name=name,
        width=w,
        height=h,
        scale=scale,
        refreshRate=120,
        x=0,
        y=0,
        transform=transform,
        mirrorOf="none",
    )


class DesktopSettingsTests(unittest.TestCase):
    def test_unsafe_and_out_of_range_preferences_are_rejected(self):
        for key, value in [
            ("gapsOut", 10000),
            ("gapsOut", True),
            ("animations", "false"),
            ("frameSheen", "false"),
            ("arbitrary.lua", "os.execute()"),
        ]:
            with self.assertRaises(ValueError):
                desktop.validate(key, value)
        self.assertEqual(desktop.validate("frameWidth", 0), 0)
        self.assertIs(desktop.validate("frameSheen", False), False)
        self.assertIs(desktop.validate("workspaceWindowHints", False), False)
        with self.assertRaises(ValueError):
            desktop.validate("workspaceWindowHints", "false")

    def test_scaled_portrait_display_extends_without_overlap(self):
        monitors = [screen("eDP-1", 2880, 1800, 1.5), screen("DP-1", 2560, 1440, 1, 1)]
        right = desktop.display_plan(monitors, "eDP-1", "extend-right")
        left = desktop.display_plan(monitors, "eDP-1", "extend-left")
        self.assertEqual(right[1]["x"], 1920)
        self.assertEqual(left[1]["x"], -1440)
        self.assertEqual(monitors[1]["x"], 0)
        self.assertEqual(
            desktop.display_plan(monitors, "DP-1", "mirror")[1]["mirrorOf"], "DP-1"
        )
        with self.assertRaises(ValueError):
            desktop.display_plan(monitors[:1], "eDP-1", "mirror")

    def test_presets_return_to_custom_normal_preferences(self):
        import io
        import sys
        from contextlib import redirect_stdout

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with (
                patch.object(desktop, "HOME", root),
                patch.object(desktop, "STATE", root / "desktop.json"),
                patch.object(desktop, "LUA", root / "desktop.lua"),
                patch.object(desktop, "PENDING", root / "pending.json"),
                patch.object(desktop.compositor, "set_desktop_options"),
            ):
                normal = dict(
                    desktop.DEFAULTS, gapsOut=27, frameWidth=4, nativeClipboard=False
                )
                desktop.persist(normal)
                for name in ("focused", "presentation", "minimal", "normal"):
                    with (
                        patch.object(sys, "argv", ["desktop-settings", "preset", name]),
                        redirect_stdout(io.StringIO()),
                    ):
                        desktop.main()
                    if name == "presentation":
                        self.assertFalse(desktop.load()["topBarEnabled"])
                        self.assertTrue(desktop.load()["dnd"])
                for key in desktop.DEFAULTS:
                    self.assertEqual(desktop.load()[key], normal[key], key)

    def test_presets_do_not_restore_old_lock_privacy_or_weather(self):
        import io
        import sys
        from contextlib import redirect_stdout

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with (
                patch.object(desktop, "HOME", root),
                patch.object(desktop, "STATE", root / "desktop.json"),
                patch.object(desktop, "LUA", root / "desktop.lua"),
                patch.object(desktop, "PENDING", root / "pending.json"),
                patch.object(desktop.compositor, "set_desktop_options"),
            ):
                desktop.persist(
                    dict(
                        desktop.DEFAULTS,
                        preset="focused",
                        weatherLocation="New city",
                        lockNotificationContents=False,
                        normalSnapshot=dict(
                            desktop.DEFAULTS,
                            weatherLocation="Old city",
                            lockNotificationContents=True,
                        ),
                    )
                )
                with (
                    patch.object(sys, "argv", ["desktop-settings", "preset", "normal"]),
                    redirect_stdout(io.StringIO()),
                ):
                    desktop.main()
                self.assertEqual(desktop.load()["weatherLocation"], "New city")
                self.assertFalse(desktop.load()["lockNotificationContents"])

    def test_every_preset_preserves_current_lock_and_weather_preferences(self):
        import io
        import sys
        from contextlib import redirect_stdout

        private = dict(
            lockMedia=False,
            lockWeather=False,
            lockNotifications=False,
            lockNotificationContents=True,
            weatherLocation="Fixture city",
            weatherFahrenheit=True,
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with (
                patch.object(desktop, "HOME", root),
                patch.object(desktop, "STATE", root / "desktop.json"),
                patch.object(desktop, "LUA", root / "desktop.lua"),
                patch.object(desktop, "PENDING", root / "pending.json"),
                patch.object(desktop.compositor, "set_desktop_options"),
            ):
                desktop.persist(dict(desktop.DEFAULTS, **private))
                for name in (
                    "focused",
                    "presentation",
                    "minimal",
                    "meeting",
                    "music",
                    "docked",
                    "normal",
                ):
                    with (
                        self.subTest(preset=name),
                        patch.object(sys, "argv", ["desktop-settings", "preset", name]),
                        redirect_stdout(io.StringIO()),
                    ):
                        desktop.main()
                        data = desktop.load()
                        self.assertEqual({key: data[key] for key in private}, private)

    def test_revert_restores_displays_without_losing_later_window_settings(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with (
                patch.object(desktop, "STATE", root / "desktop.json"),
                patch.object(desktop, "LUA", root / "desktop.lua"),
                patch.object(desktop, "PENDING", root / "pending.json"),
                patch.object(desktop.compositor, "configure_monitors") as configure,
            ):
                data = dict(
                    desktop.DEFAULTS, gapsOut=26, displays=[screen("DP-1", 1920, 1080)]
                )
                desktop.persist(data)
                desktop.PENDING.write_text(
                    json.dumps(
                        dict(
                            token="correct",
                            previous=[],
                            monitors=[screen("eDP-1", 2880, 1800, 1.5)],
                        )
                    )
                )
                desktop.revert("wrong")
                configure.assert_not_called()
                desktop.revert("correct")
                self.assertEqual(desktop.load()["gapsOut"], 26)
                self.assertEqual(desktop.load()["displays"], [])
                self.assertFalse(desktop.PENDING.exists())
                self.assertEqual(configure.call_args.args[0][0]["name"], "eDP-1")
